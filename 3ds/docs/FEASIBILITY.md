# Port status

Updated 2026-10-14. Minimum target: Old Nintendo 3DS, ARMv6K/VFPv2.

## Current verification

The application executes Anki's original Collection, scheduler/FSRS, NormalSyncer,
and full-sync orchestration. Native HTTPS uses curl 8.22.0, mbedTLS 3.6.7, console
entropy, Horizon SOC sockets, certificate/hostname validation, and TLS 1.2 minimum.

Sixteen application cases passed in Azahar's Old 3DS mode against the original
Anki desktop sync server behind a local TLS test boundary. Seven incremental
fixtures match desktop results and an independent SQL oracle: unchanged, local,
remote, disjoint edits, reviews, deletions, and conflicts. Remaining cases cover
full upload/download, creation followed by download, cancellation and interruption
with recovery, corrupt download rejection, CA/hostname rejection, and UTC offset.
An additional test verifies that two nonempty collections cannot trigger full sync
without an explicit direction. This controlled endpoint is not AnkiWeb.

Native UI verification passed four ratings, note creation/editing, deck limits,
and language changes during review and editing. Fluent direction markers are
filtered only at the Citro2D display boundary, eliminating missing-glyph boxes
without changing upstream interval calculations. Desktop reopening confirmed
SQLite integrity, note identity, FSRS data, unchanged shared presets and unrelated
notes. English/Spanish catalogs contain 94 validated messages.

Desktop checks: 38 original sync tests and eight transport/storage tests passed.
The broader repository check ran 767 original Rust workspace tests and 184 Python
tests successfully. Qt ran 132 passing tests and two failures: macOS installer
fixtures cannot clone the local application template. These are unresolved desktop
packaging failures, not passing checks. README separates earlier native upstream
unit results from these application integration tests.

## First launch and clock handling (0.2.1)

A clean-install failure after entering the old numeric timezone prompt was reported
on hardware and reproduced in Azahar: the application folder did not exist when
saving the setting. The new executable creates its folder and embedded CA file
before opening the collection. No separate certificate transfer or numeric prompt
is required. Release builds also ignore emulator action scripts.

The app opens offline immediately. Clock requests run in a separate worker and
retry after connection changes or failures. A validated HTTPS Date response gives
UTC; the saved correction supports later offline launches. The adapter sets newlib
and Chrono's process timezone together so Anki's original scheduler sees the same
local offset. Sync refreshes the correction before running the original engine.
Before the first successful calibration, timestamps follow the console clock;
existing note/card/revlog timestamps are never rewritten by the adapter.

Offline native tests passed reviews, editing and integrity verification with an
unreachable clock endpoint. Failed explicit clock updates preserve note drafts.
A delayed TLS clock response confirms that the deck screen opens before calibration.
A production build restores a cached correction with no network, retaining the
original scheduler's -120 minute local/creation offsets. Six focused host tests
cover assets, preservation, UTC signs, fractional offsets, drift and invalid dates.
Azahar's production DNS configuration fails; this is tolerated without closing the
app. Direct clock calibration on physical hardware remains to be verified.

## Hardware boundary

A prior physical Old 3DS report records ten disposable engine cases passing.
Its independent database comparison remains incomplete. Earlier physical HTTPS
probes reached official AnkiWeb securely. The later engine probe failed DNS while
an SD override forced 1.1.1.1. Production now uses console DNS and ignores probe
configuration. The final application and authenticated production AnkiWeb sync
remain unverified on hardware. No personal collection is included in testing.

## Memory and storage

Observed allocated-heap peaks in controlled application runs: 6,057,424 bytes during
upload compression, 5,083,792 during download decoding, and 4,632,216 during
incremental sync. These samples are not total RAM or guaranteed worst cases.
Reserved main/worker stacks are 2 MiB/1 MiB; SOC reserves a separate aligned 1 MiB.
Incremental bodies are capped at 8 MiB compressed and 16 MiB decoded; zstd's
window is capped at 8 MiB. Full files stream through 32 KiB buffers, respecting
upstream 100 MiB compressed and 300 MiB collection limits. Oversize operations
fail rather than truncate. Network cancellation and a 30-second idle timeout
propagate errors and release services.

The SQLite VFS uses exclusive WAL and ASCII SD paths. Shared WAL, directory fsync,
SD removal, and power-loss durability remain unsupported or unverified. Full
replacement validates through upstream code, retains a previous-collection backup,
and writes a recovery marker; reopening after an interrupted replacement restores
the previous file. This is not proof of physical crash safety. Display remains
text-only; media synchronization/playback and CSS rendering are unsupported.

## Reproduction and records

Use `just old3ds-sync-regression`, `just old3ds-sync-transport-tests`, `just old3ds-app-assets-tests`,
`just old3ds-app-sync-build`, and `just old3ds-app-sync-tests release`.
The last two use an emulator-only endpoint and isolated project-owned collections.
Use `just old3ds-app-build` for production; packaging rejects an emulator build.
Tool versions and flags are in `3ds/toolchain.sh` and the build scripts.

Raw logs/databases remain in ignored `out/3ds/`:

- `app-sync/clock-zone-final-*` and `logs/clock-zone-final-sync-suite.log`: sixteen HTTPS cases.
- `logs/final-v10-direction-guard.log`: explicit replacement-direction gate.
- `logs/clock-offline-final-ui{,-verify}.log`: offline reviews, draft preservation and desktop reopening.
- `logs/clock-final-repository-check-retry.log`: broad desktop checks; two Qt installer failures.
- `app-sync/clock-auto-zone-final/`: delayed automatic UTC correction against a strict TLS fixture.
- `startup-tests/cached-zone-fixed/result.json`: production cached-clock and scheduler offsets.
- `logs/clock-offline-host-tests.log`: six assets/clock tests.
- `normal-sync-probe/hardware-results-2026-10-14/report.txt`: earlier hardware run.
- `cleanup-2026-10-14/docs-before/`: preserved historical raw reports.

Next verification: final package on physical Old 3DS, then a separately approved
disposable AnkiWeb account test with collection-replacement behavior reviewed.
