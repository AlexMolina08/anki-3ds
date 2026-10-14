// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Two flushed slots avoid overwriting an existing file on Horizon's SD
//! filesystem. A temporary file is never loaded. Physical power-loss durability
//! is unverified.

use std::fs;
use std::io::Read;
use std::io::Write;
use std::io::{
    self,
};
use std::path::Path;

use anki::sync::login::SyncAuth;
use serde::Deserialize;
use serde::Serialize;

pub const DIRECTORY: &str = "sdmc:/3ds/anki-3ds";
const SLOTS: [&str; 2] = ["sync-auth.json", "sync-auth-backup.json"];

#[derive(Serialize, Deserialize)]
struct SavedAuth {
    generation: u64,
    hkey: String,
    endpoint: Option<String>,
    io_timeout_secs: Option<u32>,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid saved login; log in again or log out",
    )
}

impl SavedAuth {
    fn auth(&self) -> io::Result<SyncAuth> {
        if self.hkey.is_empty() || self.hkey.len() > 4096 {
            return Err(invalid());
        }
        let endpoint = self
            .endpoint
            .as_deref()
            .map(ammonia::Url::parse)
            .transpose()
            .map_err(|_| invalid())?;
        if endpoint.as_ref().is_some_and(|url| {
            url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
        }) {
            return Err(invalid());
        }
        Ok(SyncAuth {
            hkey: self.hkey.clone(),
            endpoint,
            io_timeout_secs: self.io_timeout_secs,
        })
    }
}

fn read(directory: &Path) -> io::Result<Option<(usize, SavedAuth)>> {
    let mut newest: Option<(usize, SavedAuth)> = None;
    let mut damaged = false;
    for (index, name) in SLOTS.iter().enumerate() {
        let file = match fs::File::open(directory.join(name)) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let mut bytes = Vec::new();
        file.take(16385).read_to_end(&mut bytes)?;
        let saved = serde_json::from_slice::<SavedAuth>(&bytes)
            .ok()
            .filter(|saved| bytes.len() <= 16384 && saved.auth().is_ok());
        if let Some(saved) = saved {
            if newest
                .as_ref()
                .is_none_or(|(_, old)| saved.generation > old.generation)
            {
                newest = Some((index, saved));
            }
        } else {
            damaged = true;
        }
    }
    if newest.is_none() && damaged {
        Err(invalid())
    } else {
        Ok(newest)
    }
}

pub fn load(directory: &Path) -> io::Result<Option<SyncAuth>> {
    read(directory)?.map(|(_, saved)| saved.auth()).transpose()
}

pub fn save(directory: &Path, auth: &SyncAuth) -> io::Result<()> {
    let previous = match read(directory) {
        Ok(previous) => previous,
        Err(e) if e.kind() == io::ErrorKind::InvalidData => None,
        Err(e) => return Err(e),
    };
    let saved = SavedAuth {
        generation: previous.as_ref().map_or(Ok(1), |(_, old)| {
            old.generation.checked_add(1).ok_or_else(invalid)
        })?,
        hkey: auth.hkey.clone(),
        endpoint: auth.endpoint.as_ref().map(ToString::to_string),
        io_timeout_secs: auth.io_timeout_secs,
    };
    saved.auth()?;
    if previous.as_ref().is_some_and(|(_, old)| {
        old.hkey == saved.hkey
            && old.endpoint == saved.endpoint
            && old.io_timeout_secs == saved.io_timeout_secs
    }) {
        return Ok(());
    }
    let slot = previous.as_ref().map_or(0, |(index, _)| 1 - index);
    let target = directory.join(SLOTS[slot]);
    let temporary = directory.join("sync-auth.tmp");
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec(&saved).map_err(|_| invalid())?)?;
        file.sync_all()?;
        drop(file);
        remove(&target)?;
        fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

pub fn clear(directory: &Path) -> io::Result<()> {
    // Attempt every removal even if one fails. Never report successful logout
    // while a persistent credential remains.
    let mut failure = None;
    for name in SLOTS.into_iter().chain(["sync-auth.tmp"]) {
        if let Err(e) = remove(&directory.join(name)) {
            failure = Some(e);
        }
    }
    failure.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn auth(key: &str) -> SyncAuth {
        SyncAuth {
            hkey: key.into(),
            endpoint: Some(ammonia::Url::parse("https://sync.ankiweb.net/").unwrap()),
            io_timeout_secs: Some(45),
        }
    }
    #[test]
    fn restart_rotation_and_logout() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        assert!(load(path).unwrap().is_none());
        for key in ["first", "second", "third"] {
            save(path, &auth(key)).unwrap();
            let loaded = load(path).unwrap().unwrap();
            assert_eq!(loaded.hkey, key);
            assert_eq!(
                loaded.endpoint.unwrap().as_str(),
                "https://sync.ankiweb.net/"
            );
            assert_eq!(loaded.io_timeout_secs, Some(45));
        }
        clear(path).unwrap();
        assert!(load(path).unwrap().is_none());
        assert_eq!(fs::read_dir(path).unwrap().count(), 0);
    }
    #[test]
    fn incomplete_write_and_failed_save_preserve_previous_login() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        save(path, &auth("valid")).unwrap();
        fs::write(path.join("sync-auth.tmp"), b"partial secret").unwrap();
        assert_eq!(load(path).unwrap().unwrap().hkey, "valid");
        fs::remove_file(path.join("sync-auth.tmp")).unwrap();
        fs::create_dir(path.join("sync-auth.tmp")).unwrap();
        assert!(save(path, &auth("replacement")).is_err());
        assert_eq!(load(path).unwrap().unwrap().hkey, "valid");
        assert!(clear(path).is_err());
    }
    #[test]
    fn corrupt_slot_recovers_and_errors_do_not_echo_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        save(path, &auth("valid")).unwrap();
        fs::write(path.join(SLOTS[1]), b"private malformed data").unwrap();
        assert_eq!(load(path).unwrap().unwrap().hkey, "valid");
        fs::write(path.join(SLOTS[0]), b"private malformed data").unwrap();
        assert!(!load(path).err().unwrap().to_string().contains("private"));
        save(path, &auth("new")).unwrap();
        assert_eq!(load(path).unwrap().unwrap().hkey, "new");
    }
    #[test]
    fn refuses_insecure_endpoint_and_empty_key() {
        let directory = tempfile::tempdir().unwrap();
        let mut value = auth("key");
        value.endpoint = Some(ammonia::Url::parse("http://example.com/").unwrap());
        assert!(save(directory.path(), &value).is_err());
        assert!(save(directory.path(), &auth("")).is_err());
    }
}
