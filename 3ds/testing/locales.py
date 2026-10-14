# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Validate catalog coverage and translated formatting placeholders."""

import json
import re
from pathlib import Path

root = Path(__file__).resolve().parents[1] / "app"
en = json.loads((root / "locales/en.json").read_text())
es = json.loads((root / "locales/es.json").read_text())
assert en.keys() == es.keys(), "language catalogs differ"
for key in en:
    assert re.findall(r"\{\d+\}", en[key]) == re.findall(r"\{\d+\}", es[key]), key
    assert en[key] and es[key], key
for path in (root / "src").glob("*.rs"):
    for literal in re.findall(
        r'(?:tr|i18n::format)\(\s*("(?:[^"\\]|\\.)*")', path.read_text()
    ):
        assert json.loads(literal) in en, (path, literal)
print(
    f"PASS: {len(en)} messages, two catalogs, all formatting placeholders and source references"
)
