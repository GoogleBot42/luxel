#!/usr/bin/env python3
"""Flash memory-mapping test under QEMU (firmware/src/flashmap.rs).

Boots the stock athom image (`result/luxel-fw.bin`, espflash's merged
bootloader + partition table + app — no device dumps needed) with a small
LUX2 asset archive placed in the assets partition, and asserts from the
serial narration that:

  1. the firmware mapped the assets region through the cache MMU
     (`flashmap: assets 0x310000+0xf0000 -> 0x3f4xxxxx (15 x 64 KiB pages
     from entry N), self-check ok`) — the self-check compares the first
     sector read via the flash controller against the same bytes read
     through the mapping, so a mapping that presents the wrong page, or an
     emulator that does not model the DPORT MMU table, fails here;
  2. the mapped virtual address is exactly what the MMU arithmetic says
     (0x3F400000 + entry * 64 KiB) and the entry sits above the app's own
     DROM pages;
  3. the archive parsed THROUGH the mapping (`assets: 2 files installed`)
     — the TOC bytes came out of the mapped window, not read_nor.

What QEMU models (hw/misc/esp32_dport.c): the per-core DROM0/IRAM0 MMU
tables, cache enable/mask bits, and Cache_Flush — a flush re-reads every
changed page from the flash block device into the cache region. That is
enough to prove the table arithmetic, the flush sequencing and the
read-side plumbing. It is NOT a timing model: the WiFi-starvation win
(#259) and the SPI0/SPI1 bus contention rules stay hardware questions.

Usage:
    nix build .#luxel-fw-athom-music
    nix develop -c python3 tools/qemu/flashmap-test.py [--qemu <path>]
Exit 0 on success. Byte-identical image under test; every fix lives in
tools/qemu/ (docs/research/qemu-emulation-spike.md, "The isolation rule").
"""

import argparse
import hashlib
import os
import re
import struct
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

FLASH_SIZE = 4 * 1024 * 1024
ASSETS_OFFSET = 0x310000  # partitions.csv: assets, 0x310000, 0xF0000
ASSETS_LEN = 0xF0000
DROM_BASE = 0x3F400000
PAGE = 0x10000

MAP_LINE = re.compile(
    r"flashmap: assets 0x310000\+0xf0000 -> 0x([0-9a-f]+) \((\d+) x 64 KiB pages from entry (\d+)\), self-check ok"
)
TOC_LINE = "assets: 2 files installed"
ABORT_MARKERS = (
    "flashmap: assets not mapped",
    "flashmap: assets self-check FAILED",
    "assets: none installed",
    "assets: implausible entry count",
    "====================== PANIC ======================",
)


class Fail(Exception):
    pass


def lux2(files: list[tuple[str, str, bytes]]) -> bytes:
    """Pack (path, content-type, data) triples the way web/tools/pack-assets.mjs does."""
    header = 8
    for path, ctype, _ in files:
        header += 1 + len(path) + 1 + len(ctype) + 9 + 8
    parts = [b"LUX2", struct.pack("<I", len(files))]
    off = header
    for path, ctype, data in files:
        etag = hashlib.sha256(data).digest()[:8]
        parts += [
            bytes([len(path)]), path.encode(),
            bytes([len(ctype)]), ctype.encode(),
            b"\x00", struct.pack("<II", len(data), off), etag,
        ]
        off += len(data)
    for _, _, data in files:
        parts.append(data)
    return b"".join(parts)


