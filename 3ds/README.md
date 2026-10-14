# Anki 3DS

Unofficial Old 3DS port of Anki's Rust backend. Native offline reviews use the
upstream collection and scheduler. See [app/README.md](app/README.md) for controls.

```sh
just old3ds-app-build
just old3ds-app-fixture
just old3ds-app-emulator-run
```

Builds use `toolchain.sh` and preserve the upstream desktop defaults.

- `app/`: application and UI checks
- `sqlite-vfs/`: Horizon SD storage adapter
- `sync-transport/`: native HTTPS client
- `normal-sync-probe/`: incremental-sync regression fixture
- `testing/`: upstream tests and desktop/native comparison
- Other probes: isolated platform diagnostics

[Port status](docs/FEASIBILITY.md) · [Sync design](docs/SYNC_TRANSPORT_ARCHITECTURE.md)

Use disposable data during development. Generated artifacts and test results belong
under `out/3ds/`, not in source or documentation directories.
