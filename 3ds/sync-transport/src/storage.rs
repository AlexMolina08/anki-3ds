// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::fs::File;
use std::fs::{
    self,
};
use std::io::Write;
use std::io::{
    self,
};
use std::path::Path;

/// Preserve Horizon's device-qualified path; tempfile otherwise prepends `/`.
pub fn tempfile(parent: &Path, prefix: &str) -> io::Result<tempfile::NamedTempFile> {
    #[cfg(not(target_os = "horizon"))]
    {
        tempfile::Builder::new().prefix(prefix).tempfile_in(parent)
    }
    #[cfg(target_os = "horizon")]
    {
        use std::sync::atomic::AtomicU32;
        use std::sync::atomic::Ordering;
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        for _ in 0..128 {
            let path = parent.join(format!(
                "{prefix}{stamp:x}-{:x}",
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(tempfile::NamedTempFile::from_parts(
                        file,
                        tempfile::TempPath::from_path(path),
                    ))
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no unused temporary filename",
        ))
    }
}

fn copy_flush(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = File::open(source)?;
    let mut output = File::create(destination)?;
    io::copy(&mut input, &mut output)?;
    output.sync_all()
}

/// Recover an interrupted replacement without deleting its backup.
pub fn recover(target: &Path) -> io::Result<()> {
    let marker = target.with_extension("replace-pending");
    if !marker.exists() {
        return Ok(());
    }
    let backup = std::path::PathBuf::from(fs::read_to_string(&marker)?);
    if backup.parent() != target.parent()
        || !backup
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with(".pre-sync-")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid replacement marker",
        ));
    }
    copy_flush(&backup, target)?;
    fs::remove_file(marker)
}

/// SD cannot atomically replace an existing file. Retain the flushed old file
/// and a recovery marker before removing the destination. Directory durability
/// still depends on the SD service; no power-loss guarantee is provided.
pub fn replace(source: &Path, target: &Path) -> io::Result<()> {
    recover(target)?;
    let backup = tempfile(target.parent().unwrap(), ".pre-sync-")?;
    copy_flush(target, backup.path())?;
    let (file, backup) = backup.keep().map_err(|e| e.error)?;
    drop(file);
    let marker = target.with_extension("replace-pending");
    let mut journal = File::create(&marker)?;
    journal.write_all(backup.to_string_lossy().as_bytes())?;
    journal.sync_all()?;
    drop(journal);
    fs::remove_file(target)?;
    if let Err(error) = fs::rename(source, target) {
        recover(target)?;
        return Err(error);
    }
    fs::remove_file(marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_retains_old_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("collection.anki2");
        let new = dir.path().join("new.anki2");
        fs::write(&old, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        replace(&new, &old).unwrap();
        assert_eq!(fs::read(&old).unwrap(), b"new");
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".pre-sync-")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), b"old");
    }
    #[test]
    fn missing_destination_recovers_from_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.anki2");
        let backup = dir.path().join(".pre-sync-test");
        fs::write(&backup, b"preserved").unwrap();
        fs::write(
            path.with_extension("replace-pending"),
            backup.to_str().unwrap(),
        )
        .unwrap();
        recover(&path).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"preserved");
        assert!(backup.exists());
    }
    #[test]
    fn pending_replacement_restores_previous_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.anki2");
        let backup = dir.path().join(".pre-sync-old");
        fs::write(&backup, b"old committed bytes").unwrap();
        fs::write(&path, b"incomplete restore").unwrap();
        fs::write(
            path.with_extension("replace-pending"),
            backup.to_str().unwrap(),
        )
        .unwrap();
        recover(&path).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"old committed bytes");
    }
    #[test]
    fn failed_rename_restores_previous_commit() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("collection.anki2");
        fs::write(&target, b"old committed collection").unwrap();
        assert!(replace(&dir.path().join("missing-download"), &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old committed collection");
        assert!(!target.with_extension("replace-pending").exists());
    }
}