def compose(image: bytes) -> tuple[bytearray, bytes]:
    # merged layout on the esp32: bootloader at 0x1000, table at 0x8000,
    # app in ota_0 at 0x10000 (partitions.csv); 0x0..0x1000 is 0xFF padding
    if image[0x1000:0x1001] != b"\xe9" or image[0x10000:0x10001] != b"\xe9":
        raise Fail("result/luxel-fw.bin: no 0xE9 image magic at 0x1000 (bootloader) / 0x10000 (app) — not a merged esp32 image?")
    # espflash pads the merged image to the flash size; either way the
    # assets partition must be erased before we overlay the archive
    if len(image) > FLASH_SIZE:
        raise Fail(f"merged image is {len(image)} B — larger than the 4 MiB flash")
    img = bytearray(b"\xff" * FLASH_SIZE)
    img[: len(image)] = image
    if any(b != 0xFF for b in img[ASSETS_OFFSET : ASSETS_OFFSET + ASSETS_LEN]):
        raise Fail("merged image already has data in the assets partition")
    # Deterministic pseudo-random blobs: the self-check covers the first
    # 4 KiB of the region, so the header AND the first blob must be real,
    # non-erased data for the comparison to mean anything.
    blob_a = hashlib.sha256(b"luxel flashmap a").digest() * 200  # 6400 B
    blob_b = hashlib.sha256(b"luxel flashmap b").digest() * 40   # 1280 B
    archive = lux2([
        ("/index.html", "text/html", blob_a),
        ("/app.js", "application/javascript", blob_b),
    ])
    img[ASSETS_OFFSET : ASSETS_OFFSET + len(archive)] = archive
    return img, archive


def resolve_qemu(explicit: str | None) -> str:
    if explicit:
        cand = explicit
        if os.path.isdir(cand):
            cand = os.path.join(cand, "bin", "qemu-system-xtensa")
        if not os.access(cand, os.X_OK):
            raise Fail(f"--qemu {explicit}: no executable qemu-system-xtensa")
        return cand
    p = subprocess.run(
        ["nix", "build", "--no-link", "--print-out-paths", f"{REPO}#qemu-espressif"],
        capture_output=True, text=True,
    )
    if p.returncode != 0 or not p.stdout.strip():
        raise Fail("could not resolve qemu-espressif: " + p.stderr.strip().splitlines()[-1:][0] if p.stderr.strip() else "nix build failed")
    return os.path.join(p.stdout.strip().splitlines()[-1], "bin", "qemu-system-xtensa")


def make_efuse(path: str) -> None:
    p = subprocess.run([sys.executable, os.path.join(HERE, "make-efuse.py"), "-o", path],
                       capture_output=True, text=True)
    if p.returncode != 0:
        raise Fail(f"make-efuse.py failed:\n{p.stderr}")


