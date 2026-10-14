// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::env;

fn main() {
    let devkitpro = env::var("DEVKITPRO").unwrap_or_else(|_| "/opt/devkitpro".to_string());
    let devkitarm = env::var("DEVKITARM").unwrap_or_else(|_| format!("{devkitpro}/devkitARM"));

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/ui_renderer.c");
    println!("cargo:rerun-if-changed=src/clock.c");
    println!("cargo:rerun-if-changed=src/ui_renderer.h");

    let mut cc = cc::Build::new();
    cc.compiler(format!("{devkitarm}/bin/arm-none-eabi-gcc"))
        .archiver(format!("{devkitarm}/bin/arm-none-eabi-ar"))
        .include(format!("{devkitpro}/libctru/include"))
        .file("src/ui_renderer.c")
        .file("src/clock.c")
        .define("ARM11", None)
        .define("__3DS__", None)
        .flag("-march=armv6k")
        .flag("-mtune=mpcore")
        .flag("-mfloat-abi=hard")
        .flag("-mfpu=vfp")
        .flag("-mtp=soft")
        .flag("-O2")
        .compile("ui_renderer");

    println!("cargo:rustc-link-arg=-Wl,--wrap=clock_gettime,--wrap=time");
    println!("cargo:rustc-link-lib=static=citro2d");
    println!("cargo:rustc-link-lib=static=citro3d");
}
