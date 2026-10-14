// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;

use anki::progress::Progress;
use anki::progress::ProgressState;
use anki::sync::login::sync_login_with_client;
use anki::sync::login::SyncAuth;
use anki_3ds_sync_transport::soc::NativeSoc;
use anki_3ds_sync_transport::NativeSyncClient;
use ctru::services::ps::Ps;
use futures::executor::block_on;

use super::backend::Backend;
use super::i18n::tr;

#[derive(Clone, Copy)]
pub enum Mode {
    Normal,
    Upload,
    Download,
}
pub enum Outcome {
    Complete,
    FullRequired { upload_ok: bool, download_ok: bool },
}
pub struct Completed {
    pub backend: Option<Backend>,
    pub auth: Option<SyncAuth>,
    pub result: Result<Outcome, String>,
}
#[derive(Default, serde::Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
    #[cfg(feature = "emulator")]
    pub endpoint: Option<String>,
    #[cfg(feature = "emulator")]
    pub ca_file: Option<String>,
}
impl Login {
    pub fn is_test(&self) -> bool {
        #[cfg(feature = "emulator")]
        {
            self.endpoint.is_some()
        }
        #[cfg(not(feature = "emulator"))]
        {
            false
        }
    }
    pub fn new() -> Self {
        #[cfg(feature = "emulator")]
        if let Ok(data) = std::fs::read("sdmc:/3ds/anki-3ds/sync-test.json") {
            return serde_json::from_slice(&data)
                .expect("invalid disposable sync test configuration");
        }
        Self::default()
    }
}
pub struct Job {
    receiver: mpsc::Receiver<Completed>,
    thread: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<ProgressState>>,
}
impl Job {
    pub fn start(
        backend: Backend,
        auth: Option<SyncAuth>,
        login: &Login,
        mode: Mode,
        progress: Arc<Mutex<ProgressState>>,
    ) -> Result<Self, std::io::Error> {
        progress.lock().unwrap().reset();
        let client = NativeSyncClient::new(auth.clone().unwrap_or_default());
        #[cfg(feature = "emulator")]
        let client = {
            let mut client = client;
            if let Some(endpoint) = &login.endpoint {
                client.endpoint = ammonia::Url::parse(endpoint).unwrap();
            }
            if let Some(ca) = &login.ca_file {
                client.ca_file = ca.clone();
            }
            client
        };
        let cancel = client.cancel.clone();
        let shared = progress.clone();
        let username = login.username.clone();
        let password = login.password.clone();
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("collection-sync".into())
            .stack_size(1024 * 1024)
            .spawn(move || {
                let completed = run_job(backend, client, auth, (username, password), mode, shared);
                let _ = sender.send(completed);
            })?;
        Ok(Self {
            receiver,
            thread: Some(thread),
            cancel,
            progress,
        })
    }
    pub fn finish(mut self) -> Option<Completed> {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.receiver.recv().ok()
    }
    pub fn is_syncing(&self) -> bool {
        matches!(self.progress.lock().unwrap().last_progress,Some(Progress::NormalSync(p)) if p.stage==anki::sync::collection::progress::SyncStage::Syncing)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        self.progress.lock().unwrap().want_abort = true;
    }
    pub fn poll(&mut self) -> Option<Completed> {
        match self.receiver.try_recv() {
            Ok(result) => {
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
                Some(result)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
                Some(Completed {
                    backend: None,
                    auth: None,
                    result: Err("Sync worker terminated".into()),
                })
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
    pub fn label(&self) -> String {
        match self.progress.lock().unwrap().last_progress {
            Some(Progress::NormalSync(p)) => {
                use anki::sync::collection::progress::SyncStage;
                let stage = match p.stage {
                    SyncStage::Connecting => tr("Connecting"),
                    SyncStage::Syncing => tr("Syncing changes"),
                    SyncStage::Finalizing => tr("Finalizing"),
                };
                format!(
                    "{stage}\n{}: {}\n{}: {}",
                    tr("Uploaded"),
                    p.local_update,
                    tr("Downloaded"),
                    p.remote_update
                )
            }
            Some(Progress::FullSync(p)) => super::i18n::format(
                "Collection transfer: {0} / {1} bytes",
                &[&p.transferred_bytes.to_string(), &p.total_bytes.to_string()],
            ),
            _ => tr("Connecting...").into(),
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.cancel();
            let _ = thread.join();
        }
    }
}

fn run_job(
    backend: Backend,
    mut client: NativeSyncClient,
    mut auth: Option<SyncAuth>,
    (username, mut password): (String, String),
    mode: Mode,
    progress: Arc<Mutex<ProgressState>>,
) -> Completed {
    let ps = Ps::new();
    let soc = NativeSoc::new();
    let mut backend = Some(backend);
    let result = match (ps, soc) {
        (Ok(ps), Ok(mut soc)) => {
            let login_result = if auth.is_none() {
                block_on(sync_login_with_client(&username, &password, &client)).map(|a| {
                    client.sync_key = a.hkey.clone();
                    auth = Some(a);
                })
            } else {
                Ok(())
            };
            unsafe {
                password.as_bytes_mut().fill(0);
            }
            let result = match login_result {
                Ok(()) => {
                    let (reopened, result) =
                        backend
                            .take()
                            .unwrap()
                            .sync(client, mode, &mut auth, progress.clone());
                    backend = reopened;
                    result
                }
                Err(error) => Err(Box::new(error) as Box<dyn std::error::Error>),
            };
            extern "C" {
                fn curl_global_cleanup();
            }
            unsafe {
                curl_global_cleanup();
            }
            let cleanup = soc.close();
            drop(ps);
            match cleanup {
                Ok(()) => result,
                Err(error) => Err(Box::new(error) as Box<dyn std::error::Error>),
            }
        }
        (ps, soc) => Err(format!(
            "Network initialization failed: PS={} SOC={}",
            ps.is_ok(),
            soc.is_ok()
        )
        .into()),
    };
    unsafe {
        password.as_bytes_mut().fill(0);
    }
    if let Err(error) = &result {
        super::event(&format!("SYNC_ERROR {}", super::error_text(error.as_ref())));
        if matches!(error.downcast_ref::<anki::error::AnkiError>(),Some(anki::error::AnkiError::SyncError {source}) if source.kind==anki::error::SyncErrorKind::AuthFailed)
        {
            auth = None;
        }
    }
    let result = result.map_err(|error| super::user_error(error.as_ref()));
    progress.lock().unwrap().want_abort = false;
    Completed {
        backend,
        auth,
        result,
    }
}
