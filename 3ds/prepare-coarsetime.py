# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Prepare the pinned dependency with a minimal, Horizon-only compatibility patch."""

import hashlib
import os
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import urllib.request
from pathlib import Path

from archive_utils import extract_sources

root = Path(__file__).resolve().parents[1]
lock = tomllib.loads((root / "Cargo.lock").read_text())
package = next(p for p in lock["package"] if p["name"] == "coarsetime")
assert package["version"] == "0.1.36"
cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry"
archives = list(cache.glob("cache/*/coarsetime-0.1.36.crate"))
archive_path = next(
    (
        p
        for p in archives
        if hashlib.sha256(p.read_bytes()).hexdigest() == package["checksum"]
    ),
    None,
)
if archive_path is None:
    archive_path = root / "out/3ds/sources/coarsetime-0.1.36.crate"
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    if not archive_path.exists():
        with urllib.request.urlopen(
            "https://static.crates.io/crates/coarsetime/coarsetime-0.1.36.crate",
            timeout=120,
        ) as response:
            data = response.read()
        assert hashlib.sha256(data).hexdigest() == package["checksum"], (
            "coarsetime checksum mismatch"
        )
        archive_path.write_bytes(data)
assert hashlib.sha256(archive_path.read_bytes()).hexdigest() == package["checksum"]
output = root / "out/3ds/patched-deps/coarsetime"
output.parent.mkdir(parents=True, exist_ok=True)
if output.exists():
    shutil.rmtree(output)
with tempfile.TemporaryDirectory(dir=output.parent) as temporary:
    with tarfile.open(archive_path) as archive:
        assert all(
            Path(m.name).parts[0] == "coarsetime-0.1.36" for m in archive.getmembers()
        )
        extract_sources(archive, temporary)
    shutil.copytree(Path(temporary) / "coarsetime-0.1.36", output)
subprocess.run(
    ["patch", "-p1", "-i", str(root / "3ds/coarsetime-horizon.patch")],
    cwd=output,
    check=True,
)
with (output / "Cargo.toml").open("a") as manifest:
    manifest.write("\n[workspace]\n")
print(output)
