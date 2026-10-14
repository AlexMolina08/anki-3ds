# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Package the release app; never include collections, test endpoints or secrets."""

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

root = Path(__file__).resolve().parents[2]
out = root / "out/3ds/app"
binary = out / "anki-3ds.3dsx"
ca = root / "out/3ds/https-sources/cacert.pem"
assert binary.is_file() and ca.is_file(), (
    "Build the release app and HTTPS libraries first"
)
assert b"sync-test.json" not in binary.read_bytes(), (
    "Refusing to package an emulator test build"
)
assert b"test-script.txt" not in binary.read_bytes(), (
    "Refusing production action-script support"
)
metadata = {
    "version": "0.2.1",
    "source_revision": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True
    ).strip(),
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "ca_sha256": hashlib.sha256(ca.read_bytes()).hexdigest(),
    "architecture": "ARMv6K/VFPv2",
    "hardware_tested": False,
    "collections_included": False,
    "credentials_included": False,
}
destination = out / "anki-3ds-0.2.1.zip"
with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write(binary, "3ds/anki-3ds/anki-3ds.3dsx")
    archive.write(root / "3ds/app/README.md", "README.md")
    archive.write(root / "LICENSE", "LICENSE")
    archive.write(
        root / "out/3ds/https-sources/curl-8.22.0/COPYING", "licenses/curl.txt"
    )
    archive.write(
        root / "out/3ds/https-sources/mbedtls-3.6.7/LICENSE", "licenses/mbedtls.txt"
    )
    archive.writestr("manifest.json", json.dumps(metadata, indent=2) + "\n")
with zipfile.ZipFile(destination) as archive:
    assert archive.testzip() is None
    assert (
        hashlib.sha256(archive.read("3ds/anki-3ds/anki-3ds.3dsx")).hexdigest()
        == metadata["binary_sha256"]
    )
sha = hashlib.sha256(destination.read_bytes()).hexdigest()
(out / "package-sha256.txt").write_text(f"{sha}  {destination.name}\n")
print(destination)
print(sha)
