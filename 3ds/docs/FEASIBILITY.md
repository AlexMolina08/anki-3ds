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

## Startup and clock handling

First launch creates the application folder and installs the embedded CA bundle.
No separate certificate transfer or numeric timezone setup is required. Setup
errors appear onscreen. Release builds ignore emulator action scripts.

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

## Hardware verification

Ten isolated incremental-engine cases have reported success on physical Old 3DS.
Independent comparison of those hardware databases is incomplete. Native HTTPS
has reached official AnkiWeb with certificate and hostname validation enabled.
The final application, automatic clock calibration and authenticated AnkiWeb sync
have not yet been verified on hardware. Emulator results do not establish SD
power-loss safety or production account compatibility.

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

## Running the tests

Run these commands from the repository root:

```sh
just old3ds-sync-regression
just old3ds-sync-transport-tests
just old3ds-app-assets-tests
just old3ds-locales-check
just old3ds-app-sync-build
just old3ds-app-sync-tests release
just old3ds-app-offline-clock-check
just old3ds-app-ui-verify
```

The integration suite uses the original desktop sync server, a local TLS endpoint
and generated collections under `out/3ds/`. It compares native results with the
desktop engine and independently checks SQLite rows. It never accesses a personal
collection or production account. Use a new suite label for each run to preserve
previous results. The offline UI check tests reviews and editing with an
unreachable clock endpoint; its verifier reopens the collection on desktop.

`just old3ds-upstream-tests-all` runs the original backend and utility tests on
desktop and Old 3DS emulation, plus disk-backed comparisons. See the root README
for native unit-test results and their known failures. `just check` runs the broader
desktop checks. Keep failed results when reporting a test run.

Build release binaries with `just old3ds-app-build`, then
`just old3ds-app-package`. Packaging rejects emulator-enabled binaries and excludes
collections and credentials. Tool versions and target flags are in
`3ds/toolchain.sh`. Build products and test output belong in ignored `out/3ds/`.

Further release validation requires the current application on physical Old 3DS
and authenticated synchronization using a separate test account, including review
of full-sync replacement behavior.

## 2026-10-14: persistent login and measured authentication

Hypothesis: the native app can authenticate with an explicitly authorized,
disposable developer account, save its upstream host key, and authenticate after
restart without re-entering credentials or repeating a fresh clock request.
Success requires native hostKey/meta acceptance, persistent reuse, rejection
recovery, logout, and unchanged disposable collection entities. No personal
collection or full/incremental account sync is part of this test.

Changes stay in the 3DS application/transport. Menu has separate Log in/Log out
commands. Login uses original `sync_login_with_client` and
`online_sync_status_check`, including upstream endpoint redirect and clock checks.
Two flushed token slots preserve the last valid save on application-write failure.
Ordinary network failure keeps authentication; `AuthFailed` removes both slots.
Passwords are not persisted by release builds. Saved tokens are plaintext SD
credentials; this is not encrypted storage or proven physical power-loss safety.

Clock verification has a 30-minute lifetime, checks backwards timestamps and
session RTC discontinuity, and treats legacy corrections as requiring a refresh
before network operations. Fresh restarts and repeated login/sync attempts skip
the extra clock TLS request. Offline study retains its previous correction.
Native logs expose DNS/TCP/TLS/first-byte/total durations and HTTP/curl status,
without credentials or auth headers. DNS retransmits once at 2.5 seconds within
the existing five-second per-server deadline; cancellation stays bounded.

Verified commands/results:

- `just old3ds-app-sync-build`: ARMv6K/VFPv2 emulator build passed.
- `just old3ds-app-build`: release build passed; emulator credential/script
  support is absent. Release binary SHA-256:
  `582c844809eac3ab2136815d321a93758f84afef42970098a692ea2a2c5c3621`.
- `just old3ds-app-login-check auth-clock-03`: passed native login, in-session
  reuse, restart with blank credentials and no clock request, injected disconnect
  with token-preserving retry, invalid-token removal, re-login and logout.
  Disposable note/card/revlog/grave rows remained unchanged.
- `just old3ds-app-login-live developer-account-01` and final
  `developer-account-02`: official AnkiWeb accepted
  the authorized developer account through native hostKey/meta, followed its
  redirect, and accepted the saved token after restart. Logout passed. Credentials
  and saved keys were removed from the isolated SD profile by the harness.
  Initial hostKey transfer took 1,395 ms (1,428 ms including upstream login work);
  TLS handshakes in that run took 924–968 ms. The final run also confirmed
  that repeat/restart login goes directly to the saved shard without a repeat
  HTTP 308 handshake. These are emulator measurements,
  not Old 3DS hardware timings. No account collection transfer was performed.
- `just old3ds-sync-transport-tests`: 12 passed, including four persistence tests.
- `just old3ds-app-assets-tests`: three asset and four clock tests passed.
- `just old3ds-dns-retry-tests`: the actual C DNS loop passed virtual-clock tests
  for first-packet loss/retransmit recovery, fixed timeout, cancellation and a
  failed retransmit. These tests use socket/service mocks, not physical Wi-Fi.
- `just old3ds-sync-regression`: 38 upstream sync tests passed.
- `just old3ds-app-sync-case --label login-incremental-01 --fixture
  out/3ds/normal-sync-probe/fixtures/no_changes/client.anki2 --remote-fixture
  out/3ds/normal-sync-probe/fixtures/no_changes/server.anki2 --expected-case
  no_changes --steps SYNC,BACK,EXIT`: native results matched the original desktop
  server and independent SQL oracle.
- `just old3ds-locales-check`: 102 English/Spanish messages passed.
- `just check`: initially blocked by the existing latest author's absence from
  CONTRIBUTORS history. A second run used the supported local
  `CONTRIBUTORS_BYPASS_EMAILS` allowlist without changing license declarations.
  That run reached Qt tests: 132 passed, two installer tests failed because
  Briefcase could not clone the existing macOS installer template. See
  `out/3ds/logs/sync-login-check.log`; these desktop installer failures remain.

The earlier `auth-clock-01` test correctly failed on a redundant Wi-Fi reconnect
clock request; that trigger was fixed before the passing run. A later harness fix
also preserves a saved redirected endpoint instead of overriding it with the
emulator configuration's initial endpoint.

Physical Old 3DS authenticated login and timing remain unverified. Connect/idle
deadlines remain 15/30 seconds: emulator results do not justify the previous
unmeasured hardware timing claims or arbitrary timeout increases. The next
hardware experiment is the release `.3dsx`, developer-account Log in, app restart,
Log in again, and collection-free review of the recorded transfer timings.

Protected optional phone pairing is plausible with a temporary local TLS listener
and a trusted phone client that pins a key scanned from the console. It remains
an assessment, not an implemented feature. Existing socket/mbedTLS capabilities,
browser trust limitations, sources and the smallest next experiment are recorded
in `3ds/docs/PHONE_LOGIN.md`.
