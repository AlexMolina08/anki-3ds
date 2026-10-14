# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Create auditable named coverage without combining distinct test boundaries."""

import collections
import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/testing"
inventory = json.loads((OUT / "inventory.json").read_text())
native = json.loads((OUT / "tempfile-adapted-results.json").read_text())
assert len(native["outcomes"]) == native["discovered"] == 707
assert not native["remaining"]
comparison = json.loads((OUT / "differential-comparison.json").read_text())
assert comparison["exact_match"] and comparison["all_reopened"]
fixture_rerun = OUT / "fixture-mapped-results.json"
if fixture_rerun.exists():
    rerun = json.loads(fixture_rerun.read_text())
    assert set(rerun["outcomes"]) <= set(native["outcomes"])
    for name, verdict in rerun["outcomes"].items():
        verdict = dict(verdict)
        verdict["earlier_adapted_result"] = native["outcomes"][name]
        verdict["verification_report"] = "fixture-mapped-results.json"
        native["outcomes"][name] = verdict
    native["fixture_path_rerun"] = rerun
results = {"core": native, "differential": comparison, "support": {}}
for package in ["anki_i18n", "anki_io", "anki_process"]:
    path = OUT / f"{package}-results.json"
    assert path.exists(), f"Missing fresh utility report: {package}"
    results["support"][package] = json.loads(path.read_text())
(OUT / "upstream-final-results.json").write_text(json.dumps(results, indent=2) + "\n")
with (OUT / "upstream-coverage.csv").open("w") as file:
    writer = csv.writer(file)
    writer.writerow(
        ["suite", "test", "macOS_default", "macOS_offline", "Old_3DS", "reason"]
    )
    names = set(inventory["tests"]) | {
        n.removeprefix("anki::") for n in native["outcomes"]
    }
    excluded = set(inventory["excluded_offline"])
    for name in sorted(names):
        status = native["outcomes"].get("anki::" + name)
        writer.writerow(
            [
                "rslib",
                name,
                "passed" if name in inventory["tests"] else "platform_excluded",
                "feature_excluded"
                if name in excluded
                else ("passed" if name in inventory["tests"] else "platform_excluded"),
                status["status"] if status else "feature_excluded",
                status.get("reason", "") if status else "network feature disabled",
            ]
        )
    for package, data in results["support"].items():
        for name, status in data["outcomes"].items():
            writer.writerow(
                [
                    package,
                    name,
                    "passed",
                    "not_separately_run",
                    status["status"],
                    status.get("reason", ""),
                ]
            )
for label, data in [("core", native), *results["support"].items()]:
    print(
        label, dict(collections.Counter(v["status"] for v in data["outcomes"].values()))
    )
