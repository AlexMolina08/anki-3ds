#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
rm -f out/3ds/testing/fixture-mapped-results.json
just old3ds-upstream-tests-host
just old3ds-upstream-tests-build
failed=0
if ! just old3ds-upstream-tests-run; then failed=1; fi
cp out/3ds/testing/emulator-results.json out/3ds/testing/tempfile-adapted-results.json
if ! just old3ds-upstream-support-tests; then failed=1; fi
just old3ds-differential-run
python3 3ds/testing/report.py
exit "$failed"
