#!/usr/bin/env python3
"""Reproduce the pre-guard heap-regions panic and prove the boot guard closes it.

Hardware-free regression test for the intermittent 2026-07-26 Athom panic
`esp-alloc: Exceeded the maximum of 3 heap memory regions`, which fired before
ota::init on a WLED-bootloader takeover boot, self-healed once, and — had it
ever been deterministic — would have looped forever without ever reaching the
old post-init boot guard (so it could never roll back to WLED). See
docs/research/qemu-emulation-spike.md and UPDATES.md.

ROOT CAUSE (established under QEMU): the athom firmware makes exactly two
`esp_alloc::heap_allocator!` calls -> two `add_region()`s into esp-alloc's
three-slot region array. A flash-read flake during the (ancient) WLED
bootloader's copy of the app's `.data` segment can corrupt the `HEAP` static's
slot array so it boots with stale `Some` region slots; the two real adds then
overflow the array and `add_region` panics. It is pre-guard, pre-heap, and
intermittent — exactly the reported signature.

We can't make QEMU flip a flash bit mid-bootloader, so we stand in for the
flake precisely at its observable effect: at the first `add_region` entry we
poke N slot discriminants to `Some` over the gdbstub (no firmware bytes are
modified — the image under test is byte-identical to what ships; the harness
stays isolated per CLAUDE.md).

Two modes:

  --mode selfheal (default)
      Inject on the FIRST boot only (a one-shot flake). Assert the panic fires
      before the boot println / ota::init, custom_halt reboots, the next boot
      is clean, and the takeover completes. This is the historical behaviour.

  --mode rollback
      The deterministic-flake endgame, tested directly: pre-seed the LXBG
      boot-guard record with the failed-boot counter already at its rollback
      threshold (as two prior pre-guard panics would leave it), then boot.
      Assert that `preboot_guard` — armed before the allocators by the fix —
      rolls the device back to the other OTA slot (WLED) *before* the
      allocators run, so the heap-regions panic never even fires. Before the
      fix nothing ran before the allocators, so a deterministic pre-guard
      panic looped forever; this proves the loop is closed.

      selfheal proves the panic reproduces and that preboot_guard increments
      the counter each boot (the takeover test's "LXBG, 1 attempt" assertion);
      rollback proves the counter, once it reaches the threshold, rolls back.
      Together they cover "three deterministic panics -> rollback to WLED".

Usage:
    nix build .#luxel-fw-athom-music
    nix develop -c python3 tools/qemu/heap-regions-test.py \
        --stock /path/to/athom-wled-stock.bin \
        --fs    /path/to/athom-wled-fs-configured.bin \
        [--mode selfheal|loop]

`--qemu`/`--result-dir`/inputs follow takeover-test.py (whose compose, QEMU
launch and eFuse helpers this test imports).
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)

from gdbrsp import Rsp  # noqa: E402

# Import takeover-test.py (hyphenated -> importlib) for compose/qemu/efuse.
_spec = importlib.util.spec_from_file_location("takeover_test",
                                               os.path.join(HERE, "takeover-test.py"))
tko = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(tko)

SLOT_STRIDE = 0x20  # sizeof(Option<HeapRegion>) in esp-alloc's [_; 3] array
PANIC = "Exceeded the maximum of 3 heap memory regions"
ROLLBACK = "rolling back to the other OTA slot"
BOOT_PRINTLN = "luxel-fw: boot"
OTA_INIT_MARKER = "booted from:"
HALT = "panic: rebooting in 3s"


class Fail(Exception):
    pass


def elf_sym(elf: str, pattern: str) -> int:
    out = subprocess.run(["nm", elf], capture_output=True, text=True).stdout
    hits = [int(p[0], 16) for line in out.splitlines()
            if len(p := line.split()) == 3 and re.search(pattern, p[2])]
    if not hits:
        raise Fail(f"symbol /{pattern}/ not found in {elf} "
                   "(build with .#luxel-fw-athom-music so luxel-fw.elf is present)")
    if len(hits) > 1:
        raise Fail(f"symbol /{pattern}/ is ambiguous in {elf}")
    return hits[0]


def launch(qemu: str, flash: str, efuse: str, log: str, port: int | None) -> subprocess.Popen:
    open(log, "wb").close()
    cmd = [qemu, "-display", "none", "-monitor", "none", "-machine", "esp32",
           "-drive", f"file={flash},if=mtd,format=raw",
           "-drive", f"file={efuse},if=none,format=raw,id=efuse,snapshot=on",
           "-global", "driver=nvram.esp32.efuse,property=drive,value=efuse",
           "-serial", f"file:{log}"]
    if port is not None:  # gdb stub, started paused (for the injection mode)
        cmd += ["-gdb", f"tcp::{port}", "-S"]
    return subprocess.Popen(cmd, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def seed_guard(img: bytearray, attempts: int) -> None:
    """Write an LXBG boot-guard record at 0xC000 with the failed-boot counter
    pre-set — as two prior pre-guard panics would leave it. Matches
    ota.rs::write_guard_raw's encoding (magic + attempts, other fields 0)."""
    rec = b"LXBG" + bytes([attempts, 0, 0, 0])
    img[tko.LXBG_OFFSET:tko.LXBG_OFFSET + len(rec)] = rec


