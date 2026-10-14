# Native dependency probe

> Historical early-stage probe retained for isolated regression testing.

The collection backend now builds and runs in `collection-probe` and `app`.
This executable tests dependencies in isolation, rather than application readiness.

This executable validates the isolated dependency adaptations needed before
building the original Collection API. It does not substitute for that API.

- Zstd 0.13.3 / zstd-sys 2.0.15+zstd.1.5.7: the scoped C driver selects Zstd's
  supported GNU C90 integer-type branch, retaining all legacy decoder sources.
  Single-thread compression/decompression and invalid-input rejection are tested.
- coarsetime 0.1.36: a small Horizon-only patch uses portable-atomic 1.11.1,
  already present in upstream Anki's lockfile. Instant reads use libctru's
  monotonic kernel tick and SDK ARM11 clock frequency. Desktop code is unchanged.
- Four native Rust threads perform 4,000 atomic increments above 2^32. The
  patched clock measures a 20 ms sleep. The default registered SD VFS reads the
  real disposable fixture using upstream SQL.

`prepare-coarsetime.py` verifies the upstream crate archive against Cargo.lock's
SHA-256, extracts its original source and ISC license into ignored output, and
applies the committed patch. It does not edit the registry cache or vendor a
second collection implementation. Run `just old3ds-runtime-build`, then
`just old3ds-runtime-emulator-run` with the existing disposable fixture staged.

Zstd worker threads are not validated or supported by this proof. Its C pthread
objects use newlib's 16-byte mutex layout, while pthread-3ds assumes 40 bytes.
The native test intentionally uses the default single-thread encoder. Full
Zstd multithread support needs an ABI adapter or an explicit target policy that
selects single-thread export; the desktop exporter must retain its behavior.
