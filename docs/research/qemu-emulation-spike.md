# QEMU firmware-emulation spike (2026-08-16)

Question (Jeremy): can the WLED→Luxel takeover be tested in an emulator,
hardware-free — and is it worth it?

The spike ran in two sittings on the same day. The first left a verdict of
"~80% viable, one crisp blocker"; the second root-caused that blocker and
two more behind it, and fixed all three. This file is the resulting state
of the harness, with the spike history kept where it explains *why* a
patch exists.

## Verdict

**VIABLE.** All three CPU/interrupt-level blockers are root-caused and
fixed on the QEMU side. A **stock, unmodified** firmware image — the exact
bytes that ship — plus a chip-rev-3.0 eFuse image now boots under the
patched emulator through engine init, pattern storage, and settings, all
the way to the WiFi task. It stops there because the `esp-radio` PHY blob
touches an unmodelled peripheral alias (below), which is well past what a
takeover test needs: the WLED takeover path runs *before* WiFi init.

Minimal esp-rtos guests run embassy tasks with correct float math and
exact 500 ms tick timing.

**The compose→boot→assert takeover test is unblocked.**

## The isolation rule (Jeremy, standing)

The QEMU harness is **strictly isolated**. Every fix lives on the QEMU
side, in `tools/qemu/` — derivation patches and eFuse images. The firmware
image under test is byte-identical to what ships. No guest-side
workarounds, no QEMU-conditional firmware code, no build flags, ever. If
emulation needs something the firmware doesn't already do, the emulator is
what's wrong.

This is why the diagnostic examples written during the second sitting
(`firmware/examples/fpu_*.rs` — a bare esp-hal one-float print, and an
esp-rtos embassy tick print) were **deleted rather than committed**. They
answered their questions; they exist in session history and are a few
minutes to recreate; they are not part of the product.

## Root cause 1 — QEMU never initializes CPENABLE

*Fixed, committed: "qemu: set CPENABLE=0xff on esp32 core reset".*

ESP32 silicon comes out of reset with `CPENABLE = 0xff` (all coprocessors
enabled) — a vendor reset value, not the Xtensa architectural default.
QEMU's system-mode Xtensa core leaves it at 0.

Nothing in the ROM, the second-stage bootloader, or the app ever writes
`CPENABLE` (verified by disassembly of all three). esp-hal relies
structurally on the silicon reset value. So on QEMU the *first* float
instruction anywhere in the guest traps `Cp0Disabled` (EXCCAUSE 0x20) —
and then:

1. `xtensa-lx-rt`'s `float-save-restore` `save_context` runs to spill
   `f0..f15`,
2. its first coprocessor-gated instruction (`rur.fcr`) itself faults,
   because CPENABLE is *still* 0,
3. the double-exception handler calls `save_context` again,
4. silent infinite loop — no panic, no output, the guest just parks in
   `__default_naked_double_exception`.

That is the "blocker" the first sitting characterized (it saw the symptom:
`EXCCAUSE=0x20`, `EPC1` at the main task's first float, `EXCVADDR`
0x400C200C, double fault inside `save_context`).

