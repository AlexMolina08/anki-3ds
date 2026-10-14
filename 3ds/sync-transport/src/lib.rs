// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#[cfg(target_os = "horizon")]
pub mod soc;

use std::ffi::CStr;
use std::ffi::CString;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use ammonia::Url;
use anki::prelude::TimestampMillis;
use anki::sync::collection::changes::ApplyChangesRequest;
use anki::sync::collection::changes::UnchunkedChanges;
use anki::sync::collection::chunks::ApplyChunkRequest;
use anki::sync::collection::chunks::Chunk;
use anki::sync::collection::graves::ApplyGravesRequest;
use anki::sync::collection::graves::Graves;
use anki::sync::collection::meta::MetaRequest;
use anki::sync::collection::meta::SyncMeta;
use anki::sync::collection::protocol::AsSyncEndpoint;
use anki::sync::collection::protocol::EmptyInput;
use anki::sync::collection::protocol::SyncMethod;
use anki::sync::collection::protocol::SyncProtocol;
use anki::sync::collection::sanity::SanityCheckRequest;
use anki::sync::collection::sanity::SanityCheckResponse;
use anki::sync::collection::start::StartRequest;
use anki::sync::collection::upload::UploadResponse;
use anki::sync::error::HttpError;
use anki::sync::error::HttpResult;
use anki::sync::login::HostKeyRequest;
use anki::sync::login::HostKeyResponse;
use anki::sync::login::SyncAuth;
use anki::sync::request::header_and_stream::SyncHeader;
use anki::sync::request::SyncRequest;
use anki::sync::response::SyncResponse;
use async_trait::async_trait;
use http::StatusCode;
use thiserror::Error;

pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;

pub const DEFAULT_ANKIWEB_HOST: &str = "sync.ankiweb.net";
pub const DEFAULT_SYNC_PATH_META: &str = "/sync/meta";
pub const DEFAULT_SYNC_PATH_HOSTKEY: &str = "/sync/hostKey";

#[repr(C)]
struct RawHttpResponse {
    http_status: std::os::raw::c_int,
    curl_code: std::os::raw::c_int,
    response_body: *mut u8,
    response_body_len: usize,
    error_msg: [std::os::raw::c_char; 256],
    redirect_url: [std::os::raw::c_char; 512],
    response_capacity_peak: usize,
    sampled_heap_peak: u32,
}

#[repr(C)]
struct RequestOptions {
    port: u32,
    ca_file: *const std::os::raw::c_char,
    upload_file: *const std::os::raw::c_char,
    download_file: *const std::os::raw::c_char,
    max_file_bytes: usize,
    cancelled: extern "C" fn(*mut std::ffi::c_void) -> i32,
    progress: extern "C" fn(*mut std::ffi::c_void, u64, u64) -> i32,
    context: *mut std::ffi::c_void,
}

struct TransferContext<'a> {
    cancel: &'a AtomicBool,
    progress: Option<
        anki::progress::ThrottlingProgressHandler<
            anki::sync::collection::progress::FullSyncProgress,
        >,
    >,
}
extern "C" fn check_cancel(context: *mut std::ffi::c_void) -> i32 {
    let context = unsafe { &*(context as *const TransferContext) };
    i32::from(context.cancel.load(Ordering::Acquire))
}
extern "C" fn update_progress(context: *mut std::ffi::c_void, done: u64, total: u64) -> i32 {
    let context = unsafe { &mut *(context as *mut TransferContext) };
    if let Some(progress) = &mut context.progress {
        if progress
            .update(false, |p| {
                p.transferred_bytes = done as usize;
                p.total_bytes = total as usize;
            })
            .is_err()
        {
            context.cancel.store(true, Ordering::Release);
            return 1;
        }
    }
    check_cancel(context as *mut TransferContext as *mut _)
}

