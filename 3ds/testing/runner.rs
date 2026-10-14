// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#[cfg(target_os = "horizon")]
use std::io::Write;

#[cfg(target_os = "horizon")]
use ctru::prelude::*;

pub struct TestCase {
    pub name: &'static str,
    pub run: fn(),
}
pub trait TestReturn {
    fn check(self);
}
impl TestReturn for () {
    fn check(self) {}
}
impl<E: std::fmt::Debug> TestReturn for Result<(), E> {
    fn check(self) {
        self.unwrap();
    }
}
pub fn check_return(value: impl TestReturn) {
    value.check();
}

#[cfg(target_os = "horizon")]
pub fn run(tests: &[&TestCase]) {
    let gfx = Gfx::new().unwrap();
    let _console = Console::new(gfx.top_screen.borrow_mut());
    anki_3ds_sqlite_vfs::register().unwrap();
    std::env::set_var("ANKI_TEST_MODE", "1");
    std::env::set_var("TMPDIR", "sdmc:/3ds/anki-3ds/upstream-tmp");
    std::fs::create_dir_all("sdmc:/3ds/anki-3ds/upstream-tmp").unwrap();
    let mut output = std::fs::File::create("sdmc:/3ds/anki-3ds/upstream-tests.txt").unwrap();
    std::panic::set_hook(Box::new(|info| {
        let mut out = std::fs::OpenOptions::new()
            .append(true)
            .open("sdmc:/3ds/anki-3ds/upstream-tests.txt")
            .unwrap();
        writeln!(out, "FAIL {info}").unwrap();
        out.sync_all().unwrap();
    }));
    let filter =
        std::fs::read_to_string("sdmc:/3ds/anki-3ds/upstream-filter.txt").unwrap_or_default();
    if std::fs::metadata("sdmc:/3ds/anki-3ds/filesystem-mode").is_ok() {
        let mut diagnostics = Vec::new();
        for base in [
            "sdmc:/3ds/anki-3ds/upstream-tmp",
            "/3ds/anki-3ds/upstream-tmp",
        ] {
            let path = format!("{base}/std-mkdir-diagnostic");
            let created = std::fs::create_dir(&path);
            diagnostics.push(serde_json::json!({"operation":"mkdir", "path":path,
                "result":format!("{created:?}")}));
            if created.is_ok() {
                std::fs::remove_dir(&path).unwrap();
            }
            diagnostics.push(
                serde_json::json!({"operation":"directory-open", "path":base,
                "is_directory":std::path::Path::new(base).is_dir(),
                "result":format!("{:?}",std::fs::File::open(base).map(|_| "opened"))}),
            );
            let temp = tempfile::tempdir_in(base);
            diagnostics.push(serde_json::json!({"operation":"tempdir", "path":base,
                "result":format!("{temp:?}")}));
        }
        use std::os::unix::fs::MetadataExt;
        let base = "sdmc:/3ds/anki-3ds/upstream-tmp";
        let first = format!("{base}/identity-one.txt");
        let second = format!("{base}/identity-two.txt");
        std::fs::write(&first, "one").unwrap();
        std::fs::write(&second, "two").unwrap();
        for path in [&first, &second] {
            let meta = std::fs::metadata(path).unwrap();
            diagnostics.push(serde_json::json!({"operation":"identity", "path":path,
                "dev":meta.dev(), "ino":meta.ino(),
                "canonical":format!("{:?}",std::fs::canonicalize(path))}));
        }
        diagnostics.push(serde_json::json!({"operation":"same-file-distinct",
            "result":format!("{:?}",same_file::is_same_file(&first,&second))}));
        assert_ne!(
            anki_io::canonical_sd_file(std::path::Path::new(&first)).unwrap(),
            anki_io::canonical_sd_file(std::path::Path::new(&second)).unwrap()
        );
        let upper = format!("{base}/IDENTITY-ONE.txt");
        assert_eq!(
            anki_io::canonical_sd_file(std::path::Path::new(&upper)).unwrap(),
            std::path::PathBuf::from(&first)
        );
        diagnostics.push(
            serde_json::json!({"operation":"native-file-identity-adapter",
            "distinct_files":true,"stored_case":first}),
        );
        let nfd = format!("{base}/ぱぱ-diagnostic.txt");
        let nfc = format!("{base}/ぱぱ-diagnostic.txt");
        std::fs::write(&nfd, "normalization diagnostic").unwrap();
        let names: Vec<_> = std::fs::read_dir(base)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with("-diagnostic.txt"))
            .collect();
        diagnostics.push(
            serde_json::json!({"operation":"unicode-backing-filesystem", "created":nfd,
            "stored_names":names,"nfd_exists":std::path::Path::new(&nfd).exists(),
            "nfc_exists":std::path::Path::new(&nfc).exists()}),
        );
        std::fs::remove_file(nfd).unwrap();
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
        std::fs::write(
            "sdmc:/3ds/anki-3ds/filesystem-diagnostics.json",
            serde_json::to_string_pretty(&diagnostics).unwrap(),
        )
        .unwrap();
        writeln!(output, "COMPLETE FILESYSTEM").unwrap();
        output.sync_all().unwrap();
        let apt = Apt::new().unwrap();
        while apt.main_loop() {
            gfx.wait_for_vblank();
        }
        return;
    }
    if std::fs::metadata("sdmc:/3ds/anki-3ds/differential-mode").is_ok() {
        let value = crate::scheduler::answering::horizon_differential::run();
        std::fs::write("sdmc:/3ds/anki-3ds/differential.json", value.to_string()).unwrap();
        writeln!(output, "COMPLETE DIFFERENTIAL").unwrap();
        output.sync_all().unwrap();
        let apt = Apt::new().unwrap();
        while apt.main_loop() {
            gfx.wait_for_vblank();
        }
        return;
    }
    let mut sorted = tests.to_vec();
    sorted.sort_by_key(|test| test.name);
    writeln!(output, "DISCOVERED {}", sorted.len()).unwrap();
    for test in sorted {
        writeln!(output, "LIST {}", test.name).unwrap();
    }
    output.sync_all().unwrap();
    let mut count = 0;
    for test in tests {
        if !filter.is_empty() && !filter.lines().any(|line| test.name == line) {
            continue;
        }
        writeln!(output, "START {}", test.name).unwrap();
        output.sync_all().unwrap();
        (test.run)();
        writeln!(output, "PASS {}", test.name).unwrap();
        output.sync_all().unwrap();
        count += 1;
    }
    writeln!(output, "COMPLETE {count}").unwrap();
    output.sync_all().unwrap();
    println!("PASS {count} original upstream tests");
    let apt = Apt::new().unwrap();
    while apt.main_loop() {
        gfx.wait_for_vblank();
    }
}

#[cfg(not(target_os = "horizon"))]
pub fn run(_tests: &[&TestCase]) {
    let value = crate::scheduler::answering::horizon_differential::run();
    std::fs::write(
        std::env::var("ANKI_3DS_DIFFERENTIAL_OUTPUT").unwrap(),
        value.to_string(),
    )
    .unwrap();
    println!("Completed port-specific deterministic differential scenarios");
}
