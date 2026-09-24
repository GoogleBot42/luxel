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
  enforces a total `.stack` floor. **"Firmware change" includes a
  `luxel-core` change**: `.stack` is leftover DRAM, so a `static` added in a
  crate the firmware links comes straight out of the main task's stack. A
  544 B slot table in `text.rs` — eight 64-byte strings, nothing by any
  normal standard — took `board-pixelblaze-v3` from 24,708 B to 24,164 B and
  failed the 24,576 B floor (2026-09-24, #484). The fix was to allocate the
  table on first use instead; prefer that to shrinking a board's heap
  whenever the data is optional. **Adding an HTTP ROUTE costs `.stack` too**:
  the web task's future is a `.bss` static replicated
  `server::WEB_TASK_POOL_SIZE` times and sized by the largest arm of the route
  table, so four new arms grew `web_task::POOL` by 1,224 B on pixelblaze-v3
  (2026-09-24, #478). Factoring an arm into its own `async fn` does NOT help
  (measured: 0 bytes) — the only levers are `STATICS_RESERVE` and the
  per-chip `heap_allocator!` in `main.rs`. Measure, don't estimate: v0.1.31-33
  shipped an estimated stack size that was well above the real, measured one,
  and it panicked in production.
  **Measure the BASELINE too, and on the board you are shipping to.** On the
  classic ESP32 `.stack` is leftover DRAM and the floor is currently within
  TENS of bytes, and it DRIFTS: 2026-09-19 morning `board-pixelblaze-v3`
  cleared the 24,576 B floor by 4 B; by that evening master had it 116 B
  UNDER (24,460 B), and `board-athom-music` has been under before too.
  **Nothing catches that: `tools/ci.sh` does NOT run stack-check at all**
  (grep it — there is no call; an earlier version of this rule said it
  checked pixelblaze-v3, and that was wrong). So a red stack-check after your
  change is very possibly not your
  change: `git stash`, re-measure, compare. When it IS yours, the fix is the
  one the script names — take the bytes out of that board's
  `heap_allocator!` (see `SECOND_OUTPUT_RAM` in main.rs for the per-board
  idiom), not off the floor. Gitea #515.
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
- **Nothing on the RENDER path may allocate infallibly — including code you
  did not write.** `luxel-core` is `no_std` + `alloc` and uses plain
  `Vec::resize`/`extend_from_slice`, which panic (i.e. reboot the device)
  when the heap is short. When the firmware drives a core structure that
  allocates lazily, budget it in `luxel_core::budget` BEFORE the frame that
  makes the allocation, the way `scenes::build_runtime` reserves
  `budget::compositor_scratch` before it builds any engine. The compositor's
  12 KB text scratch took the Seengreat panel down exactly this way on
  2026-09-24 — `memory allocation of 2688 bytes failed`, one frame after the
  engine AND its JIT compile had both been accepted with heap to spare
  (Gitea #702). The post-build `RUNTIME_FLOOR` check cannot see it.
- **A response body sized by the pixel count is a big allocation on a small
  heap.** At 4096 px a frame is 12 KB, and a heavy pattern can leave under
  30 KB free — so `GET /api/pixels` must be ONE *fallible* allocation, and it
  must be reserved OUTSIDE any critical section (allocating with interrupts
  masked stalls both cores on the allocator's own lock). Two allocations for
  one response — build then flatten — panicked the panel with `memory
  allocation of 12288 bytes failed` and the boot guard rolled the slot back
  (#306). Degrade to an empty/short body; never let a routine GET be the thing
  that reboots the board.
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
  the park, and the fence waits for an in-flight output DMA transfer to end
  before the SPI1 op (`output::transfer_busy`), on WHICHEVER core it parks —
  which core runs the driver is a build-time question since #306 put the
  HUB75 compose on the ProCpu. A new output driver on a dual-core chip must
  answer `transfer_busy()` honestly. A fourth rule
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
- **The render loop's `core1::beat()` at the top of `loop {}` is
  load-bearing on the dual-core boards, and it is a TRIPWIRE.** It is the
  only thing that tells the ProCpu the AppCpu is alive; if the loop stops
  iterating for 10 s of ProCpu-awake time, `core1::watchdog_task` stops
  feeding the RWDT and the board reboots (Gitea #603, `firmware/src/appwdt.rs`,
  docs/firmware.md). So: never move the stamp below a `continue`, never
  make it conditional on an engine/frame being present (a rejected pattern
  renders nothing and must still count as alive), and treat any new `await`
  in the render loop that can block for many seconds *while the ProCpu
  executor keeps running* as a reboot you just shipped. Blocking that also
  blocks the ProCpu is already forgiven — the gate credits the watchdog
  task's own lateness — but nothing else is. Changing either constant means
  re-running `cargo test -p appwdt-check`.
- Never take the flash driver out of the global (`ota::take_flash`) for a
  long burst of ops — every `with_flash` user reads busy for the whole
  window, and the failure shows up as UNRELATED symptoms (asset pushes
  "flash write failed", `/api/ota` "update already in progress", served
  assets truncating — all three were one absent driver, 2026-08-15).
  Multi-page writers borrow per op via `with_flash` with yields between ops
  (see `patterns::write_raw`, the OTA/assets writers); reserve `take_flash`
  for sequential-storage transactions that genuinely need exclusive
  multi-op ownership, and keep those short.
- **Never write a partition offset down.** Since #501 an image can meet
  three layouts — its own, the other flash size's, and the pre-#501 one a
  field device still carries — so a literal offset is wrong on two of them,
  in the way that erases user data rather than the way that fails to
  compile. Read the table: `ota::data_partition("storage")`,
  `parttab::data_labelled(table, "assets")`,
  `parttab::app_slot(table, SUBTYPE_OTA1)`; a shell script parses the csv
  the board selected (`$PARTITIONS`, `board_partitions` in
  `firmware/board-target.sh`). `tools/offset-check.py` runs in `tools/ci.sh`
  and fails the gate on a literal in `firmware/src/**` or a flashing script
  — comments, docs and `tools/qemu/` are exempt. The same rule is why the
  pattern store's geometry is resolved at boot: the key area is a fixed
  128 KiB, the log starts at `LOG_OFF` on every layout, and only the log's
  LENGTH comes from the partition (`patterns::log_len`).
- The app must fit its board's OTA slot; `firmware/Cargo.toml` sets
  `opt-level = "s"` to stay under it (see docs/boards.md for the ceiling
  history). **The slot is per board since #501** — 1,310,720 B on the 4 MB
  boards, 3,145,728 B on `board-seengreat-hub75`, from `board_ota_max` in
  `firmware/board-target.sh`; a margin percentage means nothing until you
  say which slot it is a fraction of, and `tools/image-check.sh` prints the
  rule it used on every size line. `board-seengreat-hub75` has TWO tiers
  (#634): a 16 MB device whose *bootloader* was flashed for a smaller part
  migrates to the 4 MB fallback layout, where its live `ota_0` is
  1,310,720 B, not the 3,145,728 B the gate uses — the gate stays on the
  nominal slot because the image is a release artifact and the tier is a
  property of the device, and `/api/ota` sizes every push against the table
  on flash. `MIGRATING_RELEASE=1` weighs every image against the OLD
  1,048,576 B slot at a 0 % floor instead, because a device that has not
  repartitioned is what installs a migrating release; it is OPT-IN since
  #635 — `tools/ci.sh` defaults it to 0, and only
  `.github/workflows/release.yml` still pins it (docs/releases.md).
  The canonical size measure is the CREDLESS flake build
  (`nix build .#luxel-fw-<board>` — what release CI gates); a creds-baked
  devshell build reads ~1.5 KB larger, not hugely different (AP-mode
  provisioning keeps the WiFi stack linked either way — the old warning
  that credless builds dead-code-eliminate WiFi stopped being true when
  provisioning landed). Just never compare a credless number against a
  creds-baked one; never compare one measured BEFORE a rebase against one
  measured after (master moves — a JIT-phase-0 merge landed mid-#501 and
  shifted every board by ±100 B, so both columns of a before/after table
  have to be re-taken against the master you actually sit on); and never
  compare a number measured on one HOST against one from another: every build embeds its absolute dependency
  source paths in panic `Location`s (~13.5 KB, #441), so the same commit
  weighed 1,010,432 B on the CI runner and 1,014,400 B locally
  (2026-09-08). Gate a percentage floor only on the flake artifact of the
  machine you are quoting. **That host gap is now closed** — #441's
  `--remap-path-prefix` set did it: on 2026-09-19 master's
  `c6-devkit-hosted` flake image measured 1,015,440 B locally and
  1,015,440 B in the runner's own `image-check` line (run 1858), byte for
  byte. So you CAN pre-check a margin locally before pushing, and you
  should: read the last green master run's `image-check: size ok` line out
  of the Gitea job log and compare it with your own `nix build` of the same
  commit before trusting either. `tools/ci.sh` now image-checks the release
  images for `CI_VARIANTS` (pixelblaze-v3, c6-devkit-hosted, c3-devkit)
  because a C3 build break (#413) and a C6 under-floor image (#438) both
  merged green while only `CI_BOARD` was built.
- **Take the BEFORE numbers from a separate detached worktree at
  `origin/master`, never from your own tree before you start editing.**
  `flake.nix` uses `src = lib.cleanSource ./.`, so the derivation sees the
  whole repo: an edit ANYWHERE — a doc, a comment — invalidates every
  variant, and a multi-variant sweep in your own worktree silently mixes
  pre- and post-edit images as you work (measured the same variant three
  ways in one session, 2026-09-20). Same source rule bites new files: a
  `nix build` cannot see an untracked one, so `git add` a new module before
  building or the build fails with "file not found for module" pointing at
  a file that is right there.
- **`.rodata` is NOT free — it costs image byte for byte.** A 16 KiB live
  `#[used]` array in `.rodata` grew the app image by exactly 16,384 B on
  BOTH `board-pixelblaze-v3` (1,011,392 → 1,027,776) and `board-c6-devkit`
  + `hosted-ui` (1,012,048 → 1,028,432) — measured 2026-09-19, #501. A
  sub-KB table can still land inside whatever segment-alignment slack
  happens to exist at that moment (#465 put 1,640 B of rodata in for 0 B of
  image, which is where the "rodata is free" idea came from), but that
  window is a one-off of unknown size, not a property to plan around.
  **Never trade code for tables on the assumption that the tables are
  free** — measure the image, not the section. Same lesson as #473's
  `match` → `const` table, which cost +496 B.
- **`/api/ota`'s target slot is where the running image is NOT MMU-mapped
  (`parttab::ota_target`, fed by `booted_partition()`); `otadata` is never
  an input.** Do not reintroduce esp-bootloader-esp-idf's
  `next_partition()` / `activate_next_partition()`: with `otadata` erased —
  the state every migrating boot sits in between `settle_into_ota0` and
  its next reboot — they answer the RUNNING slot (a `u8` underflow in the
  booted-slot guard), and that erased the Seengreat's live image under it
  (#655, 2026-09-21). Activation names the written slot explicitly and
  reads it back; the image's first sector is landed only after
  `appimg::verify` passes on the rest. `ota: updates go to …` on every boot
  log must never equal `/api/status.slot` — the QEMU migrate/takeover
  tests assert exactly that pair.
- **A new `mod` line in `main.rs`: look at the line ABOVE it.** The module
  list carries per-board `#[cfg(...)]` attributes (`multi_core`, `hub75`,
  `wled-takeover`) that apply to the NEXT `mod`; inserting above one of
  them silently moves the gate onto your module. Xtensa builds fine and
  the RISC-V flake variants fail with "cannot find `x` in `crate`"
  (2026-09-23, `appimg` landed under `#[cfg(multi_core)]`). Build
  `board-c3-devkit` or `nix build .#luxel-fw-c6-devkit-hosted` before
  trusting a new module, same as for a new atomic.
- **`riscv32imc` (board-c3-devkit) has no atomic read-modify-write.**
  `AtomicU32::swap` / `fetch_add` / `compare_exchange` are a HARD COMPILE
  ERROR there and nowhere else, so a static that compiles on every other
  board can still red-light CI (#413, and again in #501 where a
  region-tracking `swap` had to become load-then-store under the flash
  lease). Load/store are fine. Build board-c3-devkit before trusting a new
  atomic.
- Size gotchas measured on riscv32imc at `opt-level = "s"` (#465): `str::parse`
  instantiates ~700 B of `from_str_radix` **per integer width**, so a parser
  wanting u8/u16/u32 pays three times — hand-roll one `fn(&str) -> Option<u32>`
  and narrow with `try_from`. **This is the cheapest kilobyte in the
  firmware and it is still lying around**: 2026-09-19 `server.rs` alone was
  parsing FIVE widths (`u8` ×3, `u16`, `u32`, `i16`, `i32`); routing the
  unsigned ones through one `num()` and reading the timezone as the already-
  instantiated `i32` was **−1,088 B** on `c6-devkit-hosted` — more than the
  whole of `/api/name`, and what made #538 fit (#543). Two caveats: a width
  another module still uses stays linked (`u8`/`u16` via `devicemap.rs` and
  `outpipe.rs`), so the saving is well under "widths removed × 700 B";
  and a hand-rolled parser drops `parse`'s leading `+`, which is a wire
  behaviour change worth a doc line. Persisting a struct as the wire format it
  already parses beats a binary record by a serializer + a deserializer
  (−1.9 KB; see `PLAYLIST_KEY`/`LAYOUT_KEY`). And `slice::sort_by_key`
  instantiates driftsort, a **4,144 B stack frame** that `tools/stack-check.sh`
  will show you — for a list of a handful, insert in order instead.
- **A size "optimisation" is a hypothesis until you have built the gated
  board.** On riscv32imc the codegen's response to a small shape change is
  routinely larger than the change: 2026-09-19 (#538) holding two ≤32-byte
  device-name cells as `heapless::String<32>` instead of `String` cost
  **+2,032 B** on `c6-devkit-hosted` while saving 112 B on the Xtensa
  boards, and moving one `&'static str` into a `StaticCell` cost another
  +256 B. Below ~500 B, differences between two shapes are noise you cannot
  reason about — go find a structural saving (a dropped monomorphisation)
  instead of shaving. Record the failed experiments in docs/boards.md so the
  next person does not repeat them. The same applies in reverse when you ADD
  a comparison or a small routine: reusing an already-linked generic is NOT
  automatically the cheap option. #550 measured four shapes of one predicate
  — `Option` tuples per index, a packed `u32`, normalising into `Vec<Output>`
  to reuse the derived `PartialEq`, and an allocation-free in-place `zip` —
  at +400/+448/+272/**+80** B on `c6-devkit-hosted`. The `Vec` one, which
  wrote the least new source, was among the worst; the winner allocates and
  instantiates nothing. Write the three candidates, build the gated board
  three times, keep the number.
- Diffing symbol tables between two builds: strip the `17h<hash>E` mangling
  hash and rustc's `.NNNN` local suffix first. A raw `nm` diff shows a
  renumbered symbol as one that vanished plus one that appeared, and that
  invented #438's entire "1.26 KB of Debug tables from #424" premise — the
  tables were in the pre-#424 image byte for byte (2026-09-08).
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
- The same tax applies to the routes that answer and THEN reboot
  (`/api/apmode`, `/api/reboot`): they bypass the dispatcher's shared exit to
  write their response before signalling `REBOOT`, so each such arm is its own
  `finalize().await? + write_to` instantiation — a whole extra copy of
  picoserve's response path. Measured 2026-09-19: giving `/api/reboot` its own
  arm cost 624–704 B on the strip boards; folding it into `/api/apmode`'s arm
  (`r @ ("/api/apmode" | "/api/reboot")`, branch inside) cost 144–384 B. Any
  future "reply then reboot" route joins that arm.
- A field added to a struct SHARED with the mirror (`luxel_core::layout::View`
  and friends) is not free on boards that always pass `None`: the writer takes
  a `&View`, so the compiler cannot fold the branch away. `View::panel`
  measured 96–112 B on every strip board before it went behind a
  `luxel-core/panel` feature that only the firmware's `hub75` enables. Gate
  host-specific fields with a cargo feature, and prove it with flake builds of
  `athom-music`, `c6-devkit-hosted` and `pixelblaze-v3` before and after —
  those three are the tightest slots (Gitea #501/#513).
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
- **A codegen experiment goes through `EXTRA_RUSTFLAGS`, not `RUSTFLAGS` and
  no longer `CARGO_TARGET_<TRIPLE>_RUSTFLAGS`.** Since #441,
  `firmware/build-esp32.sh` and `tools/stack-check.sh` export `RUSTFLAGS`
  themselves (per-arch link args from `link_rustflags` plus the
  `--remap-path-prefix` set from `remap_rustflags`, both in
  `firmware/board-target.sh`), and `RUSTFLAGS` outranks BOTH
  `.cargo/config.toml`'s `[target.*] rustflags` and
  `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` — so the per-target env var is now
  silently IGNORED by those two scripts. `EXTRA_RUSTFLAGS="-C llvm-args=…"`
  is appended to what they compute and is the supported knob. Setting plain
  `RUSTFLAGS` yourself drops the link args and the remaps: the old symptom
  was the linker dying on `linker script file 'linkall.x' appears multiple
  times` (2026-09-06, #312) when the args were repeated on top of the config;
  the new one is a ~9 KB heavier image with absolute paths back in it.
  Cargo fingerprints per flag set, so
  switching back and forth is cached, not rebuilt — which also means a
  suspiciously fast "Finished in 0.1s" after changing flags is correct, not
  a stale artifact.
- **Both arches build `core`/`alloc` from source** —
  `-Zbuild-std=core,alloc -Zbuild-std-features=optimize_for_size`, in
  `firmware/build-esp32.sh`, `tools/stack-check.sh` and `flake.nix`
  alike (#501). On Xtensa that was already forced (no prebuilt core for
  the fork) and the size feature is the new half, −4,368 B on
  `board-pixelblaze-v3`; on RISC-V both halves are new — −6,992 B for
  build-std (a from-source core joins the binary's fat LTO instead of
  arriving prebuilt at opt-level 3) and −5,952 B for `optimize_for_size`,
  on `board-c6-devkit` + `hosted-ui`. It needs `RUSTC_BOOTSTRAP=1` on
  mainline stable and a toolchain carrying `rust-src`, plus a pinned copy
  of that toolchain's `library/Cargo.lock` for the flake's offline vendor
  dir — one per arch (`firmware/rust-std.Cargo.lock`,
  `firmware/rust-std-riscv.Cargo.lock`), **re-copy the matching one on a
  toolchain bump** or the sandboxed build fails resolving the std
  workspace. Per-package `opt-level = "z"` is the trap in the same area:
  measured on eight dependency crates it made the image 11,504 B BIGGER. Verify what you are about to flash from the ELF
  (`nm --print-size`, `objdump`), never from the build log.
- **Placing a function in IRAM from a chip-agnostic crate**: `esp_hal::ram`
  expands to `#[link_section = ".rwtext"]`, so a crate that must not depend
  on esp-hal (luxel-core) can spell it by hand behind a cargo feature —
  that is what `iram-vm` / `iram-builtins` / `iram-math` are (#312, #328).
  Feature-gate it: on a host target `.rwtext` is a stray section name. Which
  boards take which is `IRAM` in `firmware/board-target.sh` (mirrored by
  `iram` in flake.nix); `IRAM_OFF=1` is the A/B lever on `build-esp32.sh`
  and `tools/stack-check.sh`. **The win is per CHIP, not per change**: the
  same placement is 2.9–4.2× on builtin-heavy patterns on the classic ESP32
  and ~1 % on the S3 — never generalise one board's number to another, and
  never take it from `opbench` (4 % where a real pattern moved 76 %).
  **The cost is per chip too**: the classic ESP32 has a dedicated 128 KB
  IRAM region and `.stack` does not move; on the S3 and the C-series
  `.rwtext` is the same SRAM as `.stack`, so every byte comes off the stack
  and `tools/stack-check.sh`'s 24 KB floor is the real ceiling
  (docs/boards.md "IRAM budget", docs/firmware.md "Code placement").
- **A new ROUTE is the expensive thing, not the handler behind it.** Adding
  one awaiting arm to `server.rs`'s dispatcher costs a whole future type,
  its drop glue and a state-machine variant — **+1,424 B on the c6 hosted
  image** for a handler that parsed one token and sent one `Msg` (#598,
  2026-09-20). Making it synchronous with `Channel::try_send` was WORSE
  (+2,000 B: `try_send` is not on the path `send` already linked). Before
  inventing a route for a small piece of live state, ask what endpoint
  already owns that state and whether it can take one more line — the same
  feature as an extra verb on `POST /api/layout`, feeding a flag the render
  task already polled, cost **192 B** and deleted a `Msg` variant on the way
  in. Merging a new verb into an existing `match` arm is not automatically
  cheaper either (+368 B — the arm then branches on `verb` twice). Measure
  the credless flake image per shape; docs/boards.md keeps the table.
