# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#!/usr/bin/env python3
"""Run automated native UI test with Azahar in Old 3DS mode and capture screenshots."""

import os
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SD = ROOT / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"
LOG = ROOT / "out/3ds/logs"
SCREENSHOTS = ROOT / "3ds/docs/screenshots"

AZAHAR_APP = Path.home() / "Applications/azahar-macos-arm64-2126.2/Azahar.app"
AZAHAR_BIN = AZAHAR_APP / "Contents/MacOS/azahar"
APP_3DSX = ROOT / "out/3ds/app/anki-3ds.3dsx"
EVENTS = SD / "prototype-events.txt"
SCRIPT_FILE = SD / "test-script.txt"

sys.path.insert(
    0, str(Path.home() / "Applications/azahar-macos-arm64-2126.2/scripting")
)
import AzaharRPC


def clients():
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
        if pid in clients():
            try:
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass
    deadline = time.monotonic() + 5
    while clients() and time.monotonic() < deadline:
        time.sleep(0.1)


def setup_fixture():
    # Remove old lock
    lock = SD / "prototype.anki2.lock"
    if lock.exists():
        for entry in lock.iterdir():
            entry.unlink()
        lock.rmdir()
    for suffix in ("-wal", "-shm"):
        (SD / f"prototype.anki2{suffix}").unlink(missing_ok=True)
    (SD / "prototype.anki2").unlink(missing_ok=True)
    (ROOT / "out/3ds/app/fixture/prototype.anki2").unlink(missing_ok=True)
    EVENTS.unlink(missing_ok=True)

    print("Generating fresh fixture...")
    subprocess.run(["just", "old3ds-app-fixture"], cwd=ROOT, check=True)


def main():
    if not AZAHAR_BIN.exists():
        raise SystemExit(f"Azahar binary missing at {AZAHAR_BIN}")
    if not APP_3DSX.exists():
        raise SystemExit(f"App 3dsx missing at {APP_3DSX}")

    kill_azahar(clients())
    setup_fixture()

    LOG.mkdir(parents=True, exist_ok=True)
    SCREENSHOTS.mkdir(parents=True, exist_ok=True)

    config_dir = ROOT / "out/3ds/emulator/config/azahar-emu"
    config_dir.mkdir(parents=True, exist_ok=True)
    qt_config = config_dir / "qt-config.ini"
    qt_config.write_text("""[System]
is_new_3ds=false
is_new_3ds\\default=false
[Miscellaneous]
log_filter=*:Info Service.FS:Debug
log_filter\\default=false
[Core]
cpu_clock_percentage=100
cpu_clock_percentage\\default=false
[Debug]
enable_rpc_server=true
enable_rpc_server\\default=false
""")

    SCRIPT_FILE.write_text(
        """
SELECT 0
STUDY
REVEAL
ANSWER 2
REVEAL
ANSWER 0
REVEAL
ANSWER 1
REVEAL
ANSWER 3
BACK
EXIT
""".strip()
        + "\n"
    )

    env = os.environ.copy()
    env["XDG_DATA_HOME"] = str(ROOT / "out/3ds/emulator/data")
    env["XDG_CONFIG_HOME"] = str(ROOT / "out/3ds/emulator/config")
    env["XDG_CACHE_HOME"] = str(ROOT / "out/3ds/emulator/cache")

    log_path = LOG / "app-ui-test.log"
    log_file = log_path.open("w")

    print(f"Launching Azahar in Old 3DS mode with {APP_3DSX}...")
    proc = subprocess.Popen(
        [str(AZAHAR_BIN), str(APP_3DSX)],
        env=env,
        stdout=log_file,
        stderr=subprocess.STDOUT,
    )

    owned = {proc.pid}
    rpc = None
    screenshots_taken = 0

    try:
        deadline = time.monotonic() + 35.0
        while time.monotonic() < deadline:
            owned |= clients()
            if EVENTS.exists():
                text = EVENTS.read_text()
                if "OPEN " in text:
                    break
            time.sleep(0.3)
        else:
            raise RuntimeError("App did not open collection within timeout")

        print("App started and collection opened.")

        time.sleep(1.0)
        try:
            rpc = AzaharRPC.AzaharRPC(timeout=5)
            print("Connected to Azahar RPC!")
        except Exception as e:
            print(f"Warning: Could not connect to Azahar RPC: {e}")

        screenshot_stages = {
            "OPEN ": ("deck_browser_initial.png", "Deck Browser Initial"),
            "SELECT deck=": ("card_question.png", "Card Question"),
            "QUESTION -> ANSWER": ("card_answer.png", "Card Answer"),
            "REVIEW card=": ("review_cycle.png", "Review Cycle"),
        }
        captured = set()

        deadline = time.monotonic() + 45.0
        while time.monotonic() < deadline:
            owned |= clients()
            if EVENTS.exists():
                text = EVENTS.read_text()
                for trigger, (filename, label) in screenshot_stages.items():
                    if trigger in text and trigger not in captured and rpc is not None:
                        time.sleep(0.5)
                        shot_path = SCREENSHOTS / filename
                        if rpc.save_screenshot(str(shot_path)):
                            print(f"Captured screenshot: {label} -> {shot_path}")
                            captured.add(trigger)
                            screenshots_taken += 1

                if "CLOSE_OK" in text:
                    print("Normal clean application shutdown detected (CLOSE_OK)!")
                    break
            time.sleep(0.5)

        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass

    finally:
        kill_azahar(owned)
        log_file.close()

    print(f"UI Test finished. Screenshots captured: {screenshots_taken}")
    if EVENTS.exists():
        print("Events recorded:\n" + EVENTS.read_text())

    return 0


if __name__ == "__main__":
    sys.exit(main())
