// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::io::Write;

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
pub fn run(tests: &[&TestCase]) {
    let gfx = Gfx::new().unwrap();
    let _console = Console::new(gfx.top_screen.borrow_mut());
    std::env::set_var("TMPDIR", "sdmc:/3ds/anki-3ds/upstream-tmp");
    let mut file = std::fs::File::create("sdmc:/3ds/anki-3ds/upstream-tests.txt").unwrap();
    std::panic::set_hook(Box::new(|info| {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open("sdmc:/3ds/anki-3ds/upstream-tests.txt")
            .unwrap();
        writeln!(file, "FAIL {info}").unwrap();
        file.sync_all().unwrap();
    }));
    let filter =
        std::fs::read_to_string("sdmc:/3ds/anki-3ds/upstream-filter.txt").unwrap_or_default();
    let mut sorted = tests.to_vec();
    sorted.sort_by_key(|t| t.name);
    writeln!(file, "DISCOVERED {}", sorted.len()).unwrap();
    for test in &sorted {
        writeln!(file, "LIST {}", test.name).unwrap();
    }
    file.sync_all().unwrap();
    let mut count = 0;
    for test in sorted {
        if !filter.is_empty() && !filter.lines().any(|n| n == test.name) {
            continue;
        }
        writeln!(file, "START {}", test.name).unwrap();
        file.sync_all().unwrap();
        (test.run)();
        writeln!(file, "PASS {}", test.name).unwrap();
        file.sync_all().unwrap();
        count += 1;
    }
    writeln!(file, "COMPLETE {count}").unwrap();
    file.sync_all().unwrap();
    let apt = Apt::new().unwrap();
    while apt.main_loop() {
        gfx.wait_for_vblank();
    }
}
