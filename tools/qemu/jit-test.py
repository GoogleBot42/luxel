#!/usr/bin/env python3
"""The JIT's hardware-free execution gate: native vs interpreted pixels,
compared bit for bit on an emulated ESP32 (Gitea #658, docs/jit-design.md
§7.2).

WHAT THIS ADDS over the host gates. `crates/luxel-jit/tests/library_diff.rs`
and `engine_diff.rs` already prove the emitted code and the engine glue
against the interpreter — through an Xtensa ISA MODEL, on x86. What they
cannot tell you is whether the real bytes execute on a real Xtensa core,
inside the real firmware, out of the real `.rwtext` exec buffer, with the
real windowed-ABI register file and the real render-task stack. That is
what this does: it boots the shipping image under QEMU's ESP32 model,
lets the render task compile and run a pattern natively, reads the
rendered frame out of guest RAM, then does it again with the JIT switched
off and compares.

HOW IT GETS THE PIXELS, AND WHY IT IS NOT HTTP
----------------------------------------------
docs/jit-design.md §7.2 assumed the differential would drive `/api/code`
and snapshot `/api/pixels`. **It cannot**: QEMU's `esp32` machine models no
radio, `WifiController::new` panics inside the esp-radio PHY blob on an
unmodelled peripheral alias (docs/research/qemu-emulation-spike.md), and
the web server is spawned after that line. There is no network and no
console command interface, so no request ever reaches the guest.

What does work, and is strictly better than a snapshot endpoint:

* The render task runs on the **AppCpu**, on its own executor started well
  before the WiFi call (`firmware/src/core1.rs`, `main.rs`). It decodes
  the boot pattern, compiles it, renders a frame and publishes it to
  `shared::PIXELS` — all before the ProCpu panics.
* QEMU's gdbstub reads and writes guest memory, which
  `tools/qemu/heap-regions-test.py` already relies on. So the frame is
  read straight out of `shared::PIXELS`, and the JIT is switched off by
  writing one byte to `LUXEL_JIT_ENABLED` — the SAME switch
  `POST /api/jit` flips, which is why that static is `#[no_mangle]`. The
  switch ships OFF (firmware/src/jit.rs says why), so it is the NATIVE boot
  that does the writing.

Nothing guest-side is conditional on emulation and the image under test is
byte-identical to what `nix build .#luxel-fw-esp32-generic-jit` produces
for anyone (CLAUDE.md's QEMU-isolation rule).

WHAT IT CANNOT COVER
--------------------
**Only patterns whose first frame is a function of the pattern.** `time()`
reads wall time and the two sides do not reach frame one at the same moment
— the native boot spends its `compile_us` first. Such a pattern is reported
as ran-natively-but-not-comparable, and ONLY when the mismatch is real and
the source actually names a wall-clock builtin: a match is never explained
away (`rainbow` reads `time()` and still agrees), and a mismatch with no
clock input is a failure. `engine_diff.rs` covers those patterns properly on
the host, four frames each, with a fixed delta and a frozen clock.

**One frame per boot.** The embassy-time alarm is bound to the ProCpu,
which is spinning at INTLEVEL 3 inside the panic halt, so the render
loop's `Timer::after` never wakes. The frame compared is therefore frame
one at t=0 — enough to exercise init, `beforeRender` and the whole pixel
pass, not enough to exercise frame-to-frame state. `engine_diff.rs`
covers four frames of all 307 patterns on the host; this covers one
frame of real execution.

**One pattern per boot.** The ProCpu executor never runs, so the playlist
and resume tasks never swap anything — the device renders the pattern
build.rs baked in. `--patterns` therefore rebuilds the image per pattern
through `LUXEL_DEFAULT_PATTERN`, which is minutes rather than seconds;
the default single-pattern run is what `run-all.py` carries.

USAGE
    tools/qemu/jit-test.py [--qemu DIR] [--result-dir DIR]
    tools/qemu/jit-test.py --patterns rainbow.js,snake.js,snake-2d.js
    tools/qemu/jit-test.py --patterns @five      # the §7.1 five
Exit 0 = pass.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)

from gdbrsp import Rsp  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "takeover_test", os.path.join(HERE, "takeover-test.py")
)
tko = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(tko)

# The five of docs/jit-design.md §7.1: two strip patterns, a 2D one, the
# noise-heavy #260 probe, and a renderFrame pattern.
FIVE = [
    "rainbow.js",
    "snake.js",
    "snake-2d.js",
    "perlin-fire-wind-tunnel.js",
    "bulk-canvas-ripples-2d.js",
]

PANIC_HALT = "panic: rebooting in 3s"
BOOT_LINE = "luxel-fw: boot"
JIT_NATIVE = re.compile(r"jit: native, (\d+) fns, (\d+) B code \((\d+) B pool\), (\d+) us")
JIT_INTERP = re.compile(r"jit: interpreter \(([^)]*)\)")

# `board-esp32-generic`'s compile-time default pixel count. `shared::PIXELS`
# holds 3 bytes per pixel.
DEFAULT_PIXELS = 60

# Builtins that read WALL time. The only inputs a pattern has that are not
# a function of the pattern: everything else the VM hands it — the seed,
# `pixelCount`, the coordinates — is fixed at this boot. `random()` is not
# here because the engine seeds it with a constant.
WALL_CLOCK = ("time(", "clockHour", "clockMinute", "clockSecond",
              "clockYear", "clockMonth", "clockDay", "clockWeekday")


class Fail(Exception):
    pass


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def elf_sym(elf: str, pattern: str) -> int:
    """Address of the single symbol matching `pattern`. Same helper as
    heap-regions-test.py — every address here is build-specific and must be
    re-derived per image."""
    out = subprocess.run(["nm", elf], capture_output=True, text=True).stdout
    hits = [
        int(p[0], 16)
        for line in out.splitlines()
        if len(p := line.split()) == 3 and re.search(pattern, p[2])
    ]
    if not hits:
        raise Fail(f"symbol /{pattern}/ not found in {elf}")
    if len(hits) > 1:
        raise Fail(f"symbol /{pattern}/ is ambiguous in {elf}: {len(hits)} matches")
    return hits[0]


# --------------------------------------------------------------- building


def build_image(pattern: str | None, workdir: str) -> tuple[str, str]:
    """Build the classic-ESP32 JIT image and return `(merged flash, elf)`.

    `pattern is None` uses the flake output, which is cached and is what
    `run-all.py` wants. A named pattern has to go through build-esp32.sh
    with `LUXEL_DEFAULT_PATTERN`, because the boot default is the only
    pattern reachable under emulation.
    """
    if pattern is None:
        out = os.path.join(workdir, "result-jit")
        subprocess.run(
            ["nix", "build", f"{REPO}#luxel-fw-esp32-generic-jit", "--out-link", out],
            cwd=REPO,
            check=True,
        )
        return os.path.join(out, "luxel-fw.bin"), os.path.join(out, "luxel-fw.elf")

    src = os.path.join(REPO, "library", pattern)
    if not os.path.exists(src):
        raise Fail(f"library/{pattern} does not exist")
    env = dict(os.environ, BOARD="board-esp32-generic", EXTRA_FEATURES="jit",
               SKIP_ASSETS="1", LUXEL_DEFAULT_PATTERN=src)
    r = subprocess.run(["./build-esp32.sh"], cwd=os.path.join(REPO, "firmware"),
                       env=env, capture_output=True, text=True)
    if r.returncode != 0:
        raise Fail(f"build for {pattern} failed:\n{r.stdout[-4000:]}\n{r.stderr[-4000:]}")
    elf = os.path.join(REPO, "firmware/target/xtensa-esp32-none-elf/release/luxel-fw")
    merged = os.path.join(workdir, f"flash-{pattern}.bin")
    subprocess.run(
        ["espflash", "save-image", "--chip", "esp32", "--merge",
         "--flash-size", "4mb", "--partition-table", "partitions.csv", elf, merged],
        cwd=os.path.join(REPO, "firmware"), check=True, capture_output=True,
    )
    # Keep a private copy of the ELF: the next pattern's build overwrites
    # the one in target/ (all three classic boards share that path).
    elf_copy = os.path.join(workdir, f"elf-{pattern}")
    shutil.copy2(elf, elf_copy)
    return merged, elf_copy


def pad_flash(image_path: str, workdir: str, tag: str) -> str:
    """A writable 4 MiB copy of the merged image. Not `snapshot=on`: the
    guest's own writes (otadata, the store wipe) have to persist within a
    run, and each run gets a fresh copy so they do not persist across one."""
    data = open(image_path, "rb").read()
    if data[0x1000:0x1001] != b"\xe9" or data[0x10000:0x10001] != b"\xe9":
        raise Fail(f"{image_path}: no 0xE9 image magic at 0x1000/0x10000")
    img = bytearray(b"\xff" * tko.FLASH_SIZE)
    img[: len(data)] = data
    out = os.path.join(workdir, f"flash-{tag}.img")
    open(out, "wb").write(img)
    return out


# ---------------------------------------------------------------- running


def launch(qemu: str, flash: str, efuse: str, log: str, port: int) -> subprocess.Popen:
    open(log, "wb").close()
    cmd = [qemu, "-display", "none", "-monitor", "none", "-machine", "esp32",
           "-drive", f"file={flash},if=mtd,format=raw",
           "-drive", f"file={efuse},if=none,format=raw,id=efuse,snapshot=on",
           "-global", "driver=nvram.esp32.efuse,property=drive,value=efuse",
           "-serial", f"file:{log}",
           "-gdb", f"tcp::{port}", "-S"]
    return subprocess.Popen(cmd, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def read_pixels(r: Rsp, addr: int, pixels: int) -> bytes:
    """Decode `shared::PIXELS` — a `critical_section::Mutex<RefCell<Vec<u8>>>`.

    Sixteen bytes at the symbol, measured (the release ELF carries no
    DWARF, so the field order was pinned empirically by poisoning each
    word and seeing which one the render task restored):

        +0  RefCell borrow flag      +4  Vec capacity
        +8  Vec data pointer         +12 Vec length

    `ptr == 1` is the dangling pointer of an empty `Vec`, i.e. "no frame
    published yet this boot".
    """
    borrow, cap, ptr, length = struct.unpack("<4I", r.read_mem(addr, 16))
    want = pixels * 3
    # `ptr == 1` is an empty `Vec`'s dangling pointer: nothing was published
    # this boot. Returned as b"" rather than raised, so the CALLER can say
    # which side went quiet and why that matters.
    if not (0x3F000000 <= ptr < 0x40000000):
        return b""
    if length != want:
        raise Fail(f"PIXELS len {length}, expected {want} (borrow={borrow} cap={cap})")
    return r.read_mem(ptr, length)


def wait_for(log: str, needle: str, proc: subprocess.Popen, timeout: float) -> str:
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        text = open(log, "rb").read().decode("utf-8", "replace")
        if needle in text:
            return text
        if proc.poll() is not None:
            raise Fail(f"qemu exited early (rc={proc.returncode})")
        time.sleep(0.1)
    raise Fail(f"timed out waiting for {needle!r} after {timeout}s")


def one_boot(qemu: str, flash_src: str, elf: str, efuse: str, workdir: str,
             tag: str, jit_on: bool, timeout: float) -> tuple[bytes, str]:
    """Boot once and return `(frame bytes, serial log)`.

    The breakpoint is `try_budgeted_engine` — the activation choke point,
    reached on the render task before anything is compiled. Writing
    `LUXEL_JIT_ENABLED` there is the runtime A/B lever, so BOTH sides run
    the identical image and differ only in one byte of RAM; an A/B between
    two separately built images would also be measuring the linker.
    """
    flash = pad_flash(flash_src, workdir, tag)
    log = os.path.join(workdir, f"serial-{tag}.log")
    port = free_port()
    proc = launch(qemu, flash, efuse, log, port)
    try:
        # QEMU is started paused (-S), so the stub is up before any guest
        # instruction runs and the breakpoint cannot be missed.
        deadline = time.monotonic() + 15
        while True:
            try:
                r = Rsp("127.0.0.1", port, timeout=timeout)
                break
            except OSError:
                if time.monotonic() > deadline or proc.poll() is not None:
                    raise Fail("gdbstub never came up")
                time.sleep(0.1)
        with r:
            # `budgeted_engine`, not `try_budgeted_engine`: the outer one is
            # inlined into the render task's boot path under fat LTO, so its
            # symbol exists and is never executed. This is the first
            # out-of-line call on the activation path, and it runs before
            # `jit::try_compile` reads the switch. (The mangled length
            # prefix — `fw15budgeted_engine` vs `fw19try_budgeted_engine` —
            # is what keeps the pattern from matching both.)
            activate = elf_sym(elf, r"luxel_fw15budgeted_engine$")
            flag = elf_sym(elf, r"^LUXEL_JIT_ENABLED$")
            halt = elf_sym(elf, r"^custom_halt$")
            r.set_bp(activate)
            r.cont_until_stop()
            r.write_mem(flag, b"\x01" if jit_on else b"\x00")
            got = r.read_mem(flag, 1)
            if got != (b"\x01" if jit_on else b"\x00"):
                raise Fail(f"LUXEL_JIT_ENABLED write did not land ({got!r})")
            # MUST clear before resuming: QEMU's xtensa gdbstub does not
            # auto-step past a software breakpoint on `c`, so leaving it
            # armed re-stops at the same PC forever (docs/tools.md).
            r.clear_bp(activate)
            # Where to read the frame. NOT the panic halt: the ProCpu
            # panics inside the radio blob while the AppCpu is still
            # decoding and compiling, so at `custom_halt` there is no frame
            # yet. The halt then spins in `delay_millis(3000)` at INTLEVEL
            # 3 — during which the AppCpu runs on and publishes — and the
            # reset at the end of it is the point where the frame is
            # certainly there and certainly still the one this boot made.
            # Both breakpoints, in order, so this needs no polling and no
            # wall-clock guessing.
            reset = elf_sym(elf, r"esp_hal6system14software_reset$")
            r.set_bp(halt)
            r.set_bp(reset)
            r.cont_until_stop()  # custom_halt
            r.clear_bp(halt)  # ...and QEMU will not step past it on `c`
            r.cont_until_stop()  # software_reset, ~3 s of guest time later
            frame = read_pixels(r, elf_sym(elf, r"luxel_fw6shared6PIXELS$"), DEFAULT_PIXELS)
            r.clear_bp(reset)
        text = open(log, "rb").read().decode("utf-8", "replace")
        if BOOT_LINE not in text:
            raise Fail("guest never printed the boot line")
        return frame, text
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()


def check_one(qemu: str, efuse: str, workdir: str, pattern: str | None,
              timeout: float) -> list[str]:
    """Run one pattern both ways. Returns the notes to print; raises on a
    failure."""
    name = pattern or "rainbow.js (built in)"
    src = open(os.path.join(REPO, "library", pattern or "rainbow.js")).read()
    flash, elf = build_image(pattern, workdir)
    tag = (pattern or "default").replace(".", "_")

    native, log_n = one_boot(qemu, flash, elf, efuse, workdir, tag + "-on", True, timeout)
    m = JIT_NATIVE.search(log_n)
    if not m:
        why = JIT_INTERP.search(log_n)
        if why is None:
            raise Fail(f"{name}: no jit line on serial at all")
        # A pattern over this board's exec-buffer cap is a CORRECT outcome
        # (docs/jit-design.md §4a: whole-program or nothing, and the
        # interpreter runs it) — there is simply no native side to compare.
        # Reported, never silently dropped, and only this one reason is
        # forgiven: every other refusal means the emitter or the glue
        # changed its mind about a pattern it used to take.
        if "over the" in why.group(1) and " B cap" in why.group(1):
            return [f"--  {name}: refused, {why.group(1)} (interpreted, nothing to diff)"]
        raise Fail(f"{name}: the JIT refused it — jit.reason = {why.group(1)}")
    fns, code_b, pool_b, us = (int(g) for g in m.groups())

    interp, log_i = one_boot(qemu, flash, elf, efuse, workdir, tag + "-off", False, timeout)
    off = JIT_INTERP.search(log_i)
    if not off or off.group(1) != "disabled":
        raise Fail(f"{name}: the runtime switch did not take — "
                   + (f"got {off.group(1)!r}" if off else "no jit line on serial"))
    if not interp:
        raise Fail(f"{name}: the INTERPRETED boot published no frame either — this "
                   f"pattern does not render on this board at all, so there is no "
                   f"oracle to compare against")

    # WHEN A MISMATCH IS NOT A BUG, and how that is decided.
    #
    # QEMU's guest clock is VIRTUAL — a host-side hold at a breakpoint does
    # not advance it (measured), so identical execution gives identical
    # timing and the harness is deterministic. But the two sides of this
    # differential do NOT execute identically before frame one: the native
    # boot spends its `compile_us` compiling first. A pattern whose first
    # frame is built out of `time()` therefore renders a legitimately
    # different frame, and no amount of retrying will make it agree —
    # `perlin-fire-wind-tunnel`'s `beforeRender` is five `time()` calls and
    # a 22 ms shift is 0.4 % of its shortest ramp.
    #
    # So a mismatch is excused for exactly one reason, and only when the
    # source actually carries it: the pattern reads the wall clock. A
    # MATCH is never explained away — `rainbow` reads `time()` too and
    # still agrees bit for bit, because 5.7 ms of a 6.55 s ramp does not
    # survive 8-bit quantisation — and a mismatch in a pattern with no
    # clock input is a failure, full stop.
    #
    # The half this cannot cover is covered properly on the host:
    # `engine_diff.rs` runs four frames of all 307 patterns with a fixed
    # delta and a frozen clock.
    if not native:
        # The render task never published. On the native side that is the
        # open trap LUXEL_JIT_ENABLED documents: two of the patterns tried
        # so far compile, start, and take the AppCpu down with a clobbered
        # stack guard. Reported as a FAILURE, loudly — it is the finding
        # this gate exists to produce, and the day it stops reproducing is
        # the day someone has fixed it.
        raise Fail(f"{name}: ran natively ({fns} fns / {code_b} B in {us} us) and then "
                   f"published NO frame — the render task died. Check the serial log "
                   f"for `stack guard` / EXCCAUSE. This is the open #658 trap.")
    if not any(native):
        raise Fail(f"{name}: the native frame is all-black — nothing was proved")
    if native != interp:
        at = next(i for i, (a, b) in enumerate(zip(native, interp)) if a != b)
        detail = (f"at byte {at} (pixel {at // 3}): "
                  f"native {native[at - at % 3:at - at % 3 + 3].hex()} "
                  f"interpreted {interp[at - at % 3:at - at % 3 + 3].hex()}")
        clock = sorted(b.rstrip("(") + "()" for b in WALL_CLOCK if b in src)
        if not clock:
            raise Fail(f"{name}: PIXELS differ {detail} — and this pattern reads no "
                       f"wall clock, so the two sides should agree")
        return [f"--  {name}: ran natively ({fns} fns / {code_b} B in {us} us) but its "
                f"first frame moves with the clock ({', '.join(clock)}); the compile "
                f"shifts it, so pixels are not comparable here — differs {detail}"]
    return [f"ok  {name}: {len(native)} B frame identical, "
            f"{fns} fns / {code_b} B code ({pool_b} B pool) in {us} us"]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--qemu", default=None,
                    help="qemu-system-xtensa, or the directory holding bin/")
    ap.add_argument("--result-dir", default="result",
                    help="accepted for run-all.py's uniform argument list; this test "
                         "builds its own image (the shipped one has no JIT)")
    ap.add_argument("--patterns", default=None,
                    help="comma-separated library/ names, or @five for the §7.1 set. "
                         "Each needs its own firmware build — minutes, not seconds. "
                         "Omitted = the built-in default pattern only.")
    ap.add_argument("--timeout", type=float, default=180.0)
    ap.add_argument("--stock", default=argparse.SUPPRESS, help=argparse.SUPPRESS)
    ap.add_argument("--fs", default=argparse.SUPPRESS, help=argparse.SUPPRESS)
    args = ap.parse_args()

    if args.patterns == "@five":
        patterns: list[str | None] = list(FIVE)
    elif args.patterns:
        patterns = [p.strip() for p in args.patterns.split(",") if p.strip()]
    else:
        patterns = [None]

    qemu = tko.resolve_qemu(args.qemu)
    workdir = tempfile.mkdtemp(prefix="luxel-jit-qemu-")
    efuse = os.path.join(workdir, "efuse.bin")
    tko.make_efuse(efuse)

    print("== jit differential (native vs interpreted pixels, emulated ESP32) ==")
    print(f"   qemu    : {qemu}")
    print(f"   patterns: {len(patterns)}")
    notes: list[str] = []
    t0 = time.monotonic()
    try:
        for p in patterns:
            notes += check_one(qemu, efuse, workdir, p, args.timeout)
            print(notes[-1])
    except Fail as e:
        print(f"FAIL — {e}")
        print(f"   artifacts kept in {workdir}")
        return 1
    finally:
        pass
    diffed = sum(1 for n in notes if n.startswith("ok"))
    if diffed == 0:
        print("FAIL — no pattern was comparable; nothing was proved")
        return 1
    print(f"PASS [jit] — {diffed} compared bit-for-bit, "
          f"{len(notes) - diffed} refused or clock-dependent, "
          f"{time.monotonic() - t0:.1f}s")
    shutil.rmtree(workdir, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
