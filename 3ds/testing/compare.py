# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Require exact persisted and computed state equality; never relax tolerances."""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/testing"
host = json.loads((OUT / "differential-host.json").read_text())
native = json.loads((OUT / "differential.json").read_text())


def differences(a, b, path="$"):
    if isinstance(a, dict) and isinstance(b, dict):
        for key in sorted(set(a) | set(b)):
            if key not in a or key not in b:
                yield path + "." + key
            else:
                yield from differences(a[key], b[key], path + "." + key)
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            yield path + ".length"
        for i, (x, y) in enumerate(zip(a, b)):
            yield from differences(x, y, f"{path}[{i}]")
    elif a != b:
        yield path + ": " + repr(a) + " != " + repr(b)


errors = list(differences(host, native))
result = {
    "exact_match": not errors,
    "scenario_count": len(host["scenarios"]),
    "transition_count": sum(len(x["history"]) for x in host["scenarios"]),
    "all_reopened": all(x.get("disk_reopened", False) for x in host["scenarios"]),
    "differences": errors,
}
(OUT / "differential-comparison.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
assert not errors, (
    "Desktop and Old 3DS results differ; inspect each recorded discrepancy"
)
assert result["all_reopened"], "Disk continuation has not been verified"
