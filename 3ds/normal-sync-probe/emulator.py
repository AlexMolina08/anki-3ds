# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Run sync-probe in isolated Azahar emulator environment and verify output."""
import os
import argparse
import re
import ipaddress
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/normal-sync-probe"
PROFILE = OUT / "emulator"
APP = Path.home() / "Applications/azahar-macos-arm64-2126.2/Azahar.app"
REPORT = PROFILE / "data/azahar-emu/sdmc/3ds/anki-normal-sync-probe/report.txt"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--label", default="emulator")
    args = parser.parse_args()

    if subprocess.run(["pgrep", "-f", str(APP / "Contents/MacOS/azahar")], capture_output=True).returncode == 0:
        raise SystemExit("Close existing Azahar before running this isolated probe")

    config = PROFILE / "config/azahar-emu"
    config.mkdir(parents=True, exist_ok=True)
    config.joinpath("qt-config.ini").write_text(
        "[System]\nis_new_3ds=false\nis_new_3ds\\default=false\n"
        "[Core]\ncpu_clock_percentage=100\ncpu_clock_percentage\\default=false\n"
        "[Debugging]\nenable_rpc_server=true\nenable_rpc_server\\default=false\n"
        "[Miscellaneous]\nlog_filter=*:Info Service.SOC:Debug\nlog_filter\\default=false\n")

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "out/3ds/https-sources/cacert.pem", REPORT.parent / "cacert.pem")
    sdmc = PROFILE / "data/azahar-emu/sdmc/3ds"
    for d in ("anki-3ds", "anki-https-probe"):
        (sdmc / d).mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / "out/3ds/https-sources/cacert.pem", sdmc / d / "cacert.pem")

    fixture_root = sdmc / "anki-3ds/phase4-v1"
    if fixture_root.exists():
        raise SystemExit("Refusing to overwrite native results; retain or move the isolated profile first")
    fixture_root.mkdir(parents=True)
    shutil.copytree(OUT / "fixtures", fixture_root / "fixtures")

    REPORT.unlink(missing_ok=True)

    dns_config = subprocess.check_output(["scutil", "--dns"], text=True)
    servers = re.findall(r"nameserver\[\d+\] : ([0-9.]+)\n", dns_config)
    if not servers:
        raise SystemExit("No host-configured IPv4 DNS available for explicit emulator override")
    server = str(ipaddress.IPv4Address(servers[0]))
    (sdmc / "anki-https-probe/dns-server.txt").write_text(server + "\n")

    env = os.environ.copy()
    for kind in ("DATA", "CONFIG", "CACHE"):
        env[f"XDG_{kind}_HOME"] = str((PROFILE / kind.lower()).resolve())

    process = subprocess.Popen([
        "open", "-W", "-a", str(APP),
        "--env", f"XDG_DATA_HOME={env['XDG_DATA_HOME']}",
        "--env", f"XDG_CONFIG_HOME={env['XDG_CONFIG_HOME']}",
        "--env", f"XDG_CACHE_HOME={env['XDG_CACHE_HOME']}",
        "--args", str(OUT / "anki-3ds-normal-sync-probe.3dsx")
    ])

    try:
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            if REPORT.exists() and "complete=1" in REPORT.read_text():
                time.sleep(1)
                shutil.copy2(REPORT, OUT / f"{args.label}-report.txt")
                print(REPORT.read_text(), flush=True)
                if "complete=1 result=PASS" not in REPORT.read_text():
                    raise RuntimeError("Native probe failed; report retained")
                shutil.copy2(fixture_root / "native.json", OUT / "native.json")
                break
            if process.poll() is not None:
                raise RuntimeError(f"Azahar exited prematurely: {process.returncode}")
            time.sleep(0.5)
        else:
            raise RuntimeError("Sync probe produced no completed report within 180 seconds")
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
        subprocess.run(["pkill", "-KILL", "-f", str(APP / "Contents/MacOS/azahar")], capture_output=True)

if __name__ == "__main__":
    main()
