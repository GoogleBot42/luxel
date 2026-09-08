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
# (firmware/board-target.sh), Xtensa and RISC-V alike. The board is an ENV
# VAR; the positional is the ACTION (flash | image | log). A board name in
# the positional is now an error — it used to be ignored, so
# `./build-esp32.sh board-athom-music` built pb-v3 and said nothing (#389).
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
# push the current devshell Xtensa build (BOARD, not a positional!):
BOARD=board-pixelblaze-v3 firmware/build-esp32.sh
BOARD=board-pixelblaze-v3 tools/ota-push.sh <host>
# or a nix-built image:
nix build .#luxel-fw-pixelblaze-v3 && tools/ota-push.sh <host> result/luxel-fw-ota.bin
```

`ota-push.sh` refuses an image that is not a build of `$BOARD` — it greps
the image for that board's `board::NAME` string (Gitea #389). The classic
ESP32 boards all land at `firmware/target/xtensa-esp32-none-elf/release/
luxel-fw`, so nothing else tells them apart, and the wrong image boots
perfectly: the only symptom is the other board's `RESERVED_PINS` and pin
defaults, which can leave the strip's own GPIO unselectable.

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

**The load base is not `heap_free`.** Every load path — `Msg::Code`,
`Msg::Library` without a crossfade, both `rebuild()` callers — starts with
`engine = None; drop_prev(&mut prev)`, so the incoming pattern is measured
against free heap *after* the outgoing engine is released. The firmware
therefore brackets each load (`note_engine_heap`, `main.rs`) and reports
what the resident engine costs as `/api/status` `engine_heap`; the budget a
swap actually gets is `budget::load_base(heap_free, engine_heap)`. Predicting
against `heap_free` alone charges the incoming pattern for the outgoing one,
which is what made the editor warn about patterns that load fine (Gitea
#287) — the fatter the resident pattern, the lower `heap_free`, the louder
the false alarm. A crossfade deliberately keeps the outgoing engine alive, so
it records nothing and leaves the last clean measurement standing.

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
`assets_mapped`), and the pattern store's extent region is the other
consumer (`patterns::map_ext`, 14 × 64 KiB pages, mapped into the entries
right after the assets mapping — 29 of the classic ESP32's 64 DROM0
entries in total). What a consumer must and must not do:

- **Read it from task context only, never from an interrupt handler.** A
  mapped read is a cache miss to SPI0, and no SPI0 fill may happen while an
  esp-storage op (SPI1) is in flight. In task context that is guaranteed:
  the op's critical section keeps this core busy, and on dual-core builds
  the flash fence parks the other core. An ISR is the one thing a critical
  section does not stop.
- **Invalidate after writing flash under a mapping.** Cache lines do not
  watch SPI1. A writer (the assets upload, every pattern-store file
  write) calls `flashmap::invalidate` / `invalidate_slice` on the range
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

### The pattern store: a packed file log in a mapped region + a small key area

The `storage` partition (`0x210000`, 1 MiB) is split by **what the device
does with the bytes**, not in half (Gitea #330):

| partition-relative | absolute | size | region |
|---|---|---:|---|
| `0x00000` | `0x210000` | 128 KiB (32 pages) | **key area** — a `sequential-storage` map |
| `0x20000` | `0x230000` | 896 KiB (224 pages) | **extent region** — mapped read-only at boot (14 × 64 KiB MMU entries) |

`patterns.rs` maps the whole region once at boot (`map_ext`, self-check on
the first page read both ways, leaked) and it holds:

| partition-relative | size | what |
|---|---:|---|
| `0x20000` | 4 KiB | ad-hoc header page: magic, src/bc lengths, bc side |
| `0x21000` | 32 KiB | ad-hoc (live-coding) source |
| `0x29000` | 2 × 64 KiB | ad-hoc bytecode, TWO sides — `store_current` writes the side the running engine is not executing from and flips `CUR_BC_SIDE`, so a mapped engine never sees its code change |
| `0x49000` | **732 KiB** | the **file log**, 183 erase pages |

#### The file log (Gitea #340)

Gitea #330's store allocated in whole 4 KiB **pages** and kept the whole
directory in ONE `sequential-storage` item, whose one-page cap is what held
the store to 32 patterns. Jeremy, #340: *"We should have properly sized
files instead (the size required is known after all) just held sequentially
in memory to exact size … It may mean walking a linked list to get to a
desired file or to enumerate the existing files but that's ok."* Both halves
of that are what the log is:

- a **file** — one stored pattern: header, name, source text, LXBC — takes
  exactly the bytes it needs, rounded only to 4, and the next file starts
  immediately after;
- the log is **self-describing**. There is no directory anywhere. Boot walks
  the headers through the mapping and builds a small RAM index, so the
  pattern count is bounded by *bytes*, not by a table.

Measured on the real library (`cargo test -p patlog-check`, 305 patterns,
median source 2,853 B, median bytecode 2,008 B, 219/305 sources ≤ 4 KiB):

| | patterns that fit | of the 732 KiB log |
|---|---:|---:|
| page-granular (#330) | 32 (its table cap) | 328 KiB, 44.8 % |
| exact-packed (#340) | **119** | 722 KiB, 98.7 % |

Four is both the floor and the ceiling of the alignment tax:
`bytecode::deserialize_lean_static` borrows a blob's word region only when
it is 4-byte aligned in memory (it silently copies otherwise), and
`esp-storage`'s `WRITE_SIZE` is 4 — every flash write is a 4-byte-aligned
offset and a multiple-of-4 length.

The record, all little-endian (`firmware/src/patlog.rs`):

```text
 0  u32 magic       "PXL1"
 4  u32 self_off    this record's own log offset -- the resync anchor
 8  u32 stamp       monotonic; the highest stamp wins for a seq
