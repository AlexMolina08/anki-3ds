# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Restore the emulator's SD collection to the pristine fixture.

The collection probes mutate the collection they are given, so each run needs a
clean starting point. This copies the generated fixture over the SD copy and
removes the sidecar files a previous run may have left, since a stale WAL or a
lock marker would make the next probe read someone else's state or refuse to
open at all.

The fixture is never written to: it is read and copied.
"""

import hashlib
import json
import shutil
import sqlite3
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[2]
fixture = root / "out/3ds/fixtures/disposable.anki2"
metadata = json.loads(fixture.with_suffix(".json").read_text())
sd = root / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"

if not fixture.exists():
    raise SystemExit("Run `just old3ds-fixture-create` first")

# The fixture may have been left with an uncheckpointed WAL by whichever tool
sd.mkdir(parents=True, exist_ok=True)
target = sd / "disposable.anki2"
for suffix in ("", "-wal", "-shm", ".lock"):
    (target.parent / (target.name + suffix)).unlink(missing_ok=True)
with sqlite3.connect(f"file:{fixture}?mode=ro", uri=True) as source:
    with sqlite3.connect(target) as destination:
        source.backup(destination)

with sqlite3.connect(target) as connection:
    cards = connection.execute("select count(*) from cards").fetchone()[0]
    revlog = connection.execute("select count(*) from revlog").fetchone()[0]
    left = connection.execute("select left from cards").fetchone()[0]
    data = connection.execute("select data from cards").fetchone()[0]
if cards != 1 or revlog != 0 or left != 0 or data not in ("", "{}"):
    raise SystemExit(f"fixture is not pristine: cards={cards} revlog={revlog} left={left} data={data!r}")

digest = hashlib.sha256(target.read_bytes()).hexdigest()
result = {
    "sd_collection": str(target),
    "fixture_note_id": metadata["note_id"],
    "cards": cards,
    "revlog_rows": revlog,
    "sha256": digest,
}
(root / "out/3ds/logs/sd-fixture-reset.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result))
sys.exit(0)
