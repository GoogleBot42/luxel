---
paths:
  - "firmware/**"
---

- `#[embassy_executor::task]` functions are compiled as statics — any large
  buffer held across an `.await` in one lives for the life of the firmware and
  eats into the shared main-task stack, not a per-call frame. Put multi-KB
  buffers in a heap `Vec` instead. This class of bug bricked a device once
  (v0.1.19).
- Never call `FlashStorage::read` (esp-storage) in request/async context — it
  puts an unconditional 4 KiB sector bounce-buffer on the caller's stack. Use
  the `read_nor` path with a word-aligned offset/length/buffer instead; see
  `firmware/src/assets.rs::read_chunk` for the pattern (stage through a
  word-aligned heap buffer, then copy out the unaligned slice you actually
  want).
- After touching large statics/buffers, run `tools/stack-check.sh` — it
  measures every linked function's frame (not just your own source) and
  enforces a total `.stack` floor. Measure, don't estimate: v0.1.31-33
  shipped an estimated stack size that was well above the real, measured one,
  and it panicked in production.
- Run `cargo clippy` against the default (esp32c3) target, not the Xtensa
  build — clippy cannot drive the forked-core Xtensa `-Zbuild-std` target.
- Boot tasks that do multi-KB loads (e.g. playlist/pattern resume) must run
  after `stack.wait_config_up().await` — WiFi bring-up mallocs don't
  null-check, so a heavy load racing WiFi init OOM-panics the boot.
- Never size an infallible allocation from a length/count field read out of
  flash or any stored record — a corrupt record becomes an OOM panic-reboot
  loop (a torn pattern-store TOC record with chunk-count 32 crash-rebooted
  the Athom on EVERY /api/patterns read until serial found it, 2026-08-15).
  Validate the count against the writer's own cap and `try_reserve`; see
  `patterns::read_source`. This is the same class v0.1.25's "fallible
  everything" sweep fixed elsewhere — check for it in any new read path.
- The app must fit in a 1 MiB OTA slot; `firmware/Cargo.toml` sets
  `opt-level = "s"` to stay under it (see docs/boards.md for the ceiling
  history). Size-check WITH WiFi creds baked in — a credless build
  dead-code-eliminates the WiFi stack and hides size regressions.
- Cross-origin non-simple methods (DELETE) need an explicit `OPTIONS`
  preflight response with CORS headers in `firmware/src/server.rs`'s
  dispatcher — GET/simple-POST traffic never exercises this path, so a
  missing preflight handler only shows up as a browser-side CORS failure.
