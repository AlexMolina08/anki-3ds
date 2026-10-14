set windows-shell := ["pwsh", "-NoLogo", "-NoProfileLoadTime", "-Command"]

mod release

# Show available commands
default:
    @just --list

# Build the project
build:
    {{ ninja }} pylib qt

# Build and run Anki in development mode
run *args:
    {{ run_script }} {{ args }}

# Build and run Anki in optimized (release) mode
run-optimized *args:
    {{ if os() == "windows" { "$env:RELEASE='1'; .\\run.bat" } else { "RELEASE=1 ./run" } }} {{ args }}

# Watch web sources and rebuild/reload Anki's web stack on change (macOS/Linux)
web-watch:
    ./tools/web-watch

# Rebuild and reload Anki's web stack without restarting (macOS/Linux)
rebuild-web:
    ./tools/rebuild-web

# Build wheels (needed for some platforms)
wheels:
    {{ ninja }} wheels

# Build and run all checks (lint + test) - lets ninja handle dependencies
check:
    {{ ninja }} pylib qt check

# Run all tests (Rust, Python, TypeScript). Pass --coverage to enforce coverage, and --html to include HTML reports.
[arg("coverage", long="coverage", value="--coverage")]
[arg("html", long="html", value="--html")]
test coverage='' html='':
    just {{ if coverage == "--coverage" { "coverage " + html } else { "_test" } }}

# Run coverage for all test stacks. Pass --html to also generate HTML reports.
[arg("html", long="html", value="--html")]
coverage html='':
    just _coverage-rust {{ html }}
    just _coverage-py {{ html }}
    just _coverage-ts {{ html }}

# Run Rust tests. Pass --coverage to enforce Rust coverage, and --html to include an HTML report.
[arg("coverage", long="coverage", value="--coverage")]
[arg("html", long="html", value="--html")]
test-rust coverage='' html='':
    just {{ if coverage == "--coverage" { "_coverage-rust " + html } else { "_test-rust" } }}

# Run Python tests (pylib + qt). Pass --coverage to enforce coverage, and --html to include HTML reports.
[arg("coverage", long="coverage", value="--coverage")]
[arg("html", long="html", value="--html")]
test-py coverage='' html='':
    just {{ if coverage == "--coverage" { "_coverage-py " + html } else { "_test-py" } }}

# Run TypeScript/Svelte Vitest tests. Pass --coverage to enforce coverage, and --html to include an HTML report.
[arg("coverage", long="coverage", value="--coverage")]
[arg("html", long="html", value="--html")]
test-ts coverage='' html='':
    just {{ if coverage == "--coverage" { "_coverage-ts " + html } else { "_test-ts" } }}

# Run Playwright end-to-end tests. Pass --ui to open the interactive UI.
[arg("ui", long="ui", value="--ui")]
test-e2e ui='': _install-playwright-browsers
    {{ ninja }} pyenv ts:generated pylib qt
    {{ playwright_env }} {{ yarn }} test:e2e {{ ui }}

[private]
_test:
    {{ ninja }} check:rust_test check:pytest check:vitest

[private]
_test-rust:
    {{ ninja }} check:rust_test

[private]
_test-py:
    {{ ninja }} check:pytest

[private]
_test-ts:
    {{ ninja }} check:vitest

[private]
_coverage-rust html='':
    {{ if os_family() == "windows" { "tools\\coverage\\coverage-rust" } else { "tools/coverage/coverage-rust" } }} {{ html }}

[private]
_coverage-py html='':
    {{ ninja }} pylib qt
    just _coverage-py-pylib {{ html }}
    just _coverage-py-qt {{ html }}

[private]
_coverage-py-pylib html='':
    {{ if os_family() == "windows" { "tools\\coverage\\coverage-py" } else { "tools/coverage/coverage-py" } }} pylib {{ html }}

[private]
_coverage-py-qt html='':
    {{ if os_family() == "windows" { "tools\\coverage\\coverage-py" } else { "tools/coverage/coverage-py" } }} qt {{ html }}

[private]
_coverage-ts html='':
    {{ ninja }} node_modules ts:generated
    {{ if os_family() == "windows" { "tools\\coverage\\coverage-ts" } else { "tools/coverage/coverage-ts" } }} {{ html }}

