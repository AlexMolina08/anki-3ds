#!/usr/bin/env bash
# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
set -euo pipefail
cd "$(dirname "$0")/.."
source 3ds/toolchain.sh
for tool in python3 cmake make patch protoc curl git rustup; do
    command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool" >&2; exit 1; }
done
python3 -c 'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ required"'
test -f "$DEVKITPRO/libctru/lib/libctru.a" || { echo 'Install devkitPro 3ds-dev first.' >&2; exit 1; }
rustup toolchain install "$ANKI_3DS_RUST_TOOLCHAIN" --profile minimal --component rust-src
cargo +"$ANKI_3DS_RUST_TOOLCHAIN" install cargo-3ds --version 0.1.5 --locked
git submodule update --init --depth 1 ftl/core-repo ftl/qt-repo
python3 3ds/prepare-coarsetime.py
python3 3ds/prepare-tools.py
bash 3ds/https-probe/build-libs.sh
echo 'Setup complete. Build with: just old3ds-app-build'
