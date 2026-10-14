// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anki_io::read_file;
#[cfg(feature = "network")]
use reqwest::Client;

use crate::collection::CollectionBuilder;
use crate::prelude::*;
use crate::storage::SchemaVersion;
use crate::sync::collection::protocol::EmptyInput;
use crate::sync::error::HttpResult;
use crate::sync::error::OrHttpErr;
#[cfg(feature = "network")]
use crate::sync::http_client::HttpSyncClient;
#[cfg(feature = "network")]
use crate::sync::login::SyncAuth;

impl Collection {
    #[cfg(feature = "network")]
    /// Download collection from AnkiWeb. Caller must re-open afterwards.
    pub async fn full_download(self, auth: SyncAuth, client: Client) -> Result<()> {
        self.full_download_with_server(HttpSyncClient::new(auth, client))
            .await
    }

    #[cfg(feature = "network")]
    // pub for tests
    pub(super) async fn full_download_with_server(self, server: HttpSyncClient) -> Result<()> {
        self.full_download_with_client(server).await
    }

    #[cfg(feature = "incremental-sync")]
    pub async fn full_download_with_client<P: super::protocol::FullSyncClient>(
        self,
        server: P,
    ) -> Result<()> {
        let col_path = self.col_path.clone();
        let _col_folder = col_path.parent().or_invalid("couldn't get col_folder")?;
        let progress = self.new_progress_handler();
        self.close(None)?;
        let temp_file = server.new_download_file(&col_path)?;
        server
            .download_file(EmptyInput::request(), temp_file.path(), progress)
            .await?;
        let col = CollectionBuilder::new(temp_file.path())
            .set_check_integrity(true)
            .build()?;
        col.storage.db.execute_batch("update col set ls=mod")?;
        col.close(None)?;
        server.replace_download(temp_file, &col_path)?;
        Ok(())
    }
}

pub fn server_download(
    col: &mut Option<Collection>,
    schema_version: SchemaVersion,
) -> HttpResult<Vec<u8>> {
    let col_path = {
        let mut col = col.take().or_internal_err("take col")?;
        let path = col.col_path.clone();
        col.transact_no_undo(|col| col.storage.increment_usn())
            .or_internal_err("incr usn")?;
        col.close(Some(schema_version)).or_internal_err("close")?;
        path
    };
    let data = read_file(col_path).or_internal_err("read col")?;
    Ok(data)
}