[private]
_install-playwright-browsers:
    {{ ninja }} node_modules
    {{ playwright_env }} {{ yarn }} playwright install chromium

# Check formatting (fast, no build needed)
fmt:
    {{ ninja }} check:format

# Fix formatting
fix-fmt:
    {{ ninja }} format

# Run linting and type checking (requires build outputs)
lint:
    {{ ninja }} \
        check:clippy \
        check:mypy \
        check:ruff \
        check:eslint \
        check:svelte \
        check:typescript

# Fix auto-fixable lint issues (ruff + eslint)
fix-lint:
    {{ ninja }} fix:ruff fix:eslint

# Run minilints (copyright, contributors, licenses)
minilints:
    {{ ninja }} check:minilints

# Fix minilints (update licenses.json)
fix-minilints:
    {{ ninja }} fix:minilints

# Sync translation files
ftl-sync:
    {{ ninja }} ftl-sync

# Deprecate translation strings
ftl-deprecate:
    {{ ninja }} ftl-deprecate

# Build documentation site
docs:
    {{ uv }} run --group docs sphinx-build -b html docs out/docs/html
    @echo "Docs built at out/docs/html/index.html"

# Build and serve documentation site
docs-serve:
    {{ uv }} run --group docs sphinx-autobuild docs out/docs/html --host 127.0.0.1 --port 8000

# Build Rust API docs
docs-rust:
    cargo doc --open

# Dispatch CI workflow on a given branch or tag
ci branch:
    gh workflow run ci.yml --ref {{ branch }}

# Run Complexipy in regression-only mode
complexipy-diff:
    {{ ninja }} complexipy-diff

# Remove build outputs from out/ (pass keep-env to keep node_modules/pyenv); macOS/Linux
clean *args:
    ./tools/clean {{ args }}

# Helpers to get the right commands for the platform

ninja := if os() == "windows" { "tools\\ninja" } else { "./ninja" }
run_script := if os() == "windows" { ".\\run.bat" } else { "./run" }
playwright_env := if os() == "windows" { "set PLAYWRIGHT_BROWSERS_PATH=out\\playwright-browsers&&" } else { "PLAYWRIGHT_BROWSERS_PATH=out/playwright-browsers" }
yarn := if os() == "windows" { "out\\extracted\\node\\yarn.cmd" } else { "out/extracted/node/bin/yarn" }
uv := env("UV_BINARY", if os() == "windows" { "out\\extracted\\uv\\uv" } else { "out/extracted/uv/uv" })
export UV_PROJECT_ENVIRONMENT := if os() == "windows" { "out\\pyenv" } else { "out/pyenv" }

# Inspect the isolated Old 3DS toolchain (macOS/Linux)
old3ds-doctor:
    bash 3ds/experiment.sh doctor

# Attempt the original, complete Rust backend for Old 3DS
old3ds-backend-build:
    bash 3ds/experiment.sh backend-build

# Check the original backend's Rust compatibility with Horizon
old3ds-backend-check:
    bash 3ds/experiment.sh backend-check

# Build a standalone proof using the unchanged original Anki text module
old3ds-text-build:
    bash 3ds/experiment.sh text-build

# Isolate original backend dependency compatibility; preserve failure status
old3ds-compatibility:
    bash 3ds/experiment.sh compatibility

# Run the text proof in Azahar with isolated data and Old 3DS settings
old3ds-emulator-run:
    bash 3ds/experiment.sh emulator-run

# Build the SD-backed SQLite probe with an isolated Horizon VFS
old3ds-sqlite-build:
    bash 3ds/experiment.sh sqlite-build

# Run the SQLite probe in the same isolated Old 3DS emulator
old3ds-sqlite-emulator-run:
    bash 3ds/experiment.sh sqlite-emulator-run

# Verify persistence and crash recovery across distinct emulator processes
old3ds-sqlite-validate:
    python3 3ds/validate-sqlite.py

# Generate a disposable fixture through the installed Anki application on macOS
old3ds-fixture-create:
    bash 3ds/experiment.sh fixture-create

# Reopen a copy of the native collection with the installed upstream Anki backend
old3ds-fixture-verify:
    bash 3ds/experiment.sh fixture-verify

