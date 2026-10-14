#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
source 3ds/toolchain.sh
python3 3ds/testing/prepare.py --native-tempfile --support
export CARGO_TARGET_DIR="$PWD/out/3ds/collection-target"
export RUSTFLAGS="$CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS"
export CC_armv6k_nintendo_3ds="$PWD/3ds/cc.sh"
export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
export PROTOC_BINARY="$(command -v protoc)"
cp out/3ds/testing/upstream-tests.3dsx out/3ds/testing/backend-tests.3dsx
trap 'cp out/3ds/testing/backend-tests.3dsx out/3ds/testing/upstream-tests.3dsx' EXIT
for package in anki_i18n anki_io anki_process; do
    rm -f "out/3ds/testing/$package-results.json"
done
failed=0
for package in anki_i18n anki_io anki_process; do
    cargo +"$ANKI_3DS_RUST_TOOLCHAIN" test --manifest-path out/3ds/upstream-tests-source/Cargo.toml \
        -p "$package" --lib --release --no-run --target armv6k-nintendo-3ds \
        -Z build-std=std,panic_abort -Z panic-abort-tests 2>&1 | tee "out/3ds/logs/$package-build.log"
    elf=$(python3 - "$package" <<'PY'
import re,sys
s=open('out/3ds/logs/'+sys.argv[1]+'-build.log').read()
paths=re.findall(r'Executable unittests .* \(([^)]+)\)',s)
assert len(paths)==1
print(paths[0])
PY
)
    "$DEVKITARM/bin/arm-none-eabi-readelf" -A "$elf" > "out/3ds/testing/$package-attributes.txt"
    attributes=$(cat "out/3ds/testing/$package-attributes.txt")
    [[ "$attributes" == *"Tag_CPU_arch: v6K"* && "$attributes" == *"Tag_FP_arch: VFPv2"* && "$attributes" != *"Tag_CPU_arch: v7"* ]]
    3dsxtool "$elf" "out/3ds/testing/$package.3dsx"
    cp "out/3ds/testing/$package.3dsx" out/3ds/testing/upstream-tests.3dsx
    if ! python3 3ds/testing/emulator.py --label "$package"; then failed=1; fi
done
cp out/3ds/testing/backend-tests.3dsx out/3ds/testing/upstream-tests.3dsx
exit "$failed"