12  u32 seq         pattern identity (API id = seq ^ ID_MASK)
16  u32 src_len     source text, exact bytes
20  u32 bc_len      LXBC, exact bytes
24  u32 src_hash    FNV-1a of the source
28  u32 bc_hash     FNV-1a of the bytecode
32  u8  name_len | u8 ver | u16 0
36  u32 hdr_hash    FNV-1a of bytes 0..36 ++ the name
--- written LAST, and what makes the record real -------------------
40  u32 commit      COMMIT, or 0xFFFFFFFF while the record is torn
--- written by a delete or a re-save (NOR 1 -> 0, no erase) --------
44  u32 dead        0xFFFFFFFF while live
-------------------------------------------------------------------
48      name bytes, padded to 4
        source bytes, padded to 4
        bytecode bytes, padded to 4   <- 4-aligned, so the VM borrows it
```

`self_off` + `hdr_hash` are what make recovery **local**. The scan walks
record to record by length; when it lands on something that is not a record
it steps forward 4 bytes at a time until it finds one again. A header only
validates at the offset it was written for, so a stale copy a compaction
left behind is still readable at its old home while the new copy is readable
at its new one, and a false positive would have to forge a 32-bit hash over
its own address. **A header whose payload does not hash is one of those
"not a record" cases, length field included** — it describes bytes that are
no longer the ones it was written for, so the walk resyncs through it rather
than stepping over it. Trusting that length is how Gitea #379 lost files: a
frozen page keeps the stale header of a record a compaction just reclaimed,
and its `end()` points straight past the files that were repacked into that
space. `ver` retires every record an older firmware wrote, so a
format change costs no boot-time erase: the scan finds nothing and the first
append erases the pages it lands on.

The key area keeps only what is small, hot, and must survive power loss —
which is what a log-structured map is actually good at. **Patterns are not
in it at all any more, not even their directory:**

| key | bytes | written |
|---|---:|---|
| `FORMAT_KEY` | 4 | once, at a wipe |
| `PLAYLIST_KEY` | ≤ 3,840 | per playlist edit |
| `PLAYSTATE_KEY` | 1 | per play/pause |
| `MAP_KEY` | ≤ 3,840 | per pixel-map upload |
| `RESUME_KEY` | ~16 | per swap (skipped when unchanged) |
| `PALETTE_KEY` | ≤ 64 | per palette edit |

Caps: `MAX_SOURCE` 32 KiB and `MAX_BC` 40 KiB are sanity limits now that a
file is exact-sized (the 16 KiB HTTP request buffer bounds a POST well below
them anyway). There is **no `MAX_PATTERNS`**; `MAX_RECS` (192) is a heap
guard on the RAM index (`Vec<Rec>`, 32 B each, no names — those are read
back out of the mapping), not a format limit, and it sits well past the 119
real patterns the 732 KiB actually holds. The *scan* is uncapped on purpose:
`cursor` must be the end of the last record in the log or an append would
land on top of live ones, so only what the index keeps is capped. A log with
more than `MAX_RECS` distinct patterns leaves the index incomplete, says so
at boot, and refuses every mutation until some are deleted — a compaction
rewrites the log from the index and would drop what it cannot see.

`patlog.rs` is **format only** — the record layout, the boot scan, append
planning and compaction placement. It is `no_std`, allocation-free and
host-tested (`tools/patlog-check`, `cargo test --workspace`, 16 cases
against a NOR simulator that models erase-to-0xFF and write-clears-bits and
cuts power at every 4-byte write boundary of a save, a delete and a
compaction). `patterns.rs` owns every byte of flash I/O.

Rules the code enforces:

- **A library swap carries only the id** (`Msg::Library { id, ms }`): the
  render task decodes from `code_of(id)` (mapped) and stamps identity from
  `source_stat(id)`, which is two header fields — no flash read, no
  allocation. A swap writes nothing at all: the file was written once, at
  save (the 2026-08-15 wear rule). Without the mapping (`flashmap-off`,
  refused self-check) the same bytes are read into one transient `Vec`
  (`bytecode_of`), which is the only path that still copies.
- **A file an engine is executing from is never written, moved or erased**
  — the borrowing invariant, below.
- **A save is an append.** Erase the pages the record will touch (skipping
  the ones already erased), write the header prefix (magic first), the
  name, the source and the bytecode, **re-read the payload through the
  mapping and check both hashes**, and only then write the commit word. A
  cut before that leaves a record the next boot refuses and the previous
  version untouched; a cut after it leaves the new version whole. The old
  version is retired by one 4-byte `dead` write — NOR clears bits without
  an erase, and this build uses plain `FlashStorage`, never the encrypted
  one, so a sub-page byte write is legal.
- **A delete frees nothing**, by design: the erase unit is 4 KiB and a file
  is byte-sized. It writes the `dead` word and the bytes stay, which is also
  what makes it safe to delete a pattern an engine is still executing.
- **Compaction** is the only thing that gives space back, and only a SAVE
  that ran out of room calls it (an activation never compacts, so playlist
  churn stays wear-free). `patlog::plan` places every live file — plus any
  dead one a pinned engine is still executing — at or below its current
  offset, never inside a page a pinned file occupies; nothing ever moves up.
  The executor then rewrites one 4 KiB destination page at a time, in
  ascending order, through a RAM buffer it fills *before* erasing the page:
  every byte a page needs lives at an offset ≥ that page's start, which is
  the whole safety argument for an overlapping repack. Pages a pinned file
  occupies are skipped outright, a page whose new content equals its old one
  is not erased at all, and the stale copies above the new cursor are erased
  last, once every destination page is written. **Bound:** at most one erase
  + one 4 KiB write per page of the packed log plus one erase per swept
  page — 183 + 183 flash ops on a full log, each its own fenced door with a
  1 ms yield; `core1::fenced` feeds the watchdog every 64 fences (#309), so
  a long pass is slow, not fatal.
- **A pinned file does not cost the store the space under it** (Gitea
  #388). The cursor is a high-water mark, so a pin high in the log holds it
  above pages the repack just erased and `patlog::place` — which only ever
  looks *upward* from the cursor — never sees them. `patlog::place_free` is
  the fallback that does: the lowest wholly-erased page run big enough, or
  the exact bytes after the last file placed that way (`FREE_HINT`), so a
  hole packs as densely as the log proper. Its safety argument is one line —
  **erased flash holds no record** — and both branches check every byte the
  append will write *and* every byte its `Erase` step will erase. A save
  tries it before compacting, which also keeps a pinned log from repacking
  itself on every single save. For the same reason `compact()` no longer
  refuses on the tail alone: `patlog::free_run_after` asks what the repack
  would open up *anywhere*, because with a pin holding the cursor high the
  answer is all below it. Before this, a stale pin cut the Athom's store
  from 119 patterns to 49 with 58 % of the arena idle.
- **A compaction is a total function over the live set.** `patlog::plan`
  checks the placement as it builds it — every record placed exactly once
  and in ascending order, no two overlapping, nothing moving up, a pinned
  record at its own address, and a *moved* record never landing in a page
  the executor will skip — and returns nothing rather than a partial plan.
  `compact()` then erases nothing and the save fails loudly. There is no
  "compact what we can" path: silently dropping what a plan could not place
  is exactly Gitea #379.
- **A save that compacts re-resolves the generation it retires.** The save
  looks the previous version up before it knows it needs room; the
  compaction moves every unpinned file; writing the DEAD word to the address
  captured up front would drop four zero bytes into whatever was packed over
  it (#379).
- **A power cut mid-compaction** leaves the log repacked below the frontier
  and untouched above it. Both halves still parse — every header carries its
  own offset — so at most the one file straddling the frontier is lost. The
  host suite cuts power at every write boundary of a compaction and asserts
  exactly that, plus that every surviving file reads back byte for byte.
- **Boot rebuilds every byte of RAM state from the flash** (`reload`, which
  the end of a compaction calls too): the scan re-hashes every payload, so a
  torn file is dropped rather than handed to the engine, and duplicates
  (both versions live because a cut landed between "commit the new" and
  "retire the old") resolve to the highest stamp.
- **An unpinned pattern's mapped bytes are covered by a reader guard.**
  `GET /api/patterns/:id` escapes the source into the response buffer
  straight from the mapping; a save or compaction on the other core would
  otherwise move it mid-copy. `patterns.rs`' `BUSY` counter makes writers
  and unpinned mapped readers exclusive. Readers hold it across a
  *synchronous* copy only (microseconds), so a writer's bounded retry
  converges; the RUNNING pattern needs no guard at all, because it is
  pinned.
- Every log write goes through `ota::with_flash` one op at a time with 1 ms
  yields — the same fenced door as the assets writer — and no single fenced
  op crosses a page boundary (a fence parks the other core; 40 KiB of
  bytecode in one op would park it for the whole write). `erase_pages`
  consults the mapping first and skips pages already erased, so an append
  into a fresh tail erases only what it dirties.
- **No migration path.** `FORMAT_VERSION` (6) wipes the key area on mismatch
  and `patlog::VER` retires every older record in the log; the playground
  re-syncs the library (Jeremy's decision, #330).
- `/api/status` reports `store: {used, total, dead, patterns}` — **bytes**
  now, not 4 KiB pages: bytes used by live files, bytes in the log, bytes
  the next compaction would give back, and how many patterns are stored.
  `total` 0 means the store never came up. `dead` is `cursor − used` —
  everything below the write cursor that the index does not point at, gaps
  included — so it comes back to exactly **0** after a compaction with
  nothing pinned. With something pinned it does NOT: the cursor cannot come
  down past a frozen record, so `dead` still counts everything under it
  (the whole span, not just the frozen page — Gitea #388 measured 364,960 B
  of it on the rig). Since #388 those bytes are *usable* even while they are
  counted, so `dead` under a pin is an upper bound on what a compaction
  would reclaim rather than a measure of lost capacity; it falls back to 0
  on the first compaction after the pin clears.

#### The borrowing invariant and the pin set (Gitea #260)

The engine decodes a mapped pattern with
`luxel_core::bytecode::deserialize_lean_static`, so a running `Program`'s
`words` — its code AND its constant pool — are a `&'static [u32]` **into
the mapping**, not a heap copy. That is where the RAM saving comes from
(the resident program is its header tables: 4–11 KB instead of 12–35 KB
for the big patterns, see docs/research/flash-mmap.md "RAM accounting"),
and it is also a lifetime contract:

