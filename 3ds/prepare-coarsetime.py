# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Prepare the pinned dependency with a minimal, Horizon-only compatibility patch."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tomllib
import tarfile
import tempfile

root = Path(__file__).resolve().parents[1]
lock = tomllib.loads((root / "Cargo.lock").read_text())
package = next(p for p in lock["package"] if p["name"] == "coarsetime")
assert package["version"] == "0.1.36"
cache = Path.home() / ".cargo/registry"
archives = list(cache.glob("cache/*/coarsetime-0.1.36.crate"))
assert len(archives) == 1, "Fetch the pinned upstream dependencies first"
assert hashlib.sha256(archives[0].read_bytes()).hexdigest() == package["checksum"]
output = root / "out/3ds/patched-deps/coarsetime"
output.parent.mkdir(parents=True, exist_ok=True)
if output.exists():
    shutil.rmtree(output)
with tempfile.TemporaryDirectory(dir=output.parent) as temporary:
    with tarfile.open(archives[0]) as archive:
        assert all(Path(m.name).parts[0] == "coarsetime-0.1.36" for m in archive.getmembers())
        archive.extractall(temporary, filter="data")
    shutil.copytree(Path(temporary) / "coarsetime-0.1.36", output)
subprocess.run(["patch", "-p1", "-i", str(root / "3ds/coarsetime-horizon.patch")],
               cwd=output, check=True)
with (output / "Cargo.toml").open("a") as manifest:
    manifest.write("\n[workspace]\n")
print(output)
