#!/usr/bin/env python3
"""Fail if a partition OFFSET is hard-coded in code that should read the table.

Gitea #501 gave the fleet two partition layouts (firmware/partitions.csv and
firmware/partitions-16mb.csv) and a migrator that runs, for one boot, under a
THIRD — the pre-#501 one a field device still carries. Any module that knows
where `storage` or `assets` lives by writing the number down is wrong on two
of those three, and wrong in the way that erases user data rather than the way
that fails to compile.

So: every partition address in the firmware comes from a partition table, by
label or subtype (`ota::data_partition`, `parttab::data_labelled`,
`parttab::app_slot`), and this check is what keeps it that way.

Scope: firmware/src/**.rs and the shell scripts that flash or pack
(firmware/*.sh, tools/*.sh). NOT the QEMU harness — tools/qemu/*.py composes
flash images byte by byte and must name offsets — and NOT docs or comments,
which are where these numbers belong.

Usage: tools/offset-check.py  (run from anywhere; wired into tools/ci.sh)
"""

from __future__ import annotations

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Every offset that IDENTIFIES a partition in one of the three layouts this
# firmware can meet. Sizes are not listed: 0x100000 and 0xF0000 have honest
# non-partition uses, and a wrong size fails loudly instead of silently
# addressing someone else's region.
FORBIDDEN = {
    0x110000: "pre-#501 ota_1 (the migration staging slot)",
    0x150000: "4 MB layout ota_1",
    0x210000: "pre-#501 storage",
    0x290000: "4 MB layout storage",
    0x310000: "pre-#501/4 MB assets — and the 16 MB layout's ota_1",
    0x610000: "16 MB layout storage",
    0xA10000: "16 MB layout assets",
}

# Addresses that are NOT partitions and are fixed by the ESP-IDF bootloader
# or by this firmware's own records, so writing them down is correct:
#   0x8000  the partition table itself      (parttab::TABLE_OFFSET)
#   0x9000..0x10000  nvs/guard/otadata/phy, wiped by the WLED takeover under
#           a FOREIGN table, where there is no label of ours to look up
#   0xC000  the boot-guard record            (ota::GUARD_OFFSET)
# They are below 0x100000 and so never match FORBIDDEN anyway; this note is
# here so the next reader does not "fix" them.

HEX = re.compile(r"0x[0-9A-Fa-f_]+")


def strip_comments(text: str, ext: str) -> str:
    """Blank out comments so a doc table of offsets is not a violation."""
    out = []
    for line in text.splitlines():
        if ext == ".rs":
            # `//` and `//!` — the firmware has no `//` inside string
            # literals (checked), so this is exact enough for a gate.
            i = line.find("//")
        else:
            i = line.find("#")
        out.append(line if i < 0 else line[:i])
    return "\n".join(out)


def files() -> list[str]:
    found = []
    for base, exts in (
        (os.path.join(ROOT, "firmware", "src"), (".rs",)),
        (os.path.join(ROOT, "firmware"), (".sh",)),
        (os.path.join(ROOT, "tools"), (".sh",)),
    ):
        for dirpath, _dirs, names in os.walk(base):
            if os.path.join("tools", "qemu") in dirpath:
                continue
            for n in names:
                if n.endswith(exts):
                    found.append(os.path.join(dirpath, n))
            if base.endswith("firmware") or base.endswith("tools"):
                break  # those two are non-recursive (only their own *.sh)
    return sorted(set(found))


def main() -> int:
    bad = []
    for path in files():
        ext = os.path.splitext(path)[1]
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        code = strip_comments(text, ext)
        for lineno, line in enumerate(code.splitlines(), 1):
            for m in HEX.finditer(line):
                try:
                    v = int(m.group(0).replace("_", ""), 16)
                except ValueError:
                    continue
                if v in FORBIDDEN:
                    bad.append((path, lineno, m.group(0), FORBIDDEN[v]))

    if not bad:
        print(f"offset-check: ok — no hard-coded partition offsets in {len(files())} files")
        return 0

    for path, lineno, lit, what in bad:
        rel = os.path.relpath(path, ROOT)
        print(f"offset-check: {rel}:{lineno}: hard-coded {lit} — {what}", file=sys.stderr)
    print(
        "\nRead the partition from the TABLE instead: ota::data_partition(\"storage\"),\n"
        "parttab::data_labelled(table, \"assets\"), parttab::app_slot(table, SUBTYPE_OTA1).\n"
        "A shell script can parse the csv the board selected ($PARTITIONS, see\n"
        "firmware/board-target.sh). See tools/offset-check.py and Gitea #501.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
