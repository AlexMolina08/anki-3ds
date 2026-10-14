# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Run every collected upstream test, restarting after an abort or timeout.

A panic-abort binary cannot catch assertion failures. Persist START/PASS records
on SD, preserve the panic text, and run remaining names in a fresh process.
No failed function is retried or silently excluded. Only harness-owned emulator
processes are terminated. Never modify the existing collection fixture.
"""

import argparse
import hashlib
import json
import os
import signal
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SD = ROOT / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds"
OUT = ROOT / "out/3ds/testing"


def clients():
    result = set()
    for line in subprocess.check_output(
        ["ps", "-ax", "-o", "pid=,command="], text=True
    ).splitlines():
        fields = line.split()
        if len(fields) > 1 and fields[1].endswith("/Contents/MacOS/azahar"):
            result.add(int(fields[0]))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument(
        "--filter", help="Only names containing this string; exclusions remain explicit"
    )
    parser.add_argument(
        "--failed-from",
        type=Path,
        help="Rerun failed core functions from an earlier report",
    )
    parser.add_argument("--scenario", choices=["differential", "filesystem"])
    parser.add_argument("--label", default="emulator")
    args = parser.parse_args()
    if not args.label.replace("-", "").replace("_", "").isalnum():
        raise SystemExit("Invalid report label")
    if clients():
        raise SystemExit("Azahar already running; refuse to own an existing process")
    (OUT / f"{args.label}-results.json").unlink(missing_ok=True)
    SD.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    marker = SD / "upstream-filter.txt"
    result_path = SD / "upstream-tests.txt"
    scenario_marker = SD / f"{args.scenario}-mode" if args.scenario else None
    if scenario_marker:
        scenario_marker.write_text("")
        scenario_output = SD / (
            "differential.json"
            if args.scenario == "differential"
            else "filesystem-diagnostics.json"
        )
        scenario_output.unlink(missing_ok=True)
    discovered = None
    outcomes = {}
    runs = []
    remaining = None
    selected = None
    if args.filter or args.failed_from:
        prior = json.loads((OUT / "emulator-baseline-results.json").read_text())
        discovered = sorted(prior["outcomes"])
        if args.failed_from:
            if args.filter:
                raise SystemExit("Choose --filter or --failed-from")
            report = json.loads(args.failed_from.read_text())
            report = report.get("core", report)
            remaining = sorted(
                n
                for n, result in report["outcomes"].items()
                if result["status"] in ["failed", "blocked"]
            )
            if not set(remaining) <= set(discovered):
                raise SystemExit("Report contains unknown core names")
        else:
            remaining = [n for n in discovered if args.filter in n]
        if not remaining:
            raise SystemExit("Filter matched no discovered tests")
        selected = set(remaining)
    try:
        while remaining is None or remaining:
            index = len(runs) + 1
            result_path.unlink(missing_ok=True)
            marker.write_text("\n".join(remaining or []) + ("\n" if remaining else ""))
            owned = set()
            launcher = OUT / f"{args.label}-{index:03d}-launcher.log"
            binary_sha256 = hashlib.sha256(
                (OUT / "upstream-tests.3dsx").read_bytes()
            ).hexdigest()
            with launcher.open("w") as log:
                proc = subprocess.Popen(
                    ["just", "old3ds-upstream-tests-emulator-run"],
                    cwd=ROOT,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                )
                text = ""
                deadline = time.monotonic() + args.timeout
                try:
                    while time.monotonic() < deadline:
                        owned |= clients()
                        latest = result_path.read_text() if result_path.exists() else ""
                        if latest != text:
                            deadline = time.monotonic() + args.timeout
                            text = latest
                        if any(
                            line.startswith(("FAIL ", "COMPLETE "))
                            for line in text.splitlines()
                        ):
                            break
                        if proc.poll() is not None and not clients():
                            break
                        time.sleep(0.2)
                finally:
                    owned |= clients()
                    for pid in owned:
                        if pid in clients():
                            os.kill(pid, signal.SIGKILL)
                    try:
                        proc.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()
                    end = time.monotonic() + 5
                    while clients() and time.monotonic() < end:
                        time.sleep(0.1)
                    if clients():
                        raise RuntimeError("Owned emulator failed to terminate")
            (OUT / f"{args.label}-{index:03d}.txt").write_text(text)
            runtime = ROOT / "out/3ds/emulator/data/azahar-emu/log/azahar_log.txt"
            runtime_text = runtime.read_text() if runtime.exists() else ""
            (OUT / f"{args.label}-{index:03d}-runtime.log").write_text(runtime_text)
            if "System_IsNew3ds: false" not in runtime_text:
                raise RuntimeError("Runtime log did not confirm Old 3DS mode")
            if args.scenario:
                if (
                    any(line.startswith("FAIL ") for line in text.splitlines())
                    or not scenario_output.exists()
                ):
                    raise RuntimeError("Port-specific scenario failed: " + text)
                (OUT / scenario_output.name).write_text(scenario_output.read_text())
                print(
                    f"{args.scenario}: completed in verified Old 3DS mode", flush=True
                )
                return 0
            listed = [
                line[5:] for line in text.splitlines() if line.startswith("LIST ")
            ]
            if not listed:
                raise RuntimeError("Test executable did not produce discovery records")
            if discovered is None:
                discovered = listed
            else:
                assert discovered == listed, (
                    "Test registration changed between launches"
                )
            active = None
            for line in text.splitlines():
                if line.startswith("START "):
                    active = line[6:]
                elif line.startswith("PASS "):
                    name = line[5:]
                    assert name == active and name not in outcomes
                    outcomes[name] = {"status": "passed", "run": index}
                    active = None
            if active:
                reason = (
                    text.split("\nFAIL ", 1)[1]
                    if "\nFAIL " in text
                    else "No completion within progress timeout; hang, abort, or emulator failure unresolved"
                )
                outcomes[active] = {
                    "status": "failed" if "\nFAIL " in text else "blocked",
                    "reason": reason,
                    "run": index,
                }
                print(
                    f"{outcomes[active]['status']}: {active}: {reason.splitlines()[0]}",
                    flush=True,
                )
            runs.append(
                {
                    "index": index,
                    "completed": "\nCOMPLETE " in text,
                    "old3ds": True,
                    "binary_sha256": binary_sha256,
                }
            )
            if not active and "\nCOMPLETE " not in text:
                raise RuntimeError(
                    "Run stopped without identifying a test; do not attribute remaining tests"
                )
            remaining = [
                n
                for n in discovered
                if n not in outcomes and (selected is None or n in selected)
            ]
            summary = {
                "discovered": len(discovered),
                "outcomes": outcomes,
                "runs": runs,
                "remaining": remaining,
            }
            (OUT / f"{args.label}-results.json").write_text(
                json.dumps(summary, indent=2) + "\n"
            )
            print(
                f"Progress: {len(outcomes)}/{len(discovered)} tests accounted for",
                flush=True,
            )
    finally:
        marker.unlink(missing_ok=True)
        if scenario_marker:
            scenario_marker.unlink(missing_ok=True)
    failures = [n for n, v in outcomes.items() if v["status"] != "passed"]
    print(
        f"{len(outcomes) - len(failures)} passed; {len(failures)} failed/blocked; {len(discovered) - len(outcomes)} untested"
    )
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
