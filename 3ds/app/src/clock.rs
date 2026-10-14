// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
#[cfg(feature = "emulator")]
use std::sync::OnceLock;
use std::time::Instant;

use anki_3ds_sync_transport::soc::NativeSoc;
use anki_3ds_sync_transport::NativeSyncClient;
use ctru::services::apt::Apt;
use ctru::services::hid::Hid;
use ctru::services::hid::KeyPad;
use ctru::services::ps::Ps;
use serde::Deserialize;
use serde::Serialize;

use super::i18n::tr;
use super::ui::*;

static KNOWN: AtomicBool = AtomicBool::new(false);
static OFFSET: AtomicI32 = AtomicI32::new(0);
const PATH: &str = "sdmc:/3ds/anki-3ds/clock-correction.json";
const LEGACY: &str = "sdmc:/3ds/anki-3ds/utc-offset-minutes.txt";
#[derive(Serialize, Deserialize)]
struct Correction {
    seconds: i64,
    minutes: i32,
}
extern "C" {
    fn anki_set_clock_correction(seconds: i64);
    fn anki_local_time() -> i64;
    fn tzset();
}
fn saved() -> Option<Correction> {
    let current = std::fs::read(PATH)
        .ok()
        .and_then(|s| serde_json::from_slice::<Correction>(&s).ok());
    current
        .or_else(|| {
            let minutes: i32 = std::fs::read_to_string(LEGACY).ok()?.trim().parse().ok()?;
            (-840..=840).contains(&minutes).then_some(Correction {
                seconds: i64::from(minutes) * 60,
                minutes,
            })
        })
        .filter(|c| c.seconds.unsigned_abs() <= 54000 && (-840..=840).contains(&c.minutes))
}

fn apply(correction: &Correction) {
    // Chrono and newlib must expose the same offset to the original scheduler.
    let minutes = correction.minutes;
    let zone = format!(
        "UTC{}{:02}:{:02}",
        if minutes >= 0 { "-" } else { "+" },
        minutes.abs() / 60,
        minutes.abs() % 60
    );
    std::env::set_var("TZ", zone);
    unsafe {
        tzset();
        anki_set_clock_correction(correction.seconds);
    }
    OFFSET.store(minutes, Ordering::Relaxed);
}

pub fn init() -> i32 {
    let cache = saved();
    KNOWN.store(cache.is_some(), Ordering::Relaxed);
    let correction = cache.unwrap_or(Correction {
        seconds: 0,
        minutes: 0,
    });
    apply(&correction);
    correction.minutes
}

pub fn is_known() -> bool {
    KNOWN.load(Ordering::Relaxed)
}

pub fn connected() -> bool {
    #[cfg(feature = "emulator")]
    {
        static TEST: OnceLock<bool> = OnceLock::new();
        if *TEST.get_or_init(|| super::sync::Login::new().is_test()) {
            return true;
        }
    }
    Ui::wifi_connected()
}

pub struct Job {
    receiver: mpsc::Receiver<Result<Correction, String>>,
    worker: Option<std::thread::JoinHandle<()>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Job {
    pub fn start() -> Result<Self, String> {
        let client = NativeSyncClient::default();
        #[cfg(feature = "emulator")]
        let client = {
            let mut client = client;
            let login = super::sync::Login::new();
            if let Some(endpoint) = login.endpoint {
                client.endpoint = ammonia::Url::parse(&endpoint).map_err(|e| e.to_string())?;
            }
            if let Some(ca) = login.ca_file {
                client.ca_file = ca;
            }
            client
        };
        let cancel = client.cancel.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("clock-setup".into())
            .stack_size(1024 * 1024)
            .spawn(move || {
                let result = (|| {
                    let ps = Ps::new().map_err(|e| e.to_string())?;
                    let mut soc = NativeSoc::new().map_err(|e| e.to_string())?;
                    let time = client.server_time().map_err(|e| e.to_string());
                    let local = unsafe { anki_local_time() };
                    extern "C" {
                        fn curl_global_cleanup();
                    }
                    unsafe {
                        curl_global_cleanup();
                    }
                    let cleanup = soc.close().map_err(|e| e.to_string());
                    drop(ps);
                    cleanup?;
                    let (seconds, minutes) =
                        super::time_correction::correction(local, time?).map_err(str::to_owned)?;
                    Ok::<_, String>(Correction { seconds, minutes })
                })();
                let _ = sender.send(result);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            receiver,
            worker: Some(worker),
            cancel,
        })
    }
    pub fn poll(&mut self) -> Option<Result<i32, String>> {
        let result = match self.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err("Clock worker terminated".into()),
        };
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                return Some(Err("Clock worker terminated".into()));
            }
        }
        Some(result.and_then(|correction| {
            let data = serde_json::to_vec(&correction).map_err(|e| e.to_string())?;
            std::fs::write(PATH, data).map_err(|e| e.to_string())?;
            apply(&correction);
            KNOWN.store(true, Ordering::Relaxed);
            super::event("CLOCK_AUTO_READY");
            Ok(correction.minutes)
        }))
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn configure(
    force: bool,
    apt: &Apt,
    hid: &mut Hid,
    pending: Option<Job>,
) -> Result<i32, String> {
    if !force {
        return Ok(init());
    }
    let mut job = match pending {
        Some(job) => job,
        None => Job::start()?,
    };
    let started = Instant::now();
    let result = loop {
        if let Some(result) = job.poll() {
            break result;
        }
        if !apt.main_loop() {
            job.cancel.store(true, Ordering::Release);
        }
        hid.scan_input();
        if hid.keys_down().intersects(KeyPad::B | KeyPad::START) {
            job.cancel.store(true, Ordering::Release);
        }
        Ui::frame_begin();
        Ui::target_top(COLOR_BG_TOP);
        Ui::draw_text_centered(
            200.0,
            45.0,
            0.6,
            COLOR_TEXT_DARK,
            tr("Setting clock automatically"),
        );
        Ui::draw_text_centered(
            200.0,
            90.0,
            0.45,
            COLOR_TEXT_MUTED,
            tr("Connect to Wi-Fi once. No account needed."),
        );
        let phase = (started.elapsed().as_millis() / 120 % 8) as usize;
        for i in 0..8 {
            Ui::draw_rect(
                151.0 + i as f32 * 13.0,
                135.0,
                8.0,
                8.0,
                if i == phase {
                    COLOR_COUNT_NEW
                } else {
                    COLOR_PANEL_BORDER
                },
            );
        }
        Ui::target_bottom(COLOR_BG_BOTTOM);
        Ui::draw_text_centered(160.0, 100.0, 0.5, COLOR_TEXT_DARK, tr("Cancel [B]"));
        Ui::frame_end();
    };
    result
}

pub fn offset() -> i32 {
    OFFSET.load(Ordering::Relaxed)
}
