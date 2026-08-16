#!/usr/bin/env python3
"""Generate an ESP32 (LX6) eFuse backing image for Espressif's QEMU fork.

Why: QEMU's esp32 machine boots with all-zero eFuses, which reads back as
chip revision v0.0.  esp-hal's `ESP_HAL_CONFIG_MIN_CHIP_REVISION` gate
(default v3.0, i.e. the value every release build ships with) then panics
before `main`, and the ESP-IDF second-stage bootloader refuses images whose
header declares a minimum revision.  Presenting rev v3.0 lets *stock*,
unmodified firmware images boot under emulation — no build-time override,
and none of the rev-0 errata workarounds run.

Image format (from espressif/qemu `hw/nvram/esp32_efuse.c`): the device
`nvram.esp32.efuse` pread()s `sizeof(Esp32EfuseRegs)` = 124 bytes from
offset 0 of its drive at every reset, straight into the read registers.
The layout is a bare little-endian u32 array:

    blk0[7]  blk1[8]  blk2[8]  blk3[8]      (31 words, 124 bytes)

i.e. word N of block B is what `EFUSE_BLK<B>_RDATA<N>_REG` reads back.

Usage:
    tools/qemu/make-efuse.py [-o efuse.bin] [--mac 24:0a:c4:00:00:01]
                             [--rev 3.0] [--pad 512]

    qemu-system-xtensa -nographic -machine esp32 \
        -drive file=flash.bin,if=mtd,format=raw \
        -drive file=efuse.bin,if=none,format=raw,id=efuse \
        -global driver=nvram.esp32.efuse,property=drive,value=efuse

Chip-revision references (ESP-IDF `components/efuse/esp32/esp_efuse_table.csv`,
mirrored in esp-hal `esp-hal/src/efuse/esp32/fields.rs`):

    CHIP_VER_REV1        BLK0 bit 111, 1 bit   (word 3, bit 15)
    CHIP_VER_REV2        BLK0 bit 180, 1 bit   (word 5, bit 20)
    WAFER_VERSION_MINOR  BLK0 bit 184, 2 bits  (word 5, bits 24..25)

and the major version is assembled from three bits, not two — ESP-IDF
`components/hal/esp32/efuse_hal.c` / esp-hal `efuse::esp32::major_chip_version`:

    combine = (eco_bit2 << 2) | (CHIP_VER_REV2 << 1) | CHIP_VER_REV1
    0 -> v0,  1 -> v1,  3 -> v2,  7 -> v3,  else v0

`eco_bit2` is bit 31 of APB_CTRL_DATE_REG (0x3FF7_007C), *not* an eFuse.
QEMU already hardwires that bit set (`hw/xtensa/esp32.c`: "Emulation of
APB_CTRL_DATE_REG, needed for ECO3 revision detection"), so the eFuse image
only has to supply REV1=REV2=1 to reach combine==7 -> major 3.  Minor stays
0 -> v3.0.  (Consequently rev v1/v2 are *not* reachable on this QEMU: with
eco_bit2 stuck at 1, combine is always >= 4.)

Everything else is left at 0, which is the correct "blank chip" value:
coding scheme NONE, flash encryption off, secure boot off, no SPI pad
overrides, JTAG/download-mode enabled.  The one exception is the factory
MAC (BLK0 bits 32..79) plus its CRC8 (bits 80..87), which is written so
esp-hal/ESP-IDF don't derive an all-zero base MAC and so the CRC check in
`esp_efuse_mac_get_default()` passes.
"""

from __future__ import annotations

import argparse
import struct
import sys

# --- eFuse image geometry (hw/nvram/esp32_efuse.h, struct Esp32EfuseRegs) ---
BLK_WORDS = (7, 8, 8, 8)  # blk0 .. blk3
N_WORDS = sum(BLK_WORDS)  # 31
IMAGE_SIZE = N_WORDS * 4  # 124 bytes

# --- field table: (block, bit offset within block, width) -------------------
# Values from ESP-IDF components/efuse/esp32/esp_efuse_table.csv.
MAC = (0, 32, 48)  # byte-reversed, see set_mac()
MAC_CRC = (0, 80, 8)
CHIP_VER_REV1 = (0, 111, 1)
CHIP_VER_REV2 = (0, 180, 1)
WAFER_VERSION_MINOR = (0, 184, 2)


