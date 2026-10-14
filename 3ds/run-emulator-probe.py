# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Run a 3DS probe recipe in Azahar and always terminate the emulator.

The emulator window blocks until it is closed, and the probes only exit on the
START button, so a plain foreground run can hang indefinitely. This driver
starts the recipe, waits for the probe's own result file, and kills every
emulator process it started before returning, pass or fail.

Usage: run-emulator-probe.py <recipe> <result-filename> [--timeout SECONDS] [--fsrs]
"""

import argparse
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SD = ROOT / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"
LOG = ROOT / "out/3ds/logs"


def clients():
    """PIDs of emulator processes holding one of our probes."""
    found = set()
    for line in subprocess.check_output(["ps", "-ax", "-o", "pid=,command="], text=True).splitlines():
        fields = line.split()
        if len(fields) >= 3 and fields[1].endswith("/Contents/MacOS/azahar"):
            found.add(int(fields[0]))
    return found


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("recipe")
    parser.add_argument("result")
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument(
        "--fsrs",
        action="store_true",
        help="create the marker that switches the probe to the FSRS scheduler",
    )
    args = parser.parse_args()

    if clients():
        raise SystemExit("An Azahar emulator is already running; close it first")
    LOG.mkdir(parents=True, exist_ok=True)
    result = SD / args.result
    result.unlink(missing_ok=True)
    marker = SD / "fsrs-mode"
    marker.unlink(missing_ok=True)
    if args.fsrs:
        marker.write_text("")

    log_path = LOG / f"{args.recipe}-driver.log"
    with log_path.open("w") as output:
        process = subprocess.Popen(
            ["just", args.recipe], cwd=ROOT, stdout=output, stderr=subprocess.STDOUT
        )
        owned = set()
        status = None
        try:
            deadline = time.monotonic() + args.timeout
            while time.monotonic() < deadline:
                owned |= clients()
                if result.exists() and result.stat().st_size:
                    text = result.read_text()
                    if text.startswith("PASS") or text.startswith("FAIL"):
                        status = text
                        break
                time.sleep(0.2)
            if status is None:
                raise RuntimeError(f"probe produced no result within {args.timeout}s")
            (LOG / f"{args.recipe}.txt").write_text(status)
            print(status, flush=True)
        finally:
            marker.unlink(missing_ok=True)
            # it running blocks the next run and this session.
            for pid in owned:
                if pid in clients():
                    os.kill(pid, signal.SIGKILL)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
            end = time.monotonic() + 5
            while clients() and time.monotonic() < end:
                time.sleep(0.1)
            if clients():
                raise RuntimeError("emulator did not terminate")

    return 0 if status and status.startswith("PASS") else 1


if __name__ == "__main__":
    sys.exit(main())
