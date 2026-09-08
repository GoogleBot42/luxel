# Board → chip / rust target / toolchain map. Sourced (not executed) by
# firmware/build-esp32.sh and tools/stack-check.sh so the two never drift.
# (Part of the firmware crate: GPL-3.0-or-later, see firmware/LICENSE.)
#
# Sets, from $BOARD:
#   CHIP        espflash --chip value
#   TARGET      rustc target triple
#   XTENSA      1 → Espressif's rustc fork + -Zbuild-std (no prebuilt core);
#               0 → mainline Rust with the target installed via rustup/nix
#   CORE_O3     1 → the luxel-core crate (VM + engine, the per-pixel hot
#               path) is compiled at opt-level 3 inside the otherwise
#               size-optimized image (Gitea #260: ~18-20 KB of app image
#               for a faster interpreter); 0 → profile default ("s"), for
#               boards whose OTA-slot margin can't carry it (docs/boards.md).
#               flake.nix's firmwareVariants carry the same flag (coreO3).
#   IRAM        space-separated cargo features that place the interpreter's
#               per-pixel native code in internal SRAM (`.rwtext`) instead of
#               executing it through the flash instruction cache (Gitea #328):
#               `iram-vm` (Vm::run, 13.2 KB), `iram-builtins` (call_builtin +
#               builtin_hot, 7.3 KB), `iram-math` (hsv_to_rgb + the hot fmath
#               and noise leaves, 9.4 KB). Worth 1.5-4x on builtin-heavy
#               patterns on the classic ESP32; the budget is per board and is
#               NOT free on the S3/C-series, where IRAM and DRAM are the same
#               SRAM and every byte here comes straight out of `.stack`.
#               Per-board budget: docs/boards.md; the rule for adding code:
#               docs/firmware.md "Code placement".
#               flake.nix's firmwareVariants carry the same list (iram).
#
# Adding a board? Add its case here as well as the three files in
# docs/boards.md ("Adding a board").
# The RISC-V parts (C3 16 KB icache, C6 32 KB) have no Luxel on the bench, so
# their placement is UNMEASURED — see Gitea #337. `.rwtext` there is the same
# unified SRAM as the stack, and the C6 has the fleet's tightest budget, so the
# default is off until someone can measure it on metal.
RISCV_IRAM="${RISCV_IRAM:-}"

board_target() {
  IRAM=""
  case "$1" in
    board-pixelblaze-v3|board-athom-music|board-esp32-generic)
      # 128 KB of dedicated IRAM (SRAM0), separate from the DRAM the stack
      # comes out of: the whole per-pixel path fits with ~35 KB to spare.
      CHIP=esp32;    TARGET=xtensa-esp32-none-elf;      XTENSA=1; CORE_O3=1
      IRAM="iram-vm iram-builtins iram-math" ;;
    board-s3-devkit|board-seengreat-hub75)
      # unified SRAM: .rwtext comes out of .stack (46.0 -> 33.0 KB for
      # iram-vm alone). The other two would leave <1 KB over the 24 KB
      # floor for ~1 % — measured on the panel, not worth it (Gitea #328).
      CHIP=esp32s3;  TARGET=xtensa-esp32s3-none-elf;    XTENSA=1; CORE_O3=1
      IRAM="iram-vm" ;;
    board-c3-devkit)
      CHIP=esp32c3;  TARGET=riscv32imc-unknown-none-elf;  XTENSA=0; CORE_O3=1
      IRAM="$RISCV_IRAM" ;;
    board-c6-devkit)
      # tightest slot margin in the fleet: opt-level 3 on luxel-core would
      # put it under the 3 % CI floor (measured 2026-09-05: 50.8 → 30.5 KB)
      CHIP=esp32c6;  TARGET=riscv32imac-unknown-none-elf; XTENSA=0; CORE_O3=0
      IRAM="$RISCV_IRAM" ;;
    *)
      echo "unknown BOARD '$1' — see docs/boards.md" >&2; return 1 ;;
  esac
}

# Every board bakes its `board::NAME` (firmware/src/board.rs) into the image
# as a plain string — main.rs prints it at boot, so it is always linked.
# That string is the only thing in an app image that identifies the board:
# the three classic-ESP32 boards share one ELF path
# (target/xtensa-esp32-none-elf/release/luxel-fw), so a stale build of
# another board pushes cleanly and shows up only as the wrong
# RESERVED_PINS / defaults on the device (Gitea #389 — a pb-v3 image on the
# Athom reserved GPIO18, the pin the strip is wired to, leaving it dark with
# no in-band way to fix the pin). tools/ota-push.sh greps for it.
#
# Sets BOARD_NAME from $1. Substrings are fine and preferred where the name
# varies with a feature (board-s3-devkit gains "+ HUB75 panel" under
# `hub75`). Adding a board? Add its case here too.
BOARD_LIST="board-pixelblaze-v3 board-athom-music board-esp32-generic board-c3-devkit board-c6-devkit board-s3-devkit board-seengreat-hub75"
board_name() {
  case "$1" in
    board-pixelblaze-v3)   BOARD_NAME="Pixelblaze v3 Standard" ;;
    board-athom-music)     BOARD_NAME="Athom music-reactive WLED controller" ;;
    board-esp32-generic)   BOARD_NAME="generic ESP32 (VSPI: CLK 18, DATA 23)" ;;
    board-c3-devkit)       BOARD_NAME="ESP32-C3 devkit" ;;
    board-c6-devkit)       BOARD_NAME="ESP32-C6 devkit" ;;
    board-s3-devkit)       BOARD_NAME="ESP32-S3 devkit" ;;
    board-seengreat-hub75) BOARD_NAME="Seengreat RGB Matrix HUB75 S3" ;;
    *)
      echo "unknown BOARD '$1' — see docs/boards.md" >&2; return 1 ;;
  esac
}

