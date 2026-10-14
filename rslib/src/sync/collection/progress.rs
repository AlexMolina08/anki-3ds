// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#[cfg(feature = "network")]
use reqwest::Client;

#[cfg(feature = "network")]
use crate::error;
#[cfg(feature = "network")]
use crate::sync::collection::protocol::EmptyInput;
#[cfg(feature = "network")]
use crate::sync::collection::protocol::SyncProtocol;
#[cfg(feature = "network")]
use crate::sync::http_client::HttpSyncClient;
#[cfg(feature = "network")]
use crate::sync::login::SyncAuth;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStage {
    #[default]
    Connecting,
    Syncing,
    Finalizing,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FullSyncProgress {
    pub transferred_bytes: usize,
    pub total_bytes: usize,
}

#[cfg(feature = "network")]
pub async fn sync_abort(auth: SyncAuth, client: Client) -> error::Result<()> {
    HttpSyncClient::new(auth, client)
        .abort(EmptyInput::request())
        .await?
        .json()
}