extern "C" {
    fn anki_3ds_https_post(
        host: *const std::os::raw::c_char,
        path: *const std::os::raw::c_char,
        header_anki_sync: *const std::os::raw::c_char,
        body: *const u8,
        body_len: usize,
        timeout_ms: u32,
        connect_timeout_ms: u32,
        options: *const RequestOptions,
        out_resp: *mut RawHttpResponse,
    ) -> std::os::raw::c_int;

    fn anki_3ds_http_response_free(resp: *mut RawHttpResponse);
}

#[derive(Error, Debug)]
pub enum SyncTransportError {
    #[error("Transport error (code={0}): {1}")]
    Transport(i32, String),
    #[error("HTTP error status {0}: {1}")]
    HttpStatus(u16, String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Decompression error: {0}")]
    Decompression(String),
}

#[derive(Clone)]
pub struct NativeSyncClient {
    pub endpoint: Url,
    pub sync_key: String,
    pub session_key: String,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub cancel: Arc<AtomicBool>,
    pub ca_file: String,
}

impl Default for NativeSyncClient {
    fn default() -> Self {
        Self {
            endpoint: Url::try_from("https://sync.ankiweb.net/").expect("valid default URL"),
            sync_key: String::new(),
            session_key: simple_session_id(),
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(15),
            cancel: Arc::new(AtomicBool::new(false)),
            ca_file: "sdmc:/3ds/anki-3ds/cacert.pem".into(),
        }
    }
}

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub redirect_url: Option<String>,
}

impl NativeSyncClient {
    pub fn new(auth: SyncAuth) -> Self {
        let timeout_secs = auth.io_timeout_secs.unwrap_or(30) as u64;
        Self {
            endpoint: auth
                .endpoint
                .unwrap_or_else(|| Url::try_from("https://sync.ankiweb.net/").unwrap()),
            sync_key: auth.hkey,
            session_key: simple_session_id(),
            timeout: Duration::from_secs(timeout_secs),
            connect_timeout: Duration::from_secs(15),
            cancel: Arc::new(AtomicBool::new(false)),
            ca_file: "sdmc:/3ds/anki-3ds/cacert.pem".into(),
        }
    }

    pub fn post_raw(
        &self,
        path: &str,
        header_json: &str,
        compressed_body: &[u8],
    ) -> Result<HttpResponse, SyncTransportError> {
        self.transfer(path, header_json, compressed_body, None, None, None)
    }