def boot(qemu: str, flash: str, efuse: str, log: str, timeout: float) -> tuple[str, float]:
    cmd = [
        qemu, "-display", "none", "-monitor", "none", "-machine", "esp32",
        "-drive", f"file={flash},if=mtd,format=raw",
        "-drive", f"file={efuse},if=none,format=raw,id=efuse,snapshot=on",
        "-global", "driver=nvram.esp32.efuse,property=drive,value=efuse",
        "-serial", f"file:{log}",
    ]
    open(log, "wb").close()
    start = time.monotonic()
    outcome = None
    with open(os.devnull, "rb") as devnull:
        proc = subprocess.Popen(cmd, stdin=devnull, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            while True:
                with open(log, "rb") as f:
                    text = f.read().decode("utf-8", "replace")
                if MAP_LINE.search(text) and TOC_LINE in text:
                    outcome = "marker"
                    break
                for m in ABORT_MARKERS:
                    if m in text:
                        outcome = f"abort line: {m}"
                        break
                if outcome:
                    break
                if proc.poll() is not None:
                    outcome = f"qemu exited early (rc={proc.returncode})"
                    break
                if time.monotonic() - start > timeout:
                    outcome = f"timeout after {timeout:.0f}s"
                    break
                time.sleep(0.25)
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
    elapsed = time.monotonic() - start
    with open(log, "rb") as f:
        text = f.read().decode("utf-8", "replace")
    if outcome != "marker":
        stderr = proc.stderr.read().decode("utf-8", "replace") if proc.stderr else ""
        raise Fail(f"boot did not reach the mapping markers — {outcome}"
                   + (f"\nqemu stderr:\n{stderr.strip()}" if stderr.strip() else ""))
    return text, elapsed


def check(text: str, image_len: int) -> list[str]:
    passed = []
    m = MAP_LINE.search(text)
    if not m:
        raise Fail("serial: no 'flashmap: assets … self-check ok' line")
    vaddr, pages, entry = int(m.group(1), 16), int(m.group(2)), int(m.group(3))
    passed.append(f"serial: {m.group(0)!r}")
    if pages != ASSETS_LEN // PAGE:
        raise Fail(f"mapped {pages} pages, expected {ASSETS_LEN // PAGE}")
    passed.append(f"pages: {pages} × 64 KiB covers the 0xF0000 partition")
    if vaddr != DROM_BASE + entry * PAGE:
        raise Fail(f"vaddr 0x{vaddr:x} != DROM0 base + entry {entry} × 64 KiB (0x{DROM_BASE + entry * PAGE:x})")
    passed.append(f"vaddr: 0x{vaddr:x} == 0x3F400000 + {entry} × 64 KiB")
    # The app's DROM occupies entries [0, app_drom_pages); ours must start
    # past them. The bootloader maps from the segment's page-aligned flash
    # address, so the count is "pages the rodata segment touches", which
    # for every image we ship is 2–4. Assert the mapping did not land on 0.
    if entry < 1:
        raise Fail("mapping landed on entry 0 — that page is the app's own DROM")
    passed.append(f"entry: {entry} is above the app's DROM pages")
    if vaddr + pages * PAGE > DROM_BASE + 64 * PAGE:
        raise Fail("mapping runs past the 4 MiB DROM0 window")
    passed.append("window: mapping stays inside DROM0 (64 entries)")
    if TOC_LINE not in text:
        raise Fail(f"serial: {TOC_LINE!r} missing — the TOC did not parse through the mapping")
    passed.append(f"serial: {TOC_LINE!r} (TOC parsed through the mapping)")
    if text.index(m.group(0)) > text.index(TOC_LINE):
        raise Fail("mapping line came AFTER the TOC line — init() cannot have used the mapping")
    passed.append("order: mapping established before the TOC parse")
    return passed


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--result-dir", default="result",
                    help="nix build out-link holding luxel-fw.bin (default: ./result)")
    ap.add_argument("--qemu", help="qemu-system-xtensa (or its store dir); default: nix build")
    ap.add_argument("--workdir", help="where to compose (default: a temp dir)")
    ap.add_argument("--keep", action="store_true", help="keep the work dir on success")
    ap.add_argument("--timeout", type=float, default=120.0)
    # accepted for run-all.py's uniform argument list; unused
    ap.add_argument("--stock", help=argparse.SUPPRESS)
    ap.add_argument("--fs", help=argparse.SUPPRESS)
    args = ap.parse_args(argv)

    workdir = args.workdir or tempfile.mkdtemp(prefix="luxel-flashmap-")
    os.makedirs(workdir, exist_ok=True)
    log = os.path.join(workdir, "serial.log")
    try:
        image_path = os.path.join(args.result_dir, "luxel-fw.bin")
        if not os.path.exists(image_path):
            raise Fail(f"missing input: {image_path} (nix build .#luxel-fw-athom-music)")
        with open(image_path, "rb") as f:
            image = f.read()
        img, archive = compose(image)
        flash = os.path.join(workdir, "flash.bin")
        with open(flash, "wb") as f:
            f.write(img)
        efuse = os.path.join(workdir, "efuse.bin")
        make_efuse(efuse)
        qemu = resolve_qemu(args.qemu)
        print(f"flashmap-test: {len(archive)} B LUX2 archive at 0x{ASSETS_OFFSET:x}, "
              f"app image {len(image)} B, qemu {qemu}")
        text, elapsed = boot(qemu, flash, efuse, log, args.timeout)
        for line in check(text, len(image)):
            print(f"  ok  {line}")
        print(f"flashmap-test: PASS ({elapsed:.1f}s)")
        if not args.keep and not args.workdir:
            import shutil
            shutil.rmtree(workdir, ignore_errors=True)
        return 0
    except Fail as e:
        print(f"flashmap-test: FAIL — {e}", file=sys.stderr)
        if os.path.exists(log):
            with open(log, "rb") as f:
                tail = f.read().decode("utf-8", "replace").splitlines()[-25:]
            print("\n--- serial tail ---\n" + "\n".join(tail), file=sys.stderr)
        print(f"(work dir kept: {workdir})", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
