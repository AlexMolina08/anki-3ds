# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Check original backend sources with isolated dependency adaptations.

A tracked-source snapshot keeps generated manifests/lockfiles outside desktop
builds. --signal-mode=omit removes only the Tokio signal feature to expose the
independent networking failure; it is a diagnostic, not a working offline port.
"""
import argparse
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--signal-mode", choices=("keep", "omit"), default="keep")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
snapshot = root / "out/3ds/backend-portability-source"
if snapshot.exists():
    shutil.rmtree(snapshot)
snapshot.mkdir(parents=True)
with subprocess.Popen(["git", "archive", "HEAD", "Cargo.toml", "Cargo.lock", "rslib",
                       "proto", "ftl", "build", "tools/minilints", "pylib/rsbridge"],
                      cwd=root, stdout=subprocess.PIPE) as archive:
    subprocess.run(["tar", "-x", "-C", str(snapshot)], stdin=archive.stdout, check=True)
    assert archive.wait() == 0
for name in ("core-repo", "qt-repo"):
    destination = snapshot / "ftl" / name
    if destination.exists():
        destination.rmdir()
    destination.symlink_to(root / "ftl" / name, target_is_directory=True)
manifest = snapshot / "Cargo.toml"
text = manifest.read_text()
if args.signal_mode == "omit":
    old = 'features = ["fs", "rt-multi-thread", "macros", "signal"]'
    assert text.count(old) == 1
    text = text.replace(old, 'features = ["fs", "rt-multi-thread", "macros"]')
text += '\n[patch.crates-io]\ncoarsetime = { path = "' + str(root / "out/3ds/patched-deps/coarsetime") + '" }\n'
manifest.write_text(text)
subprocess.run(["cargo", "+nightly-2026-10-12", "check", "-p", "anki",
                "--target", "armv6k-nintendo-3ds", "-Z", "build-std=std,panic_abort"],
               cwd=snapshot, check=True)
