# Native application

## Installation

Extract the release ZIP and copy its `3ds` folder to the root of the console's
SD card, merging it with an existing `3ds` folder. The executable must be at
`SD:/3ds/anki-3ds/anki-3ds.3dsx`. Open Homebrew Launcher and select Anki 3DS.
This package requires an existing Homebrew Launcher setup; it is not a CIA
installer or a HOME Menu application.

For an upgrade, replace only `anki-3ds.3dsx`. Preserve existing collection files
and settings. The CA bundle is embedded, so no separate certificate installation
is required. The README, licenses and manifest can remain on your computer.

## Build and startup

`just old3ds-app-build` builds ARMv6K/VFPv2. `just old3ds-app-package` packages the
one executable with an embedded CA bundle, without a collection or credentials.
First launch creates its SD folder and writes `cacert.pem` if absent. Sending the
`.3dsx` through `3dslink` needs no separate certificate transfer. Existing files
are preserved. Setup errors appear onscreen instead of terminating the app.

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
- SELECT opens Menu, including language and automatic clock updates.

English/Spanish follow the console language. Menu overrides are stored in
`language.txt`; remove it to follow the system again. No timezone or numeric clock
setup is required. The app opens offline immediately. Until the first verified
HTTPS clock response, timestamps follow the console clock; afterwards the saved
correction also applies offline. Clock checks run in the background when Wi-Fi
connects and retry after failures. Sync refreshes the correction before contacting
the original synchronization engine. The status bar shows the console's local time.

The 3DS RTC carries local civil time without a timezone. The adapter obtains UTC
from the Date header of a certificate- and hostname-validated AnkiWeb HTTPS
response, without an account. It corrects UTC seconds and derives the local
scheduling offset in quarter-hour steps. A severely incorrect console date is
rejected; System Settings remains the source of the user's local date/time.
Existing `utc-offset-minutes.txt` files are read for compatibility, never requested
from new users. Successful checks save `clock-correction.json`.

Passwords and host keys stay in memory. Media playback, media synchronization,
and CSS rendering are unsupported. SD removal and power-loss recovery remain
unverified; full download retains a `.pre-sync-*` backup before replacement.

`emulator` is a development feature: it accepts `sync-test.json` for disposable
credentials, endpoint, and CA. Release builds do not read that file. It must never
be included in a hardware package. See [port status](../docs/FEASIBILITY.md).
