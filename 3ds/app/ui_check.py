# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Exercise native UI against a private fixture; capture actual emulator frames.

Never modifies the regular emulator SD or kills an existing emulator. The
script sends the same application actions as buttons; it does not verify HID
or the system keyboard. Those require a separate interactive check.
"""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/ui-check"
FIXTURE = ROOT / "out/3ds/app/fixture/prototype.anki2"
AZAHAR = (
    Path.home()
    / "Applications/azahar-macos-arm64-2126.2/Azahar.app/Contents/MacOS/azahar"
)
sys.path.insert(0, str(AZAHAR.parents[3] / "scripting"))
import AzaharRPC


def main():
    for line in subprocess.check_output(
        ["ps", "-ax", "-o", "command="], text=True
    ).splitlines():
        if line.split() and line.split()[0] == str(AZAHAR):
            raise SystemExit("An emulator is running; refusing concurrent launch")
    if OUT.exists():
        raise SystemExit(f"Refusing to overwrite {OUT}; archive prior run first")
    sd = OUT / "data/azahar-emu/sdmc/3ds/anki-3ds"
    sd.mkdir(parents=True)
    shutil.copyfile(FIXTURE, sd / "prototype.anki2")
    (sd / "utc-offset-minutes.txt").write_text("0\n")
    baseline = hashlib.sha256(FIXTURE.read_bytes()).hexdigest()
    steps = [
        "SELECT 0",
        "STUDY",
        "REVEAL",
        "LANGUAGE",
        "LANGUAGE",
        "ANSWER 2",
        "REVEAL",
        "ANSWER 0",
        "REVEAL",
        "ANSWER 1",
        "REVEAL",
        "ANSWER 3",
        "BACK",
        "ADD",
        "FIELD 0 UI_CHECK Pregunta con acentos: á é í ó ú ñ ¿Qué?",
        "FIELD 1 Respuesta inferior. "
        + ("Un texto largo para comprobar el desplazamiento. " * 14),
        "LANGUAGE",
        "SAVE",
        "BROWSE",
        "SEARCH UI_CHECK",
        "EDIT",
        "FIELD 1 UI_CHECK respuesta editada y persistida.",
        "LANGUAGE",
        "SAVE",
        "BACK",
        "OPTIONS",
        "LIMITS 7 77",
        "SAVE",
        "OPTIONS",
        "BACK",
        "STATS",
        "BACK",
        "HELP",
        "DOWN",
        "BACK",
        "MENU",
        "DOWN",
        "BACK",
        "EXIT",
    ]
    (sd / "test-script.txt").write_text("\n".join(steps) + "\n")
    config = OUT / "config/azahar-emu"
    config.mkdir(parents=True)
    (config / "qt-config.ini").write_text("""[System]
is_new_3ds=false
is_new_3ds\\default=false
[Core]
cpu_clock_percentage=100
cpu_clock_percentage\\default=false
[Debugging]
enable_rpc_server=true
enable_rpc_server\\default=false
""")
    env = os.environ.copy()
    env["TZ"] = "UTC"
    for key, folder in [
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
    ]:
        env[key] = str(OUT / folder)
    shots = OUT / "shots"
    shots.mkdir()
    events = sd / "prototype-events.txt"
    rpc = None
    captured = set()
    perf = []
    triggers = {
        "DECKS screen": "decks",
        "QUESTION screen": "question",
        "ANSWER screen": "answer",
        "EDITOR screen": "editor",
        "BROWSER screen": "browser",
        "OPTIONS screen": "options",
        "STATS": "statistics",
        "HELP": "help",
        "SYNC": "sync",
        "MENU screen": "menu",
        "SEARCH UI_CHECK": "search",
        "EDIT\n": "edit",
    }
    with (OUT / "emulator.log").open("w") as log:
        proc = subprocess.Popen(
            [
                "open",
                "-W",
                "-n",
                "-a",
                str(AZAHAR.parents[2]),
                *sum(
                    (
                        ["--env", f"{k}={env[k]}"]
                        for k in (
                            "XDG_DATA_HOME",
                            "XDG_CONFIG_HOME",
                            "XDG_CACHE_HOME",
                            "TZ",
                        )
                    ),
                    [],
                ),
                "--args",
                str(ROOT / "out/3ds/app/anki-3ds.3dsx"),
            ],
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 170
            while time.monotonic() < deadline:
                text = events.read_text(errors="replace") if events.exists() else ""
                if (
                    "OPEN_ERROR" in text
                    or "ACTION_ERROR" in text
                    or "CLOSE_ERROR" in text
                ):
                    raise RuntimeError(text)
                if rpc is None and "OPEN " in text:
                    rpc = AzaharRPC.AzaharRPC(timeout=5)
                if rpc:
                    for trigger, label in triggers.items():
                        if trigger in text and label not in captured:
                            time.sleep(0.15)
                            path = shots / f"{label}.png"
                            if not rpc.save_screenshot(str(path), res_scale=1):
                                raise RuntimeError(f"capture failed: {label}")
                            captured.add(label)
                            print(f"captured {label}", flush=True)
                    if len(perf) < 5 and "ANSWER screen" in text:
                        perf.append(rpc.perf_stats())
                if "CLOSE_OK" in text:
                    break
                if proc.poll() is not None:
                    raise RuntimeError(f"emulator exited: {proc.returncode}")
                time.sleep(0.15)
            else:
                raise RuntimeError("UI run timed out")
        finally:
            import signal

            pids = subprocess.run(
                ["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"],
                capture_output=True,
                text=True,
                check=False,
            ).stdout.split()
            for pid in pids:
                try:
                    os.kill(int(pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)
    assert hashlib.sha256(FIXTURE.read_bytes()).hexdigest() == baseline
    text = events.read_text()
    assert text.count("REVIEW card=") == 4
    assert text.count("NOTE_SAVED") == 2
    assert text.count("OPTIONS_SAVED") == 1
    assert "CLOSE_OK" in text
    (OUT / "run.json").write_text(
        json.dumps(
            {
                "captures": sorted(captured),
                "perf": perf,
                "fixture_sha256_unchanged": baseline,
                "old3ds": True,
            },
            indent=2,
        )
        + "\n"
    )
    print("Native UI actions passed; source fixture unchanged.", flush=True)


if __name__ == "__main__":
    main()
