#!/usr/bin/env python3
"""BDF -> PSF2, ASCII 0x20..0x7E only, no Unicode table.

The three fonts `luxel-core` embeds (crates/luxel-core/fonts/) are built with
this script; `crates/luxel-core/fonts/README.md` records the exact command
line and the upstream source of every input BDF.

Output layout (what `crates/luxel-core/src/text.rs` reads):

    32-byte PSF2 header, then `length * charsize` bytes of glyph data.
    Glyph `i` is codepoint `0x20 + i`, `height` rows of `ceil(width/8)`
    bytes, MSB first, left-aligned in the row.  No PSF2 Unicode table
    (FLAG_HAS_UNICODE_TABLE is 0), so the body IS the glyph sheet.

BDF geometry: the font bounding box `FONTBOUNDINGBOX fw fh fx fy` is the
cell; a glyph's own `BBX bw bh bx by` is placed inside it with BDF's
lower-left, y-up origin, so cell row 0 (the top) is y = fy + fh - 1.
"""

import argparse
import struct
import sys

FIRST, LAST = 0x20, 0x7E
PSF2_MAGIC = b"\x72\xb5\x4a\x86"


def parse_bdf(path):
    """-> (fbb, {codepoint: (bbx, [row ints, top first])})"""
    fbb = None
    glyphs = {}
    cp = None
    bbx = None
    rows = None
    with open(path, "r", encoding="latin-1") as fh:
        for line in fh:
            w = line.split()
            if not w:
                continue
            if w[0] == "FONTBOUNDINGBOX":
                fbb = tuple(int(x) for x in w[1:5])
            elif w[0] == "ENCODING":
                cp = int(w[1])
            elif w[0] == "BBX":
                bbx = tuple(int(x) for x in w[1:5])
            elif w[0] == "BITMAP":
                rows = []
            elif w[0] == "ENDCHAR":
                if cp is not None and bbx is not None and rows is not None:
                    glyphs[cp] = (bbx, rows)
                cp, bbx, rows = None, None, None
            elif rows is not None:
                rows.append(int(w[0], 16) if w[0] else 0)
    if fbb is None:
        sys.exit(f"{path}: no FONTBOUNDINGBOX")
    return fbb, glyphs


def render(fbb, bbx, rows, width, height):
    """One glyph as `height` rows of `width` bits, packed MSB-first."""
    stride = (width + 7) // 8
    cell = [0] * height
    if bbx is None:
        return bytes(stride * height)
    fw, fh, fx, fy = fbb
    bw, bh, bx, by = bbx
    # BDF pads each bitmap row up to a whole number of bytes on the RIGHT.
    src_stride = (bw + 7) // 8
    for r, raw in enumerate(rows[:bh]):
        # y of this row, then the cell row counted from the top
        row = (fy + fh - 1) - (by + bh - 1 - r)
        if not (0 <= row < height):
            continue
        acc = cell[row]
        for c in range(bw):
            bit = (raw >> (src_stride * 8 - 1 - c)) & 1
            if not bit:
                continue
            col = (bx - fx) + c
            if 0 <= col < width:
                acc |= 1 << (width - 1 - col)
        cell[row] = acc
    out = bytearray()
    for row in cell:
        # left-align the `width` bits in `stride` bytes
        out += (row << (stride * 8 - width)).to_bytes(stride, "big")
    return bytes(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("bdf")
    ap.add_argument("psf")
    ap.add_argument("--width", type=int, help="cell width (default: the BDF's)")
    ap.add_argument("--height", type=int, help="cell height (default: the BDF's)")
    a = ap.parse_args()

    fbb, glyphs = parse_bdf(a.bdf)
    width = a.width or fbb[0]
    height = a.height or fbb[1]
    stride = (width + 7) // 8
    charsize = stride * height
    n = LAST - FIRST + 1

    body = bytearray()
    missing = []
    for cp in range(FIRST, LAST + 1):
        g = glyphs.get(cp)
        if g is None:
            missing.append(cp)
            body += bytes(charsize)
        else:
            body += render(fbb, g[0], g[1], width, height)

    header = PSF2_MAGIC + struct.pack(
        "<IIIIIII",
        0,          # version
        32,         # headersize
        0,          # flags: no Unicode table
        n,          # length
        charsize,
        height,
        width,
    )
    with open(a.psf, "wb") as fh:
        fh.write(header)
        fh.write(body)
    if missing:
        print(f"warning: {len(missing)} codepoints missing: "
              + " ".join(hex(c) for c in missing), file=sys.stderr)
    print(f"{a.psf}: {n} glyphs, {width}x{height} cell, "
          f"{charsize} B/glyph, {32 + len(body)} B total")


if __name__ == "__main__":
    main()