    fn transfer(
        &self,
        path: &str,
        header_json: &str,
        compressed_body: &[u8],
        upload: Option<&Path>,
        download: Option<&Path>,
        progress: Option<
            anki::progress::ThrottlingProgressHandler<
                anki::sync::collection::progress::FullSyncProgress,
            >,
        >,
    ) -> Result<HttpResponse, SyncTransportError> {
        if self.endpoint.scheme() != "https"
            || !self.endpoint.username().is_empty()
            || self.endpoint.password().is_some()
        {
            return Err(SyncTransportError::Serialization(
                "native sync requires HTTPS without URL credentials".into(),
            ));
        }
        let host_str = self.endpoint.host_str().unwrap_or(DEFAULT_ANKIWEB_HOST);

        let c_host =
            CString::new(host_str).map_err(|e| SyncTransportError::Serialization(e.to_string()))?;
        let c_path =
            CString::new(path).map_err(|e| SyncTransportError::Serialization(e.to_string()))?;
        let c_header = CString::new(header_json)
            .map_err(|e| SyncTransportError::Serialization(e.to_string()))?;

        let ca = CString::new(self.ca_file.as_str())
            .map_err(|e| SyncTransportError::Serialization(e.to_string()))?;
        let upload = upload
            .map(|p| CString::new(p.to_string_lossy().as_bytes()))
            .transpose()
            .map_err(|e| SyncTransportError::Serialization(e.to_string()))?;
        let download = download
            .map(|p| CString::new(p.to_string_lossy().as_bytes()))
            .transpose()
            .map_err(|e| SyncTransportError::Serialization(e.to_string()))?;
        let mut context = TransferContext {
            cancel: &self.cancel,
            progress,
        };
        let options = RequestOptions {
            port: self.endpoint.port_or_known_default().unwrap_or(443) as u32,
            ca_file: ca.as_ptr(),
            upload_file: upload.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            download_file: download.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            max_file_bytes: *anki::sync::request::MAXIMUM_SYNC_PAYLOAD_BYTES,
            cancelled: check_cancel,
            progress: update_progress,
            context: &mut context as *mut TransferContext as *mut _,
        };
        let mut raw_resp = RawHttpResponse {
            http_status: 0,
            curl_code: 0,
            response_body: std::ptr::null_mut(),
            response_body_len: 0,
            error_msg: [0; 256],
            redirect_url: [0; 512],
            response_capacity_peak: 0,
            sampled_heap_peak: 0,
        };

        let rc = unsafe {
            anki_3ds_https_post(
                c_host.as_ptr(),
                c_path.as_ptr(),
                c_header.as_ptr(),
                compressed_body.as_ptr(),
                compressed_body.len(),
                self.timeout.as_millis() as u32,
                self.connect_timeout.as_millis() as u32,
                &options,
                &mut raw_resp,
            )
        };

        tracing::debug!(target: "anki_3ds_sync_transport", compressed_request_bytes = compressed_body.len(),
            response_bytes = raw_resp.response_body_len, response_capacity_peak = raw_resp.response_capacity_peak,
            sampled_heap_peak = raw_resp.sampled_heap_peak, curl_code = raw_resp.curl_code, "native transfer measurements");
        let err_msg = unsafe {
            CStr::from_ptr(raw_resp.error_msg.as_ptr())
                .to_string_lossy()
                .into_owned()
        };
        let redirect_url = unsafe {
            let s = CStr::from_ptr(raw_resp.redirect_url.as_ptr()).to_string_lossy();
            if s.is_empty() {
                None
            } else {
                Some(s.into_owned())
            }
        };

        let body_bytes = if !raw_resp.response_body.is_null() && raw_resp.response_body_len > 0 {
            let slice = unsafe {
                std::slice::from_raw_parts(raw_resp.response_body, raw_resp.response_body_len)
            };
            let mut copy = Vec::new();
            if copy.try_reserve_exact(slice.len()).is_err() {
                unsafe { anki_3ds_http_response_free(&mut raw_resp) };
                return Err(SyncTransportError::Transport(
                    -4,
                    "response allocation failed".into(),
                ));
            }
            copy.extend_from_slice(slice);
            copy
        } else {
            Vec::new()
        };

        let status = raw_resp.http_status as u16;
        unsafe { anki_3ds_http_response_free(&mut raw_resp) };

        if rc != 0 {
            return Err(SyncTransportError::Transport(rc, err_msg));
        }

        Ok(HttpResponse {
            status,
            body: body_bytes,
            redirect_url,
        })
    }

    pub async fn request<I, O>(
        &self,
        method: impl AsSyncEndpoint,
        request: SyncRequest<I>,
    ) -> HttpResult<SyncResponse<O>> {
        if request.data.len() > MAX_DECOMPRESSED_BYTES {
            return Err(HttpError::new_without_source(
                StatusCode::PAYLOAD_TOO_LARGE,
                "sync request exceeds 16 MiB buffer budget",
            ));
        }
        let endpoint_url = method.as_sync_endpoint(&self.endpoint);
        let path = endpoint_url.path();

        let header = SyncHeader {
            sync_version: request.sync_version,
            sync_key: self.sync_key.clone(),
            client_ver: request.client_version,
            session_key: self.session_key.clone(),
        };

        let header_json = serde_json::to_string(&header).map_err(|e| HttpError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            context: format!("failed to serialize sync header: {e}"),
            source: None,
        })?;

