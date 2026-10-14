# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Create an isolated upstream test-harness snapshot; preserve all test bodies.

Rust's custom test framework collects constants beside the original functions.
Only #[test] registration changes. No source checkout or assertions are edited.
"""

import argparse
import json
import re
import shutil
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--support", action="store_true")
parser.add_argument("--native-tempfile", action="store_true")
args = parser.parse_args()

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "out/3ds/upstream-tests-source"
for line in subprocess.check_output(
    ["ps", "-ax", "-o", "pid=,command="], text=True
).splitlines():
    fields = line.split()
    if (
        len(fields) > 1
        and Path(fields[1]).name == "cargo"
        and "upstream-tests-source/Cargo.toml" in line
    ):
        raise SystemExit(
            "Snapshot build already running; wait before regenerating sources"
        )

if DEST.exists():
    shutil.rmtree(DEST)
DEST.mkdir(parents=True)
paths = [
    "Cargo.toml",
    "Cargo.lock",
    ".version",
    ".cargo",
    "rslib",
    "proto",
    "ftl",
    "build",
    "tools/minilints",
    "pylib/rsbridge",
]
with subprocess.Popen(
    ["git", "archive", "HEAD", *paths], cwd=ROOT, stdout=subprocess.PIPE
) as proc:
    subprocess.run(["tar", "-x", "-C", str(DEST)], stdin=proc.stdout, check=True)
    assert proc.wait() == 0
import hashlib

source_hashes = {}
tracked = (
    subprocess.check_output(["git", "ls-files", "-z", "--", *paths], cwd=ROOT)
    .decode()
    .split("\0")
)
for relative in tracked:
    source = ROOT / relative
    if relative and source.is_file():
        target = DEST / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        source_hashes[relative] = hashlib.sha256(source.read_bytes()).hexdigest()
(ROOT / "out/3ds/testing").mkdir(exist_ok=True)
(ROOT / "out/3ds/testing/source-sha256.json").write_text(
    json.dumps(source_hashes, indent=2) + "\n"
)
for name in ["core-repo", "qt-repo"]:
    path = DEST / "ftl" / name
    path.rmdir()
    path.symlink_to(ROOT / "ftl" / name, target_is_directory=True)
manifest = DEST / "Cargo.toml"
manifest.write_text(
    manifest.read_text()
    + '\n[patch.crates-io]\ncoarsetime = { path = "'
    + str(ROOT / "out/3ds/patched-deps/coarsetime")
    + '" }\n'
)
if args.native_tempfile:
    subprocess.run(
        ["python3", str(ROOT / "3ds/testing/prepare-tempfile.py")], check=True
    )
    with manifest.open("a") as out:
        out.write(
            'tempfile = { path = "'
            + str(ROOT / "out/3ds/patched-deps/tempfile")
            + '" }\n'
        )
manifest = DEST / "rslib/Cargo.toml"
text = manifest.read_text().replace(
    "[dev-dependencies]",
    "[target.'cfg(not(target_os = \"horizon\"))'.dev-dependencies]",
)
text += "\n[target.'cfg(target_os = \"horizon\")'.dependencies]\n"
text += 'ctru-rs = { git = "https://github.com/rust3ds/ctru-rs" }\n'
text += 'anki-3ds-sqlite-vfs = { path = "' + str(ROOT / "3ds/sqlite-vfs") + '" }\n'
manifest.write_text(text)
# Seed SDK dependencies from the already verified probe lockfile.
import tomllib

lock_path = DEST / "Cargo.lock"
root_lock = lock_path.read_text()
seen = {
    (p["name"], p["version"], p.get("source"))
    for p in tomllib.loads(root_lock)["package"]
}
for block in (
    (ROOT / "3ds/collection-probe/Cargo.lock").read_text().split("[[package]]")[1:]
):
    package = tomllib.loads("[[package]]" + block)["package"][0]
    key = (package["name"], package["version"], package.get("source"))
    if key not in seen:
        root_lock += "\n[[package]]" + block
        seen.add(key)
lock_path.write_text(root_lock)
registrations = []
pattern = re.compile(
    r"#\[test\](\s*(?:#\[[^\n]+\]\s*)*)(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)\s*\("
)
for path in sorted((DEST / "rslib/src").rglob("*.rs")):
    original = path.read_text()

    def replace(match):
        name = match[2]
        registrations.append({"file": str(path.relative_to(DEST)), "function": name})
        return (
            match[1]
            + "#[test_case]\nconst __UPSTREAM_"
            + name
            + ": crate::horizon_tests::TestCase = "
            'crate::horizon_tests::TestCase { name: concat!(module_path!(), "::", "'
            + name
            + '"), run: || { crate::horizon_tests::check_return('
            + name
            + "()); } };\n"
            + match[0].replace("#[test]", "", 1)
        )

    if re.search(r"#\[(?:ignore|should_panic)", original):
        raise RuntimeError(
            f"Explicit handling required for ignored/expected-panic tests: {path}"
        )
    changed = pattern.sub(replace, original)
    if "#[test]" in changed:
        raise RuntimeError(f"Unrecognized test registration: {path}")
    path.write_text(changed)
lib = DEST / "rslib/src/lib.rs"
lib.write_text(
    "#![feature(custom_test_frameworks)]\n#![test_runner(crate::horizon_tests::run)]\n"
    + lib.read_text()
    + "\nmod horizon_tests;\n"
)
shutil.copyfile(ROOT / "3ds/testing/runner.rs", DEST / "rslib/src/horizon_tests.rs")
(ROOT / "out/3ds/testing").mkdir(exist_ok=True)
(ROOT / "out/3ds/testing/registrations.json").write_text(
    json.dumps(registrations, indent=2) + "\n"
)
print(
    f"Registered {len(registrations)} synchronous test functions; Cargo cfg determines active set."
)

# Optional deterministic input clock applies only to the differential scenario.
timestamp = DEST / "rslib/src/timestamp.rs"
text = timestamp.read_text()
needle = "fn elapsed() -> time::Duration {"
assert text.count(needle) == 1
text = text.replace(
    needle,
    needle
    + '\n    if let Ok(value) = std::env::var("ANKI_3DS_FIXED_TIME") {\n        return time::Duration::from_millis(value.parse().unwrap());\n    }',
)
timestamp.write_text(text)
answering = DEST / "rslib/src/scheduler/answering"
shutil.copyfile(
    ROOT / "3ds/testing/differential.rs", answering / "horizon_differential.rs"
)
module = answering / "mod.rs"
module.write_text(module.read_text() + "\npub(crate) mod horizon_differential;\n")

if args.support:
    for name in ["i18n", "io", "process"]:
        base = DEST / "rslib" / name
        for source in (base / "src").rglob("*.rs"):
            source.write_text(pattern.sub(replace, source.read_text()))
        lib = base / "src/lib.rs"
        lib.write_text(
            "#![feature(custom_test_frameworks)]\n#![test_runner(crate::horizon_tests::run)]\n"
            + lib.read_text()
            + '\n#[cfg(test)]\n#[path="'
            + str(ROOT / "3ds/testing/support-runner.rs")
            + '"]\nmod horizon_tests;\n'
        )
        with (base / "Cargo.toml").open("a") as out:
            out.write(
                '\n[target.\'cfg(target_os = "horizon")\'.dependencies]\nctru-rs = { git = "https://github.com/rust3ds/ctru-rs" }\n'
            )

service = DEST / "rslib/src/import_export/service.rs"
text = service.read_text()
assert text.count('env!("CARGO_MANIFEST_DIR")') == 1
text = (
    """#[cfg(all(test, target_os = "horizon"))]
macro_rules! env {
    ("CARGO_MANIFEST_DIR") => { "sdmc:/3ds/anki-3ds/upstream-source/rslib" };
}
"""
    + text
)
service.write_text(text)
sd = ROOT / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds/upstream-source"
(sd / "rslib").mkdir(parents=True, exist_ok=True)
fixture = ROOT / "pylib/tests/support/update1.apkg"
destination = sd / "pylib/tests/support/update1.apkg"
destination.parent.mkdir(parents=True, exist_ok=True)
shutil.copyfile(fixture, destination)
assert (
    hashlib.sha256(fixture.read_bytes()).digest()
    == hashlib.sha256(destination.read_bytes()).digest()
)
(ROOT / "out/3ds/testing/fixture-sha256.txt").write_text(
    hashlib.sha256(fixture.read_bytes()).hexdigest() + "  update1.apkg\n"
)
