#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
    if [[ "$argument" == zstd/lib/*.c ]]; then
        exec "$DEVKITARM/bin/arm-none-eabi-gcc" "$@" -std=gnu90
    fi
done
exec "$DEVKITARM/bin/arm-none-eabi-gcc" "$@"
