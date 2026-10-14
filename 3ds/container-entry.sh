#!/usr/bin/env bash
# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
set -euo pipefail
cd "$(dirname "$0")/.."
apt-get update
apt-get install -y --no-install-recommends python3 clang libclang-dev protobuf-compiler libssl-dev pkg-config
git config --global --add safe.directory "$PWD"
if ! command -v rustup >/dev/null; then
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/anki-rustup.sh
    sh /tmp/anki-rustup.sh -y --profile minimal --default-toolchain none
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install nightly-2026-10-12 --profile minimal --component rust-src
cargo +nightly-2026-10-12 install just --version 1.58.0 --locked
just old3ds-setup
just old3ds-review-text-tests
ANKI_3DS_RELEASE=1 just old3ds-release
