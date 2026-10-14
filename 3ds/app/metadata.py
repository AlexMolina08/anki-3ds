# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Create Homebrew Launcher metadata and a silent HOME Menu banner."""

import subprocess
import wave
from pathlib import Path

root = Path(__file__).resolve().parents[2]
out = root / "out/3ds/app"
out.mkdir(parents=True, exist_ok=True)
resources = root / "3ds/app/resources"
with wave.open(str(out / "silence.wav"), "wb") as sound:
    sound.setparams((1, 2, 22050, 0, "NONE", "not compressed"))
    sound.writeframes(bytes(22050 * 2))
subprocess.run(
    [
        str(root / "out/3ds/tools/bin/bannertool"),
        "makesmdh",
        "-s",
        "Anki 3DS",
        "-l",
        "Anki flashcards for Nintendo 3DS",
        "-p",
        "Anki 3DS contributors",
        "-i",
        str(resources / "icon.png"),
        "-o",
        str(out / "anki-3ds.smdh"),
    ],
    check=True,
)
subprocess.run(
    [
        str(root / "out/3ds/tools/bin/bannertool"),
        "makebanner",
        "-i",
        str(resources / "banner.png"),
        "-a",
        str(out / "silence.wav"),
        "-o",
        str(out / "banner.bnr"),
    ],
    check=True,
)
