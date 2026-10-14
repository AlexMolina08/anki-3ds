// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

fn main() {
    assert_eq!(std::env::var("TARGET").unwrap(), "armv6k-nintendo-3ds");
    cc::Build::new()
        .file("src/vfs.c")
        .flag("-fshort-enums")
        .include(std::env::var("DEP_SQLITE3_INCLUDE").unwrap())
        .warnings(true)
        .compile("anki_3ds_vfs");
    println!("cargo:rerun-if-changed=src/vfs.c");
}
