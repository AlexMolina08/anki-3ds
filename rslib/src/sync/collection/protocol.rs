// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::marker::PhantomData;

use ammonia::Url;
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use strum::IntoStaticStr;

use crate::prelude::TimestampMillis;
use crate::sync::collection::changes::ApplyChangesRequest;
use crate::sync::collection::changes::UnchunkedChanges;
use crate::sync::collection::chunks::ApplyChunkRequest;
use crate::sync::collection::chunks::Chunk;
use crate::sync::collection::graves::ApplyGravesRequest;
use crate::sync::collection::graves::Graves;
use crate::sync::collection::meta::MetaRequest;
use crate::sync::collection::meta::SyncMeta;
use crate::sync::collection::sanity::SanityCheckRequest;
use crate::sync::collection::sanity::SanityCheckResponse;
use crate::sync::collection::start::StartRequest;
use crate::sync::collection::upload::UploadResponse;
use crate::sync::error::HttpResult;
use crate::sync::login::HostKeyRequest;
use crate::sync::login::HostKeyResponse;
use crate::sync::request::IntoSyncRequest;
use crate::sync::request::SyncRequest;
use crate::sync::response::SyncResponse;

#[derive(IntoStaticStr, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum SyncMethod {
    HostKey,
    Meta,
    Start,
    ApplyGraves,
    ApplyChanges,
    Chunk,
    ApplyChunk,
    SanityCheck2,
    Finish,
    Abort,
    Upload,
    Download,
}

pub trait AsSyncEndpoint: Into<&'static str> {
    fn as_sync_endpoint(&self, base: &Url) -> Url;
}

impl AsSyncEndpoint for SyncMethod {
    fn as_sync_endpoint(&self, base: &Url) -> Url {
        base.join("sync/").unwrap().join(self.into()).unwrap()
    }
}

#[async_trait]
pub trait SyncProtocol: Send + Sync + 'static {
    async fn host_key(
        &self,
        req: SyncRequest<HostKeyRequest>,
    ) -> HttpResult<SyncResponse<HostKeyResponse>>;
    async fn meta(&self, req: SyncRequest<MetaRequest>) -> HttpResult<SyncResponse<SyncMeta>>;
    async fn start(&self, req: SyncRequest<StartRequest>) -> HttpResult<SyncResponse<Graves>>;
    async fn apply_graves(
        &self,
        req: SyncRequest<ApplyGravesRequest>,
    ) -> HttpResult<SyncResponse<()>>;
    async fn apply_changes(
        &self,
        req: SyncRequest<ApplyChangesRequest>,
    ) -> HttpResult<SyncResponse<UnchunkedChanges>>;
    async fn chunk(&self, req: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<Chunk>>;
    async fn apply_chunk(
        &self,
        req: SyncRequest<ApplyChunkRequest>,
    ) -> HttpResult<SyncResponse<()>>;
    async fn sanity_check(
        &self,
        req: SyncRequest<SanityCheckRequest>,
    ) -> HttpResult<SyncResponse<SanityCheckResponse>>;
    async fn finish(
        &self,
        req: SyncRequest<EmptyInput>,
    ) -> HttpResult<SyncResponse<TimestampMillis>>;
    async fn abort(&self, req: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<()>>;
    async fn upload(&self, req: SyncRequest<Vec<u8>>) -> HttpResult<SyncResponse<UploadResponse>>;
    async fn download(&self, req: SyncRequest<EmptyInput>) -> HttpResult<SyncResponse<Vec<u8>>>;
}

/// Client endpoint control used by the original incremental sync driver.
/// Protocol operations remain in SyncProtocol; this only handles routing.
pub trait SyncClient: SyncProtocol {
    fn endpoint(&self) -> &Url;
    fn set_endpoint(&mut self, endpoint: Url) -> crate::prelude::Result<()>;
}

#[cfg(feature = "network")]
impl SyncClient for crate::sync::http_client::HttpSyncClient {
    fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: Url) -> crate::prelude::Result<()> {
        self.endpoint = endpoint;
        Ok(())
    }
}

/// The sync protocol expects '{}' to be sent in requests without args.
/// Serde serializes/deserializes empty structs as 'null', so we add an empty
/// value to cause it to produce a map instead. This only applies to inputs;
/// empty outputs are returned as ()/null.
#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct EmptyInput {
    #[serde(default)]
    _pad: PhantomData<()>,
}

impl EmptyInput {
    pub(crate) fn request() -> SyncRequest<Self> {
        Self::default()
            .try_into_sync_request()
            // should be infallible
            .expect("empty input into request")
    }
}

#[cfg(feature = "incremental-sync")]
#[async_trait]
pub trait FullSyncClient: SyncClient {
    fn new_download_file(
        &self,
        target: &std::path::Path,
    ) -> crate::prelude::Result<tempfile::NamedTempFile> {
        Ok(anki_io::new_tempfile_in_parent_of(target)?)
    }

    async fn upload_file(
        &self,
        path: &std::path::Path,
        progress: crate::progress::ThrottlingProgressHandler<super::progress::FullSyncProgress>,
    ) -> HttpResult<SyncResponse<UploadResponse>>;

    async fn download_file(
        &self,
        request: SyncRequest<EmptyInput>,
        path: &std::path::Path,
        progress: crate::progress::ThrottlingProgressHandler<super::progress::FullSyncProgress>,
    ) -> HttpResult<()>;

    fn replace_download(
        &self,
        file: tempfile::NamedTempFile,
        target: &std::path::Path,
    ) -> crate::prelude::Result<()> {
        anki_io::atomic_rename(file, target, true)?;
        Ok(())
    }
}

#[cfg(feature = "network")]
#[async_trait]
impl FullSyncClient for crate::sync::http_client::HttpSyncClient {
    async fn upload_file(
        &self,
        path: &std::path::Path,
        progress: crate::progress::ThrottlingProgressHandler<super::progress::FullSyncProgress>,
    ) -> HttpResult<SyncResponse<UploadResponse>> {
        use crate::sync::error::OrHttpErr;
        let data = std::fs::read(path).or_internal_err("read collection")?;
        self.upload_with_progress(
            data.try_into_sync_request()
                .or_internal_err("upload request")?,
            progress,
        )
        .await
    }

    async fn download_file(
        &self,
        request: SyncRequest<EmptyInput>,
        path: &std::path::Path,
        progress: crate::progress::ThrottlingProgressHandler<super::progress::FullSyncProgress>,
    ) -> HttpResult<()> {
        use crate::sync::error::OrHttpErr;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .or_internal_err("open collection download")?;
        let mut writer =
            tokio::io::BufWriter::with_capacity(1024 * 1024, tokio::fs::File::from_std(file));
        self.download_into_with_progress(request, progress, &mut writer)
            .await
    }
}