> **A `Program` must never outlive the flash it borrows.** No file an
> engine is executing from may be written, moved or erased while it holds
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

A save that compacts the log in either window is a use-after-free, and on
a dual-core board the store runs on the OTHER core — this is not even an
`await`-granularity race, so "no yield point in between" proves nothing.

So `patterns.rs` keeps an explicit **pin set** that the render task
publishes, three slots plus the current pattern id as belt and braces:

| slot | set by | holds |
|---|---|---|
| 0 | `pin_code(id)` — **before** `code_of` | what a decode is about to borrow |
| 1 | `pin_running(id)` — when the new engine is installed | what the live engine borrows |
| 2 | `pin_prev_from_running()` — where `prev = engine.take()` | what the crossfade's outgoing engine borrows |

Slot 0 is released by `unpin_code` at the END of the library-swap arm,
always — the decode window is over by then and whatever it produced is
either installed (slot 1 names it) or dropped. It has to be: a decode pin
that is never released names the last library pattern the device ever
decoded for the rest of the boot, a compaction treats that file as frozen
even after it has been deleted, and the store loses every byte below it.
That is Gitea #388, and it is what the rig was actually stuck on.

Slot 2 is released by `unpin_prev`, and `render_task` never writes
`prev = None` directly: every drop goes through `drop_prev`, which drops
the engine and then releases the pin. An ad-hoc push (`Msg::Code` /
`Msg::Crossfade`) decodes a transient envelope `Vec`, so its `Program` owns
its words and `pin_running("")` clears slot 1.

