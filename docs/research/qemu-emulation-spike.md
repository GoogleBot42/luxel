# QEMU firmware-emulation spike (2026-08-16)

Question (Jeremy): can the WLED→Luxel takeover be tested in an emulator,
hardware-free — and is it worth it? Timeboxed spike; this file is the
full state so the harness work can resume where it stopped.

## Verdict so far

**~80% viable, one crisp blocker left.** Everything infrastructural
works: Espressif's QEMU fork builds reproducibly under nix
(`tools/qemu/qemu-espressif.nix`), boots our real bootloader, parses our
real partition table, loads and runs our real app image from a plain
flash *file* — which is exactly what a takeover test needs (compose the
"WLED just accepted our upload" flash image, boot, assert on serial +
the flash file afterwards). The blocker is a guest-side double fault in
the esp-rtos/xtensa-lx-rt FPU context path that stops our firmware
before its first print. It is precisely characterized (below) and looks
fixable-or-workaroundable, but it is real work, not config.

## What was proven

- `qemu-system-xtensa -machine esp32 -drive file=<4MB flash>,if=mtd`
  boots the espflash ESP-IDF bootloader, prints our partition table,
  image-loads the app (both our native layout and app-only images).
- The flash file is fully read/write — post-run assertions on
  partition-table bytes / otadata / copied images are trivial.
- Espressif publishes no usable prebuilt for NixOS (works after
  patchelf, kept at nothing — the nix build is the way) and their
  `-src.tar.xz` is NOT a dist tarball: the meson wrap subprojects
  (keycodemapdb, berkeley-softfloat/testfloat, libslirp) must be
  vendored — the derivation does this.
- esp-hal's minimum-chip-revision gate (default v3.0) panics on QEMU's
  emulated rev v0.0 — `ESP_HAL_CONFIG_MIN_CHIP_REVISION=0` at build
  time clears it. (Better long-term: craft an eFuse image presenting
  rev 3.0 so *release* images boot unmodified and the rev-0 errata
  paths don't run; QEMU takes an efuse backing drive.)
- Early guest init performs a descending write sweep across the crypto
  peripherals (suspected rev-0-related init path) that Espressif's
  device models tolerate badly — four one-line guards patched into the
  derivation fix crashes in the RSA model (assert → ignore; zero
  modulus → skip, libgcrypt hard-aborts on it; oversized
  `modexp_mode_reg` → bounds check) and the AES model (garbage
  `mode.bits` overruns the key copy / round computation). All are
  guest-garbage-tolerance bugs worth upstreaming to espressif/qemu.

## The blocker

With the models patched, the guest silently parks in
`__default_naked_double_exception`. Monitor + `llvm-addr2line` against
the ELF:

- `EXCCAUSE=0x20` (Cp0Disabled — FPU coprocessor access while
  `CPENABLE` off), `EPC1` = the embassy main task closure's first float
  use, `DEPC` in the double-exception loop, and the double fault occurs
  inside `xtensa-lx-rt`'s `save_context` (`EPC6`), `EXCVADDR=0x400C200C`
  (just past RTC-fast instruction memory).
- Our builds DO have esp-hal's default `float-save-restore` feature, and
  QEMU's esp32 core config models the FPU (`XCHAL_HAVE_FP=1`,
  `XCHAL_HAVE_CP=1`). On hardware this exact firmware runs float-heavy
  at 120+ fps — so the divergence is in how the first
  coprocessor-disabled event resolves under QEMU vs silicon.
- Working theory: the exception path that should (re)enable CP0 and
  save/restore FPU state itself touches FPU/coprocessor state while
  CPENABLE is still off under QEMU's ordering → exception-inside-
  save_context → double fault. Whether the root is a QEMU CPENABLE/
  coprocessor modeling subtlety or an xtensa-lx-rt assumption silicon
  happens to satisfy is the open question.

Next steps (in order of information value):
1. Minimal repro: a 20-line esp-hal binary that does one float op —
   boot it in this QEMU. Isolates esp-rtos/embassy from the equation.
2. eFuse image with chip rev 3.0 (drops the rev-0 paths, matches real
   silicon, lets stock release images boot).
3. If the minimal repro faults: file against espressif/qemu with the
   register dump; if it doesn't: instrument xtensa-lx-rt's
   save_context (the `float-save-restore` path) and compare.
4. File the four model-robustness patches upstream regardless.

## Is it worth finishing?

The prize is real: a CI-runnable end-to-end takeover test (compose
WLED-state flash → boot → assert takeover serial lines + repartitioned
flash bytes + inherited-creds record) — the single most dangerous code
path in the product, currently guarded only by `tools/image-check.sh`
and bench sessions. The blocker is the kind that either dissolves in a
day (upstream fix / efuse rev / xtensa-lx-rt flag) or reveals genuine
esp-rtos-under-QEMU hostility. Recommendation: one more timeboxed
session on steps 1–2; fall back to host-side trait-based takeover logic
tests (the `wledfs-check` pattern) if it resists.

## Spike mechanics (for the resumer)

- Build QEMU: `nix build --impure --expr 'import ./tools/qemu/qemu-espressif.nix {}'`
- Firmware with the rev gate lowered (dev build, worktree):
  `BOARD=board-athom-music SKIP_ASSETS=1 ESP_HAL_CONFIG_MIN_CHIP_REVISION=0 ./build-esp32.sh`
- Run: `qemu-system-xtensa -nographic -machine esp32 -drive file=flash.bin,if=mtd,format=raw`
  (plus `-monitor unix:mon.sock,server,nowait` for `info registers`;
  symbolize PCs with `llvm-addr2line -e <elf>`).
- A "WLED just accepted the upload" flash image is:
  `athom-wled-stock.bin` + configured littlefs dd'd at 0x310000 +
  the OTA app image dd'd at an app slot (0x10000 boots with stock
  otadata).
