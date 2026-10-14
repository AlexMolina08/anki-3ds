Anki 3DS now includes both Homebrew Launcher (`.3dsx`) and HOME Menu (`.cia`) builds.

- Persistent AnkiWeb login, separate Log in/Log out actions, and fewer clock requests.
- Updated navigation icons and the maintainer-supplied Anki launcher artwork.
- Pinned build inputs, automatic build artifacts, source manifests and SHA-256 checksums.

For FBI: copy `anki-3ds.cia` to the SD card, open FBI, select the file and install it.
For Homebrew Launcher: copy `anki-3ds.3dsx` to `SD:/3ds/anki-3ds/anki-3ds.3dsx`.
Both formats use the same `SD:/3ds/anki-3ds/` collection and settings. Back up that
folder before testing. Upgrades should preserve its contents.

This is an experimental, unofficial port. Native application and login checks run
in Old 3DS emulation. Physical Old 3DS verification of this release is pending.
Media synchronization, media playback and CSS card rendering are not supported.
Use a developer account and disposable collection for initial testing.