Upstream state: reported as espressif/qemu#154; PR #155 is unmerged and
esp32s3-only. Upstream QEMU proper is still broken. NuttX worked around it
guest-side (apache/nuttx#6314 — enable CP in Reset). **No merged fix
exists anywhere**, so we carry ours.

## Root cause 2 — DPORT INTR_STATUS registers were never implemented

*Fixed: `tools/qemu/patches/esp32-dport-intr-status.patch`.*

With floats working, esp-rtos guests still livelocked. QEMU's esp32
machine never implemented `DPORT_PRO/APP_INTR_STATUS_REG_0..2` (DPORT base
+ 0x0EC..0x100) — the per-source pending bitmap describing which of the 69
interrupt-matrix inputs is currently asserting.

esp-hal's level-interrupt dispatcher reads exactly those registers
(`esp_hal::interrupt::InterruptStatus::current`). They read back 0, so no
handler was ever dispatched, so nothing acked the peripheral, so the level
line stayed asserted → endless interrupt storm on CPU int 1. Concretely:
esp-rtos's FROM_CPU scheduler-start interrupt was never acked and the
scheduler never started.

**esp-hal is the only guest OS that dispatches from these registers.**
ESP-IDF (`_xt_lowint1`), Zephyr, and NuttX all dispatch off the Xtensa
`INTERRUPT` special register plus their own software tables — which is why
nobody noticed in ~8 years. That contrast is the argument to lead with
when filing upstream.

The patch also fixes a related correctness bug it exposed: when several
matrix sources are routed to the same CPU interrupt line, the line now
deasserts only when the *last* of them drops.

No prior art anywhere. We are first.

## Root cause 3 — TIMG level interrupt gated on the wrong register

*Fixed: `tools/qemu/patches/esp32-timg-level-int.patch`.*

With dispatch working, the scheduler tick still never fired.
`hw/timer/esp32_timg.c` gated the timer's level IRQ on `TIMG_INT_ENA`. On
ESP32 and S2 **silicon that register is inert for level interrupts** — the
real gate is `TIMG_Tx_LEVEL_INT_EN` in the timer's own config register,
which is what esp-hal writes (it leaves `INT_ENA` at 0 forever; see the
"on ESP32 and S2 the int_ena register is ineffective" comment in
`esp-hal/src/timer/timg.rs`). ESP-IDF sets both, hence it never noticed.

Folded into the same patch: **espressif/qemu#69** — an alarm value already
behind the counter used to silently disarm the timer instead of firing
immediately. Easy to hit under slow emulation, where a deadline routinely
lands in the past by the time it's programmed. Upstream fixed this only
for the C3 systimer (PR #148).

## The eFuse image

*Committed: `tools/qemu/make-efuse.py`.*

QEMU boots with all-zero eFuses → chip revision v0.0. esp-hal's
`ESP_HAL_CONFIG_MIN_CHIP_REVISION` gate (default v3.0, what every release
build ships with) panics before `main`, and the second-stage bootloader
refuses images whose header declares a minimum revision. The first sitting
worked around this with a build-time override — which the isolation rule
forbids. The right fix is to present real silicon's revision.

`make-efuse.py` writes the 124-byte blk0–blk3 image QEMU's
`nvram.esp32.efuse` device pread()s at every reset: `CHIP_VER_REV1`
(blk0 bit 111) + `CHIP_VER_REV2` (bit 180), which combine with QEMU's
hardwired `APB_CTRL_DATE_REG` bit into major revision 3, plus a factory
MAC and its CRC. Padded to 512 bytes by default (QEMU's raw block backend
prefers a sector multiple).

Attaching an eFuse drive is Espressif's own documented approach
(esp-toolchain-docs, `qemu/esp32` README). Note the model **writes back**
on a fuse burn, so CI should attach it `snapshot=on`.

## How to run

Build the emulator (a flake output, so nixpkgs is pinned by flake.lock):

```sh
nix build .#qemu-espressif
```

The standalone `--impure --expr 'import ./tools/qemu/qemu-espressif.nix {}'`
form still works, but resolves `<nixpkgs>` off NIX_PATH and therefore
builds a *different* store path. Prefer the flake.

Generate an eFuse image (defaults are what you want: rev 3.0, Espressif
OUI MAC, 512-byte pad):

```sh
nix develop -c python3 tools/qemu/make-efuse.py -o efuse.bin
```

Boot a **stock** 4 MB flash image — no build flags, no firmware changes:

```sh
qemu-system-xtensa -nographic -machine esp32 -m 4M \
    -drive file=flash.bin,if=mtd,format=raw \
    -drive file=efuse.bin,if=none,format=raw,id=efuse,snapshot=on \
    -global driver=nvram.esp32.efuse,property=drive,value=efuse \
    -serial file:serial.log \
    -monitor unix:/tmp/mon.sock,server,nowait
```

Gotchas:

- **The monitor socket must live in `/tmp`** (or another short path). Unix
  socket paths are capped at 108 bytes and the worktree + scratchpad paths
  blow through that with a confusing error.
- Drop `snapshot=on` from the *flash* drive: the file is read/write and
  post-run assertions on partition-table bytes, otadata, and copied images
  are the whole point of a takeover test.
- **Symbolize with `xtensa-esp32-elf-addr2line -e <elf>`.** It's in the
  devshell (from `xtensa-esp-elf-gcc`); `llvm-addr2line` is *not* — the
  first sitting's notes said otherwise.

A "WLED just accepted the upload" flash image is: `athom-wled-stock.bin` +
configured littlefs dd'd at 0x310000 + the OTA app image dd'd at an app
slot (0x10000 boots with stock otadata).

## Where a stock image gets to today

```
luxel-fw: boot (60 px default, ws2812 @ 2400000 Hz SPI)
board: Athom music-reactive WLED controller
booted from: ota_0
assets: none installed
patterns: format 0 != 4, wiping storage
patterns: 0 stored (storage @ 0x210000)
settings: 60 px, ws2812, brightness 4/31 (default)
hostname: luxel-000001
wifi: compile-time creds ("MOMCorp Intranet")
wifi: joining "MOMCorp Intranet"
DEBUG - task_create wifi 0x40084de4(0x0) stack_size = 6656 priority = 29 ...
====================== PANIC ======================
Exception occurred on ProCpu 'LoadStoreAddrError'
  EXCCAUSE: 15, EXCVADDR: 0x6000_8000
```

The `esp-radio` PHY blob (`register_chipv7_phy`) touches the unmapped
0x6000_8000 peripheral alias; the boot guard then rolls the slot and the
image reboot-loops. Modeling the radio/PHY is the only path to WiFi-era
emulation and it is a large piece of work — but it is *past* the takeover
path, so it does not block the test this spike was for.

## QEMU-vs-silicon divergences to know about

- **QEMU is stricter than silicon on invalid peripheral addresses.** The
  esp32's region-protection granularity tolerates some stray loads that
  QEMU faults on (igrr's comment on espressif/qemu#130). Expect the
  emulator to surface "impossible" `LoadStoreAddrError`s that hardware
  shrugs off — the PHY fault above is likely one of these.
- **Espressif closed the last esp-rs-related issue "Won't Do."** Their
  stated position: esp-rs divergences get fixed in esp-rs, not in their
  QEMU fork. Plan for our patches to be carried, not merged.
- **Nobody in the ecosystem runs esp-rs-on-Xtensa under QEMU in CI.** The
  ecosystem answer is Wokwi CI or hardware-in-the-loop. We own these
  patches; there is no upstream to inherit from.

## Upstream-filing candidates

Worth filing even if they sit — each is a real bug with a clean repro:

1. **CPENABLE on esp32 core reset** — the esp32 hunk of what PR #155 does
   for the s3, against espressif/qemu#154.
2. **DPORT `INTR_STATUS` registers** — the highest-value one. Lead with
   the dispatch contrast: esp-hal is the only guest reading them, which is
   both why it went unnoticed and why it's a genuine model gap.
3. **TIMG level-interrupt gating + past-alarm firing** — two silicon
   behaviors, one patch; the past-alarm half is espressif/qemu#69, already
   fixed for the C3 systimer.
4. **The four RSA/AES robustness guards** from the first sitting (RSA:
   assert → ignore, zero modulus → skip since libgcrypt hard-aborts,
   oversized `modexp_mode_reg` bounds check; AES: garbage `mode.bits`
   overrunning the key copy). Guest-garbage tolerance, uncontroversial.
5. **An esp-rs issue** (esp-hal / xtensa-lx-rt): the `float-save-restore`
   `save_context` path turns *any* CPENABLE=0 coprocessor exception into a
   silent double-fault loop, because the handler's own first gated
   instruction re-faults. Even granting that silicon resets CPENABLE to
   0xff, a handler that cannot survive the exception it exists to handle
   is a footgun. Suggested fix: enable CP in `Reset`, as NuttX did in
   apache/nuttx#6314.

## The takeover test (shipped)

The prize the spike was chasing exists: **`tools/qemu/takeover-test.py`**
(usage in its header, indexed in docs/tools.md). Compose → boot → assert,
both variants passing against a stock image:

- `--slot app1` (default) — the realistic "WLED accepted the upload into
  its inactive slot" state; runs the full 920 KiB self-copy. **~12 s.**
- `--slot app0` — image already at the destination offset; the
  skip-the-copy path. **~1.5 s.**

Two things it turned up that are worth knowing before reading a log:

- **The `software_reset()` boot doesn't survive under QEMU.** The ROM
  banner stops mid-line at `mode:DOUT, clock div:`, the TG0 watchdog
  fires, and the resulting `TG0WDT_SYS_RESET` is the boot that actually
  loads Luxel. It happens before the second-stage bootloader reads
  anything, so it's cosmetic — but the log shows three resets, not two.
- **The bootloader writes otadata back on the erased-otadata fallback**
  (`set_actual_ota_seq()`: seq=1, `ESP_OTA_IMG_VALID`, CRC over the seq
  word). So post-takeover otadata at 0xD000 is *not* erased, and neither
  is the boot-guard sector at 0xC000 — boot 2's `ota::boot_guard` has
  already written its `LXBG` record by the time anything else runs.

Also confirmed, since it had never been exercised: `esp-storage`'s
`FlashStorage::capacity()` reports the real 4 MiB under QEMU, so the
takeover's flash-size preflight passes rather than refusing to
repartition.

## Next steps

1. **PHY/radio modeling** is the only route to testing anything WiFi-era
   (OTA, MQTT, the web UI) under emulation. Large. Only worth starting if
   emulated WiFi becomes the bottleneck for something specific.
2. **File the upstream candidates** above.
