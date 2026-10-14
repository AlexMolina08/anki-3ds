# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Verify native login, token reuse, rejection recovery and logout in isolation.

Live mode accepts JSON credentials on stdin. It performs hostKey/meta only,
never a collection upload, download, or incremental sync. Secrets are removed
from the isolated SD profile before this process exits; reports contain no keys.
"""

import argparse
import hashlib
import ipaddress
import json
import os
import re
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import threading
import time
from http.server import ThreadingHTTPServer

from app_sync import AZAHAR, ROOT, TLSBoundary, database


class LoginBoundary(TLSBoundary):
    def do_POST(self):
        if self.path.endswith("/meta") and self.server.drop_meta:
            self.close_connection = True
            self.connection.shutdown(socket.SHUT_RDWR)
            self.connection.close()
            with (self.server.evidence / "http.log").open("a") as log:
                log.write("/sync/meta injected disconnect\n")
            return
        super().do_POST()


def run(out, sd, env, binary, name, steps):
    (sd / "test-script.txt").write_text("\n".join(steps) + "\n")
    events = sd / "prototype-events.txt"
    events.unlink(missing_ok=True)
    process = None
    started = time.monotonic()
    try:
        with (out / f"{name}-emulator.log").open("w") as log:
            process = subprocess.Popen(
                [
                    "open",
                    "-W",
                    "-n",
                    "-a",
                    str(AZAHAR.parents[2]),
                    *sum(
                        (
                            ["--env", f"{key}={env[key]}"]
                            for key in (
                                "XDG_DATA_HOME",
                                "XDG_CONFIG_HOME",
                                "XDG_CACHE_HOME",
                                "TZ",
                            )
                        ),
                        [],
                    ),
                    "--args",
                    str(binary.resolve()),
                ],
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            while time.monotonic() - started < 150:
                text = events.read_text(errors="replace") if events.exists() else ""
                if "CLOSE_OK" in text:
                    (out / f"{name}-events.txt").write_text(text)
                    return text
                if process.poll() is not None:
                    raise RuntimeError(f"Emulator exited during {name}")
                time.sleep(0.2)
            raise RuntimeError(f"Native login test timed out during {name}")
    finally:
        pids = subprocess.run(
            ["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"],
            capture_output=True,
            check=False,
            text=True,
        ).stdout.split()
        for pid in pids:
            try:
                os.kill(int(pid), signal.SIGTERM)
            except ProcessLookupError:
                pass
        if process:
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--label", required=True)
    args = parser.parse_args()
    credentials = (
        json.load(sys.stdin)
        if args.live
        else {"username": "dev", "password": "disposable"}
    )
    out = ROOT / "out/3ds/login-check" / args.label
    if out.exists():
        raise SystemExit("Use a new label; previous evidence is preserved")
    if (
        subprocess.run(
            ["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"],
            capture_output=True,
            check=False,
        ).returncode
        == 0
    ):
        raise SystemExit("Close existing Azahar before this isolated test")
    sd = out / "data/azahar-emu/sdmc/3ds/anki-3ds"
    sd.mkdir(parents=True)
    binary = out / "tested.3dsx"
    shutil.copyfile(ROOT / "out/3ds/app/anki-3ds.3dsx", binary)
    shutil.copyfile(
        ROOT / "out/3ds/app/fixture/prototype.anki2", sd / "prototype.anki2"
    )
    shutil.copyfile(ROOT / "out/3ds/https-sources/cacert.pem", sd / "cacert.pem")
    (sd / "language.txt").write_text("en")
    config = out / "config/azahar-emu"
    config.mkdir(parents=True)
    (config / "qt-config.ini").write_text(
        "[System]\nis_new_3ds=false\nis_new_3ds\\default=false\n"
        "[Core]\ncpu_clock_percentage=100\ncpu_clock_percentage\\default=false\n"
    )
    env = os.environ.copy()
    env["TZ"] = "UTC"
    for kind in ("DATA", "CONFIG", "CACHE"):
        env[f"XDG_{kind}_HOME"] = str(out.resolve() / kind.lower())
    server = gateway = None
    results = {}
    try:
        if args.live:
            endpoint = "https://sync.ankiweb.net/"
            ca = "sdmc:/3ds/anki-3ds/cacert.pem"
            dns = subprocess.check_output(["scutil", "--dns"], text=True)
            addresses = re.findall(r"nameserver\[\d+\] : ([0-9.]+)\n", dns)
            if not addresses:
                raise RuntimeError("No host DNS for explicit emulator override")
            dns_path = sd.parent / "anki-https-probe"
            dns_path.mkdir()
            (dns_path / "dns-server.txt").write_text(
                str(ipaddress.IPv4Address(addresses[0]))
            )
        else:
            endpoint = "https://127.0.0.1:18443/"
            ca = "sdmc:/3ds/anki-3ds/test-ca.pem"
            tls = out / "tls"
            tls.mkdir()
            subprocess.run(
                [
                    "openssl",
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-days",
                    "2",
                    "-subj",
                    "/CN=Anki 3DS disposable test",
                    "-addext",
                    "subjectAltName=IP:127.0.0.1",
                    "-keyout",
                    str(tls / "key.pem"),
                    "-out",
                    str(tls / "cert.pem"),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            shutil.copyfile(tls / "cert.pem", sd / "test-ca.pem")
            serverenv = os.environ.copy()
            serverenv.update(
                SYNC_USER1="dev:disposable",
                SYNC_HOST="127.0.0.1",
                SYNC_PORT="18080",
                SYNC_BASE=str(out.resolve() / "server"),
            )
            with (out / "server.log").open("w") as log:
                server = subprocess.Popen(
                    [str(ROOT / "target/debug/anki-sync-server")],
                    env=serverenv,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                )
            gateway = ThreadingHTTPServer(("127.0.0.1", 18443), LoginBoundary)
            gateway.evidence = out
            gateway.fault = None
            gateway.injected = False
            gateway.clock_delay = 0
            gateway.drop_meta = False
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.minimum_version = ssl.TLSVersion.TLSv1_2
            context.load_cert_chain(tls / "cert.pem", tls / "key.pem")
            gateway.socket = context.wrap_socket(gateway.socket, server_side=True)
            threading.Thread(target=gateway.serve_forever, daemon=True).start()
            time.sleep(1)
            assert server.poll() is None

        def configure(with_password):
            (sd / "sync-test.json").write_text(
                json.dumps(
                    {
                        "username": credentials["username"] if with_password else "",
                        "password": credentials["password"] if with_password else "",
                        "endpoint": endpoint,
                        "ca_file": ca,
                    }
                )
            )

        configure(True)
        before = database(sd / "prototype.anki2")
        first = run(
            out, sd, env, binary, "initial", ["LOGIN", "BACK", "LOGIN", "BACK", "EXIT"]
        )
        assert first.count("LOGIN_COMPLETE") == 2, (
            "Login failed; inspect retained events"
        )
        assert first.count("LOGIN_START") == 1, "Repeated login did not reuse token"
        assert (sd / "sync-auth.json").exists()
        results["initial_login_and_in_session_reuse"] = True
        print("PASS native login and in-session token reuse", flush=True)
        configure(False)
        restarted = run(out, sd, env, binary, "restart", ["LOGIN", "BACK", "EXIT"])
        assert "AUTH_LOADED" in restarted and "LOGIN_COMPLETE" in restarted
        assert "LOGIN_START" not in restarted, "Restart requested credentials"
        assert "CLOCK_AUTO_START" not in restarted, (
            "Fresh correction triggered redundant HTTPS request"
        )
        results["restart_without_credentials_or_clock_request"] = True
        print("PASS restart without credentials or clock request", flush=True)
        if not args.live:
            gateway.drop_meta = True
            failed = run(out, sd, env, binary, "disconnect", ["LOGIN", "BACK", "EXIT"])
            assert "SYNC_ERROR" in failed and "LOGIN_COMPLETE" not in failed
            assert list(sd.glob("sync-auth*.json")), "Network error removed login"
            gateway.drop_meta = False
            retried = run(out, sd, env, binary, "retry", ["LOGIN", "BACK", "EXIT"])
            assert "LOGIN_COMPLETE" in retried and "LOGIN_START" not in retried
            results["network_failure_preserves_login"] = True
            print(
                "PASS network error preserves token; retry needs no credentials",
                flush=True,
            )
            for path in sd.glob("sync-auth*.json"):
                data = json.loads(path.read_text())
                data["hkey"] = "deliberately-invalid-test-token"
                path.write_text(json.dumps(data))
            rejected = run(out, sd, env, binary, "rejected", ["LOGIN", "BACK", "EXIT"])
            assert "SYNC_ERROR" in rejected and "LOGIN_COMPLETE" not in rejected
            assert not list(sd.glob("sync-auth*.json")), "Rejected token remained on SD"
            configure(True)
            recovered = run(
                out, sd, env, binary, "recovered", ["LOGIN", "BACK", "EXIT"]
            )
            assert "LOGIN_COMPLETE" in recovered and "LOGIN_START" in recovered
            results["rejected_token_removed_and_relogin_succeeds"] = True
            print("PASS rejected-token cleanup and login recovery", flush=True)
        logged_out = run(out, sd, env, binary, "logout", ["LOGOUT", "BACK", "EXIT"])
        assert "LOGOUT_COMPLETE" in logged_out
        assert not list(sd.glob("sync-auth*.json"))
        assert database(sd / "prototype.anki2") == before, (
            "Login changed collection entities"
        )
        results["logout_and_collection_entities_preserved"] = True
        if not args.live:
            requests = (out / "http.log").read_text()
            assert not any(
                f"/sync/{method} " in requests
                for method in ["start", "upload", "download", "applyChanges", "finish"]
            )
            assert sum(line.startswith("/ ") for line in requests.splitlines()) == 1, (
                "Repeated clock calibration"
            )
        results["live_ankiweb"] = args.live
        results["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        (out / "verification.json").write_text(json.dumps(results, indent=2) + "\n")
        print("PASS logout; collection entities unchanged", flush=True)
    finally:
        for pattern in ["sync-auth*", "sync-test.json"]:
            for path in sd.glob(pattern):
                path.unlink()
        if gateway:
            gateway.shutdown()
            gateway.server_close()
        if server:
            server.terminate()
            server.wait(timeout=10)


if __name__ == "__main__":
    main()
