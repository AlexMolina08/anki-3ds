# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Package verified native binaries with provenance, licenses and checksums."""

import hashlib
import importlib.util
import json
import os
import subprocess
import zipfile
from pathlib import Path

root = Path(__file__).resolve().parents[2]
out = root / "out/3ds/app"
spec = importlib.util.spec_from_file_location(
    "build_info", root / "3ds/app/build-info.py"
)
build_info = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build_info)
metadata = build_info.validate()
cia = out / "anki-3ds.cia"
if metadata.get("cia_sha256") != build_info.digest(cia):
    raise SystemExit("CIA is missing or stale. Run just old3ds-app-cia first.")
revision = subprocess.check_output(
    ["git", "rev-parse", "HEAD"], cwd=root, text=True
).strip()
dirty = bool(
    subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=normal"], cwd=root
    )
)
if os.environ.get("ANKI_3DS_RELEASE") == "1" and dirty:
    raise SystemExit("Release packaging requires a clean checkout")
metadata.update(
    {
        "source_revision": revision,
        "source_dirty": dirty,
        "ca_sha256": build_info.digest(root / "out/3ds/https-sources/cacert.pem"),
        "architecture": "ARMv6K/VFPv2",
        "title_id": "000400000F3A1000",
        "hardware_tested": False,
        "collections_included": False,
        "credentials_included": False,
        "toolchain": json.loads((root / "3ds/dependencies.json").read_text()),
    }
)
version = metadata["version"]
if (
    os.environ.get("GITHUB_REF_TYPE") == "tag"
    and os.environ.get("GITHUB_REF_NAME") != f"v{version}"
):
    raise SystemExit("Release tag must match 3ds/app/Cargo.toml")
(out / "manifest.json").write_text(json.dumps(metadata, indent=2) + "\n")
files = {
    "3ds/anki-3ds/anki-3ds.3dsx": out / "anki-3ds.3dsx",
    "cia/anki-3ds.cia": cia,
    "README.md": root / "3ds/app/README.md",
    "LICENSE": root / "LICENSE",
    "licenses/curl.txt": root / "out/3ds/https-sources/curl-8.22.0/COPYING",
    "licenses/mbedtls.txt": root / "out/3ds/https-sources/mbedtls-3.6.7/LICENSE",
    "manifest.json": out / "manifest.json",
}
destination = out / f"anki-3ds-{version}.zip"
with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
    for name, path in files.items():
        archive.write(path, name)
with zipfile.ZipFile(destination) as archive:
    if archive.testzip() is not None:
        raise SystemExit("ZIP integrity check failed")
    for name, path in files.items():
        if hashlib.sha256(archive.read(name)).hexdigest() != build_info.digest(path):
            raise SystemExit(f"ZIP content mismatch: {name}")
checksums = [destination, out / "anki-3ds.3dsx", cia, out / "manifest.json"]
(out / "SHA256SUMS").write_text(
    "".join(f"{build_info.digest(p)}  {p.name}\n" for p in checksums)
)
print(destination)
print((out / "SHA256SUMS").read_text(), end="")