`compact` consults the set both ways: `patlog::plan` takes a pin *slice*
and leaves a pinned file exactly where it is, and `patlog::frozen_page`
keeps the executor from erasing any page one occupies, so each pinned file
splits the free space instead of blocking the pass. Pins are conservative
by construction — one can never free something live — but a STALE one is
not free: until #388 it cost every byte below it, and even now it costs the
frozen record's own bytes and forces the tail allocator down the
`place_free` path. Release them. A pin names a *pattern*,
so it holds that pattern's SOURCE as firmly as its bytecode: it is all one
file. The running pattern's read-back
(`GET /api/pattern`, the sync envelope) streams the source straight out of
the mapping across `await`s, and the pin is what makes that sound. An
UNPINNED pattern's mapped bytes are covered by the `BUSY` reader guard
instead.

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
- `pipe_us` — `apply_outpipe` (gamma, output palette, blur), plus the
  `/api/pixels` preview copy (`set_pixels`) on boards that keep one.
- `out_us` — `BoardOutput::write_frame`: the SPI/RMT or HUB75 driver. Rises
  with pixel count and falls with DMA; a large value here is a wire-format or
  driver problem, not a pattern problem.
- `frame_us` — the whole engine branch, from the delta math through
  `write_frame` returning. The stages nest, so `frame_us` ≈ the other three
  plus the small per-frame bookkeeping between them (delta/sync math, pin
  sync, vmerr drain).

