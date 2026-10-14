# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Drive the interactive Anki 3DS application through Azahar's HID.

The application is interactive, so unlike the collection probes it writes no
result file. This driver launches it, watches the event log it appends to on
SD, injects the documented keyboard mappings with System Events, and always
terminates the emulator it started, pass or fail. It never leaves one running.

Steps are a semicolon-separated list:

    wait:TEXT[:SECONDS]   wait for new event-log text
    key:CHAR              send one mapped key
    sleep:SECONDS         pause

Usage:
  python3 3ds/app/drive.py --steps "wait:DECKS;key:a;wait:QUESTION;key:a;wait:ANSWER;key:a"
  python3 3ds/app/drive.py --reset --reviews 4 --ratings 3,3,1,2
"""

import argparse
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SD = ROOT / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"
EVENTS = SD / "prototype-events.txt"
ARCHIVE = ROOT / "out/3ds/app/archived"
PROCESS = "azahar"


def azahar_pids():
    """PIDs of Azahar processes, which are the only ones this driver kills."""
    found = set()
    for line in subprocess.check_output(
        ["ps", "-ax", "-o", "pid=,command="], text=True
    ).splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[1].endswith("/Contents/MacOS/azahar"):
            found.add(int(fields[0]))
    return found


def kill_azahar(pids):
    for pid in pids:
        if pid in azahar_pids():
            os.kill(pid, signal.SIGKILL)
    deadline = time.monotonic() + 5
    while azahar_pids() and time.monotonic() < deadline:
        time.sleep(0.1)


def reset_fixture():
    """Archive the current disposable collection and build a fresh one."""
    ARCHIVE.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    for path in (
        SD / "prototype.anki2",
        SD / "prototype-events.txt",
        ROOT / "out/3ds/app/fixture/prototype.anki2",
    ):
        if path.exists():
            target = ARCHIVE / f"{stamp}-{path.name}"
            path.rename(target)
            print(f"archived {path} -> {target}")
    for suffix in ("-wal", "-shm"):
        (SD / f"prototype.anki2{suffix}").unlink(missing_ok=True)
    lock = SD / "prototype.anki2.lock"
    if lock.exists():
        for entry in lock.iterdir():
            entry.unlink()
        lock.rmdir()
    subprocess.run(["just", "old3ds-app-fixture"], cwd=ROOT, check=True)


def send_key(char):
    """Send one key to Azahar, bringing it to the front first."""
    script = (
        'tell application "System Events"\n'
        '  set proc to (first process whose name is "Azahar" or name is "azahar")\n'
        "  set frontmost of proc to true\n"
        "  delay 0.05\n"
        f'  keystroke "{char}"\n'
        "end tell"
    )
    subprocess.run(
        ["osascript", "-e", script], check=True, capture_output=True, text=True
    )


def events_from(offset):
    if not EVENTS.exists():
        return "", offset
    data = EVENTS.read_bytes()
    return data[offset:].decode(errors="replace"), len(data)


def wait_for(text, offset, timeout):
    """Wait for `text` to appear in event-log content written after `offset`."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        fresh, offset = events_from(offset)
        if text in fresh:
            return offset
        if not azahar_pids():
            raise RuntimeError(f"emulator exited while waiting for {text!r}")
        time.sleep(0.2)
    raise RuntimeError(f"no {text!r} within {timeout}s")


def run_steps(steps, offset, timeout):
    for step in steps:
        kind, _, argument = step.partition(":")
        if kind == "wait":
            text, _, seconds = argument.partition(":")
            offset = wait_for(text, offset, float(seconds) if seconds else timeout)
        elif kind == "key":
            send_key(argument)
            time.sleep(0.4)
        elif kind == "sleep":
            time.sleep(float(argument))
        else:
            raise SystemExit(f"Unknown step {step!r}")
    return offset


def review_steps(count, ratings):
    """Select the first deck and answer `count` cards with the given ratings."""
    steps = ["wait:DECKS", "key:a"]
    for index in range(count):
        steps += ["wait:QUESTION", "key:a", "wait:ANSWER"]
        steps.append("key:" + "xsaw"[ratings[index % len(ratings)]])
    steps += ["wait:DECKS", "key:m", "wait:CLOSE_OK"]
    return steps


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--steps", help="Semicolon-separated step list")
    parser.add_argument("--reviews", type=int, help="Answer this many cards")
    parser.add_argument("--ratings", default="3", help="Comma-separated ratings 0-3")
    parser.add_argument("--reset", action="store_true", help="Rebuild the fixture")
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    if bool(args.steps) == bool(args.reviews):
        raise SystemExit("Choose exactly one of --steps or --reviews")
    if azahar_pids():
        raise SystemExit("An Azahar emulator is already running; close it first")

    if args.reset:
        reset_fixture()
    else:
        # If the emulator crashed or was killed, clear stale lock before next launch
        lock = SD / "prototype.anki2.lock"
        if lock.exists():
            for entry in lock.iterdir():
                entry.unlink()
            lock.rmdir()

    if args.steps:
        steps = [step for step in args.steps.split(";") if step]
    else:
        ratings = [int(value) for value in args.ratings.split(",")]
        steps = review_steps(args.reviews, ratings)

    EVENTS.parent.mkdir(parents=True, exist_ok=True)
    _, offset = events_from(0)
    log = (ROOT / "out/3ds/logs/app-drive.log").open("w")
    process = subprocess.Popen(
        ["just", "old3ds-app-emulator-run"],
        cwd=ROOT,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    owned = set()
    try:
        deadline = time.monotonic() + args.timeout
        startup_log = ""
        while time.monotonic() < deadline:
            owned |= azahar_pids()
            fresh, offset = events_from(offset)
            startup_log += fresh
            if "DECKS " in startup_log:
                if steps and steps[0] == "wait:DECKS":
                    steps.pop(0)
                break
            if "OPEN " in startup_log and not steps:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("the application did not open its collection")
        run_steps(steps, offset, args.timeout)
        print("steps completed")
    finally:
        owned |= azahar_pids()
        kill_azahar(owned)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
        log.close()
        if azahar_pids():
            raise RuntimeError("emulator did not terminate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
