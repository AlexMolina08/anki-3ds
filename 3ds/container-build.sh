#!/usr/bin/env bash
# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
set -euo pipefail
cd "$(dirname "$0")/.."
image=$(python3 -c 'import json; print(json.load(open("3ds/dependencies.json"))["devkitpro_image"])')
docker run --rm --platform linux/amd64 -v "$PWD:/work" -w /work "$image" bash 3ds/container-entry.sh