class Efuse:
    def __init__(self) -> None:
        self.words = [0] * N_WORDS

    def _base(self, block: int) -> int:
        return sum(BLK_WORDS[:block])

    def set_field(self, field: tuple[int, int, int], value: int) -> None:
        block, off, width = field
        if value >= (1 << width):
            raise ValueError(f"value {value:#x} does not fit in {width} bits")
        base = self._base(block)
        for i in range(width):
            if (value >> i) & 1:
                bit = off + i
                self.words[base + bit // 32] |= 1 << (bit % 32)

    def set_mac(self, mac: bytes) -> None:
        """Factory MAC + CRC8.

        The CSV spells MAC out as six 8-bit slices in *descending* bit order:
        mac[0] at bit 72, mac[1] at 64, ... mac[5] at 32.  So the 48-bit field
        holds the MAC byte-reversed.
        """
        if len(mac) != 6:
            raise ValueError("MAC must be 6 bytes")
        packed = int.from_bytes(mac[::-1], "little")  # == big-endian of mac
        self.set_field(MAC, packed)
        self.set_field(MAC_CRC, esp_crc8(mac))

    def to_bytes(self, pad_to: int = 0) -> bytes:
        blob = struct.pack("<%dI" % N_WORDS, *self.words)
        if pad_to > len(blob):
            blob += b"\x00" * (pad_to - len(blob))
        return blob


def esp_crc8(data: bytes) -> int:
    """CRC-8/MAXIM (reflected poly 0x8c, init 0) — ESP-IDF `esp_crc8()`."""
    crc = 0
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ 0x8C if crc & 1 else crc >> 1
    return crc & 0xFF


def parse_mac(text: str) -> bytes:
    parts = text.replace("-", ":").split(":")
    if len(parts) != 6:
        raise argparse.ArgumentTypeError("MAC must be six colon-separated bytes")
    return bytes(int(p, 16) for p in parts)


def parse_rev(text: str) -> tuple[int, int]:
    major, _, minor = text.partition(".")
    return int(major), int(minor or 0)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("-o", "--output", default="efuse.bin", help="output image path")
    ap.add_argument(
        "--rev",
        type=parse_rev,
        default=(3, 0),
        metavar="MAJOR.MINOR",
        help="chip revision to present (default 3.0; only 3.x is reachable "
        "under QEMU, which hardwires APB_CTRL_DATE_REG bit 31)",
    )
    ap.add_argument(
        "--mac",
        type=parse_mac,
        default=parse_mac("24:0a:c4:00:00:01"),
        help="factory MAC (default 24:0a:c4:00:00:01, an Espressif OUI)",
    )
    ap.add_argument(
        "--pad",
        type=int,
        default=512,
        help="pad the image to N bytes (default 512: QEMU's raw block backend "
        "is happier with a sector-multiple file; 0 = exact 124 bytes)",
    )
    args = ap.parse_args(argv)

    major, minor = args.rev
    if major != 3:
        print(
            f"warning: major revision {major} is not reachable under QEMU "
            "(APB_CTRL_DATE_REG bit 31 is hardwired set); the guest will "
            "read back v0.x",
            file=sys.stderr,
        )

    e = Efuse()
    # combine == 7 -> major 3, given QEMU's eco_bit2 == 1
    e.set_field(CHIP_VER_REV1, 1 if major >= 1 else 0)
    e.set_field(CHIP_VER_REV2, 1 if major >= 2 else 0)
    e.set_field(WAFER_VERSION_MINOR, minor & 0x3)
    e.set_mac(args.mac)

    blob = e.to_bytes(args.pad)
    with open(args.output, "wb") as f:
        f.write(blob)

    mac_str = ":".join(f"{b:02x}" for b in args.mac)
    print(
        f"{args.output}: {len(blob)} bytes, chip revision v{major}.{minor}, "
        f"MAC {mac_str} (crc8 {esp_crc8(args.mac):#04x})"
    )
    for b, n in enumerate(BLK_WORDS):
        base = sum(BLK_WORDS[:b])
        nonzero = {
            i: f"{e.words[base + i]:#010x}" for i in range(n) if e.words[base + i]
        }
        if nonzero:
            print(f"  blk{b}: " + " ".join(f"w{i}={v}" for i, v in nonzero.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
