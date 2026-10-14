# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Compile the real DNS loop on host with a small Horizon service shim."""

import subprocess
import tempfile
from pathlib import Path

root = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix="anki-dns-retry-") as directory:
    path = Path(directory)
    (path / "3ds.h").write_text(
        "#pragma once\n#include <stdint.h>\n#include <stddef.h>\n"
        "#include <sys/socket.h>\n#include <netinet/in.h>\n"
        "typedef int Result;\n#define R_FAILED(value) ((value) < 0)\n"
        "#define SOL_CONFIG 0\n#define NETOPT_DNS_TABLE 0\n"
        "typedef struct { int family; struct in_addr ip; } SOCU_DNSTableEntry;\n"
        "uint64_t osGetTime(void);\n"
        "Result PS_GenerateRandomBytes(void *, size_t);\n"
        "int SOCU_GetNetworkOpt(int, int, void *, socklen_t *);\n"
    )
    subprocess.run(
        [
            "cc",
            "-std=c11",
            "-I",
            directory,
            str(root / "3ds/testing/dns-retry.c"),
            "-o",
            str(path / "test"),
        ],
        check=True,
    )
    subprocess.run([str(path / "test")], check=True)
