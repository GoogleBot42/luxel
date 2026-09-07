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
- Every flash read in main.rs (`config::read_device`, `read_wifi`,
  `assets::read_chunk`, the pattern store) goes through `ota::with_flash`,
  which is `None` until `ota::init(flash)` runs — before that a read answers
  "no record" SILENTLY (no panic, no log), so a setting read too early boots
  at its default with no clue why. Keep boot-time settings reads after
  `ota::init` (2026-09-02: the data-pin picker "applied" but never took).
- Flash-mapped regions (`firmware/src/flashmap.rs`, the assets partition
  today, the VM's code stream next — docs/firmware.md "Flash-mapped
  regions"): read them from task context only, never from an ISR; after
  writing flash under one, `flashmap::invalidate`/`invalidate_slice` the
  range before anything reads it back through the mapping; never map an
  app slot; never `unmap` something another task may still read (an
  invalid-entry load is a cache-error fault, not a catchable error); keep
  the read_nor fallback — `map` can fail and the consumer must degrade,
  not break. Map/unmap go through `flashmap::quiesced`, which the
  second-core branch must route through `core1::fenced`.
- Never size an infallible allocation from a length/count field read out of
  flash or any stored record — a corrupt record becomes an OOM panic-reboot
  loop (a torn pattern-store TOC record with chunk-count 32 crash-rebooted
  the Athom on EVERY /api/patterns read until serial found it, 2026-08-15).
  Validate the count against the writer's own cap and `try_reserve`; see
  `patterns::read_source`. This is the same class v0.1.25's "fallible
  everything" sweep fixed elsewhere — check for it in any new read path.
- Dual-core boards (esp32, esp32s3 — cfg `multi_core` from build.rs) run
  the render task on the AppCpu (`firmware/src/core1.rs`), and every flash
  op must run inside the cross-core flash fence: the other core is parked
  in IRAM for the op, because an SPI1 flash op while the other core fetches
  instructions from flash returns garbage (erase/program) or contends for
  the bus. The fenced doors are `ota::with_flash`, the `patterns::AsyncFlash`
  adapter and `ota::begin`'s reads on the taken driver — a new flash path
  goes through one of them or wraps itself in `core1::fenced`, NEVER a
  bare op on a taken `FlashStorage`. The fence must sit OUTSIDE any
  critical section (its spin-waits need interrupts enabled so the other
  core can park us in turn). Do not switch esp-storage to
  `multicore_auto_park`: the hard park stalls the other core at an
  arbitrary instruction, possibly inside a spinlock the flash op's next
  interrupt then waits on forever. Three park rules are load-bearing and
  each was a black-boxed hard hang (docs/firmware.md "Cores & tasks"): the
  park interrupt is Priority1 (a level-3 park inside a level-1 handler's
  DPORT reads wedges the bus), the AppCpu never touches RTC memory inside
  the park, and the fence waits for the strip's SPI2 DMA transfer to end
  before the SPI1 op (`output::transfer_busy`). A new output driver on a
  dual-core chip must answer `transfer_busy()` honestly. A fourth rule
  joined them 2026-09-06 (Gitea #292): the fencing core holds its OWN
  interrupts masked from the park ack until past the release, because the
  interrupts that queue up behind a 45 ms sector erase all fire on the
  instruction where esp-storage's critical section drops back to level 0 —
  while the other core is still parked — and one of them never returns.
  `/api/status` `core1.fence_timeouts` must stay 0, and `core1.last.reset`
  == `SysRtcWdt` after a session means the watchdog caught a wedge — read
  `core1.last.bb` before anything else (`bb[1]` 6 = inside the ROM op,
  7 = it returned, `bb[8]` = which call site took the fence).
- **One fence per page, never per field.** A fence is expensive — an
  interrupt on the other core, a park round-trip, and this core's
  interrupts held off for the op — so a fenced read/write must carry a
  page's worth of work, not a header's. Anything that walks flash a few
  bytes at a time needs a cache: sequential-storage gets ONE
  `PageStateCache` shared across every call (`patterns::store_cache`) and
  the raw-region writers erase+program a whole 4 KiB page per fence. When
  this rule was broken, one 650-byte `POST /api/patterns` cost 81,143
  individually fenced reads and ~13 s, and the RTC watchdog rebooted the
  board mid-save (Gitea #292); with the shared cache it is ~930 and 0.5 s.
  Measure it: `/api/status` `core1.fences` is `[begun, completed]` for this
  boot, so a before/after delta around one operation is two curls.
- **A long flash burst must feed the RTC watchdog.** It blocks the ProCpu
  executor, and the watchdog task lives there: a 728 KB asset install is
  ~15 s of erases and a garbage-collecting pattern save was measured at
  25 s, both against a 20 s timeout. `core1::fenced` feeds every 64 fences
  — taking a fence IS proof of progress, so the watchdog keeps catching a
  core that STOPPED without punishing one that is merely slow.
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
- No 64-bit division on the per-pixel path. Xtensa has a hardware 32-bit
  divide (`quos`) but every `i64`/`u64` `/` or `%` compiles to a call into
  the mask ROM's libgcc (`__divdi3`/`__udivmoddi4`, ~100+ cycles), and the
  ELF shows no local symbol for it — count them with `objdump -t | grep
  divdi`, not from the disassembly. `Fx::div`, `time()` and the 1D pixel
  coordinate carry 32-bit fast paths with bit-exact tests (#260); keep new
  hot-path arithmetic in i32/u32 and add the same kind of test.
- **`cargo check` does not prove the firmware BUILDS.** It stops before
  codegen, and the Xtensa LLVM fork's instruction selection is where the
  interesting failures are — a RISC-V board (`--target
  riscv32imac-unknown-none-elf`) is a fast syntax/type check and nothing
  more. Before claiming a firmware change compiles, run a real Xtensa
  build: `nix develop --command bash -c 'cd <worktree> &&
  BOARD=board-athom-music SKIP_ASSETS=1 ./firmware/build-esp32.sh'` from
  the worktree ROOT (~1 min against a warm target dir).
- **`rustc-LLVM ERROR: Cannot select: i32 = Constant<N>` is a backend bug,
  not your bug.** The Xtensa fork (xtensa-rust-1.95.0.0) can fail ISel on
  an ordinary integer literal once the function around it — typically an
  embassy task's `poll` — gets complex enough. `N` names the literal in the
  source (change `24 * 1024` to `23 * 1024` and it fails as
  `Constant<23552>`), which is how you find it: grep for the number.
  `#[inline(never)]` alone does NOT help — fat LTO folds the body back in.
  The fix that works is making the constant opaque with
  `core::hint::black_box`, in a small `#[inline(never)]` helper, commented
  as the toolchain workaround it is (2026-09-06, #330: resume.rs'
  `resume_headroom`). Unrelated code elsewhere can trigger it, so a build
  that breaks in a file you did not touch is expected.
- **A codegen experiment cannot go through plain `RUSTFLAGS`**: the flags in
  `firmware/.cargo/config.toml`'s `[target.'cfg(target_arch = "xtensa")']`
  are NOT replaced by the environment here — set
  `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` with the link args repeated and the
  linker dies on `linker script file 'linkall.x' appears multiple times`
  (2026-09-06, #312). Pass ONLY the extra flag
  (`CARGO_TARGET_XTENSA_ESP32S3_NONE_ELF_RUSTFLAGS="-C llvm-args=…"`) and
  let the config supply the link args. Cargo fingerprints per flag set, so
  switching back and forth is cached, not rebuilt — which also means a
  suspiciously fast "Finished in 0.1s" after changing flags is correct, not
  a stale artifact. Verify what you are about to flash from the ELF
  (`nm --print-size`, `objdump`), never from the build log.
- **Placing a function in IRAM from a chip-agnostic crate**: `esp_hal::ram`
  expands to `#[link_section = ".rwtext"]`, so a crate that must not depend
  on esp-hal (luxel-core) can spell it by hand behind a cargo feature —
  that is what `iram-vm` / `iram-builtins` are (#312). Feature-gate it: on a
  host target `.rwtext` is a stray section name. Measure before shipping
  one; `Vm::run` in IRAM bought 1.0 % for 16 KB on the S3.
