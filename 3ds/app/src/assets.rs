// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;

const CA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../out/3ds/https-sources/cacert.pem"
));

pub fn prepare() -> io::Result<()> {
    prepare_in(Path::new("sdmc:/3ds/anki-3ds"))
}

fn prepare_in(folder: &Path) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    let path = folder.join("cacert.pem");
    if path.is_file() {
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    if let Err(error) = file.write_all(CA).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let root = std::env::var("ANKI_3DS_ASSET_TEST_ROOT").expect("isolated test path");
        Path::new(&root).join(format!("{}-{name}", std::process::id()))
    }

    #[test]
    fn first_install_creates_parent_and_exact_certificate_bundle() {
        let path = folder("fresh");
        assert!(!path.exists());
        prepare_in(&path).unwrap();
        assert_eq!(fs::read(path.join("cacert.pem")).unwrap(), CA);
    }

    #[test]
    fn existing_bundle_and_collection_are_preserved() {
        let path = folder("existing");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("cacert.pem"), b"existing certificate fixture").unwrap();
        fs::write(path.join("prototype.anki2"), b"collection fixture").unwrap();
        prepare_in(&path).unwrap();
        assert_eq!(
            fs::read(path.join("cacert.pem")).unwrap(),
            b"existing certificate fixture"
        );
        assert_eq!(
            fs::read(path.join("prototype.anki2")).unwrap(),
            b"collection fixture"
        );
    }

    #[test]
    fn conflicting_file_reports_an_error() {
        let path = folder("blocked");
        fs::write(&path, b"existing file").unwrap();
        assert!(prepare_in(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), b"existing file");
    }
}