# --------------------------------------------------------------------------
# RUSTFLAGS (Gitea #441)
#
# Two problems solved here at once:
#
# 1. Every image baked ~13.5 KB of ABSOLUTE dependency source paths — one
#    `core::panic::Location` string per dependency file containing a
#    panicking construct (unwrap, index, slice, overflow check). The useful
#    part is the tail (`esp-hal-1.1.0/src/system.rs`); the 55-70 character
#    prefix is build-machine trivia repeated ~140 times.
# 2. Because that prefix is the build directory, THE SAME COMMIT WEIGHED A
#    DIFFERENT NUMBER OF BYTES ON EVERY MACHINE — measured three ways on
#    2026-09-08 the C6 hosted image came out 1,014,400 / 1,015,568 /
#    1,017,168 B, the last of which fails tools/image-check.sh's 3 %
#    OTA-slot floor. A gate on a number that moves with $PWD is not a gate.
#
# `--remap-path-prefix` (stable rustc flag) fixes both: the roots are
# discovered at build time and mapped to nothing, so a Location reads
# `esp-hal-1.1.0/src/system.rs` on every machine.
#
# The catch, and why this lives here: RUSTFLAGS **overrides** rather than
# merges with firmware/.cargo/config.toml's `[target.*] rustflags`, so every
# caller that exports it must re-supply the linker args. build-esp32.sh,
# tools/stack-check.sh and flake.nix all read the two functions below so the
# three can't drift. (Keep LINK_RUSTFLAGS in sync with .cargo/config.toml,
# which is what a bare `cargo build`/rust-analyzer still uses.)
#
# Diagnostics cost: DWARF is remapped too, so tools/decode-backtrace.sh and
# `espflash monitor --elf` print a dependency panic as
# `esp-hal-1.1.0/src/system.rs:42` instead of a path you can open directly.
# Prefix it with the registry/vendor root to find the file.

# Sets LINK_RUSTFLAGS: the per-arch linker flags .cargo/config.toml carries,
# which anything exporting RUSTFLAGS has to re-supply. Needs $XTENSA (i.e.
# call board_target first).
link_rustflags() {
  if [ "${XTENSA:-0}" = 1 ]; then
    # GNU LD from the xtensa-esp-elf toolchain
    LINK_RUSTFLAGS="-C link-arg=-Wl,-Tlinkall.x -C link-arg=-nostartfiles"
  else
    LINK_RUSTFLAGS="-C link-arg=-Tlinkall.x -C force-frame-pointers"
  fi
}

# Echoes the --remap-path-prefix flags for whatever build environment we are
# in. Longest/most specific prefix first — rustc applies the first match.
#   devshell:  $CARGO_HOME/registry/src/<index-hash>/, .../git/checkouts/<pkg>/<rev>/,
#              the repo root, and (for -Zbuild-std) the toolchain's rust-src
#   nix build: $NIX_BUILD_TOP/cargo-vendor-dir/ and $NIX_BUILD_TOP/source/
# Callers append it to $LINK_RUSTFLAGS. Must be run with the firmware crate
# as cwd (the repo root is derived from it), and after $RUSTC is exported on
# Xtensa, so the rust-src root is the one actually compiled against.
remap_rustflags() {
  local out="" d ch sysroot rustc_hash
  ch="${CARGO_HOME:-$HOME/.cargo}"
  # nix sandbox (flake builds): vendored deps and the copied-in repo source
  if [ -n "${NIX_BUILD_TOP:-}" ]; then
    out="$out --remap-path-prefix=$NIX_BUILD_TOP/cargo-vendor-dir/="
    out="$out --remap-path-prefix=$NIX_BUILD_TOP/source/="
  fi
  # crates.io registry: one <index-hash> dir in practice, glob anyway
  for d in "$ch"/registry/src/*/; do
    [ -d "$d" ] && out="$out --remap-path-prefix=$d="
  done
  # git deps (the esp-hal stack is pinned to a rev): <pkg>-<hash>/<rev>/
  for d in "$ch"/git/checkouts/*/*/; do
    [ -d "$d" ] && out="$out --remap-path-prefix=$d="
  done
  # -Zbuild-std compiles core/alloc out of the toolchain's rust-src
  sysroot=$("${RUSTC:-rustc}" --print sysroot 2>/dev/null || true)
  [ -n "$sysroot" ] && out="$out --remap-path-prefix=$sysroot/lib/rustlib/src/rust/="
  # (The dozen `/rustc/<commit-hash>/library/core/…` Locations that survive on
  # the RISC-V boards — 576 B — come from the PREBUILT core, whose paths rustc
  # already virtualized upstream. `--remap-path-prefix` matches the real local
  # path, so a second remap of the virtual name is a no-op: measured, zero
  # bytes. Only -Zbuild-std (the Xtensa boards) can reach them, via the
  # rust-src rule above.)
  # our own workspace (crates/luxel-core is a path dep of the firmware crate,
  # so cargo hands rustc an absolute path for it)
  d=$(cd .. && pwd) && out="$out --remap-path-prefix=$d/="
  printf '%s' "${out# }"
}
