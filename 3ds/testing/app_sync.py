# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Exercise the native app against the original server over disposable TLS."""

import argparse
import hashlib
import http.client
import json
import os
import shutil
import sqlite3
import ssl
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AZAHAR = (
    Path.home()
    / "Applications/azahar-macos-arm64-2126.2/Azahar.app/Contents/MacOS/azahar"
)


class TLSBoundary(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if (
            self.server.fault == "interrupt"
            and self.path.endswith("/applyChunk")
            and not self.server.injected
        ):
            self.server.injected = True
            self.send_response(503)
            self.send_header("Content-Length", "0")
            self.end_headers()
            with (self.server.evidence / "http.log").open("a") as log:
                log.write(self.path + " 503 injected transport failure\n")
            return
        upstream = http.client.HTTPConnection("127.0.0.1", 18080, timeout=120)
        upstream.request(
            "POST",
            self.path,
            body,
            {
                k: v
                for k, v in self.headers.items()
                if k.lower() not in ("host", "connection", "content-length")
            },
        )
        response = upstream.getresponse()
        data = response.read()
        if (
            self.server.fault == "cancel"
            and self.path.endswith("/chunk")
            and not self.server.injected
        ):
            self.server.injected = True
            with (self.server.evidence / "http.log").open("a") as log:
                log.write(self.path + " delayed for cancellation\n")
            time.sleep(5)
        if self.server.fault == "corrupt" and self.path.endswith("/download"):
            data = b"not a SQLite collection"
        self.send_response(response.status)
        for key, value in response.getheaders():
            if key.lower() not in ("transfer-encoding", "content-length", "connection"):
                self.send_header(key, value)
        self.send_header("Content-Length", str(len(data)))
        try:
            self.end_headers()
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass
        upstream.close()
        with (self.server.evidence / "http.log").open("a") as log:
            log.write(
                f"{self.path} {response.status} request={len(body)} response={len(data)}\n"
            )

    def log_message(self, *args):
        pass


def database(path):
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    con.create_collation(
        "unicase",
        lambda a, b: (a.casefold() > b.casefold()) - (a.casefold() < b.casefold()),
    )
    assert con.execute("pragma integrity_check").fetchone()[0] == "ok"
    result = {}
    for table in ("notes", "cards", "revlog", "graves"):
        result[table] = con.execute(f"SELECT * FROM {table} ORDER BY 1").fetchall()
    con.close()
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--label", default="upload")
    parser.add_argument("--keyboard", action="store_true")
    parser.add_argument("--empty", action="store_true")
    parser.add_argument("--full-choice-guard", action="store_true")
    parser.add_argument("--language", choices=["en", "es"], default="en")
    parser.add_argument("--screenshots", action="store_true")
    parser.add_argument(
        "--fault",
        choices=["interrupt", "cancel", "corrupt", "reject-cert", "wrong-host"],
    )
    parser.add_argument(
        "--fixture", type=Path, default=ROOT / "out/3ds/app/fixture/prototype.anki2"
    )
    parser.add_argument("--remote-fixture", type=Path)
    parser.add_argument("--expected-case")
    parser.add_argument("--utc-offset", type=int, default=0)
    parser.add_argument("--steps", default="SYNC,STUDY,STUDY,BACK,EXIT")
    args = parser.parse_args()
    out = ROOT / "out/3ds/app-sync" / args.label
    if out.exists():
        raise SystemExit(f"Refusing to overwrite {out}")
    out.mkdir(parents=True)
    shutil.copyfile(ROOT / "out/3ds/app/anki-3ds.3dsx", out / "tested.3dsx")
    existing = (
        subprocess.check_output(["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"], text=True)
        if subprocess.run(
            ["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"],
            capture_output=True,
            check=False,
        ).returncode
        == 0
        else ""
    )
    if existing.strip():
        raise SystemExit("Close existing Azahar before this isolated test")
    # The private server and CA are build-time tests, never packaged with the app.
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
            "subjectAltName=IP:127.0.0.2"
            if args.fault == "wrong-host"
            else "subjectAltName=IP:127.0.0.1",
            "-keyout",
            str(tls / "key.pem"),
            "-out",
            str(tls / "cert.pem"),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    sd = out / "data/azahar-emu/sdmc/3ds/anki-3ds"
    sd.mkdir(parents=True)
    if not args.empty:
        shutil.copyfile(args.fixture, sd / "prototype.anki2")
    shutil.copyfile(
        ROOT / "out/3ds/https-sources/cacert.pem"
        if args.fault == "reject-cert"
        else tls / "cert.pem",
        sd / "test-ca.pem",
    )
    (sd / "sync-test.json").write_text(
        json.dumps(
            {
                "username": "" if args.keyboard else "dev",
                "password": "" if args.keyboard else "disposable",
                "endpoint": "https://127.0.0.1:18443/",
                "ca_file": "sdmc:/3ds/anki-3ds/test-ca.pem",
            }
        )
    )
    (sd / "language.txt").write_text(args.language)
    (sd / "utc-offset-minutes.txt").write_text(f"{args.utc_offset}\n")
    (sd / "test-script.txt").write_text("\n".join(args.steps.split(",")) + "\n")
    config = out / "config/azahar-emu"
    config.mkdir(parents=True)
    (config / "qt-config.ini").write_text(
        "[System]\nis_new_3ds=false\nis_new_3ds\\default=false\n[Core]\ncpu_clock_percentage=100\ncpu_clock_percentage\\default=false\n[Debugging]\nenable_rpc_server=true\nenable_rpc_server\\default=false\n"
    )
    if args.remote_fixture:
        remote = out / "server/dev/collection.anki2"
        remote.parent.mkdir(parents=True)
        shutil.copyfile(args.remote_fixture, remote)
    serverenv = os.environ.copy()
    serverenv.update(
        SYNC_USER1="dev:disposable",
        SYNC_HOST="127.0.0.1",
        SYNC_PORT="18080",
        SYNC_BASE=str(out / "server"),
    )
    env = os.environ.copy()
    env["TZ"] = "Europe/Madrid" if args.utc_offset == 120 else "UTC"
    for kind in ("DATA", "CONFIG", "CACHE"):
        env[f"XDG_{kind}_HOME"] = str(out / kind.lower())
    with (out / "server.log").open("w") as sl, (out / "emulator.log").open("w") as el:
        server = subprocess.Popen(
            [str(ROOT / "target/debug/anki-sync-server")],
            env=serverenv,
            stdout=sl,
            stderr=subprocess.STDOUT,
        )
        gateway = ThreadingHTTPServer(("127.0.0.1", 18443), TLSBoundary)
        gateway.evidence = out
        gateway.fault = args.fault
        gateway.injected = False
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.minimum_version = ssl.TLSVersion.TLSv1_2
        context.load_cert_chain(tls / "cert.pem", tls / "key.pem")
        gateway.socket = context.wrap_socket(gateway.socket, server_side=True)
        thread = threading.Thread(target=gateway.serve_forever, daemon=True)
        thread.start()
        app = None
        try:
            time.sleep(1)
            if server.poll() is not None:
                raise RuntimeError((out / "server.log").read_text())
            app = subprocess.Popen(
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
                    str(out / "tested.3dsx"),
                ],
                env=env,
                stdout=el,
                stderr=subprocess.STDOUT,
            )
            deadline = time.monotonic() + 180
            rollback_checked = False
            captured = set()
            rpc = None
            if args.screenshots:
                import sys

                sys.path.insert(0, str(AZAHAR.parents[3] / "scripting"))
                import AzaharRPC

                rpc = AzaharRPC.AzaharRPC(timeout=5)
            while time.monotonic() < deadline:
                events = (
                    (sd / "prototype-events.txt").read_text(errors="replace")
                    if (sd / "prototype-events.txt").exists()
                    else ""
                )
                if rpc:
                    for marker, name in [
                        ("DECKS screen", "decks"),
                        ("SYNC_WAIT screen", "sync"),
                        ("SYNC_COMPLETE", "complete"),
                    ]:
                        if marker in events and name not in captured:
                            assert rpc.save_screenshot(
                                str(out / f"{name}-{args.language}.png"), res_scale=1
                            )
                            captured.add(name)
                if "CLOSE_OK" in events:
                    break
                if (
                    args.fault in ("interrupt", "cancel")
                    and "SYNC_ERROR" in events
                    and not rollback_checked
                ):
                    assert database(sd / "prototype.anki2") == database(args.fixture), (
                        "local rollback differed from initial entities"
                    )
                    assert database(out / "server/dev/collection.anki2") == database(
                        args.remote_fixture
                    ), "server rollback differed from initial entities"
                    rollback_checked = True
                    (out / "rollback.json").write_text(
                        json.dumps({"local_unchanged": True, "server_unchanged": True})
                        + "\n"
                    )
                if (
                    ("SYNC_ERROR" in events and not args.fault)
                    or "OPEN_ERROR" in events
                    or "ACTION_ERROR" in events
                ):
                    raise RuntimeError(events)
                native_log = out / "data/azahar-emu/log/azahar_log.txt"
                if (
                    native_log.exists()
                    and native_log.stat().st_size > 1_000_000
                    and "Exception Type:"
                    in native_log.read_text(errors="replace")[-20000:]
                ):
                    raise RuntimeError("Native exception; emulator log retained")
                if app.poll() is not None:
                    raise RuntimeError(f"Emulator exited {app.returncode}")
                time.sleep(0.2)
            else:
                raise RuntimeError("App sync timed out")
        finally:
            if app:
                import signal

                pids = subprocess.run(
                    ["pgrep", "-f", "^" + str(AZAHAR) + "( |$)"],
                    capture_output=True,
                    text=True,
                    check=False,
                ).stdout.split()
                for pid in pids:
                    os.kill(int(pid), signal.SIGTERM)
                time.sleep(1)
                for pid in pids:
                    try:
                        os.kill(int(pid), signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                app.terminate()
                try:
                    app.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()
            gateway.shutdown()
            gateway.server_close()
            server.terminate()
            server.wait(timeout=10)
    validate(args, out, sd, rollback_checked)


def validate(args, out, sd, rollback_checked):
    events = (sd / "prototype-events.txt").read_text()
    if args.full_choice_guard:
        assert "SYNC_FULL_REQUIRED" in events and "SYNC_COMPLETE" not in events, events
        assert database(sd / "prototype.anki2") == database(args.fixture)
        assert database(out / "server/dev/collection.anki2") == database(
            args.remote_fixture
        )
        http = (out / "http.log").read_text()
        assert "/upload" not in http and "/download" not in http
        (out / "verification.json").write_text(
            json.dumps(
                {"explicit_direction_required": True, "collections_unchanged": True}
            )
            + "\n"
        )
        print(
            "PASS: full sync requires explicit direction when both collections contain data"
        )
        return
    if args.fault in ("corrupt", "reject-cert", "wrong-host"):
        assert "SYNC_ERROR" in events and "SYNC_COMPLETE" not in events, events
        assert database(sd / "prototype.anki2") == database(args.fixture), (
            "failed transfer changed collection rows"
        )
        if args.fault == "corrupt":
            assert not list(sd.glob(".pre-sync-*")), (
                "invalid download reached replacement"
            )
        else:
            assert not (out / "http.log").exists(), "TLS rejection reached HTTP"
        (out / "verification.json").write_text(
            json.dumps(
                {
                    "fault": args.fault,
                    "collection_preserved": True,
                    "error_propagated": True,
                },
                indent=2,
            )
            + "\n"
        )
        print(f"PASS {args.label}: rejected operation; original collection preserved")
        return
    assert "SYNC_COMPLETE" in events, events
    if args.fault in ("interrupt", "cancel"):
        assert "SYNC_ERROR" in events and events.index("SYNC_ERROR") < events.index(
            "SYNC_COMPLETE"
        ), events
        assert "/sync/abort" in (out / "http.log").read_text(), "upstream abort missing"
        assert rollback_checked, "rollback snapshot missing"
        if args.fault == "cancel":
            assert "Interrupted" in events, events
    local = database(sd / "prototype.anki2")
    remote = database(out / "server/dev/collection.anki2")
    assert local == remote, "native and desktop server entity rows differ"
    if args.expected_case:
        import sys

        sys.path.insert(0, str(ROOT / "3ds/normal-sync-probe"))
        from verify import assert_oracle, snapshot

        expected = next(
            c
            for c in json.loads(
                (ROOT / "out/3ds/normal-sync-probe/desktop.json").read_text()
            )
            if c["case"] == args.expected_case
        )
        actual = snapshot(sd / "prototype.anki2")
        assert actual == expected["after"], (
            "native HTTPS differs from original desktop engine fixture"
        )
        case = dict(
            expected, after=actual, server=snapshot(out / "server/dev/collection.anki2")
        )
        assert_oracle(case)
        print("Independent SQL oracle and original desktop engine match")
    (out / "verification.json").write_text(
        json.dumps(
            {
                "old3ds_mode": True,
                "entity_rows_equal": True,
                "counts": {k: len(v) for k, v in local.items()},
                "binary_sha256": hashlib.sha256(
                    (out / "tested.3dsx").read_bytes()
                ).hexdigest(),
            },
            indent=2,
        )
        + "\n"
    )
    print(f"PASS {args.label}: native and original desktop server rows equal")


if __name__ == "__main__":
    main()
