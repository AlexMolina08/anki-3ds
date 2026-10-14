// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use ammonia::Url;
use anki::collection::CollectionBuilder;
use anki::prelude::*;
use anki::sync::collection::changes::*;
use anki::sync::collection::chunks::*;
use anki::sync::collection::finish::server_finish;
use anki::sync::collection::graves::*;
use anki::sync::collection::meta::*;
use anki::sync::collection::normal::SyncActionRequired;
use anki::sync::collection::protocol::*;
use anki::sync::collection::sanity::*;
use anki::sync::collection::start::*;
use anki::sync::collection::upload::UploadResponse;
use anki::sync::error::HttpError;
use anki::sync::error::HttpResult;
use anki::sync::error::OrHttpErr;
use anki::sync::login::HostKeyRequest;
use anki::sync::login::HostKeyResponse;
use anki::sync::request::SyncRequest;
use anki::sync::response::SyncResponse;
use async_trait::async_trait;
use futures::executor::block_on;
use http::StatusCode;
use rusqlite::types::ValueRef;
use serde_json::json;
use serde_json::Value;

pub type ProbeResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const CASES: &[&str] = &[
    "no_changes",
    "local",
    "remote",
    "both",
    "review",
    "deletion",
    "conflict",
    "interruption",
    "cancellation",
    "full_required",
];

struct Server {
    col: Option<Collection>,
    state: Option<ServerSyncState>,
    path: PathBuf,
    fault: Option<&'static str>,
    cancel: Arc<dyn Fn() + Send + Sync>,
    calls: Vec<String>,
}

impl Server {
    fn col(&mut self) -> HttpResult<&mut Collection> {
        if self.col.is_none() {
            self.col = Some(
                CollectionBuilder::new(&self.path)
                    .set_server(true)
                    .build()
                    .or_internal_err("reopen disposable server")?,
            );
        }
        Ok(self.col.as_mut().unwrap())
    }
    fn abort(&mut self) {
        // Matches upstream User::abort_stateful_sync_if_active(): dropping the
        // collection rolls back SQLite's open transaction. No handcrafted
        // merge.
        if self.state.take().is_some() {
            self.col = None;
        }
    }
    fn active<T>(
        &mut self,
        name: &str,
        op: impl FnOnce(&mut Collection, &mut ServerSyncState) -> anki::prelude::Result<T>,
    ) -> HttpResult<SyncResponse<T>>
    where
        T: serde::Serialize,
    {
        self.calls.push(name.into());
        let col = self.col.as_mut().or_conflict("no server collection")?;
        let state = self.state.as_mut().or_conflict("no server state")?;
        let value = op(col, state).or_bad_request("upstream server operation")?;
        if self.fault == Some("interruption") && name == "server_apply_chunk" {
            self.fault = None;
            return Err(HttpError::new_without_source(
                StatusCode::REQUEST_TIMEOUT,
                "injected lost reply",
            ));
        }
        if self.fault == Some("cancellation") && name == "server_chunk" {
            self.fault = None;
            (self.cancel)();
        }
        SyncResponse::try_from_obj(value)
    }
}

#[derive(Clone)]
struct LocalProtocol {
    server: Arc<Mutex<Server>>,
    endpoint: Url,
}

impl SyncClient for LocalProtocol {
    fn endpoint(&self) -> &Url {
        &self.endpoint
    }
    fn set_endpoint(&mut self, endpoint: Url) -> anki::prelude::Result<()> {
        self.endpoint = endpoint;
        Ok(())
    }
}

