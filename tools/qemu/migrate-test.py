#!/usr/bin/env python3
"""Compose → boot → assert the self-applied partition migration under emulation.

Hardware-free end-to-end test for `firmware/src/migrate.rs` (Gitea #501): builds
a flash image in exactly the state a field device is in the instant it finishes
receiving the migrating release over OTA — the **pre-#501 1 MiB-slot partition
table** at 0x8000, a real pattern store at the old `storage` offset, real WiFi
credentials in nvs — boots it under Espressif's patched QEMU (see
docs/research/qemu-emulation-spike.md), and asserts on both the migrator's
serial narration and the resulting flash bytes.

The image under test is **stock** — the same `nix build .#luxel-fw-athom-music`
output that ships. Nothing here touches firmware source; the QEMU harness is
strictly isolated (CLAUDE.md hard rule).

What it composes (4 MiB):

    0x000000  bootloader + app from result/luxel-fw.bin (the merged image)
    0x008000  a hand-built PRE-#501 partition table  <- the thing being migrated
    0x009000  LXCF WiFi credentials, LXDV device settings (a configured device)
    0x00D000  otadata — erased (--from ota_0) or seq=2 at 0xE000 (--from ota_1)
    0x010000  ota_0 under the OLD table (1 MiB slot)
    0x110000  ota_1 under the OLD table — the app image too, for --from ota_1
    0x210000  a generated `storage` partition: 11 patterns, 6 reserved blobs
    0x310000  a synthetic LUX2 web-asset bundle (must survive byte-identical)

The store image and the ground truth the store assertions run against both come
from `tools/storegen` — a host binary that writes the key area with the real
`sequential-storage` crate and the pattern log with the real
`firmware/src/patlog.rs`, then reads the post-run flash back and checks it. A
second Python implementation of two on-flash formats would be a test of the
test; see that crate's module docs.

Variants
--------

    --from ota_0  (default)  the device is running out of the slot the new
                  table also calls ota_0.  No self-copy: the migration goes
                  straight to staging.  One QEMU boot, one reboot.  This is
                  the whole happy path.
    --from ota_1  the device is running out of the OLD ota_1 (0x110000) — the
                  slot the new table needs as staging scratch.  The migrator
                  must copy itself into the new ota_0 at 0x10000 and reboot
                  before it can touch it.  Three boots; the self-copy and the
                  extra reboot are asserted on top of everything --from ota_0
                  asserts.  See "The self-copy path" below for why this
                  variant is the most load-bearing one in the file.

    --cut <stage>  kill QEMU mid-migration at `stage`, then RE-RUN it on the
                  same flash file and assert the migration still completes with
                  every byte intact.  Stages:
                     copy    after the self-copy and the otadata erase, before
                             staging (needs --from ota_1)
                     staged  in the middle of the staging page writes
                     stored  the instant the new store is written, before the
                             staging header records it
                     assets  after the store is marked done, before the table
                             — the narrowest window here, retried up to
                             `--cut-attempts` times because the harness only
                             wins that race about half the time
                  `table` is deliberately NOT offered — see "The window we do
                  not test" below.

    --overfill    generate a store whose LIVE log content (618,496 B) cannot
                  fit the 4 MB layout's 225,280-byte log, and assert the
                  migration REFUSES: old table still on flash, old storage
                  partition byte-identical, nothing touched.

The self-copy path
------------------

`migrate::settle_into_ota0` copies the running image into the new ota_0 when it
is executing somewhere else.  Which slot a device is in is not a detail — OTA
alternates, so at any moment about half the fleet is in ota_1:

  * old ota_0 (0x10000) IS new ota_0 (0x10000), so a device running there needs
    no copy at all — that is `--from ota_0`;
  * old ota_1 (0x110000) is where the new table's 1.25 MiB ota_0
    (0x10000..0x150000) reaches, so the migrator has to move itself down before
    it can use that slot as staging — that is `--from ota_1`.

This is the variant that earned its keep.  The first version of the overlap
guard compared the running image against the destination SLOT length rather
than the COPY length; since 0x110000 lies inside 0x10000+0x140000 it refused
every ota_1 device with `BLOCKED — image overlaps the new ota_0`, which this
test caught on its first run and which would otherwise have shipped as "half
the fleet silently never migrates".  Keep `--from ota_1` in the default suite.

The window we do not test
-------------------------

A cut *inside* the single 4 KiB partition-table sector write is unrecoverable
by design and by arithmetic: the old table's sector is erased before the new
one is programmed, the ESP-IDF second-stage bootloader will not boot a table
whose MD5 row does not verify, and stock IDF keeps no second copy. migrate.rs'
module docs say so out loud and docs/firmware.md repeats it. There is nothing a
test could assert there except "the device is bricked", so `--cut table` does
not exist. Every other stage IS covered, and each is re-runnable by
construction (the `LXMG` staging header at the head of the staging slot).

The 16 MB layout
----------------

    --board s3    compose and boot the SEENGREAT image
                  (`nix build .#luxel-fw-seengreat-hub75`, 16 MiB) on QEMU's
                  `esp32s3` machine and migrate it to
                  `firmware/partitions-16mb.csv`.  Everything `--from`,
                  `--cut` and `--overfill` do works here too; the difference
                  that matters is that this is the ONLY layout whose `assets`
                  partition moves, so it is the only one that reaches
                  `migrate::move_assets`' copy branch — the stage the
                  Seengreat's 2026-09-21 decline was suspected in (Gitea
                  #634).

`--plan-16mb` remains as the assertion-only model of the same layout (the
table encoding and the host store move); it needs no emulator and runs in a
second, so both are kept.

The bootloader ceiling, and the two-hop board
---------------------------------------------

    --old-bootloader 4mb   stamp the composed image's BOOTLOADER header for a
                  smaller part.  `g_rom_flashchip.chip_size` comes from that
                  header and an OTA never replaces the bootloader, so this is
                  the Seengreat as found on 2026-09-21 (Gitea #634): 16 MB of
                  silicon whose ROM bounds-checks every flash op at 4 MB and
                  whose bootloader would refuse to BOOT under a table reaching
                  past it.  The device does not refuse to migrate — it takes
                  the largest embedded layout that fits under the ceiling,
                  which is `partitions.csv`, through the identical code the
                  Athom ran — and narrates the ceiling on every boot.  Works
                  with `--from` and `--cut`, because the fallback is an
                  ordinary migration in every other respect.

    --reflash-bootloader 16mb   then restamp that header on the SAME flash and
                  boot again: Jeremy's one-time serial re-flash.  The ceiling
                  rises, `parttab::target_table` answers with the board's own
                  table, and the migrator runs a SECOND time — storage
                  0x290000 -> 0x610000, assets 0x310000 -> 0xa10000, staged in
                  the 4 MB layout's ota_1 rather than the pre-#501 one.  This
                  is the variant that proves `migrated: true` never means
                  "stop looking"; a board that fell back once and then stuck
                  on the small table forever is the failure this design is
                  built to avoid.

QEMU quirks you will see in the log: the SW_RESET boot that `software_reset()`
triggers dies partway through the ROM banner, the TG0 watchdog fires, and
*that* reset is the one that actually loads the app. It happens before the
second-stage bootloader touches anything, so it is cosmetic. Assertions key off
migration output and flash bytes, never reset counts.

Usage:
    nix build .#luxel-fw-athom-music
    nix develop -c python3 tools/qemu/migrate-test.py
    nix develop -c python3 tools/qemu/migrate-test.py --from ota_1
    nix develop -c python3 tools/qemu/migrate-test.py --from ota_1 --cut copy
    nix develop -c python3 tools/qemu/migrate-test.py --cut staged --keep
    nix develop -c python3 tools/qemu/migrate-test.py --cut stored
    nix develop -c python3 tools/qemu/migrate-test.py --overfill
    nix develop -c python3 tools/qemu/migrate-test.py --plan-16mb
    nix build .#luxel-fw-seengreat-hub75 --out-link result-s3
    nix develop -c python3 tools/qemu/migrate-test.py --board s3
    nix develop -c python3 tools/qemu/migrate-test.py --board s3 --from ota_1
    nix develop -c python3 tools/qemu/migrate-test.py --board s3 \
        --old-bootloader 4mb
    nix develop -c python3 tools/qemu/migrate-test.py --board s3 \
        --old-bootloader 4mb --reflash-bootloader 16mb

Exit 0 with a PASS summary listing every assertion; nonzero on the first
failure, with a tail of the serial log.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

# takeover-test.py owns the shared emulator plumbing (hyphen -> importlib).
_spec = importlib.util.spec_from_file_location(
    "takeover_test", os.path.join(HERE, "takeover-test.py"))
_tt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_tt)
resolve_qemu = _tt.resolve_qemu
make_efuse = _tt.make_efuse
otadata_entry = _tt.otadata_entry
first_diff = _tt.first_diff
Checks = _tt.Checks

# flashmap-test.py owns the LUX2 asset-bundle packer.
_fspec = importlib.util.spec_from_file_location(
    "flashmap_test", os.path.join(HERE, "flashmap-test.py"))
_fm = importlib.util.module_from_spec(_fspec)
_fspec.loader.exec_module(_fm)
lux2 = _fm.lux2


class Fail(Exception):
    pass


# --------------------------------------------------------------------------
# geometry

FLASH_SIZE = 4 * 1024 * 1024
SECTOR = 0x1000
TABLE_OFFSET = 0x8000       # parttab.rs TABLE_OFFSET — fixed by the bootloader
TABLE_LEN = 0x100           # 7 entries + the MD5 row, both layouts

LXCF_OFFSET = 0x9000        # config.rs RECORD_OFFSET
LXDV_OFFSET = 0xA000        # config.rs DEV_OFFSET
MQTT_OFFSET = 0xB000        # config.rs MQTT_OFFSET
LXBG_OFFSET = 0xC000        # ota.rs GUARD_OFFSET
# nvs is 0x9000 + 0x4000 on both layouts: the four sectors above.
OTADATA = 0xD000            # both layouts: otadata, 0xd000, 0x2000
OTADATA_1 = 0xE000          # its SECOND 4 KiB entry — see otadata_note()
DEV_VER = 8                 # config.rs DEV_VER

# The PRE-#501 layout a field device carries.
OLD_OTA0 = 0x10000
OLD_OTA1 = 0x110000
OLD_STORAGE = 0x210000
OLD_STORAGE_LEN = 0x100000
# The 4 MB layout (firmware/partitions.csv) — what every board but the
# Seengreat embeds, and what the Seengreat FALLS BACK to when its bootloader
# cannot back the big table (Gitea #634).  `NEW_STORAGE`/`NEW_STORAGE_LEN`
# are rebound per run by `select_board`; these two never move.
NEW_OTA0 = 0x10000
NEW_STORAGE_4MB = 0x290000
NEW_STORAGE_LEN_4MB = 0x80000
NEW_STORAGE = NEW_STORAGE_4MB
NEW_STORAGE_LEN = NEW_STORAGE_LEN_4MB
# Unchanged across the migration, which is the 4 MB layout's entire point.
ASSETS = 0x310000
ASSETS_LEN = 0xF0000

# patterns.rs: the packed file log starts here inside the partition, on every
# layout.  The 4 MB layout's log is what `--overfill` overruns.
LOG_AT = 0x49000
NEW_LOG_LEN = NEW_STORAGE_LEN - LOG_AT   # 0x37000 = 225,280 B

# firmware/partitions-16mb.csv.
FLASH_SIZE_16MB = 16 * 1024 * 1024
S3_STORAGE = 0x610000
S3_STORAGE_LEN = 0x400000
S3_ASSETS = 0xA10000

# Which machine and which built image each board runs under.  The S3 needs no
# efuse drive: `nvram.esp32.efuse` is the classic-ESP32 device and the esp32s3
# machine has its own (`nvram.esp32s3.efuse`), whose defaults already boot.
BOARDS = {
    "athom": dict(machine="esp32", flake="luxel-fw-athom-music", efuse=True,
                  table="partitions.csv", flash=FLASH_SIZE,
                  storage=NEW_STORAGE, storage_len=NEW_STORAGE_LEN,
                  new_assets=ASSETS, bootloader=0x1000),
    # The S3's second-stage bootloader lives at 0x0, not 0x1000 — a chip
    # property, not a layout one, and the merged image reflects it.
    "s3": dict(machine="esp32s3", flake="luxel-fw-seengreat-hub75", efuse=False,
               table="partitions-16mb.csv", flash=FLASH_SIZE_16MB,
               storage=S3_STORAGE, storage_len=S3_STORAGE_LEN,
               new_assets=S3_ASSETS, bootloader=0x0),
}
# Where the bundle ends up.  On the 4 MB layout that is where it already is —
# the whole point of keeping `assets` put — and on the 16 MB one it moves.
NEW_ASSETS = ASSETS

SSID = "MOMCorp Intranet"
PASSWORD = "hypnotoad-all-glory"

STOREGEN = os.path.join(REPO, "tools", "storegen")

# --------------------------------------------------------------------------
# the partition tables


def part_entry(ptype: int, subtype: int, offset: int, size: int, label: str,
               flags: int = 0) -> bytes:
    """One 32-byte ESP-IDF partition-table entry.

    `AA 50 | type | subtype | offset u32 | size u32 | label[16] | flags u32`
    (firmware/src/parttab/raw.rs documents the same layout from the reader's
    side).  Labels are NUL-padded; `struct`'s `16s` does that for us.
    """
    if len(label) > 16:
        raise Fail(f"partition label {label!r} is longer than 16 bytes")
    return struct.pack("<BBBBII16sI", 0xAA, 0x50, ptype, subtype,
                       offset, size, label.encode(), flags)


def part_table(rows: list[tuple]) -> bytes:
    """Serialize a whole table: the entries, then the terminator row —
    `EB EB`, 14 bytes of 0xFF, and the MD5 of the entry bytes, which is what
    the second-stage bootloader verifies before it will boot anything."""
    body = b"".join(part_entry(*r) for r in rows)
    return body + b"\xeb\xeb" + b"\xff" * 14 + hashlib.md5(body).digest()


TYPE_APP, TYPE_DATA = 0x00, 0x01
SUB_OTA0, SUB_OTA1 = 0x10, 0x11
SUB_NVS, SUB_OTADATA, SUB_PHY, SUB_SPIFFS = 0x02, 0x00, 0x01, 0x82

# The table a device flashed before #501 carries.  Subtypes and labels both
# matter: migrate.rs identifies partitions BY LABEL, and `parttab::is_luxel`
# keys off the `storage` + `assets` data entries being present — that is what
# tells the migrator "an older layout of mine" apart from "WLED's flash, which
# is the takeover's job".
OLD_ROWS = [
    (TYPE_DATA, SUB_NVS,     0x9000,   0x4000,   "nvs"),
    (TYPE_DATA, SUB_OTADATA, 0xD000,   0x2000,   "otadata"),
    (TYPE_DATA, SUB_PHY,     0xF000,   0x1000,   "phy_init"),
    (TYPE_APP,  SUB_OTA0,    0x10000,  0x100000, "ota_0"),
    (TYPE_APP,  SUB_OTA1,    0x110000, 0x100000, "ota_1"),
    (TYPE_DATA, SUB_SPIFFS,  0x210000, 0x100000, "storage"),
    (TYPE_DATA, SUB_SPIFFS,  0x310000, 0xF0000,  "assets"),
]

# firmware/partitions.csv as of #501.  Built here ONLY so the encoder above can
# be checked against ground truth — see `check_encoder`.  Everything that
# asserts on the installed table compares against the merged image's own 0x8000
# sector, which espflash wrote with esp-idf-part.
NEW_ROWS_4MB = [
    (TYPE_DATA, SUB_NVS,     0x9000,   0x4000,   "nvs"),
    (TYPE_DATA, SUB_OTADATA, 0xD000,   0x2000,   "otadata"),
    (TYPE_DATA, SUB_PHY,     0xF000,   0x1000,   "phy_init"),
    (TYPE_APP,  SUB_OTA0,    0x10000,  0x140000, "ota_0"),
    (TYPE_APP,  SUB_OTA1,    0x150000, 0x140000, "ota_1"),
    (TYPE_DATA, SUB_SPIFFS,  0x290000, 0x80000,  "storage"),
    (TYPE_DATA, SUB_SPIFFS,  0x310000, 0xF0000,  "assets"),
]

# firmware/partitions-16mb.csv (`--board s3`, and `--plan-16mb`'s model).
NEW_ROWS_16MB = [
    (TYPE_DATA, SUB_NVS,     0x9000,   0x4000,   "nvs"),
    (TYPE_DATA, SUB_OTADATA, 0xD000,   0x2000,   "otadata"),
    (TYPE_DATA, SUB_PHY,     0xF000,   0x1000,   "phy_init"),
    (TYPE_APP,  SUB_OTA0,    0x10000,  0x300000, "ota_0"),
    (TYPE_APP,  SUB_OTA1,    0x310000, 0x300000, "ota_1"),
    (TYPE_DATA, SUB_SPIFFS,  0x610000, 0x400000, "storage"),
    (TYPE_DATA, SUB_SPIFFS,  0xA10000, 0x3F0000, "assets"),
]

# The table under test, and the machine to boot it on — `select_board` swaps
# both (and everything else that differs) for the 16 MB board.
NEW_ROWS = NEW_ROWS_4MB
MACHINE = "esp32"
TABLE_NAME = "partitions.csv"
# The board's OWN table — the one its merged image carries at 0x8000, which
# is what `check_encoder` weighs the hand-rolled encoder against.  It is the
# same as NEW_ROWS except in `--fallback`, where the device deliberately
# migrates to the SMALLER layout its bootloader can back.
NOMINAL_ROWS = NEW_ROWS_4MB
NOMINAL_TABLE_NAME = "partitions.csv"


def check_encoder(merged: bytes, c: Checks) -> bytes:
    """Prove the hand-rolled encoder above is honest, then hand back the
    merged image's own table sector as the expected post-migration bytes.

    Everything in this file's flash assertions rests on a table built in
    Python.  The only way that is trustworthy is to build the table we can
    check — the NEW one — and byte-compare it against the 0x8000 sector of the
    unmodified merged image, which espflash wrote with `esp-idf-part`, the same
    crate `firmware/build.rs` embeds into the firmware.  If this passes, the
    OLD table below it was encoded by the same code and is equally real.
    """
    truth = merged[TABLE_OFFSET:TABLE_OFFSET + TABLE_LEN]
    mine = part_table(NOMINAL_ROWS)
    c.require(mine == truth,
              f"encoder: hand-built {NOMINAL_TABLE_NAME} byte-equals esp-idf-part's",
              f"first differing byte at {first_diff(mine, truth)}\n"
              f"  mine  {mine[:32].hex()}…\n  truth {truth[:32].hex()}…")
    tail = merged[TABLE_OFFSET + TABLE_LEN:TABLE_OFFSET + SECTOR]
    c.require(tail == b"\xff" * len(tail),
              "encoder: the table is exactly 0x100 B, the rest of the sector erased",
              f"first non-0xFF at {first_diff(tail, b'\xff' * len(tail))}")
    if NEW_ROWS is NOMINAL_ROWS:
        return truth
    # `--fallback`: the target is the OTHER embedded layout, so the expected
    # post-migration bytes are hand-built — licensed by the check just above,
    # which proved this encoder reproduces esp-idf-part exactly.
    return part_table(NEW_ROWS)


def otadata_note() -> str:
    """Which otadata sector --from ota_1 writes, and why.

    ESP-IDF's otadata partition is TWO erase sectors, each holding one
    `esp_ota_select_entry_t` at its start; the bootloader picks whichever
    entry has the higher valid sequence number and boots OTA slot
    `(seq - 1) % ota_slot_count`.  The pre-#501 partition is `otadata, 0xd000,
    0x2000`, so its second entry is at 0xd000 + 0x1000 = 0xE000.  Writing
    seq=2 there and leaving 0xd000 erased selects ota_1 with a single write.
    takeover-test.py does the identical thing one partition over
    (`WLED_OTADATA_1 = 0x0F000`, because WLED's otadata is at 0xE000).
    """
    return "0xE000 = second sector of otadata(0xd000+0x2000); seq=2 -> ota_1"


# --------------------------------------------------------------------------
# nvs records (a *configured* device — migrate must not wipe these)


def _cksum(b: bytes) -> int:
    """config.rs::checksum — a wrapping u32 sum of the bytes."""
    return sum(b) & 0xFFFFFFFF


def lxcf_record(ssid: str, password: str) -> bytes:
    """config.rs::write_wifi: "LXCF" u8 ver=1, u8 ssid_len, u8 pass_len, u8 0,
    ssid, password, u32-LE checksum."""
    rec = b"LXCF" + bytes([1, len(ssid), len(password), 0])
    rec += ssid.encode() + password.encode()
    return rec + struct.pack("<I", _cksum(rec))


def lxdv_record() -> bytes:
    """config.rs::write_device (v8), with the Athom's shipped-looking values:
    60 px WS2812, brightness 4/31, no power cap, no post-process, board-default
    data pin (byte 21 = 0)."""
    rec = b"LXDV" + bytes([DEV_VER, 4, 1, 0])      # ver, brightness, protocol, sync
    rec += struct.pack("<I", 60)                    # pixel_count
    rec += struct.pack("<h", 0)                     # tz_minutes
    rec += bytes([0, 0])                            # color_order, gamma_tenths
    rec += struct.pack("<H", 0)                     # cap_ma
    rec += bytes([0, 0, 0])                         # curve, blur, glow
    rec += bytes([0])                               # data_pin + 1 (0 = default)
    rec += bytes([0, 0])                            # pad
    return rec + struct.pack("<I", _cksum(rec))


# --------------------------------------------------------------------------
# storegen


def storegen(*args: str) -> str:
    """Run the host store generator/verifier.  `cargo run` rather than a
    prebuilt path so a checkout that has never built it still works; cargo's
    own target dir makes every run after the first instant."""
    cmd = ["cargo", "run", "--release", "--quiet", "--offline",
           "--manifest-path", os.path.join(STOREGEN, "Cargo.toml"), "--"] + list(args)
    p = subprocess.run(cmd, cwd=STOREGEN, capture_output=True, text=True)
    if p.returncode != 0:
        # --offline fails in a checkout with a cold cargo cache; retry online
        # before blaming the tool.
        if "--offline" in cmd and ("offline" in p.stderr or "no matching package" in p.stderr):
            cmd.remove("--offline")
            p = subprocess.run(cmd, cwd=STOREGEN, capture_output=True, text=True)
    if p.returncode != 0:
        raise Fail(f"storegen {' '.join(args[:1])} failed:\n{p.stdout}\n{p.stderr}")
    return p.stdout


# --------------------------------------------------------------------------
# compose


# esp_image_header_t byte 3, high nibble: the flash size espflash stamped
# into an image header.  2 = 4 MB, 4 = 16 MB.  The SECOND-STAGE BOOTLOADER's
# copy of this is what programs `g_rom_flashchip.chip_size`, and an OTA never
# replaces the bootloader — so it is the one number on a field device that
# still reflects how it was FLASHED rather than what it is running.
BOOTLOADER_FLASH_NIBBLE = {"4mb": 0x2, "8mb": 0x3, "16mb": 0x4}
BOOTLOADER_FLASH_SIZE = {"4mb": 4 << 20, "8mb": 8 << 20, "16mb": 16 << 20}


def compose(merged: bytes, ota: bytes, store: bytes, assets: bytes,
            from_slot: str, old_bootloader: str | None = None) -> bytearray:
    boot_at = BOARD["bootloader"]
    if merged[boot_at:boot_at + 1] != b"\xe9" or merged[0x10000:0x10001] != b"\xe9":
        raise Fail(f"luxel-fw.bin: no 0xE9 image magic at {boot_at:#x} "
                   "(bootloader) / 0x10000 (app) — not a merged esp32 image?")
    if len(merged) > FLASH_SIZE:
        raise Fail(f"merged image is {len(merged)} B — larger than the "
                   f"{FLASH_SIZE // (1024 * 1024)} MiB flash")
    if ota[:1] != b"\xe9":
        raise Fail("result/luxel-fw-ota.bin does not start with the 0xE9 image magic")
    if len(store) != OLD_STORAGE_LEN:
        raise Fail(f"generated store is {len(store)} B, expected {OLD_STORAGE_LEN}")
    if len(ota) > 0x100000:
        raise Fail(f"the app image is {len(ota)} B — it does not fit a pre-#501 "
                   "1 MiB OTA slot, so this fixture cannot represent a field device")

    img = bytearray(b"\xff" * FLASH_SIZE)
    img[:len(merged)] = merged

    # The thing under test: roll the table back to the layout the device had.
    img[TABLE_OFFSET:TABLE_OFFSET + SECTOR] = b"\xff" * SECTOR
    old = part_table(OLD_ROWS)
    img[TABLE_OFFSET:TABLE_OFFSET + len(old)] = old

    # A configured device.  The migration deliberately does NOT do the
    # takeover's config wipe — nvs holds the WiFi credentials, and a device
    # that came back without them would be unreachable (nothing on the bench
    # has a serial path).  Asserting these survive is half the point.
    img[LXCF_OFFSET:LXCF_OFFSET + SECTOR] = b"\xff" * SECTOR
    rec = lxcf_record(SSID, PASSWORD)
    img[LXCF_OFFSET:LXCF_OFFSET + len(rec)] = rec
    img[LXDV_OFFSET:LXDV_OFFSET + SECTOR] = b"\xff" * SECTOR
    rec = lxdv_record()
    img[LXDV_OFFSET:LXDV_OFFSET + len(rec)] = rec

    # otadata.  Erased selects ota_0 with no factory partition (and the
    # bootloader writes the choice back as seq=1/VALID, which is what the
    # post-run assertion expects).
    img[OTADATA:OTADATA + 0x2000] = b"\xff" * 0x2000
    if from_slot == "ota_1":
        # Running out of the OLD ota_1 — the slot the new table swallows into
        # ota_0.  The app has to be THERE and NOT at 0x10000, or the migrator
        # would (correctly) recognise its own image already in place and skip
        # the self-copy this variant exists to exercise.
        img[OLD_OTA0:OLD_OTA1] = b"\xff" * (OLD_OTA1 - OLD_OTA0)
        img[OLD_OTA1:OLD_OTA1 + len(ota)] = ota
        img[OTADATA_1:OTADATA_1 + 32] = otadata_entry(2)

    img[OLD_STORAGE:OLD_STORAGE + len(store)] = store
    img[ASSETS:ASSETS + len(assets)] = assets

    # A device flashed before its table grew: restamp the bootloader's image
    # header so the ROM (and the bootloader's own table check) believe the
    # part is that size.  One byte, no checksum to fix — esptool patches this
    # same nibble at flash time, and the bootloader's SHA-256 is only enforced
    # under secure boot (the guest prints "Attempting to boot anyway").
    if old_bootloader:
        boot_at = BOARD["bootloader"]
        nibble = BOOTLOADER_FLASH_NIBBLE[old_bootloader]
        img[boot_at + 3] = (img[boot_at + 3] & 0x0F) | (nibble << 4)
    return img


def make_assets() -> bytes:
    """A small, deterministic LUX2 bundle for the `assets` partition.

    The merged image leaves `assets` erased, and "0xFF is still 0xFF" is a weak
    way to assert that the 4 MB layout leaves the web bundle alone.  Real bytes
    there make the claim mean something — and they also let the boot log prove
    the partition is still readable under the NEW table, since `assets::init`
    parses the TOC out of it on every boot.
    """
    index = b"<!doctype html><title>luxel</title><body>migrate fixture</body>"
    blob = bytes((i * 37 + 11) & 0xFF for i in range(8192))
    return lux2([("/index.html", "text/html", index),
                 ("/app.js", "application/javascript", blob)])


# --------------------------------------------------------------------------
# emulator

# The narration migrate.rs prints.  Grep it for these; they are the contract.
# The ones that name an offset or a table are rebuilt by `select_board`.
M_START = "migrate: partition table on flash is an older Luxel layout — moving to partitions.csv"
M_REBOOT_OTA0 = "migrate: rebooting into ota_0 to free the staging slot"
M_ALREADY = f"migrate: image already at {NEW_OTA0:#x}"
M_ERASING = f"migrate: new storage {NEW_STORAGE:#x} + {NEW_STORAGE_LEN // 1024} KiB — erasing"
# The every-boot line a board behind an older bootloader prints (migrate.rs'
# boot snapshot) — the serial face of `/api/status`' `upgrade_available`.
M_UPGRADE = ("partitions: this board's BOOTLOADER caps flash at {have} B of the "
             "{cap} B the chip holds, so {target} is the largest layout it can "
             "boot — re-flash the bootloader over serial to unlock {nominal}")
M_RELOCATED = "migrate: store relocated"
M_ASSETS_STAY = f"migrate: assets stay at {ASSETS:#x} — nothing to move"
M_ASSETS_MOVE = f"migrate: moving assets {ASSETS:#x} → {NEW_ASSETS:#x}"
M_INSTALLING = "migrate: installing the new partition table"
M_INSTALLED = "migrate: partition table installed — rebooting into the new layout"
M_BLOCKED_OVERFILL = ("migrate: BLOCKED — pattern library too large for the new layout"
                      f" (need {{need}} B, have {NEW_LOG_LEN} B)")
# The refusal a device whose BOOTLOADER predates its table produces when it
# embeds NO layout that fits under the ceiling.  Since Gitea #634 the 16 MB
# board also embeds the 4 MB table, so this is what must NOT appear there —
# the board falls back instead of refusing.  (It stays reachable in principle:
# a part smaller than the 4 MB layout, which no bootloader could boot far
# enough to reach anyway, so it has no emulated fixture.)
M_BLOCKED_BOOTLOADER = ("migrate: BLOCKED — bootloader was flashed for a smaller part"
                        " — reflash it over serial (need {need} B, have {have} B)")

# Anything here means the migration gave up; fail fast rather than burning the
# whole timeout waiting for a marker that will not come.
ABORT_MARKERS = (
    "migrate: BLOCKED —",
    "migrate: self-copy failed",
    "migrate: store relocation failed",
    "migrate: asset move failed",
    "migrate: staging write failed",
    "migrate: could not build log page",
    "migrate: TABLE WRITE FAILED",
    "migrate: verify failed at",
    "migrate: read failed at",
    "partitions: flash too small",
    "partitions: erase failed at",
    "patterns: no storage partition",
    "patterns: storage partition unusable",
    "consecutive failed boots — rolling back",
    "====================== PANIC ======================",
)

ABORT_HINTS = {
    "partitions: flash too small": (
        "this is a NEW QEMU divergence, not a firmware bug: esp-storage's "
        "FlashStorage::capacity() has previously reported the real 4 MiB "
        "under emulation.  Report it rather than working around it."
    ),
    "migrate: BLOCKED — pattern store did not come up": (
        "the COMPOSED storage image is the suspect, not the migrator — "
        "check the `patterns:` line just above it in the log.  Regenerate "
        "with tools/storegen and compare its self-check output."
    ),
    "patterns: storage partition unusable": (
        "the pre-#501 `storage` entry in OLD_ROWS is malformed (patterns.rs "
        "wants >= MIN_STORAGE and both bounds 64 KiB-aligned)."
    ),
}

# The migration is done and the device came back on the NEW table when the
# pattern store reports itself resolved at the new offset.  That line is
# printed by `patterns::init`, well before WiFi (which panics under emulation
# inside the esp-radio PHY blob — expected, documented in the spike).
DONE_MARKER = f"(storage @ {NEW_STORAGE:#x})"


def select_board(board: str, target: str | None = None) -> None:
    """Point every layout-dependent constant above at `board`'s tables.

    The two boards run the SAME migrator over different geometry — migrate.rs
    contains no partition offsets at all — so the test is the same test, with
    the numbers swapped.  Rebinding module globals here rather than threading
    a layout object through thirty call sites keeps that symmetry visible and
    the diff against the 4 MB original readable.

    `target` names the layout the device is expected to END UP on when that is
    not the board's own — `"partitions.csv"` on the 16 MB board is the
    FALLBACK case (Gitea #634): 16 MB of silicon behind a bootloader flashed
    for 4 MB, which can back the small table and not the big one.  The board's
    nominal table stays in `NOMINAL_ROWS`, because that is still what its
    merged image carries at 0x8000 and what the encoder is checked against.
    """
    global BOARD, MACHINE, FLASH_SIZE, NEW_ROWS, NEW_STORAGE, NEW_STORAGE_LEN
    global NEW_ASSETS, NEW_LOG_LEN, TABLE_NAME
    global NOMINAL_ROWS, NOMINAL_TABLE_NAME
    global M_START, M_ERASING, M_ASSETS_STAY, M_ASSETS_MOVE, M_BLOCKED_OVERFILL
    global DONE_MARKER
    BOARD = BOARDS[board]
    MACHINE = BOARD["machine"]
    FLASH_SIZE = BOARD["flash"]
    NOMINAL_TABLE_NAME = BOARD["table"]
    NOMINAL_ROWS = NEW_ROWS_16MB if board == "s3" else NEW_ROWS_4MB
    if target in (None, NOMINAL_TABLE_NAME):
        TABLE_NAME = NOMINAL_TABLE_NAME
        NEW_ROWS = NOMINAL_ROWS
        NEW_STORAGE = BOARD["storage"]
        NEW_STORAGE_LEN = BOARD["storage_len"]
        NEW_ASSETS = BOARD["new_assets"]
    elif target == "partitions.csv":
        TABLE_NAME = target
        NEW_ROWS = NEW_ROWS_4MB
        NEW_STORAGE = NEW_STORAGE_4MB
        NEW_STORAGE_LEN = NEW_STORAGE_LEN_4MB
        NEW_ASSETS = ASSETS          # the 4 MB layout leaves the bundle put
    else:
        raise Fail(f"no embedded layout called {target!r}")
    NEW_LOG_LEN = NEW_STORAGE_LEN - LOG_AT
    M_START = ("migrate: partition table on flash is an older Luxel layout — "
               f"moving to {TABLE_NAME}")
    M_ERASING = (f"migrate: new storage {NEW_STORAGE:#x} + "
                 f"{NEW_STORAGE_LEN // 1024} KiB — erasing")
    M_ASSETS_STAY = f"migrate: assets stay at {ASSETS:#x} — nothing to move"
    M_ASSETS_MOVE = f"migrate: moving assets {ASSETS:#x} → {NEW_ASSETS:#x}"
    M_BLOCKED_OVERFILL = ("migrate: BLOCKED — pattern library too large for the "
                          f"new layout (need {{need}} B, have {NEW_LOG_LEN} B)")
    DONE_MARKER = f"(storage @ {NEW_STORAGE:#x})"


def run_qemu(qemu: str, flash: str, efuse: str, log: str, timeout: float,
             stop, abort_markers=ABORT_MARKERS, poll: float = 0.05):
    """Boot the composed image; return (serial log, wall seconds, outcome).

    `stop(text, flash_path)` is polled until it returns a truthy reason to kill
    QEMU.  The flash drive deliberately has no `snapshot=on`: the post-run
    assertions diff the file QEMU wrote, and the power-cut variants re-run the
    emulator on that same file.
    """
    cmd = [
        qemu, "-display", "none", "-monitor", "none", "-machine", MACHINE,
        "-drive", f"file={flash},if=mtd,format=raw",
    ]
    if efuse:
        # The classic-ESP32 efuse device only. The esp32s3 machine has its
        # own (`nvram.esp32s3.efuse`) whose defaults already boot, so the S3
        # variants pass no efuse drive at all.
        cmd += [
            "-drive", f"file={efuse},if=none,format=raw,id=efuse,snapshot=on",
            "-global", "driver=nvram.esp32.efuse,property=drive,value=efuse",
        ]
    cmd += ["-serial", f"file:{log}"]
    open(log, "wb").close()
    start = time.monotonic()
    with open(os.devnull, "rb") as devnull:
        proc = subprocess.Popen(cmd, stdin=devnull, stdout=subprocess.DEVNULL,
                                stderr=subprocess.PIPE)
        outcome = None
        try:
            while True:
                with open(log, "rb") as f:
                    text = f.read().decode("utf-8", "replace")
                hit = stop(text, flash)
                if hit:
                    outcome = hit
                    break
                for m in abort_markers:
                    if m in text:
                        outcome = f"abort line: {m.strip()}"
                        for k, v in ABORT_HINTS.items():
                            if k in text:
                                outcome += f"\n  {v}"
                        break
                if outcome:
                    break
                if proc.poll() is not None:
                    outcome = f"qemu exited early (rc={proc.returncode})"
                    break
                if time.monotonic() - start > timeout:
                    outcome = f"timeout after {timeout:.0f}s"
                    break
                time.sleep(poll)
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
    if not (outcome or "").startswith("stop:"):
        stderr = proc.stderr.read().decode("utf-8", "replace") if proc.stderr else ""
        raise Fail(f"boot did not reach the stop condition — {outcome}"
                   + (f"\nqemu stderr:\n{stderr.strip()}" if stderr.strip() else ""))
    return text, elapsed, outcome


def stop_on_done(text: str, _flash: str) -> str | None:
    """The migration ran to completion and the device rebooted into the new
    layout."""
    if M_INSTALLED in text and DONE_MARKER in text.split(M_INSTALLED)[-1]:
        return "stop: migrated and rebooted"
    return None


def stop_on_line(line: str):
    def f(text: str, _flash: str) -> str | None:
        return f"stop: {line!r}" if line in text else None
    return f


def _read_at(flash: str, at: int, n: int) -> bytes:
    with open(flash, "rb") as fh:
        fh.seek(at)
        return fh.read(n)


def stop_on_page_written(at: int, what: str, after_line: str | None = None):
    """Cut power the moment the 4 KiB page at `at` stops reading as erased.

    The late stages of the migration have no per-page narration to key off —
    `stage_log` and `write_new_store` each print one line and then move
    kilobytes — and the gaps between the lines they DO print are microseconds,
    far under any polling interval (the first version of this file keyed the
    `stored` cut on `migrate: store relocated` and the plug came out two lines
    LATER, past the table write, which proves nothing).  So the cut points
    watch the flash FILE instead, where progress is visible at page
    granularity.  QEMU's m25p80 writes through the host page cache, so a read
    here sees what the guest has written.
    """
    def f(text: str, flash: str) -> str | None:
        if after_line and after_line not in text:
            return None
        if any(b != 0xFF for b in _read_at(flash, at, SECTOR)):
            return f"stop: {what} ({at:#x} no longer erased)"
        return None
    return f


# migrate.rs' staging-header stages (S_NONE/S_STAGED/S_STORED/S_ASSETS), as
# they read out of the `LXMG` header at the head of the staging slot:
#   u32 magic "LXMG" | u32 version | u32 stage | u32 log_bytes | u32 tag | u32 crc
HDR_MAGIC = b"LXMG"
S_STAGED, S_STORED, S_ASSETS = 1, 2, 3

# Which staging stages each cut point may legitimately land on.  `None` = no
# header on flash yet.  Narrow on purpose: a trigger that starts landing
# outside its set has drifted and should be retuned, not accommodated.
CUT_STAGES = {
    "copy": (None,),
    "staged": (None,),
    "stored": (S_STAGED,),
    "assets": (S_STORED, S_ASSETS),
}


def staging_stage(flash: str, staging_at: int) -> int | None:
    """The stage the on-flash staging header records, or None if there is no
    header there yet."""
    b = _read_at(flash, staging_at, 24)
    if len(b) < 24 or b[0:4] != HDR_MAGIC:
        return None
    return struct.unpack("<I", b[8:12])[0]


def stop_on_stage(stage: int, staging_at: int):
    """Cut power the instant the staging header records `stage`.

    This is the tightest window the harness can aim at: after
    `write_hdr(S_STORED)` the migrator does one header read, two println!s and
    one more sector cycle before it writes the partition table.  Polled fast
    and with a 24-byte read, but if a run of this ever reports the table
    already installed, say so rather than loosening the assertion — the honest
    answer is "not reproducible under the emulator", not a weaker test.
    """
    def f(_text: str, flash: str) -> str | None:
        got = staging_stage(flash, staging_at)
        if got is not None and got >= stage:
            return f"stop: staging header reached stage {got}"
        return None
    return f


# --------------------------------------------------------------------------
# assertions


OTA_TARGET_RE = re.compile(r"booted from: (ota_[01])(?:(?!booted from:).)*?ota: updates go to (ota_[01]) at (0x[0-9a-f]+)", re.S)


def check_ota_targets(log: str, c: Checks) -> None:
    """Every boot in `log` — a run can have several — pairs its `booted from:`
    with the `ota: updates go to` line ota::init prints right after it.  The
    two must never name the same slot: that pairing IS the Seengreat brick
    (Gitea #655), and the boots after `settle_into_ota0` run with otadata
    ERASED, the exact state in which the pre-#655 selection got it wrong."""
    boots = OTA_TARGET_RE.findall(log)
    c.require(bool(boots), "serial: every boot names its OTA target",
              "no `booted from:` / `ota: updates go to` pair found")
    for booted, target, at in boots:
        c.require(booted != target,
                  f"serial: booted from {booted} → updates go to {target} at {at}",
                  "ota::init named the slot it is executing from — the #655 brick")
    # and no boot was left without a target at all
    c.require("ota: no update target" not in log,
              "serial: no boot refused to pick an update target",
              log[log.find("ota: no update target"):][:120] if "ota: no update target" in log else "")


def check_serial(log: str, from_slot: str, ota_len: int, side: dict,
                 c: Checks, resume_stage: int | None = None,
                 old_ota1: int = OLD_OTA1) -> None:
    """The migrator's narration for this variant.

    `old_ota1` is the LIVE table's ota_1 — the slot the migrator stages into
    and, for `--from ota_1`, the one it copies itself out of. It is the
    pre-#501 0x110000 for a first migration and the 4 MB layout's 0x150000
    for the second hop of a chained one (Gitea #634).

    `resume_stage` is the stage the `LXMG` staging header recorded when the
    plug came out — read off the FLASH after the cut, not inferred from the
    `--cut` name, because the tightest cut point can land one stage either
    side of where it aimed (`assets` catches S_STORED or S_ASSETS depending on
    how the 1 ms poll falls).  Asserting the resume against what the header
    actually says is the accurate test, not the lenient one: whatever stage is
    on flash, the re-run must pick up at exactly that one and skip exactly the
    work it records as done.
    """
    if M_INSTALLED not in log:
        raise Fail("the migration never reached the table rewrite")
    before, after = log.split(M_INSTALLED, 1)

    c.line(before, M_START, "migrate")
    # If the composed storage image were malformed this is the line that would
    # appear instead of any of the ones below, and it would mean the FIXTURE is
    # broken, not the migrator.  Assert its absence rather than only inferring
    # it from the stages that followed.
    c.require("migrate: BLOCKED — pattern store did not come up" not in log,
              "serial[migrate]: the composed pre-#501 store came up",
              "patterns::init did not resolve the old storage partition — "
              "regenerate the fixture with tools/storegen")

    # Where `/api/ota` would write, printed by ota::init on every boot and the
    # ONLY thing the emulator can assert about slot selection (no network
    # reaches a QEMU guest).  On the first boot the device runs the OLD table
    # from `from_slot` — and for --from ota_0 the fixture's otadata is ERASED,
    # which is precisely the state in which the pre-#655 firmware picked the
    # running slot and bricked the Seengreat (Gitea #655).  The target must be
    # the OTHER slot, at the LIVE table's offset for it.
    other, other_at = ("ota_1", old_ota1) if from_slot == "ota_0" else ("ota_0", NEW_OTA0)
    c.line(before, f"ota: updates go to {other} at {other_at:#x}", "migrate")
    check_ota_targets(log, c)

    copy_line = f"migrate: copying {ota_len} B {old_ota1:#x} → {NEW_OTA0:#x}"
    if from_slot == "ota_1" and resume_stage is None:
        c.line(before, copy_line, "migrate")
        # copy_region narrates every 64th sector; the LAST such line is the
        # proof that the whole image went through the verify loop rather than
        # the first few sectors of it.
        sectors = -(-ota_len // SECTOR)
        total_kib = sectors * SECTOR // 1024
        last = ((sectors - 1) // 64) * 64 * SECTOR // 1024
        c.line(before, f"migrate: copied {last}/{total_kib} KiB", "migrate")
        c.line(before, M_REBOOT_OTA0, "migrate")
    else:
        # Either the device was already in the slot the new table calls ota_0,
        # or a cut landed after the copy and the re-run boots straight into it.
        # NOTE migrate.rs prints `image already at 0x…` ONLY when it is
        # executing somewhere else AND finds its image at the destination — an
        # interrupted copy.  When exec == ota_0 it says nothing about the slot
        # at all, so "no copy line" is the assertion, not "already at".
        c.require(copy_line not in before and M_REBOOT_OTA0 not in before,
                  "serial[migrate]: no self-copy and no extra reboot",
                  "the migrator copied itself when it was already in ota_0")
        c.require(M_ALREADY not in before,
                  f"serial[migrate]: no {M_ALREADY!r} either "
                  "(it is running FROM ota_0, so there is nothing to recognise)",
                  "migrate.rs took the interrupted-copy branch unexpectedly")

    stage_line = (f"migrate: staging {len(side['patterns'])} live pattern(s), "
                  f"{side['staged_bytes']} B of log → {old_ota1 + SECTOR:#x} "
                  f"(new log holds {NEW_LOG_LEN} B)")
    if resume_stage:
        c.line(before, f"migrate: resuming at stage {resume_stage} "
                       f"({side['staged_bytes']} B of log staged)", "migrate")
        c.require(stage_line not in before,
                  "serial[migrate]: the log was NOT re-staged after the cut",
                  "the staging header did not survive, so the repack ran twice")
    else:
        # No header (or S_NONE): the re-run stages from scratch, and the OLD
        # store is what it reads — which is exactly why a cut before the
        # header write costs nothing.
        c.line(before, stage_line, "migrate")

    # S_STORED (2) and above mean the new storage region is already built, so
    # the re-run must skip the whole store move rather than erase it again.
    if not resume_stage or resume_stage < S_STORED:
        c.line(before, M_ERASING, "migrate")
        c.line(before, f"migrate: {len(side['blobs'])} reserved blob(s) carried over",
               "migrate")
        c.line(before, M_RELOCATED, "migrate")
    else:
        c.require(M_ERASING not in before and M_RELOCATED not in before,
                  f"serial[migrate]: stage {resume_stage} skipped the store move "
                  "instead of redoing it",
                  "the staging header recorded STORED but the migrator rebuilt "
                  "the region anyway")
    # S_ASSETS (3) means the asset stage is already recorded done, so the
    # re-run skips it.  On the 4 MB layout that stage is only ever the
    # "nothing to move" line; on the 16 MB one it is a real 960 KiB copy,
    # which is the branch no emulator had ever reached before this board
    # was added (Gitea #634).
    moves = NEW_ASSETS != ASSETS
    asset_line = M_ASSETS_MOVE if moves else M_ASSETS_STAY
    if not resume_stage or resume_stage < S_ASSETS:
        c.line(before, asset_line, "migrate")
        if moves:
            sectors = -(-ASSETS_LEN // SECTOR)
            last = ((sectors - 1) // 64) * 64 * SECTOR // 1024
            c.line(before,
                   f"migrate: copied {last}/{sectors * SECTOR // 1024} KiB",
                   "migrate")
    else:
        c.require(asset_line not in before,
                  f"serial[migrate]: stage {resume_stage} skipped the asset stage",
                  "the staging header recorded ASSETS but the migrator ran it again")
    c.line(before, M_INSTALLING, "migrate")
    c.ok(f"serial[migrate]: {M_INSTALLED!r}")

    # The boot that came back on the new table: running ota_0, so updates go
    # to the NEW table's ota_1 — 0x150000 on the 4 MB layout, 0x310000 on the
    # 16 MB one — never to 0x10000.
    c.line(after, "booted from: ota_0", "after")
    new_ota1 = next(r[2] for r in NEW_ROWS if r[4] == "ota_1")
    c.line(after, f"ota: updates go to ota_1 at {new_ota1:#x}", "after")
    want = (f"patterns: log {NEW_LOG_LEN} B, {len(side['patterns'])} patterns, "
            f"{side['pre_live_bytes']} B used, 0 B reclaimable, "
            f"{len(side['patterns'])} files (0 torn, 0 resyncs), "
            f"cursor {side['repacked_cursor']} (storage @ {NEW_STORAGE:#x})")
    c.line(after, want, "after")
    c.require("patterns: format" not in after,
              "serial[after]: the new key area kept its format marker (no wipe)",
              "patterns::init wiped the migrated store — the blobs did not come across")
    c.require("assets: none installed" not in after and "assets:" in after,
              "serial[after]: the web bundle is still readable at its old offset",
              "assets::init found nothing under the new table")


def check_flash(flash: bytes, composed: bytes, expected_table: bytes,
                app: bytes, c: Checks) -> None:
    """`app` is the image the fixture put in the slot the device booted from,
    which is what has to end up in the new ota_0 — literally the same bytes
    when it was already there, and byte-for-byte after the self-copy when it
    was not.  It is NOT always `luxel-fw-ota.bin`: espflash rewrites the
    flash-size nibble of the image header when it merges (0x20 -> 0x40 on the
    16 MB board), so the merged image's copy of the app differs from the OTA
    one in exactly that byte."""
    got = flash[TABLE_OFFSET:TABLE_OFFSET + SECTOR]
    want = expected_table + b"\xff" * (SECTOR - len(expected_table))
    c.require(got == want, f"flash: {TABLE_NAME} installed at 0x8000",
              f"first differing byte at {first_diff(got, want)}")

    got = flash[NEW_OTA0:NEW_OTA0 + len(app)]
    c.require(got == app,
              f"flash: ota_0 ({NEW_OTA0:#x}) holds the {len(app)} B app image "
              "byte-for-byte",
              f"first differing byte at {first_diff(got, app)}")

    # The 4 MB layout's entire point: `assets` does not move, so a migrating
    # device keeps a valid web bundle where assets.rs already maps it.  The
    # 16 MB one DOES move it, and then the same assertion runs against the
    # new offset: every byte of the bundle has to arrive.
    got = flash[NEW_ASSETS:NEW_ASSETS + ASSETS_LEN]
    want = composed[ASSETS:ASSETS + ASSETS_LEN]
    c.require(got == want,
              f"flash: the {ASSETS_LEN:#x} B web bundle is byte-identical at "
              f"{NEW_ASSETS:#x}"
              + ("" if NEW_ASSETS == ASSETS else f" (moved from {ASSETS:#x})"),
              f"first differing byte at {first_diff(got, want)}")

    check_nvs(flash, composed, c)
    check_otadata(flash, c)


def check_nvs(flash: bytes, composed: bytes, c: Checks) -> None:
    """nvs must come through untouched — with one documented exception.

    This is the assertion that separates a migration from a takeover.  The
    WLED takeover erases 0x9000..0x10000 on purpose (it is inheriting someone
    else's config); the migration must not, because nvs holds the WiFi
    credentials and nothing on the bench has a serial path back.
    """
    for off, name in ((LXCF_OFFSET, "LXCF creds"), (LXDV_OFFSET, "LXDV settings")):
        got = flash[off:off + SECTOR]
        want = composed[off:off + SECTOR]
        c.require(got == want,
                  f"flash: {name} sector {off:#x} untouched "
                  "(the migration does NOT do the takeover's config wipe)",
                  f"first differing byte at {first_diff(got, want)}")
    # 0xC000 is the boot-guard sector (ota.rs GUARD_OFFSET), rewritten by
    # `preboot_guard` on every boot before the migration check even runs — so it
    # is expected to differ, and what matters is that it reads as a clean guard.
    guard = flash[LXBG_OFFSET:LXBG_OFFSET + 8]
    c.require(guard[0:4] == b"LXBG" and guard[4] == 1 and guard[5] == 0,
              "flash: boot-guard record at 0xC000 (LXBG, 1 attempt, no force-AP)",
              f"got {guard.hex()} — >1 attempt means a boot died before "
              "clear_boot_attempts(), i.e. the migration crashed somewhere")
    tail = flash[MQTT_OFFSET:MQTT_OFFSET + SECTOR]
    c.require(tail == composed[MQTT_OFFSET:MQTT_OFFSET + SECTOR],
              f"flash: the MQTT nvs sector ({MQTT_OFFSET:#x}) untouched",
              f"first differing byte at "
              f"{first_diff(tail, composed[MQTT_OFFSET:MQTT_OFFSET + SECTOR])}")


def check_otadata(flash: bytes, c: Checks) -> None:
    """The migration erases otadata (with no factory partition, erased == boot
    ota_0) and the second-stage bootloader writes the choice back as
    seq=1/VALID with a CRC over the seq word — exactly as it does after the
    WLED takeover's wipe (see takeover-test.py's longer note)."""
    e0 = flash[OTADATA:OTADATA + 32]
    seq, = struct.unpack("<I", e0[0:4])
    state, crc = struct.unpack("<II", e0[24:32])
    want_crc = zlib.crc32(e0[0:4], 0xFFFFFFFF) & 0xFFFFFFFF
    c.require(seq == 1 and state == _tt.ESP_OTA_IMG_VALID and crc == want_crc,
              "flash: otadata 0xD000 selects ota_0 (seq=1/VALID, bootloader-written)",
              f"seq={seq} state={state} crc={crc:#010x} want {want_crc:#010x} "
              f"raw={e0.hex()}")
    e1 = flash[OTADATA_1:OTADATA_1 + 32]
    c.require(e1 == b"\xff" * 32,
              "flash: otadata entry #1 (0xE000) erased — the migration wiped "
              "the ota_1 selection it may have booted from",
              f"got {e1.hex()}")


def check_store(flash_path: str, store_bin: str, sidecar: str, workdir: str,
                c: Checks, pre_len: int | None = None,
                tag: str = "store-modelled.bin") -> None:
    """Hand the post-run image to the host verifier: every reserved blob byte
    identical, every live pattern present with byte-identical source and
    bytecode, no dead records, nothing extra.

    Then the cross-check that licenses `--plan-16mb`: run the SAME store move
    on the host (`storegen migrate`, which is `stage_log` + `write_new_store`
    with memory instead of flash) and require the result to be byte-identical
    to what the emulated device just wrote.  If the host model and the device
    agree byte for byte on the layout we CAN emulate, running that model over
    the layout we cannot is a computation rather than a guess.
    """
    out = storegen("verify", "--flash", flash_path, "--sidecar", sidecar,
                   "--at", hex(NEW_STORAGE), "--len", hex(NEW_STORAGE_LEN),
                   "--label", "new-store")
    n = out.count("\n  ok  ")
    c.require(n > 0, "store: the host verifier ran", out)
    for line in out.splitlines():
        if line.startswith("  ok  "):
            c.ok(line[6:])

    modelled = os.path.join(workdir, tag)
    storegen("migrate", "--pre", store_bin, "--sidecar", sidecar,
             "--new-len", hex(NEW_STORAGE_LEN), "--out", modelled,
             *(("--pre-len", hex(pre_len)) if pre_len else ()))
    with open(modelled, "rb") as f:
        want = f.read()
    with open(flash_path, "rb") as f:
        f.seek(NEW_STORAGE)
        got = f.read(NEW_STORAGE_LEN)
    c.require(got == want,
              "store: the host model of the store move reproduces the device's "
              f"{NEW_STORAGE_LEN} B partition byte for byte",
              f"first differing byte at {first_diff(got, want)}")


# --------------------------------------------------------------------------
# the overfill (refusal) variant


def check_refusal(log: str, flash: bytes, composed: bytes, blocked: str,
                  c: Checks) -> None:
    """A refusal is a promise too: the device keeps working exactly as it did,
    which means the OLD table and the OLD store must both be untouched."""
    c.line(log, M_START, "migrate")
    c.line(log, blocked, "migrate")
    c.require(M_ERASING not in log and M_RELOCATED not in log,
              "serial[migrate]: nothing was erased or relocated",
              "the refusal came too late — the store move had already started")
    c.require(M_INSTALLING not in log,
              "serial[migrate]: the partition table was never rewritten",
              "a blocked migration must leave the old table alone")

    old = part_table(OLD_ROWS)
    got = flash[TABLE_OFFSET:TABLE_OFFSET + len(old)]
    c.require(got == old, "flash: the PRE-#501 table is still at 0x8000",
              f"first differing byte at {first_diff(got, old)}")

    got = flash[OLD_STORAGE:OLD_STORAGE + OLD_STORAGE_LEN]
    want = composed[OLD_STORAGE:OLD_STORAGE + OLD_STORAGE_LEN]
    c.require(got == want,
              f"flash: the old storage partition {OLD_STORAGE:#x}+{OLD_STORAGE_LEN:#x} "
              "is byte-identical — nothing was touched",
              f"first differing byte at {first_diff(got, want)}")

    got = flash[NEW_STORAGE:NEW_STORAGE + NEW_STORAGE_LEN]
    want = composed[NEW_STORAGE:NEW_STORAGE + NEW_STORAGE_LEN]
    c.require(got == want,
              "flash: the would-be new storage region was not erased either",
              f"first differing byte at {first_diff(got, want)}")

    got = flash[ASSETS:ASSETS + ASSETS_LEN]
    want = composed[ASSETS:ASSETS + ASSETS_LEN]
    c.require(got == want, "flash: assets untouched by the refusal",
              f"first differing byte at {first_diff(got, want)}")


# --------------------------------------------------------------------------
# the 16 MB layout


def plan_16mb(workdir: str, c: Checks) -> None:
    """A fast, emulator-free model of `firmware/partitions-16mb.csv`.

    `--board s3` now boots that layout for real (see this file's docstring),
    so this is no longer the ONLY coverage the 16 MB table has — but it is
    still the cheap one: the table encoding, the geometric invariants the
    migration depends on, and the host store move, in about a second and with
    no QEMU at all.  Keep it in the suite ahead of the emulated variants: when
    both fail, this one says whether the layout or the migrator is at fault.

    The history is worth keeping, because it is what the S3 emulation had to
    undo.  Until 2026-09-21 `-machine esp32s3` loaded the app and then printed
    NOTHING — not a panic, not a partial banner.  It was not the firmware: the
    guest was spinning inside `esp_hal::init` on a BBPLL calibration-done bit
    the machine does not model, before esp-println exists.  That plus three
    more emulator bugs (a free-running APP CPU, three divide-by-zero SIGFPEs
    in the timer-group model, and an unbounded MMU page write that SIGSEGVed
    QEMU on a 16 MB part) are fixed in `tools/qemu/patches/`, guest-side
    unchanged, per CLAUDE.md's isolation rule.

    The store move here is still the cross-check that licenses the model:
    `storegen migrate` runs `stage_log` + `write_new_store`'s algorithm on the
    host, and the emulated variants assert its output is byte-identical to
    what the device writes.
    """
    merged = os.path.join(workdir, "s3-merged.bin")
    with open(merged, "rb") as f:
        head = f.read(TABLE_OFFSET + SECTOR)
    truth = head[TABLE_OFFSET:TABLE_OFFSET + TABLE_LEN]
    mine = part_table(NEW_ROWS_16MB)
    c.require(mine == truth,
              "16mb: hand-built 16 MB table byte-equals esp-idf-part's",
              f"first differing byte at {first_diff(mine, truth)}")

    rows = {r[4]: r for r in NEW_ROWS_16MB}
    old = {r[4]: r for r in OLD_ROWS}
    c.require(rows["storage"][2] == S3_STORAGE and rows["storage"][3] == S3_STORAGE_LEN
              and rows["assets"][2] == S3_ASSETS,
              "16mb: the constants this test uses are the csv's",
              f"{rows['storage']} {rows['assets']}")
    c.require(rows["ota_0"][2] == old["ota_0"][2],
              "16mb: ota_0 keeps its offset (0x10000), so a device already in "
              "ota_0 needs no self-copy — the same branch --from ota_0 emulates")
    c.require(rows["assets"][2] != old["assets"][2],
              "16mb: `assets` MOVES (0x310000 -> 0xa10000) — the ONLY layout "
              "that reaches migrate::move_assets' copy branch, which "
              "`--board s3` is what actually executes",
              f"{rows['assets'][2]:#x} vs {old['assets'][2]:#x}")
    old_a, new_a = old["assets"], rows["assets"]
    length = min(old_a[3], new_a[3])
    c.require(not (old_a[2] < new_a[2] + length and new_a[2] < old_a[2] + length),
              "16mb: the asset regions do not overlap, so move_assets copies "
              "rather than refusing",
              f"{old_a[2]:#x}+{length:#x} vs {new_a[2]:#x}+{length:#x}")
    c.require(rows["storage"][2] >= old["storage"][2] + 0x20000,
              "16mb: the new storage region starts past the OLD key area, so "
              "write_new_store's overlap guard passes",
              f"{rows['storage'][2]:#x} vs {old['storage'][2]:#x}+0x20000")
    # The staging slot is the OLD ota_1; it has to hold the header plus the
    # repacked log, and on this layout it also must not overlap the new ota_0.
    c.require(old["ota_1"][3] >= SECTOR + NEW_LOG_LEN,
              "16mb: the old ota_1 is big enough to stage any log the 4 MB "
              "layout could hold")
    log16 = rows["storage"][3] - LOG_AT
    c.require(log16 > NEW_LOG_LEN * 10,
              f"16mb: the log is {log16} B — {log16 // NEW_LOG_LEN}x the 4 MB "
              "layout's, so a store that fits 4 MB can never block here")
    end = max(r[2] + r[3] for r in NEW_ROWS_16MB)
    c.require(end <= FLASH_SIZE_16MB,
              f"16mb: the table needs {end:#x} B — inside a 16 MB part",
              f"{end:#x} > {FLASH_SIZE_16MB:#x}")

    # ---- the store move, computed and then verified ----
    store_bin = os.path.join(workdir, "store.bin")
    sidecar = os.path.join(workdir, "store.json")
    print(storegen("gen", "--out", store_bin, "--sidecar", sidecar).rstrip())
    post = os.path.join(workdir, "store-16mb.bin")
    print(storegen("migrate", "--pre", store_bin, "--sidecar", sidecar,
                   "--new-len", hex(S3_STORAGE_LEN), "--out", post).rstrip())

    # Drop it where the 16 MB table says `storage` lives, inside a real 16 MiB
    # image built from the Seengreat merged output, and verify it there — so
    # the offsets the verifier walks are the board's, not a bare file's.
    flash = os.path.join(workdir, "flash16.bin")
    with open(merged, "rb") as f:
        img = bytearray(f.read())
    if len(img) != FLASH_SIZE_16MB:
        raise Fail(f"the Seengreat merged image is {len(img)} B, expected "
                   f"{FLASH_SIZE_16MB}")
    with open(post, "rb") as f:
        img[S3_STORAGE:S3_STORAGE + S3_STORAGE_LEN] = f.read()
    with open(flash, "wb") as f:
        f.write(img)
    out = storegen("verify", "--flash", flash, "--sidecar", sidecar,
                   "--at", hex(S3_STORAGE), "--len", hex(S3_STORAGE_LEN),
                   "--label", "16mb-store")
    c.require("  ok  " in out, "16mb: the host verifier ran", out)
    for line in out.splitlines():
        if line.startswith("  ok  "):
            c.ok(line[6:])


# --------------------------------------------------------------------------


def sidecar_dict(path: str) -> dict:
    """The sidecar, minimally parsed.  json is in the stdlib; storegen writes
    plain JSON precisely so this side needs no format knowledge."""
    import json
    with open(path) as f:
        return json.load(f)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--result-dir", default="result",
                    help="nix build .#luxel-fw-athom-music output (default ./result)")
    ap.add_argument("--result-dir-16mb", default="result-s3",
                    help="nix build .#luxel-fw-seengreat-hub75 output "
                         "(--board s3 and --plan-16mb)")
    ap.add_argument("--board", choices=tuple(BOARDS), default="athom",
                    help="which board's layout to migrate: `athom` is the "
                         "4 MB table on the esp32 machine, `s3` is the 16 MB "
                         "table (firmware/partitions-16mb.csv) on the esp32s3 "
                         "machine — the only layout whose `assets` partition "
                         "moves")
    ap.add_argument("--qemu", help="qemu-system-xtensa (or its store dir); default: nix build")
    ap.add_argument("--from", dest="from_slot", choices=("ota_0", "ota_1"),
                    default="ota_0",
                    help="which OLD app slot the device is running from "
                         "(ota_1 exercises the self-copy + extra reboot)")
    ap.add_argument("--cut", choices=("copy", "staged", "stored", "assets"),
                    help="kill QEMU at this stage, then re-run on the same "
                         "flash and assert the migration still completes")
    ap.add_argument("--cut-pages", type=int, default=8,
                    help="--cut staged/stored: how many log pages to let "
                         "through before pulling the plug (default 8; the "
                         "fixture library is 21 pages)")
    ap.add_argument("--cut-attempts", type=int, default=6,
                    help="--cut assets: how many times to retry the race "
                         "against the table write before failing (default 6; "
                         "it is won about half the time)")
    ap.add_argument("--overfill", action="store_true",
                    help="generate a store too large for the 4 MB log and "
                         "assert the migration refuses, leaving flash alone")
    ap.add_argument("--old-bootloader", choices=tuple(BOOTLOADER_FLASH_NIBBLE),
                    help="stamp the composed image's BOOTLOADER header with "
                         "this flash size — i.e. a device serially flashed "
                         "before its table grew. `--board s3 "
                         "--old-bootloader 4mb` is the Seengreat as found on "
                         "2026-09-21 (Gitea #634): 16 MB of silicon behind a "
                         "4 MB bootloader, which migrates to the LARGEST "
                         "layout that bootloader can back — partitions.csv — "
                         "and says so. Works with --from and --cut")
    ap.add_argument("--reflash-bootloader", choices=tuple(BOOTLOADER_FLASH_NIBBLE),
                    help="after the first migration completes, restamp the "
                         "BOOTLOADER header to this size on the SAME flash "
                         "and boot again — i.e. Jeremy's one-time serial "
                         "re-flash. `--board s3 --old-bootloader 4mb "
                         "--reflash-bootloader 16mb` is the whole Seengreat "
                         "story: fall back to partitions.csv now, then "
                         "migrate AGAIN to partitions-16mb.csv (store "
                         "0x290000 -> 0x610000, assets 0x310000 -> 0xa10000)")
    ap.add_argument("--plan-16mb", action="store_true",
                    help="assertion-only coverage of the 16 MB layout (no "
                         "emulation — see plan_16mb's docstring)")
    ap.add_argument("--workdir", help="where to compose (default: a temp dir)")
    ap.add_argument("--keep", action="store_true", help="keep the work dir on success")
    ap.add_argument("--timeout", type=float, default=600.0,
                    help="overall boot timeout in seconds (default 600)")
    args = ap.parse_args(argv)
    # Which layout the device can actually reach on the first boot: the
    # board's own, unless a stamped-down bootloader cannot back it, in which
    # case parttab::target_table picks the largest embedded one that fits.
    target = None
    if args.old_bootloader:
        nominal = NEW_ROWS_16MB if args.board == "s3" else NEW_ROWS_4MB
        if max(r[2] + r[3] for r in nominal) > BOOTLOADER_FLASH_SIZE[args.old_bootloader]:
            target = "partitions.csv"
    select_board(args.board, target)
    if args.cut == "copy" and args.from_slot != "ota_1":
        ap.error("--cut copy needs --from ota_1 (there is no self-copy otherwise)")
    if args.cut and args.cut != "copy" and args.from_slot != "ota_0":
        ap.error(f"--cut {args.cut} is a --from ota_0 variant; the self-copy "
                 "reboot would land the re-run in ota_0 anyway")
    if args.overfill and (args.cut or args.from_slot != "ota_0"):
        ap.error("--overfill is its own variant; it takes no --cut/--from")
    if args.old_bootloader and args.overfill:
        ap.error("--old-bootloader picks a smaller TARGET; --overfill is "
                 "about the store not fitting one. Run them separately")
    if args.old_bootloader and args.board == "athom":
        ap.error("--old-bootloader only bites where the board's table runs "
                 "past the stamped ceiling, i.e. --board s3 (the 4 MB table "
                 "fits any part that ever ran Luxel)")
    if args.reflash_bootloader:
        if not args.old_bootloader:
            ap.error("--reflash-bootloader is the SECOND half of an "
                     "--old-bootloader run; there is nothing to re-flash "
                     "otherwise")
        if target is None:
            ap.error(f"--old-bootloader {args.old_bootloader} already backs "
                     "this board's own table, so the first migration lands "
                     "on it and there is no second hop")
        if (max(r[2] + r[3] for r in NOMINAL_ROWS)
                > BOOTLOADER_FLASH_SIZE[args.reflash_bootloader]):
            ap.error(f"--reflash-bootloader {args.reflash_bootloader} still "
                     f"cannot back {NOMINAL_TABLE_NAME}; the second migration "
                     "would not happen")

    workdir = args.workdir or tempfile.mkdtemp(prefix="luxel-migrate-")
    os.makedirs(workdir, exist_ok=True)
    log = os.path.join(workdir, "serial.log")
    try:
        rc = run(args, workdir, log)
    except Fail as e:
        print(f"\nFAIL [{label(args)}]: {e}", file=sys.stderr)
        for name in sorted(os.listdir(workdir)):
            if not name.endswith(".log"):
                continue
            path = os.path.join(workdir, name)
            with open(path, "rb") as f:
                tail = f.read().decode("utf-8", "replace").splitlines()[-40:]
            print(f"\n--- {name} tail ---", file=sys.stderr)
            print("\n".join(tail), file=sys.stderr)
        print(f"\nwork dir kept: {workdir}", file=sys.stderr)
        return 1
    if args.keep or args.workdir:
        print(f"work dir: {workdir}")
    else:
        shutil.rmtree(workdir, ignore_errors=True)
    return rc


