# Nintendo 3DS port

Build, installation, CI and release instructions are maintained in the
[repository README](../README.md).

- [Application controls and SD files](app/README.md)
- [Verification results and limitations](docs/FEASIBILITY.md)
- [Sync transport architecture](docs/SYNC_TRANSPORT_ARCHITECTURE.md)
- [Optional phone login assessment](docs/PHONE_LOGIN.md)

The application targets the original 3DS. Probe crates retain the small platform
experiments behind its storage, backend and network adapters. They are developer
diagnostics, not alternative applications or current feature lists. Generated
files belong in `out/3ds/`.
