# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Build a named runtime inventory from Rust libtest listings."""

import collections
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/testing"


def names(mode):
    return set(
        re.findall(r"^(.+): test$", (OUT / f"host-{mode}-list.txt").read_text(), re.M)
    )


default, offline = names("default"), names("offline")
assert offline <= default
result = {
    "default": len(default),
    "offline": len(offline),
    "excluded_offline": sorted(default - offline),
    "groups": dict(
        sorted(collections.Counter(n.split("::")[0] for n in default).items())
    ),
    "tests": sorted(default),
}
(OUT / "inventory.json").write_text(json.dumps(result, indent=2) + "\n")
print(
    f"{len(default)} default tests; {len(offline)} offline; {len(default - offline)} feature-excluded"
)
