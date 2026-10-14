// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::env;
use std::path::PathBuf;

fn main() {
    for path in [
        "c/https_client.c",
        "c/https_client.h",
        "c/dns.c",
        "c/dns.h",
        "c/entropy.c",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("horizon") {
        return;
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let prefix = manifest_dir.join("../../out/3ds/https-prefix");
    let devkitpro = env::var("DEVKITPRO").unwrap_or_else(|_| "/opt/devkitpro".to_string());
    let devkitarm = env::var("DEVKITARM").unwrap_or_else(|_| format!("{devkitpro}/devkitARM"));
    let libctru = PathBuf::from(&devkitpro).join("libctru");

    let mut native = cc::Build::new();
    if env::var_os("CARGO_FEATURE_EMULATOR").is_some() {
        native.define("ANKI_3DS_EMULATOR", None);
    }
    native
        .compiler(format!("{devkitarm}/bin/arm-none-eabi-gcc"))
        .archiver(format!("{devkitarm}/bin/arm-none-eabi-ar"))
        .file("c/https_client.c")
        .file("c/dns.c")
        .file("c/entropy.c")
        .include("c")
        .include(prefix.join("include"))
        .include(libctru.join("include"))
        .flag("-march=armv6k")
        .flag("-mtune=mpcore")
        .flag("-mfloat-abi=hard")
        .flag("-mfpu=vfp")
        .flag("-mtp=soft")
        .flag("-D__3DS__")
        .flag("-O2")
        .compile("anki_3ds_https");

    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        libctru.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=curl");
    println!("cargo:rustc-link-lib=static=mbedtls");
    println!("cargo:rustc-link-lib=static=mbedx509");
    println!("cargo:rustc-link-lib=static=mbedcrypto");
    println!("cargo:rustc-link-lib=static=ctru");
}
