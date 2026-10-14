#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
source 3ds/toolchain.sh
python3 3ds/prepare-coarsetime.py

prefix="$PWD/out/3ds/https-prefix"
export CARGO_TARGET_DIR="$PWD/out/3ds/sync-probe-target"
export RUSTFLAGS="$CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS -L $prefix/lib -l curl -l mbedtls -l mbedx509 -l mbedcrypto"
export CC_armv6k_nintendo_3ds="$PWD/3ds/cc.sh"
export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
export PROTOC_BINARY="$(command -v protoc)"

mkdir -p out/3ds/logs out/3ds/sync-probe
cargo +"$ANKI_3DS_RUST_TOOLCHAIN" 3ds build --manifest-path 3ds/sync-probe/Cargo.toml --release \
    2>&1 | tee out/3ds/logs/sync-probe-build.log

elf="$CARGO_TARGET_DIR/armv6k-nintendo-3ds/release/anki-3ds-sync-probe.elf"
attributes=$("$DEVKITARM/bin/arm-none-eabi-readelf" -A "$elf")
[[ "$attributes" == *"Tag_CPU_arch: v6K"* && "$attributes" == *"Tag_FP_arch: VFPv2"* && "$attributes" != *"Tag_CPU_arch: v7"* ]]

smdhtool --create "Anki Sync Probe" "Native AnkiWeb sync integration test" "Anki 3DS contributors" "$DEVKITPRO/libctru/default_icon.png" out/3ds/sync-probe/anki-3ds-sync-probe.smdh
3dsxtool "$elf" out/3ds/sync-probe/anki-3ds-sync-probe.3dsx --smdh=out/3ds/sync-probe/anki-3ds-sync-probe.smdh
shasum -a 256 out/3ds/sync-probe/anki-3ds-sync-probe.3dsx > out/3ds/sync-probe/sha256.txt
echo "Built out/3ds/sync-probe/anki-3ds-sync-probe.3dsx"
