# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Restart/crash tests against disposable SQLite files in isolated Azahar SD.

Never runs against personal collections. Refuses to start while this ROM is
already running. Each run owns its emulator PID; abrupt termination is deliberate.
"""
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
ROM = ROOT / "out/3ds/sqlite-target/armv6k-nintendo-3ds/release/anki-3ds-sqlite-probe.3dsx"
SD = ROOT / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"
LOG = ROOT / "out/3ds/logs"


def clients():
    result = subprocess.check_output(["ps", "-ax", "-o", "pid=,command="], text=True)
    found = set()
    for line in result.splitlines():
        fields = line.split()
        if len(fields) >= 3 and fields[1].endswith("/Contents/MacOS/azahar") and str(ROM) in fields[2:]:
            found.add(int(fields[0]))
    return found


def launch(label, expect_pass=True):
    if clients():
        raise RuntimeError("Close the SQLite probe emulator before validation")
    result = SD / "result.txt"
    result.unlink(missing_ok=True)
    with (LOG / f"sqlite-{label}-launcher.log").open("w") as output:
        process = subprocess.Popen(["just", "old3ds-sqlite-emulator-run"], cwd=ROOT,
                                   stdout=output, stderr=subprocess.STDOUT)
        owned = set()
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                owned |= clients()
                if result.exists() and result.stat().st_size:
                    status = result.read_text()
                    if status.startswith("PASS") or status.startswith("FAIL"):
                        break
                time.sleep(0.2)
            else:
                raise RuntimeError("Probe produced no result within 30 seconds")
            if expect_pass != status.startswith("PASS"):
                raise RuntimeError(status)
            (LOG / f"sqlite-{label}.txt").write_text(status)
            runtime = ROOT / "out/3ds/emulator/data/azahar-emu/log/azahar_log.txt"
            log = runtime.read_text()
            if "System_IsNew3ds: false" not in log:
                raise RuntimeError("Old 3DS runtime mode was not verified")
            (LOG / f"sqlite-{label}-runtime.log").write_text(log)
            print(f"{label}: {status.strip()}", flush=True)
            return status
        finally:
            for pid in owned:
                if pid in clients():
                    os.kill(pid, signal.SIGKILL)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
            deadline = time.monotonic() + 5
            while clients() and time.monotonic() < deadline:
                time.sleep(0.1)
            if clients():
                raise RuntimeError("Emulator did not terminate")


def counter(status, field):
    return int(re.search(rf"{field}: (\d+)", status)[1])


def main():
    LOG.mkdir(parents=True, exist_ok=True)
    SD.mkdir(parents=True, exist_ok=True)
    if (SD / "crash-request").exists() or (SD / "probe.sqlite.lock").exists():
        raise RuntimeError("Existing crash request or lock requires offline inspection")
    first = launch("restart-first")
    second = launch("restart-second")
    assert counter(second, "Previous launches") == counter(first, "Committed launches")
    (SD / "crash-request").write_text("Disposable crash test\n")
    crashed = launch("crash-armed")
    assert "Crash recovery armed: true" in crashed
    assert (SD / "probe.sqlite-wal").stat().st_size > 200_000
    assert (SD / "probe.sqlite.lock").is_dir()
    (SD / "crash-request").unlink()
    locked = launch("stale-lock", expect_pass=False)
    assert "locked" in locked
    # Explicit offline stale-lock recovery: never deletes a live client's lock.
    assert not clients()
    (SD / "probe.sqlite.lock").rmdir()
    recovered = launch("crash-recovered")
    assert counter(recovered, "Previous launches") == counter(crashed, "Committed launches")
    assert counter(recovered, "Previous launches") != 999999
    collection_sql = "Anki schema 18: note/card SQL PASS" in first
    if collection_sql:
        assert 'Prior fixture tags: " old3ds_probe "' in second
        assert "Anki schema 18: note/card SQL PASS" in recovered
    summary = {"anki_collection_sql": collection_sql, "restart_persistence": True, "committed_wal_recovery": True,
               "uncommitted_rollback": True, "stale_lock_rejected": True,
               "physical_hardware": False}
    (LOG / "sqlite-validation.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
