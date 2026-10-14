# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Record app/backend provenance without opening any collection."""

import hashlib
import json
import re
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(
    "/Applications/Anki.app/Contents/Resources/app_packages/anki/_rsbridge.so"
)
strings = subprocess.check_output(["strings", str(BINARY)], text=True)
versions = sorted(set(re.findall(r"/fsrs-([0-9]+\.[0-9]+\.[0-9]+)/", strings)))
assert versions, "No embedded FSRS source paths; version is unknown"
script = ROOT / "out/3ds/testing/app-version.py"
script.write_text(
    'import json\nfrom anki import buildinfo\nprint(json.dumps({k:v for k,v in vars(buildinfo).items() if not k.startswith("_")}, default=str))\n'
)
import os

app = json.loads(
    subprocess.check_output(
        [str(ROOT / "out/3ds/fixtures/python-launcher"), str(script)],
        env={
            **os.environ,
            "PYTHONHOME": "/Applications/Anki.app/Contents/Frameworks/Python.framework/Versions/3.13",
            "PYTHONPATH": "/Applications/Anki.app/Contents/Resources/app_packages",
        },
        text=True,
    )
)
lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
fsrs = next(p for p in lock["package"] if p["name"] == "fsrs")
app_commit = subprocess.check_output(
    ["git", "rev-parse", app["buildhash"]], cwd=ROOT, text=True
).strip()
result = {
    "installed": app,
    "installed_revision": app_commit,
    "binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(),
    "binary_embedded_fsrs_source_versions": versions,
    "repository_revision": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip(),
    "repository_version": (ROOT / ".version").read_text().strip(),
    "repository_fsrs": fsrs,
    "backend_diff": subprocess.check_output(
        ["git", "diff", "--stat", app_commit, "HEAD", "--", "rslib", "Cargo.lock"],
        cwd=ROOT,
        text=True,
    ),
}
path = ROOT / "out/3ds/testing/versions.json"
path.write_text(json.dumps(result, indent=2) + "\n")
print(
    json.dumps(
        {
            k: v
            for k, v in result.items()
            if k not in ["repository_fsrs", "backend_diff"]
        },
        indent=2,
    )
)