        let compressed_body = zstd::encode_all(&request.data[..], 0).map_err(|e| HttpError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            context: format!("failed to compress request body: {e}"),
            source: None,
        })?;

        if compressed_body.len() > MAX_RESPONSE_BYTES {
            return Err(HttpError::new_without_source(
                StatusCode::PAYLOAD_TOO_LARGE,
                "compressed sync request exceeds 8 MiB buffer budget",
            ));
        }
        let resp = self
            .post_raw(path, &header_json, &compressed_body)
            .map_err(|e| self.transport_error(e))?;

        let status_code = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::BAD_GATEWAY);

        if status_code == StatusCode::PERMANENT_REDIRECT {
            let redirect_target = resp.redirect_url.unwrap_or_default();
            return Err(HttpError {
                code: StatusCode::PERMANENT_REDIRECT,
                context: redirect_target,
                source: None,
            });
        }

        if !status_code.is_success() {
            let body_preview = format!("HTTP {} from native sync transport", status_code.as_u16());
            return Err(HttpError {
                code: status_code,
                context: body_preview,
                source: Some(Box::new(anki::error::error_for_status_code(
                    String::new(),
                    status_code,
                ))),
            });
        }

        let decompressed_body = decompress_zstd_body_if_needed(&resp.body)?;
        Ok(SyncResponse::from_vec(decompressed_body))
    }
}

fn decompress_zstd_body_if_needed(body: &[u8]) -> HttpResult<Vec<u8>> {
    if body.len() >= 4 && body[0..4] == [0x28, 0xb5, 0x2f, 0xfd] {
        let mut decoder = zstd::Decoder::new(body).map_err(|e| HttpError {
            code: StatusCode::BAD_GATEWAY,
            context: format!("invalid zstd stream: {e}"),
            source: None,
        })?;
        decoder.window_log_max(23).map_err(io_http)?;
        let mut out = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let count = decoder.read(&mut buffer).map_err(|e| HttpError {
                code: StatusCode::BAD_GATEWAY,
                context: format!("zstd decompression error: {e}"),
                source: None,
            })?;
            if count == 0 {
                break;
            }
            if count > MAX_DECOMPRESSED_BYTES - out.len() {
                return Err(HttpError::new_without_source(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "decompressed sync response exceeds 16 MiB budget",
                ));
            }
            out.try_reserve(count).map_err(|_| {
                HttpError::new_without_source(
                    StatusCode::INSUFFICIENT_STORAGE,
                    "sync response allocation failed",
                )
            })?;
            out.extend_from_slice(&buffer[..count]);
        }
        Ok(out)
    } else {
        Ok(body.to_vec())
    }
}

fn simple_session_id() -> String {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(123456789);
    format!("{:016x}", seed)
}

#[async_trait]
impl SyncProtocol for NativeSyncClient {
    async fn host_key(
        &self,
        req: SyncRequest<HostKeyRequest>,
    ) -> HttpResult<SyncResponse<HostKeyResponse>> {
        self.request(SyncMethod::HostKey, req).await
    }

    async fn meta(&self, req: SyncRequest<MetaRequest>) -> HttpResult<SyncResponse<SyncMeta>> {
        self.request(SyncMethod::Meta, req).await
    }

    async fn start(&self, req: SyncRequest<StartRequest>) -> HttpResult<SyncResponse<Graves>> {
        self.request(SyncMethod::Start, req).await
    }

    async fn apply_graves(
        &self,
        req: SyncRequest<ApplyGravesRequest>,
    ) -> HttpResult<SyncResponse<()>> {
        self.request(SyncMethod::ApplyGraves, req).await
    }

    async fn apply_changes(
        &self,
        req: SyncRequest<ApplyChangesRequest>,
    ) -> HttpResult<SyncResponse<UnchunkedChanges>> {
        self.request(SyncMethod::ApplyChanges, req).await
    }