**On a pipelined board the stages no longer nest** (`pipelined` cfg — see
"The frame pipeline" below). `frame_us` is the render PERIOD on core 1: the
delta math, `Engine::frame`, the crossfade blend, and the hand-off copy —
`vm_us` plus a few hundred microseconds, and no longer `vm + pipe + out`.
`pipe_us` and `out_us` are published by `output_task` from core 0 and are
averages over the frames IT processed, which is `out_fps`, not `fps`. Adding
them to `frame_us` is meaningless there; the frame period is
`max(frame_us, pipe_us + out_us)` instead.

`out_fps` — frames actually written to the wire in the last second — is
published only on a pipelined board, and is 0 elsewhere (every rendered frame
is written by construction). It differs from `fps` exactly when the output
stage is the slower half: at 4096 px an empty render renders 125 frames and
the panel's rescan boundary paces 123 of them onto the glass. Read `fps` as
"what the pattern's motion is computed at" and `out_fps` as "what the panel
showed"; a large gap is wasted render work, not throughput.

Only the **pattern** branch is instrumented. Frames driven by live input
(DDP/E1.31) and idle loops with no engine are not timed, and the divisor is
the count of timed frames in the window — not `FPS`, which counts every loop
iteration. A second with no pattern frame stores 0 in all four.

The hot path costs three extra `Instant::now()` reads and four integer adds
per frame; no formatting, allocation, or float work happens inside the loop.
`frame_us` is wall time inside the branch, so it includes any preemption by
the WiFi/network tasks — compare stages against each other, not against a
theoretical cycle budget.

### `vm_us` is dominated by instruction-cache layout, not by dispatch

On the flash-cached Xtensa parts the largest single term in `vm_us`, for any
pattern that calls a builtin, is whether `Vm::run` (~13 KB) and
`Vm::call_builtin` (~21 KB) both stay resident in the flash instruction
cache. It is worth far more than anything inside the dispatch loop, and it is
**not monotonic in code size** — a kilobyte of movement in `luxel-core` is
worth tens of percent in either direction (Gitea #312/#318, measured on the
Athom):

