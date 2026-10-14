# Anki 3DS

Anki 3DS is an unofficial native port of Anki's Rust backend for the original
Nintendo 3DS. It uses upstream collection, scheduler/FSRS and synchronization
code, with Horizon adapters for storage, HTTPS and the user interface.

The application supports offline reviews, note creation/editing, search and deck
settings. English and Spanish follow the console language, with a menu override.
Incremental and full synchronization pass controlled Old 3DS emulator tests.
The final application and authenticated AnkiWeb synchronization still need
physical hardware verification. Media playback, media sync and CSS rendering
are not supported. See [port status](docs/FEASIBILITY.md) for test results,
memory limits and storage caveats.

## Build and run

Development uses Rust `nightly-2026-10-12` with `rust-src`, `cargo-3ds`, devkitPro's
devkitARM/libctru tools, `protoc`, Python and `just`. The target is
`armv6k-nintendo-3ds`; build scripts check ARMv6K/VFPv2 ELF attributes.
`toolchain.sh` defines paths and flags. Run from the repository root:

```sh
just old3ds-doctor
just old3ds-app-build
just old3ds-app-package
```

The package contains the Homebrew Launcher executable, licenses and a manifest.
It embeds the CA bundle and includes no collection or credentials. First launch
creates the application folder on SD. See [application instructions](app/README.md)
for installation, controls, account entry and clock handling.

For local testing in Azahar's Old 3DS mode:

```sh
just old3ds-app-fixture
just old3ds-app-emulator-run
```

These commands use a generated collection and an isolated emulator profile.
The emulator path can be set through `ANKI_3DS_AZAHAR`. Test accounts and generated
collections should be used while hardware and account compatibility are evaluated.

## Source layout and contributing

- `app/`: application, UI and application-level checks.
- `sqlite-vfs/`: Horizon SD storage adapter.
- `sync-transport/`: native HTTPS client; synchronization algorithms remain upstream.
- `normal-sync-probe/`: original incremental engine regression tests.
- `testing/`: upstream test runners and desktop/native comparisons.
- `fixtures/`: collection generators and independent result verifiers.
- Other `*-probe/` directories: focused platform diagnostics. Early probes are
  historical tests, not descriptions of current application capabilities.

Keep platform changes narrow and preserve upstream desktop behavior. Add focused
regression coverage for behavior changes, run the relevant commands listed in
[port status](docs/FEASIBILITY.md), and run `just check` for desktop regressions.
Report whether results come from desktop, emulation or physical Old 3DS.
[Sync architecture](docs/SYNC_TRANSPORT_ARCHITECTURE.md) describes the transport
boundary and collection replacement behavior.

Probe source and Cargo lockfiles remain tracked for reproducibility. The app also
uses `https-probe/build-libs.sh` to build its native dependencies. Generated
binaries, collections, SD images, logs and archives belong in ignored `out/3ds/`.
