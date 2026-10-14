#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source 3ds/toolchain.sh

verify_old3ds_elf() {
    local attributes
    attributes="$("$DEVKITARM/bin/arm-none-eabi-readelf" -A "$1")"
    if [[ "$attributes" != *"Tag_CPU_arch: v6K"* || "$attributes" != *"Tag_FP_arch: VFPv2"* || "$attributes" == *"Tag_CPU_arch: v7"* ]]; then
        echo "ELF architecture is not ARMv6K/VFPv2; refusing Old 3DS proof." >&2
        return 1
    fi
    echo "ELF architecture verified: ARMv6K, VFPv2."
}

case "${1:-}" in
    doctor)
        rustc +"$ANKI_3DS_RUST_TOOLCHAIN" --version
        cargo +"$ANKI_3DS_RUST_TOOLCHAIN" 3ds --version
        "$CC_armv6k_nintendo_3ds" --version | head -n 1
        test -f "$DEVKITPRO/libctru/lib/libctru.a"
        command -v 3dsxtool
        command -v protoc
        rustc +"$ANKI_3DS_RUST_TOOLCHAIN" --print cfg --target armv6k-nintendo-3ds
        ;;
    backend-build|backend-check)
        operation="${1#backend-}"
        export PROTOC_BINARY="${PROTOC_BINARY:-$(command -v protoc)}"
        mkdir -p out/pylib/anki out/ts/lib/generated out/3ds/logs
        cargo +"$ANKI_3DS_RUST_TOOLCHAIN" "$operation" --locked -p anki \
            --target armv6k-nintendo-3ds -Z build-std=std,panic_abort \
            2>&1 | tee "out/3ds/logs/${1}.log"
        ;;
    fixture-reset)
        mkdir -p out/3ds/logs
        python3 3ds/fixtures/reset-sd.py
        ;;
    fixture-create|fixture-verify|review-verify|fsrs-continuation)
        mkdir -p out/3ds/fixtures out/3ds/logs
        cc 3ds/fixtures/python-launcher.c -o out/3ds/fixtures/python-launcher
        script=3ds/fixtures/create.py
        if [[ "$1" == fixture-verify ]]; then script=3ds/fixtures/verify-roundtrip.py; fi
        if [[ "$1" == review-verify ]]; then script=3ds/fixtures/verify-review.py; fi
        if [[ "$1" == fsrs-continuation ]]; then script=3ds/fixtures/verify-fsrs-continuation.py; fi
        pylib_path=/Applications/Anki.app/Contents/Resources/app_packages
        if [[ "$1" == fsrs-continuation ]]; then
            pylib_path="$PWD/out/pylib:$PWD/pylib:$pylib_path"
        fi
        PYTHONHOME=/Applications/Anki.app/Contents/Frameworks/Python.framework/Versions/3.13 \
        PYTHONPATH="$pylib_path" \
            out/3ds/fixtures/python-launcher "$script"
        ;;
    backend-portability-check|backend-no-signal-check)
        python3 3ds/prepare-coarsetime.py
        mkdir -p out/3ds/logs
        export CC_armv6k_nintendo_3ds="$PWD/3ds/cc.sh"
        export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
        export PROTOC_BINARY="${PROTOC_BINARY:-$(command -v protoc)}"
        export CARGO_TARGET_DIR="$PWD/target"
        signal_mode=keep
        if [[ "$1" == backend-no-signal-check ]]; then signal_mode=omit; fi
        python3 3ds/check-backend-portability.py --signal-mode="$signal_mode" \
            2>&1 | tee "out/3ds/logs/$1.log"
        ;;
    collection-build)
        python3 3ds/prepare-coarsetime.py
        mkdir -p out/3ds/logs
        export CARGO_TARGET_DIR="$PWD/out/3ds/collection-target"
        export RUSTFLAGS="${RUSTFLAGS:-} $CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS"
        export CC_armv6k_nintendo_3ds="$PWD/3ds/cc.sh"
        export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
        export PROTOC_BINARY="${PROTOC_BINARY:-$(command -v protoc)}"
        (cd 3ds/collection-probe && cargo +"$ANKI_3DS_RUST_TOOLCHAIN" 3ds build --release) \
            2>&1 | tee out/3ds/logs/collection-build.log
        verify_old3ds_elf "$CARGO_TARGET_DIR/armv6k-nintendo-3ds/release/anki-3ds-collection-probe.elf"
        ;;
    runtime-build)
        python3 3ds/prepare-coarsetime.py
        mkdir -p out/3ds/logs
        export CARGO_TARGET_DIR="$PWD/out/3ds/runtime-target"
        export RUSTFLAGS="${RUSTFLAGS:-} $CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS"
        export CC_armv6k_nintendo_3ds="$PWD/3ds/cc.sh"
        export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
        (cd 3ds/runtime-probe && cargo +"$ANKI_3DS_RUST_TOOLCHAIN" 3ds build --locked --release) \
            2>&1 | tee out/3ds/logs/runtime-build.log
        verify_old3ds_elf "$CARGO_TARGET_DIR/armv6k-nintendo-3ds/release/anki-3ds-runtime-probe.elf"
        ;;
    sqlite-build)
        mkdir -p out/3ds/logs
        export CARGO_TARGET_DIR="$PWD/out/3ds/sqlite-target"
        export RUSTFLAGS="${RUSTFLAGS:-} $CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS"
        export LIBSQLITE3_FLAGS="-DSQLITE_OS_OTHER=1 -DSQLITE_TEMP_STORE=3"
        (cd 3ds/sqlite-probe && cargo +"$ANKI_3DS_RUST_TOOLCHAIN" 3ds build --locked --release) \
            2>&1 | tee out/3ds/logs/sqlite-build.log
        verify_old3ds_elf "$CARGO_TARGET_DIR/armv6k-nintendo-3ds/release/anki-3ds-sqlite-probe.elf"
        ;;
    text-build)
        mkdir -p out/3ds/logs
        export CARGO_TARGET_DIR="$PWD/out/3ds/text-target"
        export RUSTFLAGS="${RUSTFLAGS:-} $CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS"
        (cd 3ds/text-probe && cargo +"$ANKI_3DS_RUST_TOOLCHAIN" 3ds build \
            --locked --release) 2>&1 | tee out/3ds/logs/text-build.log
        verify_old3ds_elf "$CARGO_TARGET_DIR/armv6k-nintendo-3ds/release/anki-3ds-text-probe.elf"
        ;;
    compatibility)
        mkdir -p out/3ds/logs
        export CARGO_TARGET_DIR="$PWD/target"
        failed=0
        for feature in runtime signals network clock; do
            if cargo +"$ANKI_3DS_RUST_TOOLCHAIN" check \
                --manifest-path 3ds/compat-probe/Cargo.toml \
                --locked \
                --target armv6k-nintendo-3ds -Z build-std=std,panic_abort \
                --features "$feature" > "out/3ds/logs/compat-$feature.log" 2>&1; then
                echo "$feature: check passed (not execution)"
            else
                echo "$feature: check failed; see out/3ds/logs/compat-$feature.log"
                failed=1
            fi
        done
        exit "$failed"
        ;;
    emulator-run|sqlite-emulator-run|runtime-emulator-run|collection-emulator-run|upstream-tests-emulator-run|app-emulator-run)
        emulator="${ANKI_3DS_AZAHAR:-$HOME/Applications/azahar-macos-arm64-2126.2/Azahar.app/Contents/MacOS/azahar}"
        binary="$PWD/out/3ds/text-target/armv6k-nintendo-3ds/release/anki-3ds-text-probe.3dsx"
        if [[ "$1" == sqlite-emulator-run ]]; then
            binary="$PWD/out/3ds/sqlite-target/armv6k-nintendo-3ds/release/anki-3ds-sqlite-probe.3dsx"
        fi
        if [[ "$1" == runtime-emulator-run ]]; then
            binary="$PWD/out/3ds/runtime-target/armv6k-nintendo-3ds/release/anki-3ds-runtime-probe.3dsx"
        fi
        if [[ "$1" == collection-emulator-run ]]; then
            binary="$PWD/out/3ds/collection-target/armv6k-nintendo-3ds/release/anki-3ds-collection-probe.3dsx"
        fi
        if [[ "$1" == upstream-tests-emulator-run ]]; then
            binary="$PWD/out/3ds/testing/upstream-tests.3dsx"
        fi
        if [[ "$1" == app-emulator-run ]]; then
            binary="$PWD/out/3ds/app/anki-3ds.3dsx"
        fi
        test -x "$emulator" || { echo "Azahar executable missing: $emulator" >&2; exit 1; }
        test -f "$binary" || { echo "Run just old3ds-text-build first." >&2; exit 1; }
        export XDG_DATA_HOME="$PWD/out/3ds/emulator/data"
        export XDG_CONFIG_HOME="$PWD/out/3ds/emulator/config"
        export XDG_CACHE_HOME="$PWD/out/3ds/emulator/cache"
        mkdir -p "$XDG_DATA_HOME/azahar-emu" "$XDG_CONFIG_HOME/azahar-emu" \
            "$XDG_CACHE_HOME/azahar-emu" out/3ds/logs
        cat > "$XDG_CONFIG_HOME/azahar-emu/qt-config.ini" <<'EOF'
[System]
is_new_3ds=false
is_new_3ds\default=false
[Miscellaneous]
log_filter=*:Info Service.FS:Debug
log_filter\default=false
[Core]
cpu_clock_percentage=100
cpu_clock_percentage\default=false
EOF
        if [[ "$OSTYPE" == darwin* && "$emulator" == *.app/Contents/MacOS/azahar ]]; then
            open -n -W -a "${emulator%/Contents/MacOS/azahar}" \
                --env "XDG_DATA_HOME=$XDG_DATA_HOME" \
                --env "XDG_CONFIG_HOME=$XDG_CONFIG_HOME" \
                --env "XDG_CACHE_HOME=$XDG_CACHE_HOME" --args "$binary"
        else
            "$emulator" "$binary" 2>&1 | tee out/3ds/logs/azahar-console.log
        fi
        ;;
    *)
        echo "Usage: $0 {doctor|collection-build|runtime-build|runtime-emulator-run|backend-portability-check|backend-no-signal-check|fixture-create|fixture-verify|backend-build|backend-check|text-build|sqlite-build|compatibility|emulator-run|sqlite-emulator-run}" >&2
        exit 2
        ;;
esac
