# Local patches to third-party crates

Carried as patch files (Jeremy's preference over vendoring whole trees).
The patched source never lives in git: `firmware/vendor/<crate>` is
gitignored and materialized from a Nix derivation in `flake.nix` — the
devshell's `shellHook` symlinks it into the tree for `cargo` builds (so
it appears on first `nix develop` in any checkout/worktree), and
`mkFirmware` copies it in during hermetic builds. `Cargo.toml`'s
`[patch.crates-io]` points at that path.

To change a patch: edit the materialized copy is NOT the way (it's a
read-only store path) — regenerate the `.patch` against a pristine crate
unpack, then re-enter the devshell.

Adding a patch on top of the stack (the recipe used for `plane-repeats`,
2026-09-26; no network needed — the pristine tarball is already in the
store as `/nix/store/*-esp-hub75-0.14.0.tar.gz`):

```sh
mkdir base new && tar xzf /nix/store/*-esp-hub75-0.14.0.tar.gz -C base --strip-components=1
cd base && for p in esp-hal-git atomic-swap dma-position; do   # the stack, in order
  patch -p1 -s < $REPO/firmware/patches/esp-hub75-0.14.0-$p.patch; done; cd ..
cp -r base new     # edit new/src/... ; then:
{ echo "<header paragraph: what and why>"; echo;
  diff -ruN base new | sed -e 's|^--- base/|--- a/|' -e 's|^+++ new/|+++ b/|'; } \
  > $REPO/firmware/patches/esp-hub75-0.14.0-<name>.patch
```

Then list it in `flake.nix` (`mkEspHub75Src.patches`, in order), add a
section below, `git add` the file (the devshell will not enter otherwise —
see .claude/skills/worktree-setup), and prove it re-applies: unpack a
fresh copy, apply all patches in order, `diff -r` against `new` — must be
empty. `git diff --check` flags the blank context lines (` `) every
unified diff has; ignore that, the older patches have them too.

## esp-hub75-0.14.0-esp-hal-git.patch

Upstream (https://github.com/liebman/esp-hub75, MIT OR Apache-2.0)
targets crates.io `esp-hal 1.1.0`, but this firmware pins the whole
esp-hal stack to git rev `7c7f3726` (see `firmware/Cargo.toml`
`[patch.crates-io]` — the classic-ESP32 PHY-calibration fix), and the
esp-hal API drifted after the 1.1.0 release. Two mechanical fixes, no
behavior change:

- `src/lcd_cam.rs`: `dma::TxChannelFor<LCD_CAM>` was replaced by
  `lcd_cam::LcdDmaTxChannel` (the I8080 driver erases the channel itself
  now); swap the import and four constructor trait bounds.
- `src/bcm/mod.rs`: `Preparation.direction` was removed — direction is
  implied by the `DmaTxBuffer` trait; drop the assignment.

Drop the patch (and the flake materialization) once an esp-hub75 release
supports the esp-hal API at or past our pinned rev.

## esp-hub75-0.14.0-atomic-swap.patch

Applies on top of the esp-hal-git patch. Makes the circular-DMA framebuffer
swap frame-atomic (Gitea #376): two descriptor rings, one per framebuffer,
and the swap is a single aligned store rewriting the running ring's tail
`next` to the other ring's head — which the DMA reads only when it wraps, so
the switch lands exactly on a panel frame boundary. Upstream rewrites every
descriptor's `buffer` pointer mid-pass instead and its own SAFETY note admits
the result: one partially-mixed frame per swap (filmed on the bench panel
2026-09-07). Cost is one extra ring in `.bss` — 3048 B on the 64x64/7-plane
bench panel. The patch header has the full mechanism, including how the
frame-count ISR observes the flip landing (GDMA `OUT_DSCR`) and the
single-ring fallback that keeps upstream behaviour for callers who allocate
their own descriptors. Offer it upstream.

## esp-hub75-0.14.0-dma-position.patch

Applies on top of the atomic-swap patch. Two read-only accessors on `Hub75`
for the spare-plane swap (Gitea #610, `firmware/src/hub75.rs`
`hub75-spare-plane`): `dma_position()` — which ring and which descriptor the
engine is on right now, plus whether an `out_eof` is pending, from the same
GDMA `OUT_DSCR` probe the atomic swap already registers — and
`last_eof_us()`, the frame-count ISR's timestamp of the current pass's
start. No behaviour change; nothing in the ISR moves. The firmware uses the
position to decide whether the MSB run of the current pass still has room
for the plane copy, and to count a copy that overran it.

## esp-hub75-0.14.0-plane-repeats.patch

Applies on top of the dma-position patch. Lets the firmware install a
per-plane REPEAT SCHEDULE for the BCM descriptor chain (Gitea #460 / #789,
`luxel_hub75::Schedule`): `set_plane_repeats(&[u8])` before `Hub75::new`
(0 = the stock `2^(planes-1-idx)`), read by `fill_full_chain` for every
ring it builds, with `dma_descriptor_count_scheduled` / `max_dma_chunk_size`
for sizing. This is what turns a truncated-OE low plane into a single
emission instead of `2^k` of them — the refresh half of the brighter ↔
faster trade; the OE half is in the framebuffer words
(`luxel_hub75::format_scheduled`). The counts must not change while a
driver is running (both rings and the ISR's pass arithmetic assume them),
so the setting is applied at boot.
