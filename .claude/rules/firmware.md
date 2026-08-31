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
- `cargo clippy` DOES work on the Xtensa (`-Zbuild-std`) boards — but only
  with the esp toolchain's `bin/` PREPENDED to `PATH`. Exporting only
  `RUSTC`/`RUSTDOC` (the `tools/stack-check.sh` recipe — sufficient for
  builds) makes cargo pick mainline `clippy-driver` off PATH, which dies
  compiling the forked `core` with `unrecognized intrinsic` errors that
  look like a broken toolchain (rediscovered 2026-08-22). And run clippy
  per board feature set, not just the default: a `large_stack_arrays`
  error in hub75.rs was invisible except under `board-s3-devkit,hub75`.
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
- Never take the flash driver out of the global (`ota::take_flash`) for a
  long burst of ops — every `with_flash` user reads busy for the whole
  window, and the failure shows up as UNRELATED symptoms (asset pushes
  "flash write failed", `/api/ota` "update already in progress", served
  assets truncating — all three were one absent driver, 2026-08-15).
  Multi-page writers borrow per op via `with_flash` with yields between ops
  (see `patterns::write_raw`, the OTA/assets writers); reserve `take_flash`
  for sequential-storage transactions that genuinely need exclusive
  multi-op ownership, and keep those short.
- The app must fit in a 1 MiB OTA slot; `firmware/Cargo.toml` sets
  `opt-level = "s"` to stay under it (see docs/boards.md for the ceiling
  history). The canonical size measure is the CREDLESS flake build
  (`nix build .#luxel-fw-<board>` — what release CI gates); a creds-baked
  devshell build reads ~1.5 KB larger, not hugely different (AP-mode
  provisioning keeps the WiFi stack linked either way — the old warning
  that credless builds dead-code-eliminate WiFi stopped being true when
  provisioning landed). Just never compare a credless number against a
  creds-baked one.
- JSON/response bodies are built with `luxel_core::jsonview`'s push
  helpers (`push_piece`, `push_u32/i32/u64/i64`, `push_hex`,
  `Fx::dec_str`), NOT `format!` — and literal appends go through
  `push_piece`, not bare `push_str`. Two measured reasons (#168,
  docs/size-report.md): every `format!` site carries its own Arguments
  plumbing, and inlined `push_str` costs MORE image than the fmt it
  replaces (a naive conversion grew the C6 image 8.6 KB). `format!` on an
  error type that only implements `Display` is fine — `core::fmt` stays
  linked via `println!`/`Debug` regardless.
- Cross-origin non-simple methods (DELETE) need an explicit `OPTIONS`
  preflight response with CORS headers in `firmware/src/server.rs`'s
  dispatcher — GET/simple-POST traffic never exercises this path, so a
  missing preflight handler only shows up as a browser-side CORS failure.
- Every HTTP response in `firmware/src/server.rs` goes out as the ONE
  `Reply` type (status + `heapless::Vec<(&'static str, HVal)>` + `ApiBody`).
  Never return a picoserve response TUPLE (`(CORS, JSON, body)`,
  `(StatusCode, [hdr; N], "…")`) from a new handler, never add a second
  header-value type beside `HVal`, and never hand a header a `V: Display`
  that isn't `HVal`: picoserve monomorphizes `IntoResponse::write_to` per
  tuple shape and `ForEachHeader::call` per value type, so each one is a
  fresh multi-KB copy of the whole response path. Collapsing 13 shapes into
  `Reply` was worth −24 KB of image (#167, docs/size-report.md); one new
  tuple silently gives a chunk of it back. Same reason the dispatcher stays
  a hand-written flat-match `PathRouterService`: picoserve's `MethodRouter`
  wraps the writer in a private `IgnoreBody<W>` for HEAD — a second writer
  type that duplicates every GET instantiation. A new body kind is a new
  `ApiBody` variant; a runtime header value is `HVal::Owned`.