    async fn chunk(&self, req: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<Chunk>> {
        self.request(SyncMethod::Chunk, req).await
    }

    async fn apply_chunk(
        &self,
        req: SyncRequest<ApplyChunkRequest>,
    ) -> HttpResult<SyncResponse<()>> {
        self.request(SyncMethod::ApplyChunk, req).await
    }

    async fn sanity_check(
        &self,
        req: SyncRequest<SanityCheckRequest>,
    ) -> HttpResult<SyncResponse<SanityCheckResponse>> {
        self.request(SyncMethod::SanityCheck2, req).await
    }

    async fn finish(
        &self,
        req: SyncRequest<EmptyInput>,
    ) -> HttpResult<SyncResponse<TimestampMillis>> {
        self.request(SyncMethod::Finish, req).await
    }

    async fn abort(&self, req: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<()>> {
        let was_cancelled = self.cancel.swap(false, Ordering::AcqRel);
        let mut client = self.clone();
        client.timeout = Duration::from_secs(5);
        let response = client.request(SyncMethod::Abort, req).await;
        self.cancel.store(was_cancelled, Ordering::Release);
        response
    }

    async fn upload(&self, req: SyncRequest<Vec<u8>>) -> HttpResult<SyncResponse<UploadResponse>> {
        self.request(SyncMethod::Upload, req).await
    }

    async fn download(&self, req: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<Vec<u8>>> {
        self.request(SyncMethod::Download, req).await
    }
}

impl anki::sync::collection::protocol::SyncClient for NativeSyncClient {
    fn endpoint(&self) -> &Url {
        &self.endpoint
    }
    fn set_endpoint(&mut self, endpoint: Url) -> anki::prelude::Result<()> {
        let same_host = endpoint.origin() == self.endpoint.origin();
        let official = |url: &Url| {
            url.host_str()
                .is_some_and(|host| host == "ankiweb.net" || host.ends_with(".ankiweb.net"))
        };
        if endpoint.scheme() != "https"
            || endpoint.port_or_known_default() != Some(443)
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || !(same_host || (official(&self.endpoint) && official(&endpoint)))
        {
            return Err(anki::error::SyncError {
                info: "untrusted native sync redirect".into(),
                kind: anki::error::SyncErrorKind::Other,
            }
            .into());
        }
        self.endpoint = endpoint;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anki::sync::collection::protocol::SyncClient;

    use super::*;

    #[test]
    fn response_preserves_binary_bytes() {
        let bytes = [0, 255, 0, 128, 10, 13];
        assert_eq!(decompress_zstd_body_if_needed(&bytes).unwrap(), bytes);
        let compressed = zstd::encode_all(&bytes[..], 0).unwrap();
        assert_eq!(decompress_zstd_body_if_needed(&compressed).unwrap(), bytes);
    }

    #[test]
    fn oversized_decompressed_response_fails_without_truncation() {
        let input = vec![0; MAX_DECOMPRESSED_BYTES + 1];
        let compressed = zstd::encode_all(&input[..], 0).unwrap();
        assert_eq!(
            decompress_zstd_body_if_needed(&compressed)
                .unwrap_err()
                .code,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn damaged_zstd_response_fails() {
        assert_eq!(
            decompress_zstd_body_if_needed(&[0x28, 0xb5, 0x2f, 0xfd, 255])
                .unwrap_err()
                .code,
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn redirect_rejects_downgrade_and_untrusted_hosts() {
        let mut client = NativeSyncClient::default();
        for target in [
            "http://sync.ankiweb.net/",
            "https://attacker.invalid/",
            "https://sync.ankiweb.net:444/",
            "https://secret@sync.ankiweb.net/",
        ] {
            assert!(
                client.set_endpoint(Url::parse(target).unwrap()).is_err(),
                "{target}"
            );
            assert_eq!(client.endpoint.as_str(), "https://sync.ankiweb.net/");
        }
        client
            .set_endpoint(Url::parse("https://sync2.ankiweb.net/").unwrap())
            .unwrap();
        assert_eq!(client.endpoint.as_str(), "https://sync2.ankiweb.net/");
    }
}

pub mod storage;

fn io_http(error: impl std::fmt::Display) -> HttpError {
    HttpError::new_without_source(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

impl NativeSyncClient {
    fn transport_error(&self, error: SyncTransportError) -> HttpError {
        let kind = if self.cancel.load(Ordering::Acquire) {
            anki::error::AnkiError::Interrupted
        } else {
            let category = match &error {
                SyncTransportError::Transport(28, _) => anki::error::NetworkErrorKind::Timeout,
                SyncTransportError::Transport(-2, info) if info.contains("timeout") => {
                    anki::error::NetworkErrorKind::Timeout
                }
                SyncTransportError::Transport(-2 | 6 | 7, _) => {
                    anki::error::NetworkErrorKind::Offline
                }
                _ => anki::error::NetworkErrorKind::Other,
            };
            anki::error::NetworkError {
                info: error.to_string(),
                kind: category,
            }
            .into()
        };
        HttpError {
            code: StatusCode::BAD_GATEWAY,
            context: "native transport failed".into(),
            source: Some(Box::new(kind)),
        }
    }
    fn file_header(
        &self,
        request: SyncRequest<anki::sync::collection::protocol::EmptyInput>,
    ) -> HttpResult<String> {
        serde_json::to_string(&SyncHeader {
            sync_version: request.sync_version,
            sync_key: self.sync_key.clone(),
            client_ver: request.client_version,
            session_key: self.session_key.clone(),
        })
        .map_err(io_http)
    }
    fn file_error(&self, error: std::io::Error) -> HttpError {
        if self.cancel.load(Ordering::Acquire) {
            HttpError {
                code: StatusCode::BAD_GATEWAY,
                context: "interrupted".into(),
                source: Some(Box::new(anki::error::AnkiError::Interrupted)),
            }
        } else {
            io_http(error)
        }
    }
    fn check_file_response(&self, response: HttpResponse) -> HttpResult<HttpResponse> {
        let code = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
        if !code.is_success() {
            return Err(HttpError {
                code,
                context: format!("HTTP {}", code.as_u16()),
                source: Some(Box::new(anki::error::error_for_status_code(
                    String::new(),
                    code,
                ))),
            });
        }
        Ok(response)
    }
}

#[async_trait]
impl anki::sync::collection::protocol::FullSyncClient for NativeSyncClient {
    fn new_download_file(&self, target: &Path) -> anki::prelude::Result<tempfile::NamedTempFile> {
        Ok(storage::tempfile(target.parent().unwrap(), ".download-")?)
    }

    async fn upload_file(
        &self,
        path: &Path,
        progress: anki::progress::ThrottlingProgressHandler<
            anki::sync::collection::progress::FullSyncProgress,
        >,
    ) -> HttpResult<SyncResponse<UploadResponse>> {
        use anki::sync::request::IntoSyncRequest;
        let compressed = storage::tempfile(path.parent().unwrap(), ".sync-").map_err(io_http)?;
        let mut encoder = zstd::Encoder::new(compressed.as_file(), 0).map_err(io_http)?;
        let mut input = std::fs::File::open(path).map_err(io_http)?;
        copy_with_cancel(&mut input, &mut encoder, &self.cancel).map_err(|e| self.file_error(e))?;
        encoder
            .finish()
            .map_err(io_http)?
            .sync_all()
            .map_err(io_http)?;
        let header = self.file_header(
            EmptyInput::default()
                .try_into_sync_request()
                .map_err(io_http)?,
        )?;
        let endpoint = SyncMethod::Upload.as_sync_endpoint(&self.endpoint);
        let response = self
            .transfer(
                endpoint.path(),
                &header,
                &[],
                Some(compressed.path()),
                None,
                Some(progress),
            )
            .map_err(|e| self.transport_error(e))?;
        let response = self.check_file_response(response)?;
        Ok(SyncResponse::from_vec(decompress_zstd_body_if_needed(
            &response.body,
        )?))
    }

    async fn download_file(
        &self,
        request: SyncRequest<EmptyInput>,
        path: &Path,
        progress: anki::progress::ThrottlingProgressHandler<
            anki::sync::collection::progress::FullSyncProgress,
        >,
    ) -> HttpResult<()> {
        let compressed = storage::tempfile(path.parent().unwrap(), ".sync-").map_err(io_http)?;
        let body = zstd::encode_all(&request.data[..], 0).map_err(io_http)?;
        let header = self.file_header(request)?;
        let endpoint = SyncMethod::Download.as_sync_endpoint(&self.endpoint);
        let response = self
            .transfer(
                endpoint.path(),
                &header,
                &body,
                None,
                Some(compressed.path()),
                Some(progress),
            )
            .map_err(|e| self.transport_error(e))?;
        self.check_file_response(response)?;
        let mut input = std::fs::File::open(compressed.path()).map_err(io_http)?;
        let mut magic = [0; 4];
        input.read_exact(&mut magic).map_err(io_http)?;
        input.seek(SeekFrom::Start(0)).map_err(io_http)?;
        let mut output = std::fs::File::create(path).map_err(io_http)?;
        let limit = *anki::sync::request::MAXIMUM_SYNC_PAYLOAD_BYTES_UNCOMPRESSED;
        let bytes = if magic == [0x28, 0xb5, 0x2f, 0xfd] {
            let mut decoder = zstd::Decoder::new(input).map_err(io_http)?;
            decoder.window_log_max(23).map_err(io_http)?;
            copy_with_cancel(&mut decoder.take(limit + 1), &mut output, &self.cancel)
                .map_err(|e| self.file_error(e))?
        } else {
            copy_with_cancel(&mut input.take(limit + 1), &mut output, &self.cancel)
                .map_err(|e| self.file_error(e))?
        };
        if bytes > limit {
            return Err(HttpError::new_without_source(
                StatusCode::PAYLOAD_TOO_LARGE,
                "collection exceeds download limit",
            ));
        }
        output.sync_all().map_err(io_http)?;
        Ok(())
    }

    fn replace_download(
        &self,
        file: tempfile::NamedTempFile,
        target: &Path,
    ) -> anki::prelude::Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(anki::error::AnkiError::Interrupted);
        }
        #[cfg(target_os = "horizon")]
        {
            storage::replace(file.path(), target)?;
            Ok(())
        }
        #[cfg(not(target_os = "horizon"))]
        {
            storage::replace(file.path(), target).map_err(Into::into)
        }
    }
}

fn copy_with_cancel(
    input: &mut impl std::io::Read,
    output: &mut impl std::io::Write,
    cancel: &AtomicBool,
) -> std::io::Result<u64> {
    let mut buffer = [0u8; 32 * 1024];
    let mut total = 0;
    #[cfg(target_os = "horizon")]
    let mut heap_peak = 0;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "sync cancelled",
            ));
        }
        let count = input.read(&mut buffer)?;
        if count == 0 {
            #[cfg(target_os = "horizon")]
            tracing::debug!(target:"anki_3ds_sync_transport",sampled_heap_peak=heap_peak,stream_bytes=total,"native codec measurements");
            return Ok(total);
        }
        output.write_all(&buffer[..count])?;
        #[cfg(target_os = "horizon")]
        {
            extern "C" {
                fn anki_3ds_transport_heap_used() -> u32;
            }
            heap_peak = heap_peak.max(unsafe { anki_3ds_transport_heap_used() });
        }
        total += count as u64;
    }
}
