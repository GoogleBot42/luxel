#!/usr/bin/env bash
# NixOS-only: make espup's prebuilt Xtensa toolchain runnable. espup installs
# dynamically-linked binaries expecting /lib64/ld-linux — absent on NixOS.
# Run once after `espup install --targets esp32`, from the devshell:
#   ./patch-esp-toolchain.sh
set -euo pipefail

INTERP=$(nix eval --raw nixpkgs#stdenv.cc.bintools.dynamicLinker)
ZLIB=$(nix build --no-link --print-out-paths nixpkgs#zlib)
GCCLIB=$(nix eval --raw nixpkgs#stdenv.cc.cc.lib.outPath)
PATCHELF=$(nix build --no-link --print-out-paths nixpkgs#patchelf)/bin/patchelf

TC="$HOME/.rustup/toolchains/esp"
RPATH_TOOLS="\$ORIGIN/../lib:$ZLIB/lib:$GCCLIB/lib"
RPATH_PLAIN="$ZLIB/lib:$GCCLIB/lib"

patch_tree() {
  local dir=$1 rpath=$2
  find "$dir" -type f 2>/dev/null | while read -r f; do
    head -c4 "$f" 2>/dev/null | grep -q $'\x7fELF' || continue
    case "$f" in *.rlib | *.o | *.a) continue ;; esac
    "$PATCHELF" --set-interpreter "$INTERP" "$f" 2>/dev/null || true
    "$PATCHELF" --set-rpath "$rpath" "$f" 2>/dev/null || true
  done
}

echo "patching rustc/cargo…"
patch_tree "$TC/bin" "$RPATH_TOOLS"
patch_tree "$TC/libexec" "$RPATH_TOOLS"
echo "patching shared libraries…"
patch_tree "$TC/lib" "$RPATH_TOOLS"
echo "patching rustlib host tools (rust-lld, gcc-ld)…"
patch_tree "$TC/lib/rustlib" "$RPATH_TOOLS"
echo "patching xtensa-esp-elf GNU toolchain…"
for gt in "$TC"/xtensa-esp-elf/*/xtensa-esp-elf; do
  patch_tree "$gt/bin" "$RPATH_PLAIN"
  patch_tree "$gt/libexec" "$RPATH_PLAIN"
done

"$TC/bin/rustc" -vV | head -1
echo "ok"
