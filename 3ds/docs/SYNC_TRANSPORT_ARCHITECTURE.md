# Sync implementation

The application owns an original Anki `Collection`. A worker calls
`Collection::normal_sync_with_client`, which constructs the original
`NormalSyncer<P: SyncClient>`. `SyncClient` extends upstream `SyncProtocol`.
`NativeSyncClient` serializes those upstream types and transports bytes through
zstd, curl/mbedTLS, and Horizon SOC. Desktop `HttpSyncClient` remains the default
`network` implementation. No merge algorithm or duplicate protocol type lives in
`3ds/`.

The `incremental-sync` feature exposes original orchestration without Reqwest,
Tokio networking, Mio, or socket2. Original USNs, conflict resolution, tombstones,
note/card/revlog processing, sanity checks, and collection transactions remain in
`rslib/src/sync/collection`. A failed normal sync rolls back and attempts original
server abort. Native requests complete synchronously inside the async trait;
a single worker uses a futures executor. The UI polls shared progress and an
atomic cancellation flag, then joins the worker before closing its collection.

Login calls the original generic host-key entrypoint. Nintendo's keyboard masks
the password. Credentials remain in RAM until exit and never enter diagnostics.
Anki chooses whether full sync is required and which directions are permitted.
The UI requires explicit direction when both collections contain data, followed
by two confirmations. Generic `FullSyncClient` file hooks preserve original
full-upload preparation, limits, full-download validation, and completion updates.
Desktop hooks retain the existing tempfile and atomic-rename behavior.

Native full-file hooks stream to SD scratch files. Validated download replacement
retains the old file, records a pending marker, and restores the backup on recovery.
Horizon temporary paths preserve the `sdmc:/` prefix. Rename-failure and interrupted
replacement tests pass; physical power-loss durability is not established.

The transport validates certificates and hostnames, requires TLS 1.2 or later,
and restricts redirects before forwarding authentication. Production uses console
DNS; only an emulator build reads the isolated test endpoint/DNS configuration.
`NativeSoc` owns and releases its aligned 1 MiB buffer. Requests reject oversized
bodies: 8 MiB compressed, 16 MiB decoded, 8 MiB zstd window. Full files use 32 KiB
buffers and upstream 100/300 MiB limits. Cancellation applies to DNS, transfers,
and codecs; idle transfers time out after 30 seconds. Binary payloads are preserved.

`3ds/testing/app_sync.py` executes the app against the original desktop Anki server
through a local strict-TLS boundary. It compares native/server databases, the same
desktop fixtures, and an independent SQL oracle. Fault injection occurs only at the
external protocol boundary. Neither this server nor the test configuration is a
production runtime dependency. See FEASIBILITY.md for results and remaining gates.
