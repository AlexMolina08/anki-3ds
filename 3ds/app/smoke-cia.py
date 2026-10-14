# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Install and open a CIA in a disposable Old 3DS emulator profile."""

import argparse
import hashlib
import json
import os
import signal
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AZAHAR = Path(
    os.environ.get(
        "ANKI_3DS_AZAHAR",
        str(
            Path.home()
            / "Applications/azahar-macos-arm64-2126.2/Azahar.app/Contents/MacOS/azahar"
        ),
    )
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("label")
    args = parser.parse_args()
    out = ROOT / "out/3ds/cia-smoke" / args.label
    out.mkdir(parents=True, exist_ok=False)
    if (
        subprocess.run(
            ["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"],
            capture_output=True,
            check=False,
        ).returncode
        == 0
    ):
        raise SystemExit("Close the existing emulator before this isolated check")
    config = out / "config/azahar-emu"
    config.mkdir(parents=True)
    (config / "qt-config.ini").write_text(
        "[System]\nis_new_3ds=false\nis_new_3ds\\default=false\n[Debugging]\nenable_rpc_server=true\nenable_rpc_server\\default=false\n"
    )
    cia = ROOT / "out/3ds/app/anki-3ds.cia"
    env = os.environ.copy()
    for kind in ["DATA", "CONFIG", "CACHE"]:
        env[f"XDG_{kind}_HOME"] = str(out / kind.lower())

    def stop(process):
        pids = subprocess.run(
            ["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"],
            capture_output=True,
            text=True,
            check=False,
        ).stdout.split()
        for pid in pids:
            try:
                os.kill(int(pid), signal.SIGTERM)
            except ProcessLookupError:
                pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            for pid in pids:
                try:
                    os.kill(int(pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass
            process.terminate()
            process.wait(timeout=5)

    def launch(arguments, log):
        return subprocess.Popen(
            [
                "open",
                "-W",
                "-n",
                "-a",
                str(AZAHAR.parents[2]),
                *sum(
                    (
                        ["--env", f"XDG_{kind}_HOME={env[f'XDG_{kind}_HOME']}"]
                        for kind in ["DATA", "CONFIG", "CACHE"]
                    ),
                    [],
                ),
                "--args",
                *arguments,
            ],
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
        )

    with (out / "install.log").open("w") as log:
        process = launch(["--install", str(cia)], log)
        try:
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                titles = list(
                    (out / "data").glob("**/title/00040000/0f3a1000/content/*.app")
                )
                if titles:
                    time.sleep(2)
                    break
                time.sleep(0.25)
            else:
                raise RuntimeError(
                    "CIA installation did not produce an installed title"
                )
        finally:
            stop(process)
    with (out / "launch.log").open("w") as log:
        process = launch([str(titles[0])], log)
        try:
            events = out / "data/azahar-emu/sdmc/3ds/anki-3ds/prototype-events.txt"
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                text = events.read_text(errors="replace") if events.exists() else ""
                if any(marker in text for marker in ["OPEN_ERROR", "ACTION_ERROR"]):
                    raise RuntimeError(text)
                if "DECKS screen" in text:
                    (out / "events.txt").write_text(text)
                    (out / "verification.json").write_text(
                        json.dumps(
                            {
                                "cia_sha256": hashlib.sha256(
                                    cia.read_bytes()
                                ).hexdigest(),
                                "installed_title_id": "000400000f3a1000",
                                "old_3ds_emulation": True,
                                "opened_collection": True,
                                "physical_hardware_tested": False,
                            },
                            indent=2,
                        )
                        + "\n"
                    )
                    print(
                        "PASS: CIA installed and reached the deck screen in Old 3DS emulation"
                    )
                    return
                time.sleep(0.25)
            raise RuntimeError("Installed CIA did not reach the deck screen")
        finally:
            stop(process)


if __name__ == "__main__":
    main()
