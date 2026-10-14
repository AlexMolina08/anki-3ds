# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Create an isolated fixture through the checkout-built backend.

The decks cover what the console has to render: plain text, HTML and entities,
Cloze, a reversed template, a custom template with filters, and a card whose
media the console cannot play. No personal collection is opened.
"""

import json
from pathlib import Path

from anki import buildinfo
from anki.collection import Collection

root = Path(__file__).resolve().parents[2]
directory = root / "out/3ds/app/fixture"
directory.mkdir(parents=True, exist_ok=True)
path = directory / "prototype.anki2"
if path.exists():
    raise SystemExit(f"Refusing to overwrite {path}")
collection = Collection(str(path))
collection.set_config("fsrs", True)

basic = collection.models.by_name("Basic")
plain_decks = ["A Text", "B More text", "C Empty"]
for name in plain_decks:
    deck_id = collection.decks.id(name)
    if name == "C Empty":
        continue
    for index in range(8):
        note = collection.new_note(basic)
        note["Front"] = f"{name} question {index + 1}"
        note["Back"] = f"{name} answer {index + 1}"
        collection.add_note(note, deck_id)

# block-level tags turned into line breaks rather than run together.
html_deck = collection.decks.id("D HTML")
html_notes = [
    ("<b>Bold</b> and <i>italic</i>", "An &amp; entity: &lt;tag&gt;"),
    ("First line<br>Second line", "<div>Block one</div><div>Block two</div>"),
    ("<script>alert(1)</script>Visible", "<style>p{color:red}</style>Styled"),
    ("Ampersand &amp;amp; and nbsp&nbsp;here", "Quotes: &quot;q&quot; &apos;a&apos;"),
]
for front, back in html_notes:
    note = collection.new_note(basic)
    note["Front"] = front
    note["Back"] = back
    collection.add_note(note, html_deck)

cloze = collection.models.by_name("Cloze")
cloze_deck = collection.decks.id("E Cloze")
cloze_notes = [
    "The {{c1::capital}} of France is {{c2::Paris}}.",
    "{{c1::Water}} freezes at {{c2::0}} degrees.",
]
for text in cloze_notes:
    note = collection.new_note(cloze)
    note["Text"] = text
    note["Back Extra"] = "extra note"
    collection.add_note(note, cloze_deck)

reversed_model = collection.models.by_name("Basic (and reversed card)")
reversed_deck = collection.decks.id("F Reversed")
note = collection.new_note(reversed_model)
note["Front"] = "reversed front"
note["Back"] = "reversed back"
collection.add_note(note, reversed_deck)

custom = collection.models.new("Custom Filtered")
collection.models.add_field(custom, collection.models.new_field("F"))
template = collection.models.new_template("T")
template["qfmt"] = "{{text:F}}"
template["afmt"] = "{{FrontSide}}<hr id=answer>{{text:F}} shown"
collection.models.add_template(custom, template)
collection.models.add(custom)
custom_deck = collection.decks.id("G Custom")
note = collection.new_note(custom)
note["F"] = "<i>filtered</i>"
collection.add_note(note, custom_deck)

media_deck = collection.decks.id("H Media")
note = collection.new_note(basic)
note["Front"] = 'A picture: <img src="picture.png"> and sound [sound:clip.mp3]'
note["Back"] = "Text after the media."
collection.add_note(note, media_deck)

metadata = {
    "version": buildinfo.version,
    "buildhash": buildinfo.buildhash,
    "cards": collection.card_count(),
    "notes": collection.note_count(),
    "decks": [name for name in plain_decks if name != "C Empty"]
    + ["D HTML", "E Cloze", "F Reversed", "G Custom", "H Media"],
}
collection.close()
(directory / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
sd = root / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"
sd.mkdir(parents=True, exist_ok=True)
destination = sd / "prototype.anki2"
if destination.exists():
    raise SystemExit(
        f"Fixture created, but refusing to overwrite SD collection {destination}"
    )
import shutil

shutil.copyfile(path, destination)
print(json.dumps(metadata))
