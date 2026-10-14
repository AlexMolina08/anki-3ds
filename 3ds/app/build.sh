#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
source 3ds/toolchain.sh
python3 3ds/prepare-coarsetime.py
export CARGO_TARGET_DIR="$PWD/out/3ds/collection-target"
export RUSTFLAGS="$CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS"
export CC_armv6k_nintendo_3ds="$PWD/3ds/cc.sh"
export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
export PROTOC_BINARY="$(command -v protoc)"
mkdir -p out/3ds/logs out/3ds/app
cargo +"$ANKI_3DS_RUST_TOOLCHAIN" 3ds build --manifest-path 3ds/app/Cargo.toml --release "$@" \
    2>&1 | tee out/3ds/logs/app-build.log
elf="$CARGO_TARGET_DIR/armv6k-nintendo-3ds/release/anki-3ds.elf"
attributes=$("$DEVKITARM/bin/arm-none-eabi-readelf" -A "$elf")
[[ "$attributes" == *"Tag_CPU_arch: v6K"* && "$attributes" == *"Tag_FP_arch: VFPv2"* && "$attributes" != *"Tag_CPU_arch: v7"* ]]
3dsxtool "$elf" out/3ds/app/anki-3ds.3dsx
shasum -a 256 out/3ds/app/anki-3ds.3dsx > out/3ds/app/sha256.txt
