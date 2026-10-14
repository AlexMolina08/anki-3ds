# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Preserve native SD absolute paths in upstream tempfile's execution support."""

import hashlib
import shutil
import tarfile
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
package = next(
    p
    for p in tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
    if p["name"] == "tempfile"
)
archives = list(
    (Path.home() / ".cargo/registry/cache").glob(
        f"*/tempfile-{package['version']}.crate"
    )
)
assert len(archives) == 1
assert hashlib.sha256(archives[0].read_bytes()).hexdigest() == package["checksum"]
output = ROOT / "out/3ds/patched-deps/tempfile"
if output.exists():
    shutil.rmtree(output)
with tempfile.TemporaryDirectory(dir=output.parent) as temporary:
    with tarfile.open(archives[0]) as archive:
        archive.extractall(temporary, filter="data")
    shutil.copytree(Path(temporary) / f"tempfile-{package['version']}", output)
source = output / "src/util.rs"
text = source.read_text()
needle = "    if !base.is_absolute() {"
assert text.count(needle) == 1
text = text.replace(
    needle,
    """    // Horizon devoptab uses a drive-qualified absolute SD path. Unix Path
    // considers it relative; prefixing the working directory makes it invalid.
    let native_sd_absolute = cfg!(target_os = "horizon")
        && base.as_os_str().to_string_lossy().starts_with("sdmc:/");
    if !base.is_absolute() && !native_sd_absolute {""",
)
source.write_text(text)
with (output / "Cargo.toml").open("a") as manifest:
    manifest.write("\n[workspace]\n")
print(output)
