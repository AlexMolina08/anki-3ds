# Native application

`just old3ds-app-build` builds ARMv6K/VFPv2. `just old3ds-app-package` packages the
executable and CA bundle without a collection or credentials.

The app opens `sdmc:/3ds/anki-3ds/prototype.anki2`. If absent, the original Anki
backend creates an empty collection; you can then sign in and download a collection.
Use disposable data until final hardware and authenticated AnkiWeb verification.

- Decks: D-pad selects; A studies; Left/Right expands or collapses.
- Review: A reveals; Y Again, X Hard, A Good, R Easy. Touch ratings also work.
- Add/Edit: select a field for the Nintendo keyboard; X saves, B cancels.
- Browse: upstream search syntax; X opens search.
- Sync: enter username and masked password. B cancels; START cancels and exits
  after cleanup. Full sync requires a direction when both collections contain data, then asks
  twice before replacement.
- SELECT opens Menu, including language and console time-zone settings.

English/Spanish follow the console language. Menu overrides are stored in
`language.txt`; remove it to follow the system again. The clock offset is minutes
**east** of UTC and is stored in `utc-offset-minutes.txt`. The application corrects
Horizon's local RTC into UTC and passes the opposite sign to Anki's offset API.

Passwords and host keys stay in memory. Media playback, media synchronization,
and CSS rendering are unsupported. SD removal and power-loss recovery remain
unverified; full download retains a `.pre-sync-*` backup before replacement.

`emulator` is a development feature: it accepts `sync-test.json` for disposable
credentials, endpoint, and CA. Release builds do not read that file. It must never
be included in a hardware package. See [port status](../docs/FEASIBILITY.md).
