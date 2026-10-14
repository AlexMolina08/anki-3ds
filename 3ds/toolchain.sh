#!/usr/bin/env bash
export DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
export DEVKITARM="${DEVKITARM:-$DEVKITPRO/devkitARM}"
export PATH="$HOME/.cargo/bin:$DEVKITARM/bin:$DEVKITPRO/tools/bin:$PATH"
export CC_armv6k_nintendo_3ds="$DEVKITARM/bin/arm-none-eabi-gcc"
export CXX_armv6k_nintendo_3ds="$DEVKITARM/bin/arm-none-eabi-g++"
export AR_armv6k_nintendo_3ds="$DEVKITARM/bin/arm-none-eabi-ar"
export CARGO_TARGET_ARMV6K_NINTENDO_3DS_LINKER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/linker.sh"
# Rust's target specifies -mtune but not -march at link time. GCC 16's default
# multilib is ARMv7; explicitly select ARMv6K libraries for the original 3DS.
export CARGO_TARGET_ARMV6K_NINTENDO_3DS_RUSTFLAGS="-C link-arg=-march=armv6k"
# libctru supplies Horizon's socket/ioctl headers in addition to newlib.
export CFLAGS_armv6k_nintendo_3ds="-I$DEVKITPRO/libctru/include -mfloat-abi=hard -mtune=mpcore -mtp=soft -march=armv6k"
export CXXFLAGS_armv6k_nintendo_3ds="-mfloat-abi=hard -mtune=mpcore -mtp=soft -march=armv6k"
export CARGO_TERM_COLOR=never
export ANKI_3DS_RUST_TOOLCHAIN=nightly-2026-10-12
