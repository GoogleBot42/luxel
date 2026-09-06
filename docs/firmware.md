# Firmware targets & bring-up

## Supported boards

| board feature | chip | arch | toolchain | notes |
|---|---|---|---|---|
| `board-c3-devkit` (default) | ESP32-C3 | RISC-V | mainline Rust (in the flake) | bare C3 devkits; SPI CLK GPIO6 / DATA GPIO7 |
| `board-pixelblaze-v3` | ESP32 (WROOM-32) | Xtensa | Espressif rustc fork (in the flake) | Pixelblaze v3 Standard — the preferred real-hardware target |
| `board-athom-music` | ESP32 (WROOM-32E) | Xtensa | Espressif rustc fork (in the flake) | Athom music-reactive WLED controller (OTA-only, riskier) |
| `board-esp32-generic` | ESP32 (WROOM/DevKitC) | Xtensa | Espressif rustc fork (in the flake) | generic devkit, VSPI defaults (CLK GPIO18 / DATA GPIO23) |
| `board-s3-devkit` | ESP32-S3 | Xtensa | Espressif rustc fork (in the flake) | ESP32-S3-DevKitC-1 (CLK GPIO12 / DATA GPIO11) — **untested on metal** |
| `board-c6-devkit` | ESP32-C6 | RISC-V (`riscv32imac`) | mainline Rust (in the flake) | ESP32-C6-DevKitC-1 (CLK GPIO6 / DATA GPIO7) — **untested on metal** |

Pin maps, per-board status, and the add-a-board recipe live in
[docs/boards.md](boards.md).

Both toolchains come from `nix develop` — no imperative setup. The Xtensa
one (Espressif's rustc fork + xtensa GNU linker, needed because mainline
Rust has no Xtensa backend) is packaged in the flake as fixed-output
derivations of the official esp-rs/rust-build and espressif/crosstool-NG
release artifacts, patched by autoPatchelfHook; the devshell exports
`XTENSA_RUST_HOME` and puts `xtensa-esp32-elf-gcc` on PATH
(x86_64-linux only for now — add per-system artifact hashes to extend).

Build (devshell, incremental — day-to-day development):

```sh
# ESP32-C3 (default)
cd firmware && cargo build --release            # or `cargo run --release` to flash

# any board — build-esp32.sh maps $BOARD to chip/target/toolchain
# (firmware/board-target.sh), Xtensa and RISC-V alike
BOARD=board-pixelblaze-v3 ./build-esp32.sh      # or `… ./build-esp32.sh flash`
BOARD=board-c6-devkit ./build-esp32.sh

# RAM-constrained profile (docs/boards.md "The `small-chip` profile")
EXTRA_FEATURES=small-chip BOARD=board-athom-music ./build-esp32.sh

# No on-device web app; the hosted playground drives it instead
# (docs/boards.md "Hosted-UI builds") — ~14 KB of OTA slot back, and
# nothing is written to the assets partition
EXTRA_FEATURES=hosted-ui BOARD=board-c6-devkit ./build-esp32.sh
```

`EXTRA_FEATURES` adds non-board cargo features (space-separated);
`tools/stack-check.sh` takes the same variable.

Build (nix package, hermetic — reproducible images):

```sh
nix build .#luxel-fw-pixelblaze-v3     # also: luxel-fw-c3-devkit,
                                        #   luxel-fw-athom-music, luxel-fw-esp32-generic,
                                        #   luxel-fw-s3-devkit, luxel-fw-c6-devkit
ls result/                              # luxel-fw.elf + luxel-fw.bin
espflash write-bin 0 result/luxel-fw.bin   # full-flash image (bootloader+partitions+app)
```

A pure build bakes no WiFi credentials (offline render-only image). To
bake them, pass via environment with an impure eval:

```sh
LUXEL_SSID='net' LUXEL_PASS='secret' nix build .#luxel-fw-pixelblaze-v3 --impure
```