# Test native Zstd, portable 64-bit atomics, and the Horizon progress clock
old3ds-runtime-build:
    bash 3ds/experiment.sh runtime-build

# Execute the runtime dependency tests in isolated Old 3DS mode
old3ds-runtime-emulator-run:
    bash 3ds/experiment.sh runtime-emulator-run

# Check original backend sources with verified clock and Zstd adaptations
old3ds-backend-portability-check:
    bash 3ds/experiment.sh backend-portability-check

# Remove only signals in a disposable source snapshot to isolate networking
old3ds-backend-no-signal-check:
    bash 3ds/experiment.sh backend-no-signal-check

# Attempt genuine upstream CollectionBuilder and search with the native VFS
old3ds-collection-build:
    bash 3ds/experiment.sh collection-build

# Execute the upstream Collection probe in isolated Old 3DS mode
old3ds-collection-emulator-run:
    bash 3ds/experiment.sh collection-emulator-run

# Run the Collection probe and always close the emulator afterwards
old3ds-collection-review: old3ds-fixture-reset
    python3 3ds/run-emulator-probe.py old3ds-collection-emulator-run collection-result.txt

# Same, with the FSRS scheduler enabled on the console
old3ds-collection-review-fsrs: old3ds-fixture-reset
    python3 3ds/run-emulator-probe.py old3ds-collection-emulator-run collection-result.txt --fsrs

# Reopen the reviewed collection with the installed desktop Anki backend
old3ds-review-verify:
    bash 3ds/experiment.sh review-verify

# Restore the emulator's SD collection to the pristine single-card fixture
old3ds-fixture-reset:
    bash 3ds/experiment.sh fixture-reset

# Reset the SD collection, run a review, and verify it with desktop Anki
old3ds-review-cycle: old3ds-collection-review old3ds-review-verify

# Same cycle, but with the FSRS scheduler enabled on the console
old3ds-review-cycle-fsrs: old3ds-collection-review-fsrs old3ds-review-verify

# Have desktop Anki schedule the next review from the console's FSRS state.
# Depends on the FSRS run so it can never read a legacy collection left by an
# earlier recipe.
old3ds-fsrs-continuation: old3ds-collection-review-fsrs
    bash 3ds/experiment.sh fsrs-continuation

# Build original backend tests with an isolated Horizon execution harness
old3ds-upstream-tests-build:
    bash 3ds/testing/build.sh --native-tempfile

# Execute original macOS backend tests with default and offline features
old3ds-upstream-tests-host:
    bash 3ds/testing/host.sh

# Launch the original test executable in isolated Old 3DS mode
old3ds-upstream-tests-emulator-run:
    bash 3ds/experiment.sh upstream-tests-emulator-run

# Execute collected upstream assertions; preserve every failure and resume remaining tests
old3ds-upstream-tests-run:
    python3 3ds/testing/emulator.py

# Execute the same controlled port-specific scenarios on the desktop snapshot
old3ds-differential-host:
    PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/out/3ds/collection-target" ANKI_TEST_MODE=1 ANKI_3DS_DIFFERENTIAL_ROOT="$PWD/out/3ds/testing" ANKI_3DS_DIFFERENTIAL_OUTPUT="$PWD/out/3ds/testing/differential-host.json" cargo +nightly-2026-10-12 test --manifest-path out/3ds/upstream-tests-source/Cargo.toml -p anki --lib --no-default-features --release

# Execute controlled disk-backed scenarios and require exact desktop/3DS equality
old3ds-differential-run: old3ds-differential-host
    python3 3ds/testing/emulator.py --scenario differential --label differential
    python3 3ds/testing/compare.py

# Run original translation, I/O and process utility tests on Old 3DS
old3ds-upstream-support-tests:
    bash 3ds/testing/support.sh

# Rerun desktop, original 3DS backend/utility tests and exact disk comparisons
old3ds-upstream-tests-all:
    bash 3ds/testing/all.sh
old3ds-app-build:
    bash 3ds/app/build.sh

old3ds-app-emulator-run:
    bash 3ds/experiment.sh app-emulator-run

old3ds-app-fixture:
    PYTHONPATH="$PWD/out/pylib:$PWD/pylib" out/pyenv/bin/python 3ds/app/fixture.py

