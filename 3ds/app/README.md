# Native application

## Installation

The release includes two install formats:

- Homebrew Launcher: copy `3ds/anki-3ds/anki-3ds.3dsx` from the ZIP to the same
  location on SD, then open it in Homebrew Launcher.
- HOME Menu: copy `cia/anki-3ds.cia` to SD, select it in FBI and install it.

Both formats use `SD:/3ds/anki-3ds/` for the collection and settings. Back up this
folder before initial hardware testing. Installing the CIA does not import a
collection or credentials.

For an upgrade, replace the `.3dsx` or install the updated CIA. Preserve collection files
and settings. The CA bundle is embedded, so no separate certificate installation
is required. The README, licenses and manifest can remain on your computer.

## Build and startup

`just old3ds-release` builds ARMv6K/VFPv2 `.3dsx` and `.cia` binaries and packages
them with licenses, a manifest and checksums. See the repository README for setup.
First launch creates its SD folder and writes `cacert.pem` if absent. Sending the
`.3dsx` through `3dslink` needs no separate certificate transfer. Existing files
are preserved. Setup errors appear onscreen instead of terminating the app.

The app opens `sdmc:/3ds/anki-3ds/prototype.anki2`. If absent, the original Anki
backend creates an empty collection; you can then sign in and download a collection.
Use disposable data until final hardware and authenticated AnkiWeb verification.

- Decks: D-pad selects; A studies; Left/Right expands or collapses.
- Review: A reveals; Y Again, X Hard, A Good, R Easy. Touch ratings also work.
  Revealed cloze answers appear in blue in their context on the top screen.
  The bottom answer panel stays available. Up/Down scrolls both panels;
  Left/Right moves five lines. Swipe the answer text or drag its right scrollbar
  to read long cards without moving the rating buttons.
- Add/Edit: select a field for the Nintendo keyboard; X saves, B cancels.
- Browse: upstream search syntax; X opens search.
- Menu → Log in: enter your AnkiWeb email and masked password once. Login checks
  authentication without uploading or downloading a collection. The saved login
  works after restarting. Menu → Log out removes it from this SD card.
- Sync reuses the saved login, or asks for credentials when needed. B cancels;
  START cancels and exits after cleanup. Full sync requires a direction when both collections contain data, then asks
  twice before replacement.
- SELECT opens Menu, including language and automatic clock updates.

English/Spanish follow the console language. Menu overrides are stored in
`language.txt`; remove it to follow the system again. No timezone or numeric clock
setup is required. The app opens offline immediately. Until the first verified
HTTPS clock response, timestamps follow the console clock; afterwards the saved
correction also applies offline. Clock checks run in the background when Wi-Fi
connects with a stale correction and retry after failures. Login and sync reuse
corrections verified within 30 minutes. Missing, expired or backwards-clock
timestamps trigger calibration; an RTC jump during the session also expires it. The status bar shows the console's local time.

The 3DS RTC carries local civil time without a timezone. The adapter obtains UTC
from the Date header of a certificate- and hostname-validated AnkiWeb HTTPS
response, without an account. It corrects UTC seconds and derives the local
scheduling offset in quarter-hour steps. A severely incorrect console date is
rejected; System Settings remains the source of the user's local date/time.
Existing `utc-offset-minutes.txt` files are read for compatibility, never requested
from new users. Successful checks save `clock-correction.json`.

Passwords stay in memory and are cleared after login. Host keys and their sync
endpoint are saved in `sync-auth.json` and `sync-auth-backup.json`. Both contain
plaintext bearer credentials: anyone who can read the SD card can use them. Never
share these files. Logout deletes both slots and any incomplete temporary file.
A network error keeps the token; an upstream authentication rejection removes it
and the next attempt asks for credentials. Two flushed slots preserve the previous
login during replacement; physical SD power-loss durability is unverified.

Media playback, media synchronization and CSS rendering are unsupported. SD removal and power-loss recovery remain
unverified; full download retains a `.pre-sync-*` backup before replacement.

`emulator` is a development feature: it accepts `sync-test.json` for disposable
credentials, endpoint, and CA. Release builds do not read that file. It must never
be included in a hardware package. See [port status](../docs/FEASIBILITY.md).