(A git-untracked creds file wouldn't work: flakes only see tracked files.)
Cred-baked images contain the password in plaintext (image + world-readable
nix store) — don't build them on shared machines or share the .bin.
Two lockfiles feed the hermetic build: `firmware/Cargo.lock` (our deps) and
`firmware/rust-std.Cargo.lock` (the std workspace's deps, needed by
-Zbuild-std; re-copy from
`$XTENSA_RUST_HOME/lib/rustlib/src/rust/library/Cargo.lock` on toolchain
bumps).

WiFi credentials bake in at build time until NVS provisioning lands (M3):
`LUXEL_SSID=net LUXEL_PASS=secret cargo build …`. Without them (or with
empty values) the firmware runs offline (render-only).

## OTA updates

The partition table (firmware/partitions.csv) is pure A/B: ota_0 + ota_1
app slots (1 MB each), no factory partition (this device has no distinct
golden image — the serial flash is the same build that ships OTA, so
factory was 1 MB of dead weight). Serial flash lands in ota_0; OTA writes
alternate ota_0/ota_1. The bootloader validates images before jumping, so a
corrupt upload falls back to the currently working slot; if both OTA slots
are ever bad it boots ota_0 (the bootloader's default when no factory
partition exists). Serial recovery always works regardless. The 1 MB freed
by dropping factory is the `storage` partition (device pattern library).

```sh
# push the current devshell Xtensa build:
firmware/build-esp32.sh && tools/ota-push.sh <host>
# or a nix-built image:
nix build .#luxel-fw-pixelblaze-v3 && tools/ota-push.sh <host> result/luxel-fw-ota.bin
```

`POST /api/ota` takes the raw app image (espflash save-image output — the
package's `luxel-fw-ota.bin`, NOT the merged `luxel-fw.bin`), streams it to
the inactive slot sector-by-sector (~4 KB peak RAM), activates it, and
reboots ~400 ms after replying. `/api/status` reports `slot` (which app
partition is running) and `version` — ota-push.sh uses it to confirm the
device came back.

Migrating a device that predates the OTA layout requires ONE serial flash
of the merged image (it rewrites the partition table):
`espflash write-bin 0 result/luxel-fw.bin` — after that, everything is OTA.

## Stack & heap invariants

Three real incidents — the v0.1.4 OTA-crash root cause (2026-07-06), the
v0.1.19 boot brick, and the v0.1.31-33 deterministic stack panics
(2026-07-27) — all trace back to the same memory model. This section is
the permanent home for those lessons; UPDATES.md has the full incident
writeups under those dates.

**The main-task stack is leftover DRAM.** esp-hal gives the main task
whatever RWDATA the linker doesn't claim for `.data`/`.bss` — every static
shrinks it further, including the `esp_alloc::heap_allocator!` arenas and
embassy task futures. There is no linker error for shrinking it too far;
the failure is a runtime stack overflow ("write to the stack guard value on
ProCpu"), and because the WiFi blob's own statics sit in the same region,
overflow symptoms can look like blob corruption rather than a stack bug.
`firmware/src/main.rs::main` runs two `heap_allocator!` calls per board: a
`#[esp_hal::ram(reclaimed)]` region (96 KB on esp32 / 64 KB on the C3 —
DRAM the WiFi blob would otherwise reserve before init reclaims it) and the
main heap region (80 KB on esp32 / 160 KB on the C3). Whatever DRAM is left
after both becomes `.stack`.

**Task futures are statics.** `#[embassy_executor::task]` functions compile
to statics, so a large buffer held across an `.await` inside one lives in
`.bss` for the life of the firmware, not on a per-call frame. v0.1.19's
first cut put ~12 KB of MQTT/netin buffers in task futures and bricked the
boot (measured stack ≈ 10.7 KB). Big task buffers must be heap `Vec`s.

**WiFi NMI frames land on whatever stack is current.** The single main
stack runs the whole embassy executor, picoserve's response path,
esp-storage's flash ops, and the WiFi level-6 NMI frames — a tight main
stack plus one NMI atop a deep call overflows even when normal execution
alone would have fit. This was the actual trigger in both the v0.1.4 and
v0.1.33 incidents: a request-context flash read at picoserve's max call
depth, with a WiFi NMI frame landing on top.

**Never call `FlashStorage::read` (esp-storage) in request/async context.**
It puts an unconditional 4 KiB sector bounce-buffer on the caller's stack.
Use `read_nor` instead — word-aligned offset/length/buffer, reads straight
into the destination, zero stack cost. `read_chunk` in
`firmware/src/assets.rs` is the reference pattern: stage through a
word-aligned heap buffer, then copy out the unaligned slice actually
wanted.

**Measure `.stack`, don't estimate it.** `readelf -S` (or
`tools/stack-check.sh`, see docs/tools.md) is ground truth. v0.1.31 shipped
on an arithmetic estimate of ~27 KB of leftover stack; the real, linked
`.stack` was 18,140 B (17,884 B in v0.1.32) — ~2 KB above the measured
15.6 KB overflow point — and every request-context flash read panicked
deterministically. `tools/stack-check.sh` now fails the build if `.stack`
drops under a 24 KB floor, on top of its existing per-function frame-budget
check across the whole linked image (deps and build-std core/alloc
included — the class of check that originally caught esp-storage's
`FlashStorage::read` bounce buffer).

**Heap economics.** Two `esp_alloc` regions per board (above). Boot tasks
that do multi-KB loads (playlist/pattern resume) must run after
`stack.wait_config_up().await` — WiFi bring-up mallocs don't null-check, so
a heavy load racing WiFi init shows up as a `StoreProhibited` crash inside
the blob, not a clean OOM panic. The engine holds exactly one decoded
`Program`; swap-path allocations are fallible (`try_reserve_exact`,
`firmware/src/main.rs`). `RUNTIME_FLOOR` (20 KB) is the floor
`try_budgeted_engine` checks after a pattern loads — a pattern that fits
its array budget but still leaves the heap under the floor is rejected as
a vmerr instead of panicking. `budgeted_engine` derives the array budget
itself as `esp_alloc::HEAP.free() - (RUNTIME_FLOOR + 4 KiB)`, clamped to a
16 KB minimum — byte-accurate per array element, so one big array isn't
taxed for overhead that only swarms of tiny arrays pay.

Both numbers live in **`luxel_core::budget`**, not in `main.rs`: the web
editor imports the same constants through the wasm build to warn the user,
before a push, that their pattern won't fit the device they're connected to
(Gitea #15; docs/webui.md "Capacity warning"). Change them there and the
device and its prediction move together — a divergence would mean the editor
promising a pattern fits a device that then rejects it.

**WS2812 (bit-serial protocols) requires the DMA SPI path, never blocking
writes.** Blocking `Spi::write` splits every frame into 64-byte FIFO
transactions with a busy-wait between them; 64 B = 512 SPI bits, not
divisible by WS2812's 3-SPI-bits-per-LED-bit encoding, so every chunk
boundary corrupts a bit mid-symbol, and a WiFi interrupt landing in the
inter-chunk gap stretches it past the strip's latch threshold (partial
frame, rest of the frame re-addresses from pixel 0). `main.rs` now runs
every board's SPI through `.with_dma()` (`DMA_SPI2` on esp32, `DMA_CH0` on
the C3, both typed `SpiDma`) so each frame is one continuous transfer.
Clocked protocols (APA102/SK9822) are immune — they never showed the bug,
which is why it went unnoticed until the first single-wire WS2812 test.

**Clippy runs on the default target only.** `cargo clippy` targets the
default `board-c3-devkit` feature (mainline rustc); it can't drive the
Xtensa `-Zbuild-std` build (clippy-driver has no Xtensa backend). The
stack lints declared at the top of `main.rs`
(`#![deny(clippy::large_stack_arrays)]`, tuned via `firmware/clippy.toml`)
are board-independent, so running clippy on the C3 build still covers the
Xtensa boards' source.

## Flash-mapped regions

`firmware/src/flashmap.rs` maps page-aligned flash ranges read-only into the
CPU's data bus through the cache MMU — the same page table the bootloader
programs for the app's own rodata — so large immutable data is read as
memory instead of copied through esp-storage's flash controller (Jeremy's
decision, 2026-09-05; design and per-chip page arithmetic in
docs/research/flash-mmap.md). The web assets partition is mapped at boot
(`assets::map_region`, 15 × 64 KiB pages; `/api/status` reports
`assets_mapped`), and the pattern engine's execution-ready bytecode is the
next consumer (Gitea #260). What a consumer must and must not do:

- **Read it from task context only, never from an interrupt handler.** A
  mapped read is a cache miss to SPI0, and no SPI0 fill may happen while an
  esp-storage op (SPI1) is in flight. In task context that is guaranteed:
  the op's critical section keeps this core busy, and on dual-core builds
  the flash fence parks the other core. An ISR is the one thing a critical
  section does not stop.
- **Invalidate after writing flash under a mapping.** Cache lines do not
  watch SPI1. A writer (the assets upload, the pattern store's raw-slot
  writer) calls `flashmap::invalidate` / `invalidate_slice` on the range
  before anyone reads it back through the mapping; on the classic ESP32
  that is a whole-cache flush of both cores, elsewhere an address-range
  invalidate.
- **Never map an app slot, never unmap a region something may still
  read.** A load from an invalid MMU entry is a cache-error fault, not a
  recoverable error. Mappings are normally made once at boot and leaked.
- **Page-aligned offsets only** (64 KiB on the ESP32/S3/C3; the C6's page
  size is a register, 64 KiB by default). `partitions.csv` keeps every
  mappable region aligned; lengths round up to whole pages.
- **Keep the read_nor fallback.** `map` can fail (`flashmap-off` build, no
  free entries, or the boot self-check refusing a mapping that does not
  present the page asked for), and every consumer answers that with the
  path it had before — slower, never broken. `assets::mapped()` returning
  `None` is that signal for the assets consumer.
- The cross-core half (`flashmap::quiesced` → `core1::fenced`) is wired
  when the second-core render executor lands; until then the AppCpu is
  halted and a critical section is the whole story.

### The pattern store's mapped half and the code arena

`patterns.rs` maps the raw upper half of the `storage` partition
(`0x290000`, 512 KiB, 8 pages) at boot the same way (`map_raw`, self-check
on the header page, leaked) and executes the running pattern from it —
`patterns::current_code()` is the VM consumer contract's slice: rodata for
the built-in default, the ad-hoc read-back slot for a pushed pattern, or the
pattern's **code arena** extent for a library pattern. Layout of the raw
half (partition-relative):

| offset | size | what |
|---|---:|---|
| `0x80000` | 4 KiB | header page: magic, ad-hoc src/bc lengths, bc side |
| `0x81000` | 32 KiB | ad-hoc source (one page past `MAX_SOURCE` = 30 KB) |
| `0x89000` | 2 × 64 KiB | ad-hoc bytecode, TWO sides — `store_current` writes the side the running engine is not executing from and flips `CUR_BC_SIDE`, so a mapped engine never sees its code change |
| `0xA9000` | 348 KiB (**87 × 4 KiB pages**) | the **code arena** |

The arena is a page-granular **extent allocator** (`extents.rs` +
`patterns.rs`, Gitea #281): one stored pattern's LXBC per extent, a
contiguous run of 4 KiB erase pages, 4-byte aligned — the one property XIP
needs, and the reason no off-the-shelf flash filesystem fits (littlefs, ekv
and sequential-storage all store a file as linked or moving blocks). 87
pages hold every pattern a device can store (`MAX_PATTERNS` = 24) several
times over: the median library blob is under 1 KB, the largest ~26 KB, and
the hard cap is `MAX_BC` ≈ 38 KB = 10 pages.

It replaced 7 fixed 40 KiB slots (PR #276, one day old) that cached seven
patterns and wasted ~90 % of the same region. With extents there is no
eviction at all — and therefore no LRU bookkeeping, and no "saving a
pattern silently throws out someone else's cache".

**`extents.rs` is planning only** — a page bitmap, first-fit, the
compaction plan, and the directory format. It is `no_std`, allocation-free
and host-tested (`tools/extent-check`, `cargo test --workspace`, 18 cases
including an exhaustive compaction-vs-prediction sweep and a 4,000-step
churn fuzz that re-checks the bitmap against the extents every step).
`patterns.rs` owns every byte of flash I/O.

Rules the code enforces:

- A **library swap carries only the id** (`Msg::Library { id, ms }`): the
  render task decodes from `code_of(id)` (mapped) or, without an extent,
  from a transient `bytecode_of` Vec that it then offers an extent
  (`cache_code(.., may_compact = false)`), so playlist churn writes flash
  at most once per pattern and then never. `save()`'s caller
  (`POST /api/patterns`) passes `may_compact = true` — a user action may
  slide other extents down to open a contiguous run. Neither path ever
  evicts: if the pool cannot fit the blob, the pattern stays on the chunk
  path exactly as before. Identity and read-back lengths come from
  `source_stat(id)`, which streams the source out of its chunks (no source
  Vec, no envelope Vec anywhere in a library activation).
- **An extent an engine is executing from is never written, freed or
  moved** — the borrowing invariant, below. A re-save allocates a NEW
  extent, writes it, invalidates, hash-checks, publishes the directory, and
  only then frees the superseded one — so a power cut never loses both.
  When the re-saved pattern is the running one the old extent stays in the
  directory (its pages are still executing) as a stale generation, and is
  swept the next time the allocator runs with something else on the strip.
  `delete` is the same: `arena_forget` KEEPS a pinned pattern's extent
  (dropping it would hand its pages to the next save, which would erase
  them under the live VM) and the sweep reclaims it once the engine lets
  go.
- **Compaction** runs only when a save finds no contiguous hole and
  `compacted_free_run()` says packing would open one (no wasted erases
  otherwise). It slides live extents toward page 0 one at a time; a pinned
  extent stays put and splits the free space instead of blocking the
  pass. Each move un-publishes the extent, copies **one page
  per `ota::with_flash` op with yields between** — the destination is
  strictly below the source, so ascending page order is a safe overlapping
  move — invalidates, hash-checks and re-publishes. A power cut mid-pass
  therefore costs at most the extent in flight, which re-caches from its
  chunks on its next activation.
- Every arena write is `write_raw` (erase + word-aligned page writes, one
  `ota::with_flash` per op with yields — the same quiesce path as the
  assets writer, so the second-core fence hook lands in one place), then
  `flashmap::invalidate_slice`, then a hash check of the mapped bytes, then
  the RAM directory, then the persisted directory (a reserved-key map item,
  `ARENA_KEY`). Boot rebuilds the bitmap from the directory and drops any
  extent whose pattern/generation is not in the index, whose bytes do not
  hash to the recorded value, or that does not fit the current layout — a
  torn or stale extent is never handed to the engine. One transaction at a
  time (`ArenaGuard`): a second `cache_code` while a save or compaction is
  in flight degrades to the chunk path rather than racing for pages.
- `/api/status` reports `arena: [used_pages, total_pages]`.
- Without the mapping (`flashmap-off`, refused self-check) the arena is off
  (`patterns: code arena off`), `arena` reads `[0, 0]`, `current_code()` is
  `None`, and every path reads through a Vec exactly as before.

#### The borrowing invariant and the pin set (Gitea #260)

The engine decodes a mapped pattern with
`luxel_core::bytecode::deserialize_lean_static`, so a running `Program`'s
`words` — its code AND its constant pool — are a `&'static [u32]` **into
the mapping**, not a heap copy. That is where the RAM saving comes from
(the resident program is its header tables: 4–11 KB instead of 12–35 KB
for the big patterns, see docs/research/flash-mmap.md "RAM accounting"),
and it is also a lifetime contract:

> **A `Program` must never outlive the flash it borrows.** No extent an
> engine is executing from may be written, moved or freed while it holds
> it.

`shared::get_current_pattern_id()` alone does NOT express that — it names
one pattern, and there are two windows where an engine executes something
else:

- a swap decodes the **incoming** pattern's mapped bytes *before* it
  becomes the current pattern (between `code_of` and
  `set_current_pattern_id`), and
- a **crossfade** keeps the outgoing engine alive as the blend source
  (`prev` in `render_task`) for up to several seconds *after* the current
  pattern id has moved on.

A save that compacts the arena in either window is a use-after-free, and on
a dual-core board the store runs on the OTHER core — this is not even an
`await`-granularity race, so "no yield point in between" proves nothing.

So `patterns.rs` keeps an explicit **pin set** that the render task
publishes, three slots plus the current pattern id as belt and braces:

| slot | set by | holds |
|---|---|---|
| 0 | `pin_code(id)` — **before** `code_of` | what a decode is about to borrow |
| 1 | `pin_running(id)` — when the new engine is installed | what the live engine borrows |
| 2 | `pin_prev_from_running()` — where `prev = engine.take()` | what the crossfade's outgoing engine borrows |

Slot 2 is released by `unpin_prev`, and `render_task` never writes
`prev = None` directly: every drop goes through `drop_prev`, which drops
the engine and then releases the pin. An ad-hoc push (`Msg::Code` /
`Msg::Crossfade`) decodes a transient envelope `Vec`, so its `Program` owns
its words and `pin_running("")` clears slot 1.

`compact_move`, `cache_code`'s stale-generation sweep, the
superseded-generation free and `arena_forget` all consult the set;
`extents.rs`' `next_move`/`compacted_free_run` take a pin *slice* and each
pinned extent splits the free space. Pins are conservative by
construction — a stale one wastes arena pages until the next swap
overwrites it, and can never free something live.

The ad-hoc read-back slot upholds the same contract by its two-sided
layout: `store_current` writes the side `CUR_BC_SIDE` does not point at,
and `render_task` drops the outgoing engine (`drop_prev`) before
`persist_current_pattern` runs, so no engine ever borrows the side being
erased.

The **rodata default** is borrowed too, which is why `PATTERN_BC` goes
through a `#[repr(C)]` wrapper with a zero-sized `[u32; 0]` field:
`include_bytes!` has alignment 1 and `deserialize_lean_static` silently
copies a blob whose word region is not 4-aligned in memory.

## Render-loop timing counters

`render_task` publishes `FPS` — frames rendered in the last full second —
once a second, and since Gitea #260 it publishes four per-stage timers on the
same tick: `FRAME_US`, `VM_US`, `PIPE_US`, `OUT_US` in `firmware/src/shared.rs`,
served as `frame_us` / `vm_us` / `pipe_us` / `out_us` by `/api/status`. Each is
the **average microseconds per rendered frame** over that window.

- `vm_us` — `Engine::frame`: the VM evaluating the pattern, plus the outgoing
  engine's frame and the blend while a crossfade runs. Normally the dominant
  term, and the one a pattern's own cost shows up in.
- `pipe_us` — the `/api/pixels` preview copy (`set_pixels`) plus
  `apply_outpipe` (gamma, output palette, blur).
- `out_us` — `BoardOutput::write_frame`: the SPI/RMT or HUB75 driver. Rises
  with pixel count and falls with DMA; a large value here is a wire-format or
  driver problem, not a pattern problem.
- `frame_us` — the whole engine branch, from the delta math through
  `write_frame` returning. The stages nest, so `frame_us` ≈ the other three
  plus the small per-frame bookkeeping between them (delta/sync math, pin
  sync, vmerr drain).

Only the **pattern** branch is instrumented. Frames driven by live input
(DDP/E1.31) and idle loops with no engine are not timed, and the divisor is
the count of timed frames in the window — not `FPS`, which counts every loop
iteration. A second with no pattern frame stores 0 in all four.

The hot path costs three extra `Instant::now()` reads and four integer adds
per frame; no formatting, allocation, or float work happens inside the loop.
`frame_us` is wall time inside the branch, so it includes any preemption by
the WiFi/network tasks — compare stages against each other, not against a
theoretical cycle budget.

## Cores & tasks: the render task runs on the second core

Classic ESP32 and ESP32-S3 are dual-core; the C3/C6/S2/C2 are not. Until
2026-09-05 every task ran on the ProCpu: esp-rtos pins the WiFi driver's
task there, `esp_rtos::main`'s executor is the ProCpu main thread, and a
render frame — 55–250 ms at 4096 px on the S3 (Gitea #260) — held that one
core for its whole duration, so the web pool got a thin slice per frame and
the 228 KB playground bundle took 31–62 s to download (Gitea #259). The
Athom reproduced it at 2048 px: a 228 KB bundle took 20.7 s to download beside rainbow, 34.8 s beside snake-2d (fps 12 and 8).

**Layout now** (`firmware/src/core1.rs`, cfg `multi_core` — emitted by
`firmware/build.rs` for the `esp32` / `esp32s3` chip features; single-core
boards compile the module down to no-ops and spawn the render task on the
main executor exactly as before):

| core | what runs there |
|---|---|
| ProCpu (core 0) | `esp_rtos::main`'s executor: WiFi driver task (pinned by esp-radio), network stack, the web pool, MQTT/DDP/E1.31/SNTP, playlist, resume, sensors, reboot; every flash write except the render task's own ad-hoc pattern persist |
| AppCpu (core 1) | a second esp-rtos scheduler (`esp_rtos::start_second_core`) whose main thread runs a thread-mode embassy executor with exactly one task: `render_task` |

`core1::start` heap-allocates the AppCpu stack (20 KB, leaked; a
static would come straight out of the leftover-DRAM main stack, see above),
fills it with a pattern, starts the second core, and returns once that
core's flash-fence handler is armed. `/api/status` reports
`core1.stack` as `[used, total]` from the fill-pattern high-water mark —
measured 10,896 B under tools/render-bench.mjs through 2048 px snake-2d
(20,480 B allocated). If the allocation fails the render task falls back to the ProCpu
(logged `core1: stack alloc failed`).

Cross-core sharing needed no changes: the render task talks to the rest
through `embassy_sync::Channel`/`Signal` and `BlockingMutex<
CriticalSectionRawMutex>` cells (`shared.rs`) plus independent atomics.
esp-hal's critical section is a multicore spinlock (esp-sync: Acquire/Release
CAS on the core id, interrupts masked while held), so every `Shared<T>` cell
stays sound; the `Relaxed` atomics are single independent values, never a
flag that publishes another write. Wakes across cores go through the
esp-rtos scheduler (a cross-core software interrupt), which is how
`Timer::after` and channel wakes reach the AppCpu executor. Both output
drivers are `Blocking` (SPI DMA polled, HUB75 circular DMA with no ISR),
so no interrupt affinity moved; peripheral interrupts enabled from `main`
stay on the ProCpu.

**Three hard-won rules for the park, all black-boxed on the Athom
(2026-09-05)** — each one was a hard hang (no panic, no reboot) within a
minute of running `library/snake-2d.js` with the playground bundle
downloading, reproduced 6/6 and isolated with a test build that could park
without touching flash, park without touching RTC memory, or skip parking:

1. **The park handler runs at interrupt Priority1, not 3.** A level-3 park
   can land inside a level-1 handler (esp-rtos's task switch, esp-hal's
   dispatcher) in the middle of its DPORT/APB register reads; the ESP32's
   interrupted-peripheral-read errata then wedge the bus bridge for both
   cores. At level 1 the park only ever preempts thread-mode code, and the
   handler raises the mask itself (INTENABLE = 0) once inside.
2. **The AppCpu never touches RTC memory inside the park.** The fence's
   black box lives in RTC slow memory; an AppCpu bump of it from the park
   handler wedged the ProCpu's next RTC-memory access. Only the ProCpu
   writes the black box.
3. **The fence waits for the strip's SPI2 DMA transfer to end before the
   SPI1 op** (`output::transfer_busy`, the `SPI_CMD.usr` bit read straight
   from the register). On the classic ESP32 the SPI hosts share the DMA
   engine, and a ROM SPI1 flash op issued while an SPI2 transfer is in
   flight hangs the CPU issuing it. Single-core builds could never hit this
   — the blocking DMA write held the only core — so it is new with the
   second core, and it is why a heavy pattern (more of the frame spent
   mid-transfer when the park lands) hung sooner than rainbow.

4. **The fencing core masks its OWN interrupts for the whole fenced
   window** — from the park acknowledgement until after the other core is
   released (`Gitea #292`, 2026-09-06). This is the half of ESP-IDF's
   `spi_flash_disable_interrupts_caches_and_other_cpu()` the fence was
   missing, and without it a 728 KB `POST /api/assets` wedged the ProCpu
   5/5 after 60–70 KB. The black box put the wedge AFTER the ROM erase
   returned, on the instruction where esp-storage's own critical section
   (`rsil 5`) drops back to level 0 and every interrupt that queued up
   behind a ~45 ms sector erase fires at once — with the other core still
   parked. One of those handlers never returns. Masking until past the
   release defers them to a point where both cores are running, and the
   same install then ran 5/5 clean in 13–15 s. The cost is this core's
   interrupt latency for one flash op; esp-storage already masked levels
   1–5 for the ROM call, so the new exposure is level 6+ across the op and
   every level across the microsecond release. Diagnosing this needed one
   thing the black box did not have: which side of the driver call the
   wedge was on, now recorded as ProCpu phase 6 (inside esp-storage and
   the ROM routine) vs 7 (it returned).

Because a wedge of this class is a silent hang, dual-core builds also arm
the **RTC watchdog** (20 s, fed every 3 s by a task on the ProCpu
executor — `core1::watchdog_task` — AND every 64 fences from
`core1::fenced` itself, because a long flash burst blocks that executor:
a 728 KB asset install is ~15 s of erases and a garbage-collecting pattern
save was measured at 25 s. Taking a fence is proof of progress, so the
watchdog still catches a core that stopped) and keep a **black box in RTC
slow memory** that survives the reset: `/api/status` `core1.last` reports
the previous run's reset reason, the fence's phase per core, fence/park
counts and timeouts, and which call site
(`core1::tag`) held the fence in flight (`core1.rs` documents the layout).
A `last.reset` of `SysRtcWdt` with a ProCpu phase of 3, 6 or 7 is the
signature of the hangs above.

**The flash fence** is the one piece of real multicore machinery. SPI flash
is shared between the cache (every instruction fetch from flash-resident
code on EITHER core) and the flash driver; while the driver's ROM call runs,
the other core must not fetch from flash — an erase/program returns garbage
and a read contends for the bus (ESP-IDF stalls the other CPU for every op,
reads included). esp-storage's default multicore strategy therefore makes
every flash WRITE fail while the second core runs, and its `auto_park`
alternative hard-stalls the other core at an arbitrary instruction —
possibly inside a spinlock (scheduler, heap, any critical section) that the
flash-writing core's next interrupt then spins on forever. Luxel instead
parks cooperatively: `core1::fenced(op)` raises the other core's park
software interrupt (SWI2 parks the ProCpu, SWI3 the AppCpu; SWI0/1 are
esp-rtos's), whose handler runs at the top vectored priority from IRAM and
spins on a DRAM flag. Interrupts are masked inside every critical section,
so the handler can only run once its core holds no lock. The fence sits
OUTSIDE the flash driver's critical section (`ota::with_flash`) with
interrupts enabled, so two cores fencing at once resolve through the park
handler instead of deadlocking. The four fenced doors — `ota::with_flash`,
the `patterns::AsyncFlash` adapter (sequential-storage over a leased
driver), `ota::begin`'s partition-table reads, and `flashmap::quiesced`
(cache-MMU table programming and the whole-cache flush; Gitea #272 — the
ESP32's DPORT MMU registers must not be read while the other core runs,
and a flush must not race a core executing from flash or reading a
mapping; mapped *reads* need nothing, see docs/research/flash-mmap.md
"Concurrency") — cover every flash op in the image; `FlashStorage` is constructed with `multicore_ignore()` because
the fence supplies the guarantee it asks for. `/api/status` `core1.
fence_timeouts` counts park requests the other core did not acknowledge
within 100 ms (the op proceeds; nonzero means something is wedged — it
stayed 0 through every bench and OTA below), `core1.fence_wait_us` is the longest
park wait seen — the render-side cost of one flash op (≤ 2.1 ms seen; typically 40–800 µs — it includes waiting out an in-flight strip transfer), and
`core1.fences` is `[begun, completed]` for this boot, so the fence rate of
any one operation is a before/after delta.

**A fence is expensive, so a fenced op must be page-sized.** Each one costs
an interrupt and a park round-trip on the other core plus this core's
interrupt latency for the op — of the order of half a millisecond, against
about ten microseconds for the flash read itself. The raw-region writers
were always shaped that way (one fence per 4 KiB: erase, then one program
pass — `assets::AssetWriter`, `ota::OtaWriter`, `patterns::write_raw`), but
the pattern store was not: `map::fetch_item`/`store_item` were each handed
a fresh `NoCache`, so every call re-scanned all 128 pages and every item
header in them, one fenced 8-byte read at a time. Measured on the Athom
(Gitea #292): **81,143 fenced reads, ~13 s, for one 650-byte POST
/api/patterns** — long enough that the RTC watchdog rebooted the board
mid-save. There is now ONE `PageStateCache` for the store region
(`patterns::store_cache`), kept across transactions and passed to every
call; the flash LEASE (`ota::take_flash` hands the driver to one caller at
a time, across both cores) is what makes a single shared `&mut` sound, and
anything that writes inside the range around sequential-storage's back —
the format wipe in `patterns::init` — resets it. Same save: **929 fences,
0.5 s**. A 46 KB save is 5–25 s and 5,000–33,000 fences depending on
whether it garbage-collects.

**Measured on the Athom** (classic ESP32, 60 px WS2812 strip, same tree
either side of the change, `tools/render-bench.mjs`):

| px | pattern | fps before → after | fps during download | vm_us before → after | out_us | heap_free (min) before → after | bundle download before → after |
|---:|---|---:|---:|---:|---:|---:|---:|
| 60 | rainbow | 121 → 123 | 96 → 120 | 793 → 716 | 2,497 | 105,456 → 84,960 B | 1.73 s → **1.10 s** |
| 60 | snake (1D) | 121 → 123 | 90 → 120 | 1,775 → 1,618 | 2,532 | 103,108 → 82,612 B | 1.79 s → **1.04 s** |
| 60 | snake-2d (smart=100) | 116 → 119 | 78 → 116 | 2,523 → 2,268 | 2,548 | 83,524 → 63,028 B | 2.12 s → **1.01 s** |
| 2048 | rainbow | 12 → 12 | 11 → 12 | 18,762 → 17,326 | 68,366 | 75,636 → 55,140 B | 20.73 s → **0.94 s** |
| 2048 | snake (1D) | 9 → 9 | 9 → 9 | 48,340 → 46,379 | 68,312 | 73,288 → 52,792 B | 27.54 s → **0.92 s** |
| 2048 | snake-2d (smart=100) | 8 → 9 | 6 → 8 | 58,599 → 54,476 | 68,376 | 53,704 → 33,208 B | 34.79 s → **1.17 s** |

(`tools/render-bench.mjs`, Athom, master 731ce81 either side; `vm_us`/`out_us` are the #263 per-stage timers, µs per frame; the bundle is the 228,553-byte gzip'd playground `index-*.js` fetched while the pattern runs. At 2048 px the frame is the WS2812 wire time — `out_us` 68 ms of a 86–123 ms frame — so fps cannot move much; the `vm_us` drop of 7–8 % is the render core no longer being preempted by WiFi.)

Rendering on its own core changes fps by itself only marginally: fps is bounded by the wire at 2048 px (`out_us` ≈ 68 ms of every frame is the WS2812 transfer), so it stays at 12/9/9; what the second core buys the *engine* is `vm_us` −7…8 % (no WiFi preemption) and, for the web server, a bundle download that no longer depends on the frame at all: 20.7–34.8 s → 0.9–1.2 s at 2048 px, 1.7–2.1 s → 1.0–1.1 s at 60 px, with fps during the download equal to fps without it.

**Not verified on metal**: the ESP32-S3 / HUB75 boards build green with
the same code paths (Gitea #266). Splitting one frame's pixels
across both cores is a separate step that needs a purity analysis of
`render`'s callees (Gitea #265).

## Board: Pixelblaze v3 Standard (preferred dev target)

Jeremy has two identical v3s: one stays the untouched compatibility oracle,
the other becomes the Luxel dev unit. All pins below are from the official
schematic published in <https://github.com/simap/pixelblaze>
(V3/hardware/PB32_3.x.pdf) — public docs, no firmware reversing.

ESP-WROOM-32 (Xtensa, 4 MB flash), AP2112K 3.3 V regulator, micro-USB is
**power-only** (D+/D− unconnected in the schematic).

| function | GPIO | notes |
|---|---|---|
| LED DATA | 23 (VSPI MOSI) | through onboard 3.3→5 V level shifter + 100 Ω |
| LED CLOCK | 18 (VSPI SCK) | same shifter — APA102/SK9822 native, WS281x uses DATA only |
| status LED | 12 | Luxel lights it at boot; strapping pin, output-only use |
| button | 32 | unused so far |
| expansion header | GND, RST, 3v3, RX, TX, IO0, IO25, IO26 | sensor board / serial; labels silkscreened at 45° beside each pin, on the edge opposite the screw terminals ("RST" = the ESP32 EN/reset pin) |

### Flashing + restore procedure (serial, fully recoverable)

The expansion header carries everything esptool needs. Wire a 3.3 V
USB-UART adapter: GND→GND, adapter TX→RX0, adapter RX→TX0. Power the
board from its own micro-USB (don't connect the adapter's power pin).

**Entering the ROM bootloader** ("hold IO0"): jumper IO0→GND on the
header, then reset the chip while the jumper is in place — briefly touch
RST→GND (the board's silkscreen says RST; the schematic calls it EN), or
unplug/replug USB power. The pin is only sampled at reset;
once the bootloader is running the jumper can come off (it stays in the
bootloader until the next reset). This same entry step precedes *every*
esptool/espflash command below — the tools hard-reset the chip when they
finish, and without DTR/RTS wired to EN/IO0 they can't re-enter the
bootloader themselves. Leaving the IO0 jumper in for the whole session
also works; just remove it before the final reset so the firmware boots.

Then:

```sh
# 1. one-time backup of the ENTIRE stock flash (bootloader + partitions +
#    app + settings). This is the restore path — Electromage's downloadable
#    update files only apply through a *running* Pixelblaze updater, so
#    they cannot resurrect a device we've overwritten.
espflash read-flash 0 0x400000 pb-v3-stock.bin    # or esptool read_flash

# 2. flash Luxel
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash

# 3. restore stock whenever wanted
espflash write-bin 0 pb-v3-stock.bin              # or esptool write_flash 0
```

Keep `pb-v3-stock.bin` somewhere safe (it contains the device's WiFi
config and saved patterns — don't commit it).

## Board: Athom WLED ESP32 music-reactive controller (Jeremy's unit)

ESP32-WROOM-32E, 4 MB flash on the older revision (ships with WLED-SR
0.13.2 "Toki-SR"). Two clocked LED channels and relay-switched strip power:

| function | GPIO |
|---|---|
| DATA1 / CLK1 | 18 / 5 |
| DATA2 / CLK2 | 17 / 16 (not driven yet) |
| strip VCC relay | 2 — **must be high or the strips stay dark** |
| button | 0 |
| IR receiver | 25 |
| PDM mic (I2S SD/WS) | 32 / 15 (unused; sound-reactive is a later milestone) |

Pins are from Athom's product page; confirm against the unit via WLED
(Config → LED Preferences, or `http://<ip>/cfg.json`) before first flash.

## Flashing the Athom (plan — do not do this yet)

The board has no USB; WLED's `/update` OTA is the only stock path. Our
esp-bootloader-esp-idf images are IDF-bootloader-compatible so OTA *should*
take them, but:

1. Once Luxel is on, WLED can only be restored through **Luxel's own OTA**
   — which doesn't exist yet. Flashing now would one-way the device.
2. Recovery without OTA means opening the case for the serial pads.

So: validate on a socketed devkit first, implement OTA upload in Luxel
(M3), keep a copy of the Athom's current WLED-SR .bin, and only then try
the controller. Until that, the dodecahedron strip or the SK9822 can hang
off a devkit.
