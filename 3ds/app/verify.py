# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Validate UI review persistence with the original desktop backend, read-only source copy."""

import json
import shutil
import subprocess
from pathlib import Path

from anki.collection import Collection

root = Path(__file__).resolve().parents[2]
for line in subprocess.check_output(
    ["ps", "-ax", "-o", "command="], text=True
).splitlines():
    if line.split() and line.split()[0].endswith("/Contents/MacOS/azahar"):
        raise SystemExit("Close Azahar before verifying the collection copy")
sd = root / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"
events = (sd / "prototype-events.txt").read_text()
if "CLOSE_ERROR" in events or "OPEN_ERROR" in events:
    raise SystemExit("Application recorded a database error")
assert events.count("CLOSE_OK") >= 2, "Need two normal application exits"
reviews = [line for line in events.splitlines() if line.startswith("REVIEW ")]
assert reviews, "No physical-input reviews recorded"
expected = [int(line.split("rating=")[1]) + 1 for line in reviews]
assert set(expected) == {1, 2, 3, 4}, "Each of four ratings must be exercised"
path = root / "out/3ds/app/verified.anki2"
shutil.copyfile(sd / "prototype.anki2", path)
assert not (sd / "prototype.anki2.lock").exists(), "Normal exit left ownership lock"
assert not (sd / "prototype.anki2-wal").exists(), "Normal close left uncheckpointed WAL"
collection = Collection(str(path))
stored_cards = collection.db.all("select * from cards order by id")
stored_history = collection.db.all("select * from revlog order by id")
assert collection.db.scalar("pragma integrity_check") == "ok"
rows = collection.db.all("select cid,ease,ivl,factor from revlog order by id")
assert [row[1] for row in rows] == expected
card_ids = {int(line.split("card=")[1].split()[0]) for line in reviews}
for cid in card_ids:
    reps, data = collection.db.first("select reps,data from cards where id=?", cid)
    assert reps == sum(row[0] == cid for row in rows)
    assert json.loads(data).get("s") is not None, "Missing FSRS stability"
    assert json.loads(data).get("d") is not None, "Missing FSRS difficulty"
metadata = json.loads((root / "out/3ds/app/fixture/metadata.json").read_text())
assert collection.card_count() == metadata["cards"]
assert collection.note_count() == metadata["notes"]
collection.close()
collection = Collection(str(path))
assert collection.db.all("select * from cards order by id") == stored_cards
assert collection.db.all("select * from revlog order by id") == stored_history
collection.close()
result = {
    "integrity_check": "ok",
    "reviews": len(rows),
    "ratings": expected,
    "desktop_backend_reopened": True,
    "cards": metadata["cards"],
    "notes": metadata["notes"],
    "normal_exits": events.count("CLOSE_OK"),
}
result["cards_and_revlog_unchanged_across_desktop_reopen"] = True
(root / "out/3ds/app/verification.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result))
