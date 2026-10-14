# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Package sync-probe for Old 3DS hardware verification."""
import hashlib
import shutil
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'out/3ds/sync-probe'
STAGE = OUT / 'hardware'
APP = STAGE / '3ds/anki-sync-probe'
APP.mkdir(parents=True, exist_ok=True)

for name in ('dns-server.txt', 'report.txt'):
    APP.joinpath(name).unlink(missing_ok=True)

shutil.copy2(OUT / 'anki-3ds-sync-probe.3dsx', APP / 'anki-3ds-sync-probe.3dsx')
shutil.copy2(OUT / 'anki-3ds-sync-probe.smdh', APP / 'anki-3ds-sync-probe.smdh')

ca = ROOT / 'out/3ds/https-sources/cacert.pem'
expected = 'a41b5d356aea97a529fe27e0f7316d2f9d946d75927476cf9cf1b90637d00505'
assert hashlib.sha256(ca.read_bytes()).hexdigest() == expected, 'CA bundle changed'
shutil.copy2(ca, APP / 'cacert.pem')

archive = OUT / 'anki-sync-probe-old3ds.zip'
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as output:
    for file in sorted(STAGE.rglob('*')):
        if file.is_file():
            output.write(file, file.relative_to(STAGE))

with zipfile.ZipFile(archive) as package:
    assert package.testzip() is None
    assert not any(name.endswith('dns-server.txt') for name in package.namelist())
    assert package.read('3ds/anki-sync-probe/anki-3ds-sync-probe.3dsx') == (OUT / 'anki-3ds-sync-probe.3dsx').read_bytes()

OUT.joinpath('package-sha256.txt').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
print(f"Created {archive}")