| change | loop microbench | `perlin-fire-wind-tunnel` |
|---|---|---|
| `debug_stop` out of `Vm::run` (−1.3 KB) | −4.5 % | **−46.5 %** (270.8 → 144.8 µs/px) |
| `binop_const` inlined into the fused arms (+1.3 KB) | −7.5 % | **+38 %** |
| the same, one arm only (+0.5 KB) | −3.1 % | **+57 %** |

So measure an engine change **twice**: `tools/opbench.mjs` for the dispatch
loop and `tools/patbench.mjs` for a real pattern (docs/tools.md). The loop
microbenchmark executes no builtin and therefore cannot see this term at all,
and the two routinely point in opposite directions. Use a *stateless* probe
pattern — `perlin-fire-wind-tunnel` repeats to ±0.3 %, while `snake-2d`'s
per-frame work depends on game state and swung 74 % between two builds a
kilobyte apart.

## Code placement: the interpreter's per-pixel code

The section above says `vm_us` is dominated by instruction-cache layout. This
is what the fleet does about it (Gitea #328, measured 2026-09-06 on master
`e08b2b2`).

**The per-pixel path is a three-tier builtin ladder plus its leaves:**

| tier | function | esp32 bytes | reached by |
|---|---|---:|---|
| 0 | `Vm::run` | 13,170 | every op |
| 1 | `Vm::builtin_fast` | (inlined) | the 22 builtins the dispatch loop answers itself |
| 2 | `Vm::call_builtin` + `Vm::builtin_hot` | 1,980 + 5,095 | the ~30 builtins a `render` calls per pixel — transcendentals, noise, `dist`/`hypot`, `paint`, `canvasGet` |
| 3 | `Vm::builtin_cold` | 14,810 | the other 90 arms: easings, beziers, arrays, transforms, palette/GPIO/clock/canvas-write setters |
| leaf | `hsv_to_rgb`, `fmath::*`, `noise::*` | 183 / 42–1,248 each / 127–2,988 each | whichever the pattern actually calls |

`builtin_cold` is `#[cold] #[inline(never)]` and is reached only through
`builtin_hot`'s `_` arm, so a pattern that never calls an easing never pulls
those 14.8 KB through the cache. The `noise` entry points (`perlin`, `fbm`,
`ridge`, `turbulence`, `simplex2`, `simplex3`) are `#[inline(never)]` for the
same reason: inlined, `simplex2_inner`/`simplex3_inner` land *inside*
`builtin_hot` and every `sin`-only pattern drags 7 KB of noise with it.
The tiering is measured, not guessed — `tools/profile-library.mjs` over all
299 `library/` patterns leaves a **40× gap** between the least-used tier-2 arm
(≥ 0.6 calls/px in the pattern that uses it) and the most-used tier-3 arm
(≤ 0.016 calls/px); tiers 1+2 answer 99.4 % of every builtin call the library
makes.

**Then the hot tiers go into internal SRAM.** `luxel-core` carries three
device-only cargo features that add `#[link_section = ".rwtext"]` — what
`esp_hal::ram` expands to, spelled by hand because `luxel-core` does not
depend on esp-hal:

| feature | what moves | esp32 bytes |
|---|---|---:|
| `iram-vm` | `Vm::run` | 13,572 |
| `iram-builtins` | `call_builtin` + `builtin_hot` | 7,304 |
| `iram-math` | `hsv_to_rgb`, the hot `fmath` (sin/cos in turns and radians, sqrt/isqrt48, hypot, exp2/log2/pow, atan2) and the `noise` entry points | 9,412 |

They are OFF by default and must stay off on hosts. Which ones a board gets is
`IRAM` in `firmware/board-target.sh` (mirrored by `iram` in `flake.nix`'s
`firmwareVariants`); `IRAM_OFF=1` on `build-esp32.sh` / `tools/stack-check.sh`
is the A/B lever. Per-board budget: docs/boards.md.

### What it buys, and why the number is so different per chip

`tools/patbench.mjs`, µs/px, Athom @ 256 px and the Seengreat panel @ 4096 px,
each build OTA'd and measured with `tools/opbench.mjs` in the same run:

| | cycles/op | `rainbow` | `perlin-fire-wind-tunnel` | `kaleidoscope-2d` | `snake` |
|---|---:|---:|---:|---:|---:|
| Athom, no placement | 104.9 | 6.379 | 167.51 | 626.23 | 15.469 |
| Athom, shipped | **100.8** | **6.184** | **58.53** (2.86×) | **150.20** (4.17×) | **14.322** |
| panel, no placement | 84.0 | 4.752 | 45.39 | 102.36 | 11.696 |
| panel, shipped | **83.4** | **4.719** | **44.79** | **101.16** | **11.531** |

