# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Record and validate the exact native build inputs before packaging."""

import hashlib
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/app"
ELF = ROOT / "out/3ds/collection-target/armv6k-nintendo-3ds/release/anki-3ds.elf"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    paths = subprocess.check_output(
        [
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "3ds/app",
            "3ds/sync-transport",
            "3ds/sqlite-vfs",
            "3ds/coarsetime-horizon.patch",
            "3ds/toolchain.sh",
            "3ds/cc.sh",
            "3ds/linker.sh",
            "3ds/old3ds.specs",
            "3ds/dependencies.json",
            "Cargo.toml",
            "Cargo.lock",
            ".cargo",
            "rslib",
            "proto",
            "ftl",
        ],
        cwd=ROOT,
    ).split(b"\0")
    result = hashlib.sha256()
    for name in sorted(set(paths)):
        path = ROOT / name.decode()
        if name and path.is_file() and path.suffix not in {".md"}:
            result.update(name + b"\0" + path.read_bytes())
    for module in ["ftl/core-repo", "ftl/qt-repo"]:
        result.update(
            subprocess.check_output(["git", "rev-parse", f"HEAD:{module}"], cwd=ROOT)
        )
    result.update((ROOT / "out/3ds/https-sources/cacert.pem").read_bytes())
    return result.hexdigest()


def validate():
    info = json.loads((OUT / "build.json").read_text())
    if (
        info["inputs_sha256"] != inputs()
        or info["binary_sha256"] != digest(OUT / "anki-3ds.3dsx")
        or info["elf_sha256"] != digest(ELF)
    ):
        raise SystemExit("Native build is stale. Run just old3ds-app-build first.")
    if (
        b"sync-test.json" in (OUT / "anki-3ds.3dsx").read_bytes()
        or b"test-script.txt" in (OUT / "anki-3ds.3dsx").read_bytes()
    ):
        raise SystemExit("Refusing to package an emulator build")
    return info


def main():
    if sys.argv[1:] == ["--check"]:
        validate()
        return
    info = {
        "version": tomllib.loads((ROOT / "3ds/app/Cargo.toml").read_text())["package"][
            "version"
        ],
        "build_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "inputs_sha256": inputs(),
        "binary_sha256": digest(OUT / "anki-3ds.3dsx"),
        "elf_sha256": digest(ELF),
    }
    (OUT / "build.json").write_text(json.dumps(info, indent=2) + "\n")


if __name__ == "__main__":
    main()
