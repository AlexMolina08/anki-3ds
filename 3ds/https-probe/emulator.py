# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Run only this probe in a private Old 3DS emulator profile; retain native evidence."""
import os
import argparse
import re
import ipaddress
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/https-probe"
PROFILE = OUT / "emulator"
APP = Path.home() / "Applications/azahar-macos-arm64-2126.2/Azahar.app"
REPORT = PROFILE / "data/azahar-emu/sdmc/3ds/anki-https-probe/report.txt"

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--hold",action="store_true",help="Wait for physical START input through emulator UI and native cleanup evidence")
    parser.add_argument("--label",default="emulator",choices=("emulator","cancel"))
    args=parser.parse_args()
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
    REPORT.unlink(missing_ok=True)
    dns_config = subprocess.check_output(["scutil", "--dns"], text=True)
    servers = re.findall(r"nameserver\[\d+\] : ([0-9.]+)\n", dns_config)
    if not servers:
        raise SystemExit("No host-configured IPv4 DNS available for explicit emulator override")
    server = str(ipaddress.IPv4Address(servers[0]))
    REPORT.parent.joinpath("dns-server.txt").write_text(server + "\n")
    OUT.joinpath("emulator-dns.txt").write_text("Explicit host DNS override: " + server + "\n")
    env = os.environ.copy()
    for kind in ("DATA", "CONFIG", "CACHE"):
        env[f"XDG_{kind}_HOME"] = str((PROFILE / kind.lower()).resolve())
    log = OUT / "emulator-console.txt"
    process = subprocess.Popen([
        "open", "-W", "-a", str(APP),
        "--env", f"XDG_DATA_HOME={env['XDG_DATA_HOME']}",
        "--env", f"XDG_CONFIG_HOME={env['XDG_CONFIG_HOME']}",
        "--env", f"XDG_CACHE_HOME={env['XDG_CACHE_HOME']}",
        "--args", str(OUT / "anki-https-probe.3dsx")
    ])
    try:
        deadline = time.monotonic() + 240
        while time.monotonic() < deadline:
            if REPORT.exists() and "complete=" in REPORT.read_text():
                time.sleep(1)
                shutil.copy2(REPORT, OUT / f"{args.label}-report.txt")
                print(REPORT.read_text(), flush=True)
                try:
                    import sys
                    sys.path.insert(0, str(APP.parent / "scripting"))
                    from AzaharRPC import AzaharRPC
                    AzaharRPC(timeout=2, retries=0).save_screenshot(str(OUT / f"{args.label}-screen.png"))
                except Exception as error:
                    print(f"Screenshot unavailable: {error}")
                if args.hold:
                    print("Waiting for START input and native cleanup",flush=True)
                    end=time.monotonic()+120
                    while time.monotonic()<end:
                        if "cleanup=1" in REPORT.read_text():
                            shutil.copy2(REPORT,OUT / f"{args.label}-report.txt")
                            print("Native cleanup verified",flush=True)
                            break
                        time.sleep(.2)
                    else:
                        raise RuntimeError("START/cleanup was not observed")
                break
            if process.poll() is not None:
                raise RuntimeError(f"Azahar exited: {process.returncode}; see {log}")
            time.sleep(.2)
        else:
            raise RuntimeError("Native probe produced no completed report within 240 seconds")
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
        subprocess.run(["pkill", "-9", "-f", "Azahar"], capture_output=True)
if __name__ == "__main__":
    main()