def label(args: argparse.Namespace) -> str:
    if args.plan_16mb:
        return "plan-16mb"
    tag = f"{args.board} " if args.board != "athom" else ""
    if args.old_bootloader:
        tag += f"bootloader:{args.old_bootloader}"
        if args.reflash_bootloader:
            tag += f"->{args.reflash_bootloader}"
        tag += " "
    if args.overfill:
        return tag + "overfill"
    return tag + args.from_slot + (f" cut:{args.cut}" if args.cut else "")


def run(args: argparse.Namespace, workdir: str, log: str) -> int:
    c = Checks()

    if args.plan_16mb:
        print("== migration test, 16 MB layout (assertion-only, no emulation) ==")
        src = os.path.join(args.result_dir_16mb, "luxel-fw.bin")
        if not os.path.exists(src):
            raise Fail(f"missing input: {src}"
                       "\n  run: nix build .#luxel-fw-seengreat-hub75 "
                       "--out-link result-s3")
        shutil.copyfile(src, os.path.join(workdir, "s3-merged.bin"))
        plan_16mb(workdir, c)
        print(f"\nPASS [plan-16mb] — {len(c.passed)} assertions")
        for a in c.passed:
            print(f"  ok  {a}")
        return 0

    result_dir = args.result_dir_16mb if args.board == "s3" else args.result_dir
    ota_path = os.path.join(result_dir, "luxel-fw-ota.bin")
    merged_path = os.path.join(result_dir, "luxel-fw.bin")
    for p in (ota_path, merged_path):
        if not os.path.exists(p):
            raise Fail(f"missing input: {p}\n  run: nix build "
                       f".#{BOARD['flake']} --out-link {result_dir}")
    with open(ota_path, "rb") as f:
        ota = f.read()
    with open(merged_path, "rb") as f:
        merged = f.read()

    print(f"== partition migration test [{label(args)}] ==")
    print(f"   layout    : {TABLE_NAME} on -machine {MACHINE}, "
          f"{FLASH_SIZE // (1024 * 1024)} MiB flash")
    print(f"   app image : {ota_path} ({len(ota)} B)")
    print(f"   work dir  : {workdir}")

    expected_table = check_encoder(merged, c)

    # ---- the store fixture ----
    store_bin = os.path.join(workdir, "store.bin")
    sidecar = os.path.join(workdir, "store.json")
    gen = ["gen", "--out", store_bin, "--sidecar", sidecar]
    if args.overfill:
        gen.append("--overfill")
    print(storegen(*gen).rstrip())
    side = sidecar_dict(sidecar)
    c.require(side["log_at"] == LOG_AT and side["key_area_len"] == 0x20000,
              "fixture: the generated store uses this firmware's geometry",
              f"log_at={side['log_at']:#x} key_area_len={side['key_area_len']:#x}")
    if args.overfill:
        c.require(side["staged_bytes"] > NEW_LOG_LEN,
                  f"fixture: the library needs {side['staged_bytes']} B, more "
                  f"than the {NEW_LOG_LEN} B the 4 MB log offers",
                  "the --overfill library is not actually too large")
    else:
        c.require(side["staged_bytes"] <= NEW_LOG_LEN,
                  f"fixture: the library repacks to {side['staged_bytes']} B of "
                  f"the {NEW_LOG_LEN} B log "
                  f"({100 * side['staged_bytes'] // NEW_LOG_LEN}% full)",
                  "the fixture library does not fit — it would only test the refusal")
        c.require(len(side["patterns"]) >= 10 and side["pre_dead_bytes"] > 0,
                  f"fixture: {len(side['patterns'])} live patterns and "
                  f"{side['pre_dead_bytes']} B of dead records to repack over",
                  f"{len(side['patterns'])} patterns, {side['pre_dead_bytes']} B dead")

    with open(store_bin, "rb") as f:
        store = f.read()
    assets = make_assets()
    img = compose(merged, ota, store, assets, args.from_slot,
                  old_bootloader=args.old_bootloader)
    flash = os.path.join(workdir, "flash.bin")
    with open(flash, "wb") as f:
        f.write(img)
    composed = bytes(img)
    efuse = ""
    if BOARD["efuse"]:
        efuse = os.path.join(workdir, "efuse.bin")
        make_efuse(efuse)

    qemu = resolve_qemu(args.qemu)
    print(f"   qemu      : {qemu}")
    if args.from_slot == "ota_1":
        print(f"   otadata   : {otadata_note()}")

    # ---- boot ----
    total = 0.0
    # One variant ends in a refusal rather than a migration: the library that
    # cannot fit the new log.  Everything else migrates.
    blocked = None
    if args.overfill:
        blocked = M_BLOCKED_OVERFILL.format(need=side["staged_bytes"])
    upgrade_line = None
    if args.old_bootloader:
        # The fixture only means something if the stamped ceiling really does
        # exclude the board's own table and really does admit the fallback —
        # otherwise the run would quietly become an ordinary migration.
        nominal_need = max(r[2] + r[3] for r in NOMINAL_ROWS)
        target_need = max(r[2] + r[3] for r in NEW_ROWS)
        have = BOOTLOADER_FLASH_SIZE[args.old_bootloader]
        c.require(nominal_need > have >= target_need,
                  f"fixture: the stamped bootloader offers {have} B — too "
                  f"little for {NOMINAL_TABLE_NAME} ({nominal_need} B), enough "
                  f"for {TABLE_NAME} ({target_need} B), so the FALLBACK is "
                  "what is under test",
                  f"{have} B against {nominal_need}/{target_need} B")
        upgrade_line = M_UPGRADE.format(have=have, cap=FLASH_SIZE,
                                        target=TABLE_NAME,
                                        nominal=NOMINAL_TABLE_NAME)
    if blocked:
        print(f"   booting (expecting: {blocked})…")
        text, dt, _ = run_qemu(
            qemu, flash, efuse, log, args.timeout,
            stop=stop_on_line(blocked),
            # every abort marker EXCEPT the refusal we are here to see
            abort_markers=tuple(m for m in ABORT_MARKERS if m != "migrate: BLOCKED —"),
            poll=0.02)
        total += dt
        print(f"   refused in {dt:.1f}s wall")
        with open(flash, "rb") as f:
            written = f.read()
        check_refusal(text, written, composed, blocked, c)
        print(f"\nPASS [{label(args)}] — {len(c.passed)} assertions in {total:.1f}s")
        for a in c.passed:
            print(f"  ok  {a}")
        return 0

    resumed: int | None = None
    if args.cut:
        n = args.cut_pages
        stops = {
            # AFTER the whole self-copy and the otadata erase, before staging:
            # migrate.rs reboots next, so this is the last moment that is still
            # "stage none" with the image already in place.  A line is enough
            # here — the previous flash op finished 4 s of emulated copying ago.
            "copy": stop_on_line(M_REBOOT_OTA0),
            # Mid-staging: `n` pages of the repacked log are in the staging
            # slot, the header is not written yet, and the OLD store is still
            # entirely intact.  The re-run must re-stage from scratch.
            "staged": stop_on_page_written(
                OLD_OTA1 + SECTOR + n * SECTOR,
                f"staging reached page {n}", after_line="migrate: staging "),
            # Mid store-move: the staging header says STAGED, the new storage
            # region has been erased, the blobs are across and `n` pages of the
            # log are in their new home.  This is the harshest cut in the file
            # — the OLD log is being abandoned while the NEW one is half
            # written — and the re-run must redo the whole stage from staging.
            "stored": stop_on_page_written(
                NEW_STORAGE + LOG_AT + n * SECTOR,
                f"the new store's log reached page {n}", after_line=M_ERASING),
            # The store move is done and recorded; only the assets branch (a
            # no-op on this layout) and the table write are left.
            "assets": stop_on_stage(S_STORED, OLD_OTA1),
        }
        cut_log = os.path.join(workdir, "serial-cut.log")
        # `assets` aims at the narrowest window in the whole migration — the
        # two sector cycles between "the store move is recorded" and "the
        # table is written" — and the harness loses that race about half the
        # time: a 1 ms Python poll plus a terminate() is simply not fast
        # against two emulated flash ops.  Rather than pretend (a wider
        # trigger would silently become a different test) or give up (the
        # window is real and re-runnable, and skipping it would leave the
        # stage-2 resume path untested), it is retried from a fresh compose.
        # Measured hit rate ~50 %, so six attempts is ~98 %; if this ever
        # starts exhausting them, the honest answer is to record the cut point
        # as not reproducible, not to loosen the check below.
        attempts = args.cut_attempts if args.cut == "assets" else 1
        cut_text = ""
        for attempt in range(1, attempts + 1):
            with open(flash, "wb") as f:
                f.write(composed)          # a fresh pre-migration device
            note = f" (attempt {attempt}/{attempts})" if attempts > 1 else ""
            print(f"   booting (power cut at `{args.cut}`){note}…")
            cut_text, dt, why = run_qemu(qemu, flash, efuse, cut_log, args.timeout,
                                         stop=stops[args.cut],
                                         poll=0.001 if args.cut == "assets" else 0.02)
            total += dt
            print(f"   {why} after {dt:.1f}s wall — pulling the plug")
            # The only cut that proves nothing is one past the point of no
            # return.  Check the FLASH, not the log: the table write is one
            # sector op and its "installed" line is printed after it.
            with open(flash, "rb") as f:
                cut_img = f.read()
            still_old = (cut_img[TABLE_OFFSET:TABLE_OFFSET + TABLE_LEN]
                         == part_table(OLD_ROWS))
            if still_old and M_INSTALLED not in cut_text:
                break
            if attempt < attempts:
                print("   …the table went down first; recomposing and retrying")
        c.require(still_old and M_INSTALLED not in cut_text,
                  f"cut[{args.cut}]: the plug came out with the OLD table still "
                  f"on flash (attempt {attempt}/{attempts})",
                  "the cut landed past the point of no return every time, so "
                  "the re-run is not a recovery — retune the trigger or record "
                  "this cut point as not reproducible under the emulator")
        # What the staging header records NOW is what the re-run has to resume
        # from.  Read it rather than assuming: the `assets` trigger is a 1 ms
        # poll against a two-sector window and lands on S_STORED or S_ASSETS
        # depending on the run, and both are legitimate cut points.
        resumed = staging_stage(flash, OLD_OTA1)
        want = CUT_STAGES[args.cut]
        c.require(resumed in want,
                  f"cut[{args.cut}]: the staging header reads "
                  f"{'stage ' + str(resumed) if resumed is not None else 'no header'}",
                  f"expected one of {want} — the trigger is drifting; retune it "
                  "rather than widening this set")
        print(f"   staging header: "
              f"{'stage ' + str(resumed) if resumed is not None else 'not written yet'}")
        print("   re-booting the cut flash…")

    text, dt, _ = run_qemu(qemu, flash, efuse, log, args.timeout,
                           stop=stop_on_done, poll=0.05)
    total += dt
    print(f"   migrated and rebooted in {dt:.1f}s wall")

    if upgrade_line:
        # Before AND after: the ceiling is a standing fact about the board,
        # so migrate.rs' boot snapshot prints it on every boot — which is
        # what lets `/api/status` keep answering `upgrade_available: true`
        # on a device that is now fully migrated to the smaller layout.
        before, after = text.split(M_INSTALLED, 1)
        c.line(before, upgrade_line, "migrate")
        c.line(after, upgrade_line, "after")
        c.require(M_BLOCKED_BOOTLOADER.split(" (need")[0] not in text,
                  "serial[migrate]: the bootloader ceiling did NOT block the "
                  "migration — it selected a smaller layout instead",
                  "the board refused rather than falling back (Gitea #634)")

    # Every cut re-runs from ota_0: `copy` erases otadata before the plug comes
    # out, and the others never left it.
    slot_for_serial = "ota_0" if args.cut else args.from_slot
    check_serial(text, slot_for_serial, len(ota), side, c, resume_stage=resumed)

    with open(flash, "rb") as f:
        written = f.read()
    # What the device booted from: the merged image's app when it was already
    # in ota_0, the OTA image when the fixture put it in ota_1 for the
    # self-copy variant (and after any --cut, which re-runs from ota_0 but
    # over flash the self-copy may already have rewritten).
    app = (bytes(composed[NEW_OTA0:NEW_OTA0 + len(ota)])
           if args.from_slot == "ota_0" else ota)
    check_flash(written, composed, expected_table, app, c)
    check_store(flash, store_bin, sidecar, workdir, c)

    if args.reflash_bootloader:
        total += phase_two(args, workdir, qemu, efuse, flash, composed, ota,
                           app, store_bin, sidecar, side, c)

    print(f"\nPASS [{label(args)}] — {len(c.passed)} assertions in {total:.1f}s")
    for a in c.passed:
        print(f"  ok  {a}")
    return 0


