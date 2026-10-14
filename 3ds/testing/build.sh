#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
source 3ds/toolchain.sh
python3 3ds/prepare-coarsetime.py
python3 3ds/testing/prepare.py "$@"
export CARGO_TARGET_DIR="$PWD/out/3ds/collection-target"
export RUSTFLAGS="$CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS"
export CC_armv6k_nintendo_3ds="$PWD/3ds/cc.sh"
export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
export PROTOC_BINARY="$(command -v protoc)"
cargo +"$ANKI_3DS_RUST_TOOLCHAIN" test --manifest-path out/3ds/upstream-tests-source/Cargo.toml \
    -p anki --lib --no-default-features --release --no-run --target armv6k-nintendo-3ds \
    -Z build-std=std,panic_abort -Z panic-abort-tests 2>&1 | tee out/3ds/logs/upstream-3ds-build.log
elf=$(python3 - <<'PY'
from pathlib import Path
import re
log = Path('out/3ds/logs/upstream-3ds-build.log').read_text()
paths = re.findall(r'Executable unittests .* \(([^)]+)\)', log)
assert len(paths) == 1, 'Expected exactly one upstream test executable'
assert Path(paths[0]).is_file()
print(paths[0])
PY
)
attributes=$("$DEVKITARM/bin/arm-none-eabi-readelf" -A "$elf")
[[ "$attributes" == *"Tag_CPU_arch: v6K"* && "$attributes" == *"Tag_FP_arch: VFPv2"* && "$attributes" != *"Tag_CPU_arch: v7"* ]]
3dsxtool "$elf" out/3ds/testing/upstream-tests.3dsx.tmp
mv out/3ds/testing/upstream-tests.3dsx.tmp out/3ds/testing/upstream-tests.3dsx
shasum -a 256 "$elf" out/3ds/testing/upstream-tests.3dsx > out/3ds/testing/binary-sha256.txt
"$DEVKITARM/bin/arm-none-eabi-size" "$elf" > out/3ds/testing/binary-size.txt
printf '%s\n' "$attributes" > out/3ds/testing/elf-attributes.txt
