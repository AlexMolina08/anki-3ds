# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Verify native UI mutations using upstream desktop Anki on a closed copy."""

import json
import shutil
import subprocess
from pathlib import Path

from anki.collection import Collection

root = Path(__file__).resolve().parents[2]
out = root / "out/3ds/ui-check"
sd = out / "data/azahar-emu/sdmc/3ds/anki-3ds"
for line in subprocess.check_output(
    ["ps", "-ax", "-o", "command="], text=True
).splitlines():
    if line.split() and line.split()[0].endswith("/Contents/MacOS/azahar"):
        raise SystemExit("Close the emulator before verification")
events = (sd / "prototype-events.txt").read_text()
assert "CLOSE_OK" in events and "ACTION_ERROR" not in events
assert not (sd / "prototype.anki2.lock").exists()
assert not (sd / "prototype.anki2-wal").exists()
path = out / "verified.anki2"
shutil.copyfile(sd / "prototype.anki2", path)
col = Collection(str(path))
assert col.db.scalar("pragma integrity_check") == "ok"
notes = col.find_notes("UI_CHECK")
assert len(notes) == 1
note = col.get_note(notes[0])
assert note.fields == [
    "UI_CHECK Pregunta con acentos: á é í ó ú ñ ¿Qué?",
    "UI_CHECK respuesta editada y persistida.",
]
saved_ids = [
    int(line.split("id=")[1])
    for line in events.splitlines()
    if line.startswith("NOTE_SAVED")
]
assert saved_ids == [notes[0], notes[0]], "Edit must preserve note identity"
reviews = [line for line in events.splitlines() if line.startswith("REVIEW card=")]
expected = [int(line.split("rating=")[1]) + 1 for line in reviews]
assert expected == [3, 1, 2, 4]
assert col.db.list("select ease from revlog order by id") == expected
for cid in {int(line.split("card=")[1].split()[0]) for line in reviews}:
    card = col.get_card(cid)
    assert card.reps == sum(
        int(line.split("card=")[1].split()[0]) == cid for line in reviews
    )
    assert card.memory_state is not None
native_deck = col.decks.by_name("A Text")
assert native_deck["newLimit"] == 7 and native_deck["reviewLimit"] == 77
native_configs = col.db.all("select * from deck_config order by id")
native_notes = col.note_count()
native_cards = col.card_count()
snapshot = {
    table: col.db.all(f"select * from {table} order by id")
    for table in ["notes", "cards", "revlog", "decks", "deck_config"]
}
col.close()
col = Collection(str(path))
for table, rows in snapshot.items():
    assert col.db.all(f"select * from {table} order by id") == rows
col.close()
baseline_path = out / "baseline.anki2"
shutil.copyfile(root / "out/3ds/app/fixture/prototype.anki2", baseline_path)
baseline = Collection(str(baseline_path))
assert native_notes == baseline.note_count() + 1
assert native_cards == baseline.card_count() + 1
assert native_configs == baseline.db.all("select * from deck_config order by id"), (
    "Shared presets changed"
)
for table in ["notes"]:
    for row in baseline.db.all(f"select * from {table} order by id"):
        assert row in snapshot[table], "Unrelated note changed"
baseline.close()
result = {
    "integrity": "ok",
    "ratings": expected,
    "add_edit_same_note_id": notes[0],
    "new_limit": 7,
    "review_limit": 77,
    "shared_presets_unchanged": True,
    "unrelated_notes_unchanged": True,
    "desktop_reopen_unchanged": True,
    "cards": native_cards,
    "notes": native_notes,
}
(out / "verification.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result))