def free_port() -> int:
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close()
    return p


def run(args: argparse.Namespace, workdir: str, log: str) -> int:
    ota_path = os.path.join(args.result_dir, "luxel-fw-ota.bin")
    elf_path = os.path.join(args.result_dir, "luxel-fw.elf")
    for p in (args.stock, args.fs, ota_path, elf_path):
        if not os.path.exists(p):
            hint = "\n  run: nix build .#luxel-fw-athom-music" if args.result_dir in p else ""
            raise Fail(f"missing input: {p}{hint}")

    heap = elf_sym(elf_path, r"esp_alloc4HEAP$")
    add_region = elf_sym(elf_path, r"EspHeap10add_region$")
    print(f"== heap-regions test, --mode {args.mode} ==")
    print(f"   app image : {ota_path}")
    print(f"   HEAP=0x{heap:08x}  add_region=0x{add_region:08x}  inject {args.slots} stale slots")
    print(f"   work dir  : {workdir}")

    stock = open(args.stock, "rb").read()
    fs = open(args.fs, "rb").read()
    ota = open(ota_path, "rb").read()
    img = tko.compose(stock, fs, ota, "app1")  # realistic post-upload state
    if args.mode == "rollback":
        seed_guard(img, 2)  # counter already at the rollback threshold
    flash = os.path.join(workdir, "flash.bin")
    open(flash, "wb").write(img)
    efuse = os.path.join(workdir, "efuse.bin")
    tko.make_efuse(efuse)

    qemu = tko.resolve_qemu(args.qemu)
    print(f"   qemu      : {qemu}")
    passed: list[str] = []
    t0 = time.monotonic()

    def read_log() -> str:
        with open(log, "rb") as f:
            return f.read().decode("utf-8", "replace")

    def wait(needle: str, timeout: float, frm: int = 0) -> int:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            idx = read_log().find(needle, frm)
            if idx != -1:
                return idx + len(needle)
            time.sleep(0.12)
        raise Fail(f"timed out after {timeout:.0f}s waiting for {needle!r}")

    if args.mode == "rollback":
        proc = launch(qemu, flash, efuse, log, None)  # free-running, no gdb
    else:
        proc = launch(qemu, flash, efuse, log, (port := free_port()))

    try:
        if args.mode == "selfheal":
            time.sleep(0.5)
            r = Rsp("127.0.0.1", port, timeout=args.timeout)
            r.set_bp(add_region)
            stop = r.cont_until_stop()  # boot 1's first add_region
            if not stop.startswith(b"T05"):
                raise Fail(f"unexpected first stop: {stop!r}")
            for i in range(args.slots):  # stand in for the flash-flake corruption
                r.write_mem(heap + i * SLOT_STRIDE, b"\x01\x00\x00\x00")
            r.clear_bp(add_region)
            r.cont_nowait()
            print(f"   [{time.monotonic()-t0:5.1f}s] injected {args.slots} stale slots (boot 1)")

            end = wait(PANIC, 40)
            passed.append(f"panic reproduced: {PANIC!r}")
            last_boot = read_log().split(PANIC)[0].rsplit("rst:", 1)[-1]
            if OTA_INIT_MARKER in last_boot:
                raise Fail("panic came AFTER ota::init — not the pre-guard bug")
            if BOOT_PRINTLN in last_boot:
                raise Fail("panic came AFTER the boot println — not at the allocator")
            passed.append("panic precedes the boot println and ota::init (fires at the allocator)")
            end = wait(HALT, 10, end)
            passed.append("custom_halt reboots after the panic")
            end = wait(BOOT_PRINTLN, 30, end)
            end = wait(OTA_INIT_MARKER, 20, end)
            passed.append("next boot reaches ota::init — self-healed")
            end = wait(tko.REBOOT_LINE, 150, end)
            passed.append("takeover completes on the healed boot")

        else:  # rollback
            print(f"   [{time.monotonic()-t0:5.1f}s] seeded LXBG failed-boot counter = 2, booting…")
            end = wait(ROLLBACK, 60)
            passed.append(f"preboot_guard rolled back at the threshold: {ROLLBACK!r}")
            # The rollback must PREEMPT the allocators — no heap-regions panic,
            # and it happens before the boot println (which is after the
            # allocators). Both prove preboot_guard runs before the heap.
            head = read_log()[:end]
            if PANIC in head:
                raise Fail("the heap-regions panic fired despite the guard — not preempted")
            passed.append("rollback preempts the allocators (no heap-regions panic)")
            boot_prints = head.count(BOOT_PRINTLN)
            if boot_prints:
                raise Fail(f"boot println appeared before rollback ({boot_prints}x) — "
                           "guard ran too late")
            passed.append("rollback precedes the boot println (guard runs before the heap)")

        wall = time.monotonic() - t0
        print(f"\nPASS [{args.mode}] — {len(passed)} assertions in {wall:.1f}s")
        for a in passed:
            print(f"  ok  {a}")
        return 0
    finally:
        if proc.poll() is None:
            proc.terminate()
            try: proc.wait(timeout=10)
            except subprocess.TimeoutExpired: proc.kill()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--stock", required=True, help="athom-wled-stock.bin (4 MiB dump)")
    ap.add_argument("--fs", required=True, help="athom-wled-fs-configured.bin (littlefs)")
    ap.add_argument("--result-dir", default="result",
                    help="nix build .#luxel-fw-athom-music output (default ./result)")
    ap.add_argument("--qemu", help="qemu-system-xtensa (or its store dir); default: nix build")
    ap.add_argument("--mode", choices=("selfheal", "rollback"), default="selfheal")
    ap.add_argument("--slots", type=int, default=2,
                    help="stale Some slots to inject (2 => the realistic 'add #2 overflows' case)")
    ap.add_argument("--workdir", help="where to compose (default: a temp dir)")
    ap.add_argument("--keep", action="store_true", help="keep the work dir on success")
    ap.add_argument("--timeout", type=float, default=300.0)
    args = ap.parse_args(argv)
    if not (1 <= args.slots <= 3):
        ap.error("--slots must be 1..3")

    workdir = args.workdir or tempfile.mkdtemp(prefix="luxel-heapregions-")
    os.makedirs(workdir, exist_ok=True)
    log = os.path.join(workdir, "serial.log")
    try:
        rc = run(args, workdir, log)
    except Fail as e:
        print(f"\nFAIL [{args.mode}]: {e}", file=sys.stderr)
        if os.path.exists(log):
            tail = open(log, "rb").read().decode("utf-8", "replace").splitlines()[-40:]
            print("\n--- serial tail ---\n" + "\n".join(tail), file=sys.stderr)
        print(f"\nwork dir kept: {workdir}", file=sys.stderr)
        return 1
    if args.keep or args.workdir:
        print(f"work dir: {workdir}")
    else:
        shutil.rmtree(workdir, ignore_errors=True)
    return rc


if __name__ == "__main__":
    sys.exit(main())
