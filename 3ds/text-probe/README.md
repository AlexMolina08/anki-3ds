# Text module regression probe

> Historical early-stage probe retained for isolated regression testing.

Runs six assertions from Anki's original text module: HTML removal, entity
handling, HTML detection and Unicode normalization. The collection backend now
builds and runs in `collection-probe` and `app`; this smaller executable isolates
text processing from storage and networking.

From the repository root:

```sh
just old3ds-text-build
just old3ds-emulator-run
```

The executable displays the assertion result. Press START to exit. The emulator
uses an isolated profile configured for Old 3DS.
