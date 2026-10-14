#!/usr/bin/env bash
set -euo pipefail
adapter_dir="$(cd "$(dirname "$0")" && pwd)"
args=()
for arg in "$@"; do
    case "$arg" in
        -specs=3dsx.specs) args+=("-specs=$adapter_dir/old3ds.specs") ;;
        *) args+=("$arg") ;;
    esac
done
exec "${DEVKITARM:-/opt/devkitpro/devkitARM}/bin/arm-none-eabi-gcc" \
    -march=armv6k "${args[@]}" -lcitro2d -lcitro3d
