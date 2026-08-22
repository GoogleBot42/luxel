# Board → chip / rust target / toolchain map. Sourced (not executed) by
# firmware/build-esp32.sh and tools/stack-check.sh so the two never drift.
# (Part of the firmware crate: GPL-3.0-or-later, see firmware/LICENSE.)
#
# Sets, from $BOARD:
#   CHIP        espflash --chip value
#   TARGET      rustc target triple
#   XTENSA      1 → Espressif's rustc fork + -Zbuild-std (no prebuilt core);
#               0 → mainline Rust with the target installed via rustup/nix
#
# Adding a board? Add its case here as well as the three files in
# docs/boards.md ("Adding a board").
board_target() {
  case "$1" in
    board-pixelblaze-v3|board-athom-music|board-esp32-generic)
      CHIP=esp32;    TARGET=xtensa-esp32-none-elf;      XTENSA=1 ;;
    board-s3-devkit)
      CHIP=esp32s3;  TARGET=xtensa-esp32s3-none-elf;    XTENSA=1 ;;
    board-c3-devkit)
      CHIP=esp32c3;  TARGET=riscv32imc-unknown-none-elf;  XTENSA=0 ;;
    board-c6-devkit)
      CHIP=esp32c6;  TARGET=riscv32imac-unknown-none-elf; XTENSA=0 ;;
    *)
      echo "unknown BOARD '$1' — see docs/boards.md" >&2; return 1 ;;
  esac
}