#[async_trait]
impl SyncProtocol for LocalProtocol {
    async fn host_key(
        &self,
        _: SyncRequest<HostKeyRequest>,
    ) -> HttpResult<SyncResponse<HostKeyResponse>> {
        Err(HttpError::new_without_source(
            StatusCode::NOT_IMPLEMENTED,
            "excluded from incremental probe",
        ))
    }
    async fn upload(&self, _: SyncRequest<Vec<u8>>) -> HttpResult<SyncResponse<UploadResponse>> {
        Err(HttpError::new_without_source(
            StatusCode::NOT_IMPLEMENTED,
            "excluded from incremental probe",
        ))
    }
    async fn download(&self, _: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<Vec<u8>>> {
        Err(HttpError::new_without_source(
            StatusCode::NOT_IMPLEMENTED,
            "excluded from incremental probe",
        ))
    }
    async fn meta(&self, req: SyncRequest<MetaRequest>) -> HttpResult<SyncResponse<SyncMeta>> {
        let mut server = self.server.lock().unwrap();
        server.calls.push("server_meta".into());
        server.abort();
        SyncResponse::try_from_obj(server_meta(req.json()?, server.col()?)?)
    }
    async fn start(&self, req: SyncRequest<StartRequest>) -> HttpResult<SyncResponse<Graves>> {
        let mut server = self.server.lock().unwrap();
        server.abort();
        server.col()?;
        server.state = Some(ServerSyncState::new("disposable-session"));
        server.active("server_start", |col, state| {
            server_start(req.json().map_err(AnkiError::from)?, col, state)
        })
    }
    async fn apply_graves(
        &self,
        req: SyncRequest<ApplyGravesRequest>,
    ) -> HttpResult<SyncResponse<()>> {
        let input = req.json()?;
        self.server
            .lock()
            .unwrap()
            .active("server_apply_graves", |col, state| {
                server_apply_graves(input, col, state)
            })
    }
    async fn apply_changes(
        &self,
        req: SyncRequest<ApplyChangesRequest>,
    ) -> HttpResult<SyncResponse<UnchunkedChanges>> {
        let input = req.json()?;
        self.server
            .lock()
            .unwrap()
            .active("server_apply_changes", |col, state| {
                server_apply_changes(input, col, state)
            })
    }
    async fn apply_chunk(
        &self,
        req: SyncRequest<ApplyChunkRequest>,
    ) -> HttpResult<SyncResponse<()>> {
        let input = req.json()?;
        self.server
            .lock()
            .unwrap()
            .active("server_apply_chunk", |col, state| {
                server_apply_chunk(input, col, state)
            })
    }
    async fn chunk(&self, _: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<Chunk>> {
        self.server
            .lock()
            .unwrap()
            .active("server_chunk", server_chunk)
    }
    async fn sanity_check(
        &self,
        req: SyncRequest<SanityCheckRequest>,
    ) -> HttpResult<SyncResponse<SanityCheckResponse>> {
        self.server
            .lock()
            .unwrap()
            .active("server_sanity_check", |col, _| {
                server_sanity_check(req.json().map_err(AnkiError::from)?, col)
            })
    }
    async fn finish(
        &self,
        _: SyncRequest<EmptyInput>,
    ) -> HttpResult<SyncResponse<TimestampMillis>> {
        let mut server = self.server.lock().unwrap();
        let result = server.active("server_finish", |col, _| server_finish(col));
        if result.is_ok() {
            server.state = None;
        }
        result
    }
    async fn abort(&self, _: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<()>> {
        let mut server = self.server.lock().unwrap();
        server.calls.push("abort_drop_collection".into());
        server.abort();
        SyncResponse::try_from_obj(())
    }
}

/// Read-only database evidence, including every scheduling and FSRS field.
pub fn snapshot(col: &Collection) -> ProbeResult<Value> {
    let db = col.storage.db();
    let mut tables = serde_json::Map::new();
    for table in [
        "notes",
        "cards",
        "revlog",
        "graves",
        "notetypes",
        "decks",
        "deck_config",
        "tags",
    ] {
        let mut statement = db.prepare(&format!("select * from {table} order by 1,2"))?;
        let count = statement.column_count();
        let names: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let mut rows = statement.query([])?;
        let mut output = Vec::new();
        while let Some(row) = rows.next()? {
            let mut values = Vec::new();
            for idx in 0..count {
                values.push(match row.get_ref(idx)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(i) => json!(i),
                    ValueRef::Real(f) => json!(f),
                    ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                    ValueRef::Blob(b) => json!({"blob": b}),
                });
            }
            output.push(values);
        }
        tables.insert(table.into(), json!({"columns": names, "rows": output}));
    }
    tables.insert(
        "usn".into(),
        json!(db.query_row("select usn from col", [], |r| r.get::<_, i64>(0))?),
    );
    tables.insert(
        "integrity".into(),
        json!(db.query_row("pragma integrity_check", [], |r| r.get::<_, String>(0))?),
    );
    Ok(Value::Object(tables))
}

pub fn run(root: &Path, log: impl Fn(&str)) -> ProbeResult<Value> {
    let mut results = Vec::new();
    for &name in CASES {
        log(&format!(
            "case={name} enter=Collection::normal_sync_with_client engine=original_NormalSyncer"
        ));
        let dir = root.join(name);
        let progress = Arc::new(Mutex::new(Default::default()));
        let mut col = CollectionBuilder::new(dir.join("client.anki2"))
            .set_shared_progress_state(progress.clone())
            .set_check_integrity(true)
            .build()?;
        let before = snapshot(&col)?;
        let protocol = LocalProtocol {
            server: Arc::new(Mutex::new(Server {
                col: Some(
                    CollectionBuilder::new(dir.join("server.anki2"))
                        .set_server(true)
                        .set_check_integrity(true)
                        .build()?,
                ),
                state: None,
                path: dir.join("server.anki2"),
                fault: match name {
                    "interruption" => Some("interruption"),
                    "cancellation" => Some("cancellation"),
                    _ => None,
                },
                cancel: {
                    let progress = progress.clone();
                    Arc::new(move || progress.lock().unwrap().want_abort = true)
                },
                calls: vec![],
            })),
            endpoint: Url::parse("https://disposable.invalid/")?,
        };
        let server_before = snapshot(protocol.server.lock().unwrap().col()?)?;
        #[cfg(target_os = "horizon")]
        log(&format!(
            "case={name} sampled_heap_with_two_collections={}",
            anki_3ds_sqlite_vfs::heap_used()
        ));
        let first = block_on(col.normal_sync_with_client(protocol.clone()));
        #[cfg(target_os = "horizon")]
        log(&format!(
            "case={name} sampled_heap_after_sync={}",
            anki_3ds_sqlite_vfs::heap_used()
        ));
        let first_result = format!("{first:?}");
        log(&format!("case={name} first={first_result}"));
        let mut rolled_back = None;
        if name == "interruption" || name == "cancellation" {
            match (&first, name) {
                (Err(AnkiError::NetworkError { source }), "interruption") => {
                    assert_eq!(source.kind, anki::error::NetworkErrorKind::Timeout);
                }
                (Err(AnkiError::Interrupted), "cancellation") => (),
                _ => return Err(format!("wrong injected error: {first_result}").into()),
            }
            let after_error = snapshot(&col)?;
            let server_after_error = snapshot(protocol.server.lock().unwrap().col()?)?;
            // Unbury/config work can occur before the client transaction; the
            // committed card/note/revlog/grave rows and USN must remain intact.
            for table in ["cards", "notes", "revlog", "graves", "usn"] {
                assert_eq!(
                    before[table], after_error[table],
                    "client rollback {name}: {table}"
                );
                assert_eq!(
                    server_before[table], server_after_error[table],
                    "server rollback {name}: {table}"
                );
            }
            assert_eq!(after_error["integrity"], "ok");
            rolled_back = Some(json!({"client":after_error,"server":server_after_error}));
            progress.lock().unwrap().want_abort = false;
            let recovery = block_on(col.normal_sync_with_client(protocol.clone()))?;
            assert_eq!(recovery.required, SyncActionRequired::NoChanges);
            log(&format!("case={name} rollback=verified recovery=complete"));
        } else {
            let output = first?;
            if name == "full_required" {
                assert!(matches!(
                    output.required,
                    SyncActionRequired::FullSyncRequired { .. }
                ));
            } else {
                assert_eq!(output.required, SyncActionRequired::NoChanges);
            }
        }
        let after = snapshot(&col)?;
        let mut server = protocol.server.lock().unwrap();
        let remote = snapshot(server.col()?)?;
        log(&format!(
            "case={name} upstream_server_calls={:?}",
            server.calls
        ));
        results.push(json!({"case":name,"first":first_result,"before":before,
            "server_before":server_before,"rollback":rolled_back,"after":after,
            "server":remote,"calls":server.calls}));
        col.close(None)?;
        server.col.take().unwrap().close(None)?;
        log(&format!("case={name} closed=1 integrity=ok"));
        #[cfg(target_os = "horizon")]
        log(&format!(
            "case={name} sampled_heap_bytes={} vfs_flushes={}",
            anki_3ds_sqlite_vfs::heap_used(),
            anki_3ds_sqlite_vfs::flushes()
        ));
    }
    Ok(json!(results))
}
