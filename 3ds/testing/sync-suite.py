# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Run controlled native application synchronization cases without production data."""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
label = sys.argv[1] if len(sys.argv) > 1 else "release"
base = ROOT / "out/3ds/normal-sync-probe/fixtures"
runs = [
    (case, case, "SYNC,BACK,EXIT", None, 0, False)
    for case in (
        "no_changes",
        "local",
        "remote",
        "both",
        "review",
        "deletion",
        "conflict",
    )
]
runs += [
    ("upload", None, "SYNC,STUDY,STUDY,BACK,EXIT", None, 0, False),
    ("download", "full_required", "SYNC,DOWN,STUDY,STUDY,BACK,EXIT", None, 0, False),
    ("fresh-download", "full_required", "SYNC,STUDY,STUDY,BACK,EXIT", None, 0, True),
    ("interrupt", "local", "SYNC,BACK,SYNC,BACK,EXIT", "interrupt", 0, False),
    ("cancel", "local", "SYNC,CANCEL,BACK,SYNC,BACK,EXIT", "cancel", 0, False),
    (
        "corrupt",
        "full_required",
        "SYNC,DOWN,STUDY,STUDY,BACK,EXIT",
        "corrupt",
        0,
        False,
    ),
    ("reject-cert", "local", "SYNC,BACK,EXIT", "reject-cert", 0, False),
    ("wrong-host", "local", "SYNC,BACK,EXIT", "wrong-host", 0, False),
    ("timezone", "local", "SYNC,BACK,EXIT", None, 120, False),
]
for name, case, steps, fault, offset, empty in runs:
    args = [
        "python3",
        str(ROOT / "3ds/testing/app_sync.py"),
        "--label",
        f"{label}-{name}",
        "--steps",
        steps,
        "--utc-offset",
        str(offset),
    ]
    if case:
        args += [
            "--fixture",
            str(base / case / "client.anki2"),
            "--remote-fixture",
            str(base / case / "server.anki2"),
        ]
        if case != "full_required":
            args += ["--expected-case", case]
    if fault:
        args += ["--fault", fault]
    if empty:
        args += ["--empty"]
    with (ROOT / f"out/3ds/logs/{label}-{name}.log").open("w") as log:
        subprocess.run(args, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)
    print("PASS", name, flush=True)
print(f"PASS: {len(runs)} controlled native HTTPS cases")
