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
pattern's **code arena** slot for a library pattern. Layout of the raw half
(partition-relative): header page at `0x80000`; ad-hoc source (96 KiB) at
`0x81000`; ad-hoc bytecode as TWO 64 KiB sides at `0x99000` / `0xA9000` —
`store_current` writes the side the running engine is not executing from
and flips `CUR_BC_SIDE`, so a mapped engine never sees its code change; the
arena at `0xB9000`: 7 slots × 40 KiB (≥ the 38 KB library bytecode cap),
one stored pattern's contiguous, 4-byte-aligned LXBC each, with the slot
table (seq, bytecode generation, length, FNV-1a) under the reserved map key
`ARENA_KEY`. Rules the code enforces:

- A **library swap carries only the id** (`Msg::Library { id, ms }`): the
  render task decodes from `code_of(id)` (mapped) or, without a slot, from a
  transient `bytecode_of` Vec that it then offers to a FREE or stale slot
  (`cache_code(.., evict = false)`), so playlist churn writes flash at most
  7 times per library state and then never. `save()`'s caller
  (`POST /api/patterns`) fills with `evict = true` — a user action may
  reclaim the least-recently-activated slot. Identity and read-back lengths
  come from `source_stat(id)`, which streams the source out of its chunks
  (no source Vec, no envelope Vec anywhere in a library activation).
- **The running pattern's slot is never written or evicted** (eviction
  skips `running_seq()`); a re-save of the running pattern goes to another
  slot and the old one becomes stale (generation mismatch) — reclaimable
  once something else is running.
- Every arena write is `write_raw` (erase + word-aligned page writes, one
  `ota::with_flash` per op with yields — the same quiesce path as the assets
  writer, so the second-core fence hook lands in one place), then
  `flashmap::invalidate_slice`, then a hash check of the mapped bytes, then
  the RAM table, then the persisted table. Boot drops any table entry whose
  pattern/generation is not in the index or whose mapped bytes do not hash
  to the recorded value — a torn slot is never handed to the engine.
- Without the mapping (`flashmap-off`, refused self-check) the arena is off
  (`patterns: code arena off`), `current_code()` is `None`, and every path
  reads through a Vec exactly as before.

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