The classic ESP32 gains 2.9–4.2× on builtin-heavy patterns; the S3 gains 1 %.
Same code, same features on the dispatch loop — the difference is the part
each chip's flash cache can hold while the render task runs on the second
core. Do not generalise a placement result from one chip to another, and do
not trust `opbench` for any of this: it moved 4 % while `kaleidoscope-2d`
moved 76 %.

### The rule for adding hot code

1. **A function on the per-pixel path belongs in tier 2 or a leaf, never in
   `builtin_cold`, and a function that is not on it must never grow tier 2.**
   Check with `tools/profile-library.mjs`, not intuition.
2. **Big leaves stay `#[inline(never)]`.** Anything over ~500 B that is called
   once per builtin call (noise, the wide fmath) is cheaper as its own symbol:
   the pattern that does not call it does not pay for it.
3. **Adding to `.rwtext` is not free.** On the classic ESP32 IRAM is a
   separate 128 KB region and `.stack` does not move; on the S3 and the
   C-series it is the *same* SRAM as the stack, and every byte comes straight
   out of `.stack` (docs/boards.md). Run `tools/stack-check.sh` for the board.
4. **Measure both benches on both chips before believing anything.** The
   effect is not monotonic in size: on the Athom the hot/cold split *alone*
   was +80 % on `perlin-fire-wind-tunnel` and +73 % on `snake` — a pattern
   that only executes `Vm::run`, which is byte-identical (13,170 B) in every
   build in this table. It is pure placement, and only pinning the code in
   SRAM makes it reproducible.

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

On a **pipelined** board (`pipelined` cfg = `hub75` + `multi_core`) the
ProCpu also runs `pipeline::output_task`, and `render_task` keeps only the
VM — see "The frame pipeline" below.

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

### The frame pipeline (Gitea #306)

Moving the render loop to core 1 left the AppCpu doing two unrelated jobs
back to back: evaluate the pattern, then compose the frame for the wire. On
the 64x64 HUB75 panel the second job is a flat ~6 ms at 4096 px whatever the
pattern does — it walks every pixel and sets seven bitplane bits for it — so
the frame period was `vm + out` on a core that had nothing else to do.

`firmware/src/pipeline.rs` splits the two. The render task hands each
finished RGB frame to an `output_task` on the ProCpu and goes straight back
to the VM; core 0 composes frame N while core 1 renders N+1, and the period
becomes `max(vm, out)`. At 4096 px on the Seengreat panel: rainbow 39 → 52
fps, rgb-only 60 → 87, empty render 99 → 125, 1D snake 19 → 21, 2D snake
12 → 13.

