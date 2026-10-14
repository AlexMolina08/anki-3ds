# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Fetch verified release inputs and compile host packaging tools locally."""

import hashlib
import json
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
from pathlib import Path

from archive_utils import extract_sources

ROOT = Path(__file__).resolve().parents[1]
LOCK = json.loads((ROOT / "3ds/dependencies.json").read_text())
TOOLS = ROOT / "out/3ds/tools"


def fetch(entry, target):
    if (
        target.exists()
        and hashlib.sha256(target.read_bytes()).hexdigest() == entry["sha256"]
    ):
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(entry["url"], timeout=120) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != entry["sha256"]:
        raise RuntimeError(f"Checksum mismatch: {entry['url']}")
    temporary = target.with_suffix(target.suffix + ".tmp")
    temporary.write_bytes(data)
    temporary.replace(target)


def source(name):
    entry = LOCK[name]
    archive = TOOLS / "sources" / f"{name}-{entry['version']}.tar.gz"
    fetch(entry, archive)
    destination = TOOLS / "sources" / f"{name}-{entry['version']}"
    if not destination.exists():
        with tempfile.TemporaryDirectory(dir=archive.parent) as temporary:
            with tarfile.open(archive) as bundle:
                extract_sources(bundle, temporary)
            roots = list(Path(temporary).iterdir())
            if len(roots) != 1 or not roots[0].is_dir():
                raise RuntimeError(f"Unexpected {name} archive layout")
            shutil.copytree(roots[0], destination)
    return destination


def main():
    fetch(LOCK["ca"], ROOT / "out/3ds/https-sources/cacert.pem")
    (TOOLS / "bin").mkdir(parents=True, exist_ok=True)
    makerom = source("makerom") / "makerom"
    subprocess.run(["make", "-C", str(makerom), "deps", "-j4"], check=True)
    subprocess.run(["make", "-C", str(makerom), "-j4"], check=True)
    shutil.copy2(makerom / "bin/makerom", TOOLS / "bin/makerom")
    banner = source("bannertool")
    # Standard C++ rejects initialized variable-length arrays. Padding is 1–3 bytes.
    compression = banner / "source/3ds/lz11.cpp"
    text = compression.read_text()
    if "u8 pad[padLength] = {0};" in text:
        compression.write_text(
            text.replace("u8 pad[padLength] = {0};", "u8 pad[3] = {0};")
        )
    build = TOOLS / "bannertool-build"
    subprocess.run(
        ["cmake", "-S", str(banner), "-B", str(build), "-DCMAKE_BUILD_TYPE=Release"],
        check=True,
    )
    subprocess.run(["cmake", "--build", str(build), "-j4"], check=True)
    shutil.copy2(build / "bannertool", TOOLS / "bin/bannertool")
    print(f"Packaging tools ready: {TOOLS / 'bin'}")


if __name__ == "__main__":
    main()
