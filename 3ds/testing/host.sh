#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
export ANKI_TEST_MODE=1
export CARGO_TERM_COLOR=never
mkdir -p out/3ds/logs out/3ds/testing
just test-rust 2>&1 | tee out/3ds/logs/upstream-official-host.log
for mode in default offline; do
    flags=()
    if [[ "$mode" == offline ]]; then flags+=(--no-default-features); fi
    cargo test --locked -p anki --lib "${flags[@]}" 2>&1 | tee "out/3ds/logs/upstream-host-$mode.log"
    cargo test --locked -p anki --lib "${flags[@]}" -- --list > "out/3ds/testing/host-$mode-list.txt" 2>&1
done
PYTHONPATH="$PWD/out/pylib:$PWD/pylib" out/pyenv/bin/python -m pytest pylib/tests \
    --junitxml=out/3ds/testing/pylib.xml 2>&1 | tee out/3ds/logs/upstream-host-pylib.log
python3 3ds/testing/inventory.py
