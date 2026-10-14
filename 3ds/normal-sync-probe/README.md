# Original NormalSyncer Phase 4 probe, revision 1

This standalone development executable runs the original Anki incremental driver
on ten pairs of disposable collections. Its in-process SyncProtocol fixture calls
Anki's original server operations, without a network server. It then runs the same
Collection/NormalSyncer entrypoint with NativeSyncClient against unauthenticated
AnkiWeb metadata, expecting HTTP 400. It does not authenticate or perform a
collection-changing AnkiWeb operation. No personal account or collection is used.

## Build and validate

Run from the repository root:

```sh
just old3ds-normal-sync-host
just old3ds-normal-sync-build
just old3ds-normal-sync-emulator
just old3ds-normal-sync-verify
just old3ds-normal-sync-package
just old3ds-sync-regression
```

The host recipe refuses existing input fixtures and desktop output. Retain old
artifacts under another project-owned directory before starting a fresh run.
The emulator recipe refuses existing `phase4-v1` data for the same reason.
Inputs are created through upstream APIs; fixture-only SQL establishes a prior
sync baseline and controlled conflicting timestamps. No SQL computes a merge.
The Python verifier independently inspects closed SQLite files, validates known
fixture outcomes, and compares full entity rows with desktop output.

## Physical Old 3DS run

Use the versioned `anki-normal-sync-phase4-v1-*-old3ds.zip` produced after emulator
verification. Copy its `3ds` folder onto SD, preserving the existing offline app.
The ZIP adds `3ds/anki-normal-sync-probe/` and
`3ds/anki-3ds/phase4-v1/fixtures/`. The CA file is the same pinned trust store used
by the native transport. Do not replace any personal collection.

Start **Anki NormalSync v4** in Homebrew Launcher. Wait for `complete=1 result=PASS`,
then press START. Retrieve:

- `/3ds/anki-normal-sync-probe/report.txt`
- `/3ds/anki-3ds/phase4-v1/native.json`
- `/3ds/anki-3ds/phase4-v1/results/` (closed disposable databases)

A repeated run refuses an existing results folder. Retain that folder before a
new run. Do not remove VFS lock directories while an executable is running.
A failure or power interruption can leave locks; no automatic lock deletion or
collection replacement occurs. Test only disposable data.

The console clock must be correct and Wi-Fi configured. Strict certificate and
hostname verification remains enabled. Hardware packaging excludes the emulator
DNS override. Physical execution is unverified until these new outputs are read.

## Boundaries

No full sync, media sync, persistent credentials, SD replacement, or production
collection synchronization is enabled. Collection tests use an in-process protocol
fixture; only the rejected metadata path uses NativeSyncClient/HTTPS. Curl remains
blocking on the calling thread. Engine cancellation is tested at protocol boundaries;
UI cancellation during a native HTTP request is not integrated. NativeSoc owns
and frees the caller-allocated SOC buffer after checked shutdown; the probe also
checks duplicate initialization rejection and successful service reinitialization. Power-loss durability
and physical SD fault recovery remain unverified.