def phase_two(args: argparse.Namespace, workdir: str, qemu: str, efuse: str,
              flash: str, composed: bytes, ota: bytes, app: bytes,
              store_bin: str, sidecar: str, side: dict, c: Checks) -> float:
    """Jeremy re-flashes the bootloader, and the device migrates AGAIN.

    The first half of this run left a 16 MB board fully migrated to the 4 MB
    layout because that is the largest one its bootloader could back.  Here
    that bootloader is replaced — one byte of its image header, the same
    nibble `compose` stamped down, which is exactly what
    `firmware/build-esp32.sh flash` rewrites over serial — and the SAME flash
    is booted again.  `parttab::target_table` now answers with the board's own
    table, `migrated` goes back to false, and the migrator runs a second time:
    storage 0x290000 -> 0x610000, assets 0x310000 -> 0xa10000, staged in the
    4 MB layout's ota_1 (0x150000) rather than the pre-#501 one.

    This is the half that proves `migrated: true` never becomes "stop
    looking" — the failure mode this whole ticket exists to avoid is a board
    that falls back once and is then stuck on the small table forever.
    """
    # The LIVE table for this hop is the one the first migration installed.
    live_rows = {r[4]: r for r in NEW_ROWS}
    live_ota1 = live_rows["ota_1"][2]
    live_storage, live_storage_len = live_rows["storage"][2], live_rows["storage"][3]
    # Lift the store the first hop produced out of flash: it is the SOURCE
    # the host model has to reproduce this hop from.
    pre2 = os.path.join(workdir, "store-4mb.bin")
    with open(flash, "rb") as f:
        f.seek(live_storage)
        with open(pre2, "wb") as g:
            g.write(f.read(live_storage_len))

    # Re-point every layout constant at the board's OWN table…
    select_board(args.board, None)
    # …and re-flash the bootloader in place.
    boot_at = BOARD["bootloader"]
    with open(flash, "r+b") as f:
        f.seek(boot_at + 3)
        b = f.read(1)[0]
        f.seek(boot_at + 3)
        f.write(bytes([(b & 0x0F) | (BOOTLOADER_FLASH_NIBBLE[args.reflash_bootloader] << 4)]))
    print(f"\n   -- bootloader re-flashed for {args.reflash_bootloader} "
          f"(header nibble at {boot_at + 3:#x}) — booting again --")
    print(f"   layout    : {TABLE_NAME}, storage {NEW_STORAGE:#x} + "
          f"{NEW_STORAGE_LEN // 1024} KiB, assets -> {NEW_ASSETS:#x}")

    log2 = os.path.join(workdir, "serial-phase2.log")
    text, dt, _ = run_qemu(qemu, flash, efuse, log2, args.timeout,
                           stop=stop_on_done, poll=0.05)
    print(f"   migrated again and rebooted in {dt:.1f}s wall")

    # The ceiling is gone, so the nag must be too — on BOTH boots of this
    # phase, the migrating one and the one that came back on the big table.
    c.require("re-flash the bootloader over serial to unlock" not in text,
              "serial[phase2]: the upgrade nag is gone — the re-flashed "
              "bootloader backs this board's own layout",
              "migrate.rs still reports upgrade_available after the re-flash")
    check_serial(text, "ota_0", len(ota), side, c, old_ota1=live_ota1)

    with open(flash, "rb") as f:
        written = f.read()
    # Licensed by check_encoder's run in phase one: the same encoder built
    # the table that byte-equals esp-idf-part's output for this board.
    check_flash(written, composed, part_table(NEW_ROWS), app, c)
    check_store(flash, pre2, sidecar, workdir, c, pre_len=live_storage_len,
                tag="store-modelled-16mb.bin")
    # The abandoned 4 MB `storage` is now inside the new ota_1; nothing reads
    # it again, and nothing is expected to have cleaned it up either.
    c.require(NEW_STORAGE != live_storage and NEW_ASSETS != ASSETS,
              f"phase2: both regions really moved — storage {live_storage:#x} "
              f"-> {NEW_STORAGE:#x}, assets {ASSETS:#x} -> {NEW_ASSETS:#x}",
              "the second hop was a no-op")
    return dt


if __name__ == "__main__":
    raise SystemExit(main())
