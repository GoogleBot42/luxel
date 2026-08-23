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
