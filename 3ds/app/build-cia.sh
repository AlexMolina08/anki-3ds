#!/usr/bin/env bash
# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
set -euo pipefail
cd "$(dirname "$0")/../.."
source 3ds/toolchain.sh
python3 3ds/app/build-info.py --check
python3 3ds/app/metadata.py
version=$(python3 -c 'import tomllib; print(tomllib.load(open("3ds/app/Cargo.toml", "rb"))["package"]["version"])')
IFS=. read -r major minor micro <<< "$version"
elf=out/3ds/collection-target/armv6k-nintendo-3ds/release/anki-3ds.elf
makerom -f cia -o out/3ds/app/anki-3ds.cia -target t -rsf 3ds/app/application.rsf \
    -elf "$elf" -icon out/3ds/app/anki-3ds.smdh -banner out/3ds/app/banner.bnr \
    -exefslogo -major "$major" -minor "$minor" -micro "$micro"
shasum -a 256 out/3ds/app/anki-3ds.cia

python3 - <<'PYINFO'
import hashlib,json
from pathlib import Path
p=Path("out/3ds/app/build.json"); info=json.loads(p.read_text()); info["cia_sha256"]=hashlib.sha256(Path("out/3ds/app/anki-3ds.cia").read_bytes()).hexdigest(); p.write_text(json.dumps(info,indent=2)+"\n")
PYINFO