old3ds-app-verify:
    PYTHONPATH="$PWD/out/pylib:$PWD/pylib" out/pyenv/bin/python 3ds/app/verify.py

old3ds-app-assets-tests:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p out/3ds/startup-tests
    trial=$(mktemp -d "$PWD/out/3ds/startup-tests/assets-XXXXXX")
    CARGO_MANIFEST_DIR="$PWD/3ds/app" PATH="$HOME/.cargo/bin:$PATH" rustc +nightly-2026-10-12 --edition 2021 --test 3ds/app/src/assets.rs -o "$trial/tests"
    ANKI_3DS_ASSET_TEST_ROOT="$trial" "$trial/tests"
    PATH="$HOME/.cargo/bin:$PATH" rustc +nightly-2026-10-12 --edition 2021 --test 3ds/app/src/time_correction.rs -o "$trial/clock-tests"
    "$trial/clock-tests"

old3ds-app-package:
    python3 3ds/app/package.py

# Isolated native UI actions and persistence checks (never personal collections).
old3ds-app-ui-check:
    python3 3ds/app/ui_check.py

old3ds-app-offline-clock-check:
    python3 3ds/app/ui_check.py --offline-clock

old3ds-app-ui-verify:
    PYTHONPATH="$PWD/out/pylib:$PWD/pylib" out/pyenv/bin/python 3ds/app/verify_ui.py

# Isolated secure HTTPS transport experiment; no account or collection access.
old3ds-https-libs:
    bash 3ds/https-probe/build-libs.sh

old3ds-https-build:
    bash 3ds/https-probe/build.sh

old3ds-https-emulator:
    python3 3ds/https-probe/emulator.py

old3ds-https-package:
    python3 3ds/https-probe/package.py

# Upstream sync transport integration probe (Rust + native C HTTPS)
old3ds-sync-probe-build:
    bash 3ds/sync-probe/build.sh

old3ds-sync-probe-emulator:
    python3 3ds/sync-probe/emulator.py

old3ds-sync-probe-package:
    python3 3ds/sync-probe/package.py

# Original incremental engine on disposable collections; no production sync.
old3ds-normal-sync-build:
    bash 3ds/normal-sync-probe/build.sh

old3ds-normal-sync-host:
    PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/out/3ds/normal-sync-host-target" PROTOC_BINARY="$(command -v protoc)" cargo +nightly-2026-10-12 run --manifest-path 3ds/normal-sync-probe/Cargo.toml --release -- prepare
    PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/out/3ds/normal-sync-host-target" PROTOC_BINARY="$(command -v protoc)" cargo +nightly-2026-10-12 run --manifest-path 3ds/normal-sync-probe/Cargo.toml --release

old3ds-normal-sync-emulator:
    python3 3ds/normal-sync-probe/emulator.py

old3ds-normal-sync-verify:
    python3 3ds/normal-sync-probe/verify.py

old3ds-normal-sync-package:
    python3 3ds/normal-sync-probe/package.py

old3ds-sync-regression:
    PATH="$HOME/.cargo/bin:$PATH" PROTOC_BINARY="$(command -v protoc)" cargo +nightly-2026-10-12 test -p anki --lib sync::

old3ds-sync-transport-tests:
    PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/out/3ds/normal-sync-host-target" PROTOC_BINARY="$(command -v protoc)" cargo +nightly-2026-10-12 test --manifest-path 3ds/sync-transport/Cargo.toml --lib

old3ds-app-sync-build:
    bash 3ds/app/build.sh --features emulator

old3ds-app-sync-tests label='release':
    PATH="$HOME/.cargo/bin:$PATH" PROTOC_BINARY="$(command -v protoc)" cargo +nightly-2026-10-12 build -p anki-sync-server
    python3 3ds/testing/sync-suite.py {{label}}

# Login-only checks never upload/download a collection. Live credentials are JSON on stdin.
old3ds-app-login-check label:
    python3 3ds/testing/login_check.py --label {{label}}

old3ds-app-login-live label:
    python3 3ds/testing/login_check.py --live --label {{label}}

old3ds-dns-retry-tests:
    python3 3ds/testing/dns_retry.py

old3ds-app-sync-case *args:
    python3 3ds/testing/app_sync.py {{args}}

old3ds-locales-check:
    python3 3ds/testing/locales.py