> **Correction (2026-09-07, Gitea #378).** The parenthetical here used to
> read "123 of them reaching the panel". It did not: `out_fps` was counting
> `write_frame` CALLS, refusals included, and a refused call did not retry
> until the render loop came round ~8 ms later, so the compose kept missing
> the rescan boundary. The panel was showing roughly **60** of those 125.
> Every figure in this paragraph above ~60 is a COMPOSE rate. Vsync pacing
> (#387) makes the fast rows land at the rescan rate for real, ~112 of a
> 115 Hz rescan; the render-bound rows (rainbow, the snakes) are unaffected
> because they were never near the ceiling. docs/boards.md "Vsync: the panel
> is the clock" has the measurements.

| stays on core 1 | moves to core 0 |
|---|---|
| `Engine::frame`, the crossfade blend, the pattern clock, message drain, playlist pre-flight, `vm_us`/`frame_us` | `apply_outpipe` + its gamma/palette caches, the brightness read, `BoardOutput::write_frame`, the driver's `resize`, `pipe_us`/`out_us`/`out_fps` |

- **One buffer, borrowed not owned.** A single frame buffer travels in a
  `BlockingMutex<CriticalSectionRawMutex>` cell announced by a `Signal`. The
  render task takes it, fills it, publishes it and holds nothing between
  frames. Without vsync it never blocks: if the output task still has the
  buffer when the next frame is ready, that frame is dropped — the same
  best-effort contract `Hub75Output::write_frame` already had with the DMA
  swap — and a frame published over an unclaimed one replaces it; newest
  wins.
- **Under vsync that buffer IS the clock** (Gitea #387, on any driver whose
  `paces_frames()` says it has a display clock — today, the panel). The
  output task holds each frame until the previous DMA swap has landed, and
  the render task's `emit` waits for the buffer to come back instead of
  dropping the frame, taking only a genuinely free one rather than stealing
  its own unclaimed frame back. The result is exactly one frame composed,
  swapped and displayed per panel rescan: `fps`, `out_fps` and `rescan_hz`
  converge, and nothing is rendered only to be thrown away. The VM still
  overlaps the compose, because the render task waits at `emit` — after the
  pattern has run — rather than before it, which is why a render-bound
  pattern keeps the full `max(vm, out)` win above. `DROPPED` should stay at
  zero on such a board. Both waits give up after 50 ms so a dead panel
  degrades the frame rate instead of freezing the engine.
- **The frame is copied, not swapped.** `Engine::frame` may legitimately
  return the PREVIOUS frame (a pattern under its own `frameRate` cap), so
  alternating the engine's own pixel buffer would show a stale frame on
  those. A 12 KB memcpy at 4096 px is ~50 µs against a 5–77 ms frame.
- **It costs no RAM.** A pipeline needs one more live frame than a serial
  loop does, and at 4096 px that 12 KB is not there to spare (the 2D snake
  sits ~8 KB above `RUNTIME_FLOOR`; the first cut of this change made it fail
  the floor check outright). It is paid for by deleting the frame it
  replaces: `shared::PIXELS`, the snapshot `GET /api/pixels` serves, was a
  second full copy of the frame that had just been composed. On a pipelined
  build the travelling buffer IS that snapshot — `pipeline::preview` reads it
  out of the slot whenever it is parked there — and `set_pixels` is never
  called. Measured `heap_free` is identical to the pre-pipeline build row for
  row, and `pipe_us` fell from ~44 to ~11 µs with the memcpy gone.
- **`preview` is ONE fallible allocation, reserved outside the critical
  section.** This is a response built on a heap that a 4096 px pattern can
  leave under 30 KB free, so a second 12 KB temporary is the difference
  between serving the preview and an OOM panic — the first cut of #306 made
  two and panicked the panel under `library/snake-2d.js` (caught on serial:
  `memory allocation of 12288 bytes failed`, software reset, and after a few
  of them a boot-guard rollback). Allocating inside the slot's critical
  section would also stall BOTH cores on the allocator's own lock. On failure
  it answers an empty body, which is what the snapshot already returned
  before the first frame; the pre-#306 `get_pixels()` was an infallible
  `Vec::clone` made inside that same kind of critical section.
- **The fence needs nothing new here**, but it did need generalizing: it now
  waits for an in-flight output DMA transfer whichever core it parks, not
  only the AppCpu, because which core runs the driver is a build-time
  question now. Nothing in the pipeline touches flash, and its two critical
  sections are a handful of pointer moves, so a park request lands between
  them at worst; the output task waits on a `Signal`, never on a spin.

**Why strips are NOT pipelined.** The gate is `hub75`, not `multi_core`, and
that is a measurement, not caution. A strip's `out_us` is *wire* time, not
CPU work: `write_frame` busy-waits on an SPI DMA transfer whose length the
WS2812/SK9822 protocol fixes. Measured on the Athom at 420 px with the gate
widened to `multi_core`:

| | wire rate (`out_fps`) | bundle download | worst fence park wait |
|---|---|---|---|
| serial (shipped) | 59 fps | 1.29 s | 96 µs |
| pipelined | **60 fps** | **2.60 s** | **6,223 µs** |

`fps` reads 123 on the pipelined row — the render task free-running at its
pacing cap, producing twice the frames the wire can take — which is exactly
the illusion `out_fps` exists to puncture. The +1 fps buys a 2× slower web
server, because the ProCpu now spends the frame busy-waiting on DMA: the
Gitea #259 starvation the second-core split was built to fix. The win a strip
wants is to overlap the DMA with the VM on the SAME core (start the transfer,
wait for it at the top of the next `write_frame`, the way `Hub75Output`
already handles its swap) — no second core, no extra RAM, and it works on the
single-core chips too. That is Gitea #343.

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
