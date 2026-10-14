# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Versioned hardware package. Inputs only; never replaces the offline app."""
import hashlib
import json
import shutil
import subprocess
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/normal-sync-probe"
verification = json.loads((OUT / "verification.json").read_text())
assert verification["passed"] == 10 and verification["failed"] == 0
binary = OUT / "anki-3ds-normal-sync-probe.3dsx"
sha = hashlib.sha256(binary.read_bytes()).hexdigest()
stage = OUT / f"hardware-v1-{sha[:12]}"
if stage.exists():
    raise SystemExit(f"Refusing to overwrite {stage}")
app = stage / "3ds/anki-normal-sync-probe"
app.mkdir(parents=True)
shutil.copy2(binary, app / "anki-normal-sync-phase4-v1.3dsx")
shutil.copy2(OUT / "anki-3ds-normal-sync-probe.smdh", app / "anki-normal-sync-phase4-v1.smdh")
fixture_root = stage / "3ds/anki-3ds/phase4-v1/fixtures"
fixture_root.mkdir(parents=True)
for fixture in sorted((OUT / "fixtures").rglob("*.anki2")):
    destination = fixture_root / fixture.relative_to(OUT / "fixtures")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(fixture, destination)
ca = ROOT / "out/3ds/https-sources/cacert.pem"
assert hashlib.sha256(ca.read_bytes()).hexdigest() == "a41b5d356aea97a529fe27e0f7316d2f9d946d75927476cf9cf1b90637d00505"
shutil.copy2(ca, stage / "3ds/anki-3ds/cacert.pem")
shutil.copy2(ROOT / "3ds/normal-sync-probe/README.md", stage / "README.md")
(stage / "BUILD.json").write_text(json.dumps({"phase":4,"revision":1,"binary_sha256":sha,
    "emulator_cases_passed":10,"physical_verified":False,"input_sha256":verification["input_sha256"],
    "upstream_revision":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
    "source_sha256":json.loads((OUT/"source-sha256.json").read_text())},indent=2)+"\n")
archive = OUT / f"anki-normal-sync-phase4-v1-{sha[:12]}-old3ds.zip"
with zipfile.ZipFile(archive,"w",zipfile.ZIP_DEFLATED) as package:
    for file in sorted(stage.rglob("*")):
        if file.is_file(): package.write(file,file.relative_to(stage))
with zipfile.ZipFile(archive) as package:
    assert package.testzip() is None
    assert not any(n.endswith(("dns-server.txt","report.txt","native.json")) for n in package.namelist())
    assert hashlib.sha256(package.read("3ds/anki-normal-sync-probe/anki-normal-sync-phase4-v1.3dsx")).hexdigest() == sha
(OUT / "package-sha256.txt").write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+"  "+archive.name+"\n")
print(archive)
print(f"binary_sha256={sha}")
