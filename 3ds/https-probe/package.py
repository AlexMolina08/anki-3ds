# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Package only the standalone probe, trust store, instructions, and licenses."""
import hashlib
import shutil
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'out/3ds/https-probe'
STAGE = OUT / 'hardware'
APP = STAGE / '3ds/anki-https-probe'
APP.mkdir(parents=True, exist_ok=True)
for name in ('dns-server.txt', 'report.txt'):
    APP.joinpath(name).unlink(missing_ok=True)
for name in ('anki-https-probe.3dsx', 'anki-https-probe.smdh'):
    shutil.copy2(OUT / name, APP / name)
ca = ROOT / 'out/3ds/https-sources/cacert.pem'
expected = 'a41b5d356aea97a529fe27e0f7316d2f9d946d75927476cf9cf1b90637d00505'
assert hashlib.sha256(ca.read_bytes()).hexdigest() == expected, 'CA bundle changed; review source/date and checksum before packaging'
shutil.copy2(ca, APP / 'cacert.pem')
shutil.copy2(ROOT / '3ds/https-probe/README.md', STAGE / 'RUN-ON-OLD-3DS.md')
licenses = STAGE / 'licenses'
licenses.mkdir(exist_ok=True)
for source, name in ((ROOT / 'LICENSE','anki.txt'),
                     (ROOT / 'out/3ds/https-sources/curl-8.22.0/COPYING','curl.txt'),
                     (ROOT / '3ds/https-probe/licenses/libctru.txt','libctru.txt'),
                     (ROOT / 'out/3ds/https-sources/mbedtls-3.6.7/LICENSE','mbedtls.txt')):
    shutil.copy2(source, licenses / name)
for source in (ROOT / 'out/3ds/https-sources/mbedtls-3.6.7/3rdparty').rglob('*'):
    if source.is_file() and source.name.startswith(('LICENSE','COPYING')):
        shutil.copy2(source, licenses / ('mbedtls-' + source.parent.name + '-' + source.name))
source_dir = STAGE / 'source'
source_dir.mkdir(exist_ok=True)
for source in (ROOT / '3ds/https-probe').iterdir():
    if source.is_file():
        shutil.copy2(source, source_dir / source.name)
shutil.copy2(ROOT / '3ds/old3ds.specs', source_dir / 'old3ds.specs')
checksums = []
for file in sorted(STAGE.rglob('*')):
    if file.is_file() and file.name != 'SHA256SUMS.txt':
        checksums.append(f'{hashlib.sha256(file.read_bytes()).hexdigest()}  {file.relative_to(STAGE)}')
STAGE.joinpath('SHA256SUMS.txt').write_text('\n'.join(checksums)+'\n')
archive = OUT / 'anki-https-probe-old3ds.zip'
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as output:
    for file in sorted(STAGE.rglob('*')):
        if file.is_file():
            output.write(file,file.relative_to(STAGE))
with zipfile.ZipFile(archive) as package:
    assert package.testzip() is None
    assert not any(name.endswith('dns-server.txt') for name in package.namelist())
    assert package.read('3ds/anki-https-probe/anki-https-probe.3dsx') == (OUT / 'anki-https-probe.3dsx').read_bytes()
OUT.joinpath('package-sha256.txt').write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+'  '+archive.name+'\n')
print(archive)
