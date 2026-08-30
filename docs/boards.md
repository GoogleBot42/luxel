# Boards

Luxel targets ESP32-class chips through [esp-hal]. A *board* is a cargo
feature that picks the chip, names the hardware, sets sane strip defaults,
and wires the handful of pins that differ between products. Everything else
(protocol, pixel count, color order, gamma, power cap) is a runtime setting.

Build any board with:

```sh
cd firmware
# any board — the chip, rust target and toolchain come from $BOARD
# (firmware/board-target.sh); Xtensa boards pick up the Espressif fork
# from the nix devshell automatically:
BOARD=board-pixelblaze-v3 ./build-esp32.sh          # build only
BOARD=board-esp32-generic ./build-esp32.sh flash    # flash app + web assets
BOARD=board-c6-devkit ./build-esp32.sh              # RISC-V, mainline rustc

# the default board (C3) is also just plain cargo:
cargo build --release --no-default-features --features board-c3-devkit
```

Hermetic images (no devshell needed) come from the flake — one package per
board: `nix build .#luxel-fw-pixelblaze-v3` (also `luxel-fw-c3-devkit`,
`luxel-fw-athom-music`, `luxel-fw-esp32-generic`, `luxel-fw-s3-devkit`,
`luxel-fw-c6-devkit`, `luxel-fw-s3-hub75`,
`luxel-fw-seengreat-hub75`); see docs/firmware.md for the
credential-baking caveats.

## Supported boards

| feature | chip | strip pins | defaults | pixel cap | status | notes |
|---|---|---|---|---|---|---|
| `board-c3-devkit` (default) | ESP32-C3 | CLK GPIO6, DATA GPIO7 | SK9822, 60 px | 2048 | supported (hardware-verified) | bare devkit |
| `board-pixelblaze-v3` | ESP32 | CLK GPIO18, DATA GPIO23 | SK9822, 300 px | 2048 | supported (the dev unit) | official PB v3 Standard schematic; onboard 5 V level shifter; status LED GPIO12 (lit at boot = Luxel alive); button GPIO32 (unused) |
| `board-athom-music` | ESP32 | CLK1 GPIO5, DATA1 GPIO18 | WS2812, 60 px | 2048 | builds, untested on hardware | Athom music-reactive WLED controller — demoted from bench hardware, config stays maintained; strip-VCC relay on GPIO2 must be driven high or the strip stays dark; channel 2 + mic + IR unused for now |
| `board-esp32-generic` | ESP32 | CLK GPIO18, DATA GPIO23 | WS2812, 60 px | 2048 | builds, untested on hardware | VSPI defaults — most WROOM/DevKitC boards break these out |
| `board-s3-devkit` | ESP32-S3 | CLK GPIO12, DATA GPIO11 | WS2812, 60 px | 2048 | **builds, UNTESTED ON METAL** | ESP32-S3-DevKitC-1; SPI2/FSPI IO_MUX pins (direct DMA route), clear of the octal-PSRAM pins GPIO33–37 |
| `board-c6-devkit` | ESP32-C6 | CLK GPIO6, DATA GPIO7 | WS2812, 60 px | 2048 | **builds, UNTESTED ON METAL** | ESP32-C6-DevKitC-1; SPI2/FSPI IO_MUX pins (same numbers as the C3 by coincidence of the IO_MUX tables), clear of the onboard RGB LED on GPIO8 |
| `board-s3-devkit` + `hub75` | ESP32-S3 | HUB75 (14 pins, `board::hub75_pins!`) | HUB75 64x64 panel, 4096 px | **4096** | **builds, UNTESTED ON METAL** | LCD_CAM + circular-DMA BCM rescan via patched esp-hub75 (firmware/patches/); pin map = the esp-hub75 S3 example's (a panel on jumper wires); strip SPI not wired at all; protocol switches rejected (fixed wire format); nix variant `luxel-fw-s3-hub75` |
| `board-seengreat-hub75` | ESP32-S3 | HUB75 (14 pins, `board::hub75_pins!`) | HUB75 64x64 panel, 4096 px | **4096** | **builds, UNTESTED ON METAL** | Seengreat "RGB Matrix HUB75 S3" (ESP32-S3-WROOM-1-N16R8): a purpose-built panel driver board, so the feature turns `hub75` on itself. Pin map transcribed from the [vendor wiki](https://seengreat.com/wiki/214/) — R1 IO5, G1 IO4, B1 IO6, R2 IO15, G2 IO7, B2 IO17, A IO8, B IO18, C IO10, D IO9, E IO16, CLK IO12, LAT IO11, OE IO13; both panel outputs (ribbon + plug-in header) share those pins. Codec/mics (Gitea #142), microSD, RTC and PSRAM unused (see below); nix variant `luxel-fw-seengreat-hub75` |

All eight combos build clean (verified compile + image-size check +
`tools/image-check.sh` + `tools/stack-check.sh`). "Untested on hardware"
means the wiring is reviewed against the vendor pinout but the board has
never been lit up; the S3/C6 rows go further — **no S3 or C6 exists on the
bench at all**, so nothing beyond "it compiles, links, fits the OTA slot
and keeps a sane stack" has been established. Treat their pin choices,
heap sizing and radio behaviour as unverified — hardware bring-up is
tracked in Gitea #56, and the Seengreat board plus its 64x64 panel in
Gitea #75 (which also owns the first real FPS and heap-floor numbers at
4096 px; nothing in this file has been measured on a panel). Both
protocols run over
SPI: SK9822/APA102 uses CLK+DATA; WS281x uses DATA only (encoded
bitstream), so a WS2812 board simply leaves CLK unconnected — the pin
still gets claimed.

Two classic-ESP32-only wiring bits are deliberately *not* extended to the
new boards: the PB sensor-expansion UART (GPIO3, `#[cfg(feature =
"esp32")]` in main.rs — devkits have no such header) and the Athom strip
relay.

## The 1 MiB OTA-slot ceiling

The partition table (firmware/partitions.csv) is pure A/B with 1 MiB
(1,048,576-byte) app slots, so the app image — what `espflash save-image`
emits and `/api/ota` writes — must stay under that or OTA rejects it
(crossed once at v0.1.17; opt-level "s" bought it back — history and diet
options in docs/size-report.md). Per-board app images at v0.1.39,
remeasured 2026-08-29 (devshell builds with WiFi creds baked in — a
credless build strips the WiFi stack and reads ~1.5 KB smaller, which is
what CI measures):

| board | app image | slot margin |
|---|---:|---:|
| `board-c3-devkit` | 905,344 B | 143,232 B |
| `board-pixelblaze-v3` | 949,888 B | 98,688 B |
| `board-athom-music` | 949,696 B | 98,880 B |
| `board-esp32-generic` | 949,760 B | 98,816 B |
| `board-s3-devkit` | 890,640 B | 157,936 B |
| `board-s3-devkit` + `hub75` | 885,872 B | 162,704 B |
| `board-seengreat-hub75` | 885,872 B | 162,704 B |
| `board-c6-devkit` | 997,344 B | **51,232 B** |

Re-measured again later the same day, after the global post-process chain:
+2.8–3.6 KB on every board, evenly (the engine's chain stages plus the v7
config record and the wider `/api/output` handlers).

The three classic-ESP32 variants differ only by a few hundred bytes (same
chip feature set; only board.rs strings and the wiring lines change), so
checking one of them per release is enough — but the *chips* are not
interchangeable for size purposes: the C6 is ~93 KB fatter than the C3 for
identical source (bigger radio blob / riscv32imac codegen), and at 4.9% it
now owns the tightest margin in the fleet. It is the board that will hit
the 1 MiB ceiling first; check `board-c6-devkit` on any release that grows
the image. Measure with:

```sh
# chip/target for $BOARD come from firmware/board-target.sh
espflash save-image --chip esp32c6 \
  target/riscv32imac-unknown-none-elf/release/luxel-fw /tmp/ota.bin && stat -c %s /tmp/ota.bin
```

`.stack` (the leftover-DRAM main-task stack, `tools/stack-check.sh`) at
the same revision: pixelblaze-v3 29,244 B · athom-music 29,348 B ·
esp32-generic 29,324 B · c3-devkit 39,568 B · s3-devkit 51,108 B
(50,500 B with `hub75`, and the same 50,500 B for
`board-seengreat-hub75` — the delta is the DMA descriptor static; the
two ~28 KB framebuffers are heap-leaked at boot, not statics) ·
c6-devkit 141,256 B — all above the 24 KB floor, and no function frame
over the 12 KB budget on any of them. The
S3/C6 numbers come from reusing the C3's 160 KB heap on chips with more
DRAM; when hardware exists, the right follow-up is to spend some of that
slack on heap (pattern capacity) rather than leave it as stack. That is
now more than a nicety on the panel boards: at 4096 px the per-frame
buffers alone are ~48 KB of heap (see the pixel-cap section), so the S3's
~26 KB of surplus stack is the obvious place to find it — measured on
metal in #75, not guessed at here.

## Pixel caps are per board

`board::MAX_PIXELS` is the hard ceiling on a runtime pixel count — what
`/api/config` validates against, what `/api/status` reports as
`max_pixels`, and what the render task clamps to. It is **per board**
(Gitea #74), not one global constant:

- **strip boards: 2048.** A 4096-px WS2812 encode buffer alone is ~36 KB,
  which the classic ESP32's 80 KB heap cannot carry alongside the WiFi
  blob. Raising it globally would turn a clean "pixels must be 1..=N"
  rejection into a heap-exhaustion crash.
- **HUB75 panel boards: 4096**, because a 64x64 panel *is* 4096 pixels and
  anything less renders the bottom rows black. The panel path never builds
  an encode buffer at all — the driver owns two bitplane framebuffers,
  allocated once at boot — so the extra 2048 pixels cost only the
  per-frame RGB buffers.

A `const` assertion in board.rs fails the build if a panel's area ever
exceeds its board's cap, so the half-dark panel that shipped between #72
and #74 cannot come back silently.

Heap cost at 4096 px, by inspection (each buffer is 3 B/px and grows to
the active pixel count): the engine's frame buffer, the crossfade blend
buffer, the outpipe wire buffer and the `/api/pixels` snapshot — ~12 KB
each, ~48 KB together — on top of the panel's two ~28 KB framebuffers.
Against the S3's 224 KB of configured heap that leaves roughly 70 KB for
WiFi plus pattern arrays, which the budgeted-engine machinery
(`luxel_core::budget`) polices exactly as it does on a strip: a pattern
that doesn't fit is rejected with a vmerr, never a panic. **These are
arithmetic, not measurements** — real `heap_free` and FPS at 4096 px are
#75's job.

The playground reads the cap from `/api/status`'s `max_pixels` on every
poll (falling back to `/api/config`'s `max` for older firmware), so the
editor's pixel control clamps to whatever board is actually connected.
`web/tools/maxpixels-e2e.mjs` is the regression check.

## Big-flash and PSRAM modules (the Seengreat board)

The Seengreat board carries an ESP32-S3-WROOM-1-**N16R8**: 16 MB of flash
and 8 MB of octal PSRAM. Luxel uses neither, deliberately.

**Flash: the standard 4 MB `firmware/partitions.csv` stays** (decision for
Gitea #73). A 16 MB module runs it fine — the last 12 MB is simply
unallocated. Growing the table would buy nothing today and cost real
complexity: the OTA app slots are capped at 1 MiB by the tripwire above
either way, the storage partition (1 MB) is nowhere near full, and the
assets partition (0xF0000 = 983,040 B) currently holds a 641 KB bundle
with ~35% headroom. Against that, a second table would need a per-board
partition file threaded through `build-esp32.sh`, `flake.nix`, the
release workflow, `build.rs`'s `esp-idf-part` serialization *and*
`src/takeover.rs` (which writes the table during a WLED takeover) — and
would fork the "one image, one layout" property that makes OTA and the
installer page simple. Revisit only when something actually needs the
space — Gitea #143 records the conditions that would justify it.

**PSRAM is not initialised.** Nothing in the current firmware wants it:
DMA framebuffers must live in internal SRAM regardless, and the engine's
hot per-frame buffers would be slower on PSRAM than in DRAM. Its one
plausible use is the same one already noted for WROVER modules — a
dedicated arena for large pattern arrays, letting the array budget grow
without touching the DRAM heap. That stays a future idea (docs/ideas.md),
not a v1 requirement.

## Beyond the current boards: chip-support assessment (2026-07-29)

What a chip actually needs to run Luxel, derived from the v0.1.34
memory accounting (per-allocation profiling + on-device validation, see
UPDATES.md v0.1.34):

- **WiFi.** Not negotiable — without it there is no web UI, no OTA, no
  MQTT, no sync; that isn't meaningfully Luxel.
- **~230–240 KB of usable data RAM.** Baseline statics (~83 KB) + main
  stack (30 KB, the v0.1.33 lesson) + WiFi blob (~50 KB heap) + web pool
  (~50 KB heap+static at 3 slots) + the 20 KB runtime floor + room for a
  modest pattern. Chips above ~300 KB run most of the library; the full
  322-pattern library (Music Sequencer V3 included) is proven on the
  classic ESP32's 520 KB as of v0.1.34.
- **SPI.** Both LED protocols run over SPI (no RMT dependency) — every
  variant qualifies.
- **4 MB flash.** UNCHANGED by any RAM relaxation: A/B OTA alone is
  2 MB, and the storage partition became load-bearing in v0.1.34 (the
  current-pattern read-back slot lives there). 2 MB variants are out.

Pattern capacity is a per-chip quality tier, not a support gate: the
budgeted engine + floor check + "pattern too large" vmerr + playlist
pre-flight mean a smaller chip *rejects giants cleanly* instead of
crashing. That machinery is what makes the lower tiers cheap to support.

| tier | chips | assessment |
|---|---|---|
| 1 — supported today | ESP32 (classic), C3 | Classic: full library, both bench boards. C3: already a board feature; unified SRAM means no instruction/data-bus split, so despite 400 vs 520 KB total it's the *more* comfortable target (224 KB heap configured vs the classic's 176). |
| 2 — **shipped 2026-08-22, untested on metal** | S3, C6 | `board-s3-devkit` / `board-c6-devkit` exist as of v0.1.39. The claim above ("board-feature diffs + toolchains we already have") held: no firmware logic changed, but the *build* plumbing did — build-esp32.sh and stack-check.sh had the classic-ESP32 chip/target/toolchain hardcoded and now share `firmware/board-target.sh`, and the flake needed the `riscv32imac` target for the C6. S3 (512 KB, cheap ubiquitous modules, optional PSRAM) is the "recommended hardware" pick for new builds; C6 is the C3 successor. Still no bench hardware: images build, fit the slot and link every load-bearing feature, and nothing more is known. |
| 3 — works, giants reject | S2 | 320 KB clears the baseline with room for small/medium patterns; the heavy tail of the library rejects cleanly. Single-core is fine (the firmware is one async executor). |
| 4 — experimental only | C2/ESP8684 (4 MB-flash variants only) | ~272 KB total leaves ~20 KB pattern headroom even with the small-chip profile (web pool 2, tuned WiFi buffers — see below). Runs the simple tier of the library. Only worth it with a concrete product reason. |
| no | H2, P4 | H2 has no WiFi (802.15.4/BLE only). P4 has no radio at all and the C6-companion path doesn't exist in bare-metal Rust yet. Neither is a RAM problem, so no tuning changes the answer. Watch: C5 (5 GHz), once esp-hal support matures. |

### The `small-chip` profile (tiers 3–4)

`small-chip` is a cargo feature, not a board — combine it with a board
feature to build the RAM-constrained profile:

```
EXTRA_FEATURES=small-chip BOARD=board-athom-music firmware/build-esp32.sh
EXTRA_FEATURES=small-chip BOARD=board-athom-music tools/stack-check.sh
```

It bundles three things, all `cfg`-gated so the default build is byte-for-byte
unaffected:

| knob | default | small-chip | why |
|---|---|---|---|
| `server::WEB_TASK_POOL_SIZE` | 3 | 2 | each slot is ~8.6 KB of static task arena (picoserve's whole response-path future) |
| esp32 `heap_allocator!` | 80 KB | 88 KB | banks the freed arena as heap; keeps `.stack` in the measured ~30 KB zone |
| `ControllerConfig` RX pools | static **6** / dynamic 32 / AMPDU RX on | static 4 / dynamic 16 / AMPDU RX off | the WiFi blob's static RX buffers are ~1.6 KB each, allocated in `esp_wifi_init` and never freed |

**Measured on the Athom rig (idle `heap_free`, v0.1.39, 2026-08-22):**
stock-pool default 98,352 → small-chip 115,548 → small-chip + WiFi tuning
**125,460** (+27.1 KB total, of which **+9.9 KB is the WiFi tuning** — an
A/B of the two small-chip builds). Nearly all of the WiFi share is
`static_rx_buf_num` 10→4; the dynamic pools and AMPDU buffers are
on-demand, so capping them bounds the worst case but reclaims almost
nothing at idle. Don't push `static_rx_buf_num` below 4 without a fresh
soak — the blob's allocations do not null-check, so an undersized pool
under load is a StoreProhibited crash, not a clean error.

**The default build now takes the mild half of that trim too**
(`static_rx_buf_num` 10→6 and nothing else — AMPDU RX stays on and the
dynamic pool stays at 32, so RX behavior on a busy network is unchanged
and only the never-freed idle reservation shrinks). A/B on the Athom,
same day: **98,352 → 104,832 idle `heap_free`, +6,480 B**, which is
exactly 4 × ~1.62 KB. Soak evidence in the UPDATES.md entry (hw-bench
321/322, 44 k DDP frames alongside a 6-way API hammer with serial
attached, cold loads at parity). Two consequences for the numbers above:
the whole profile is now worth **+20.6 KB over the default** (104,832 →
125,460) rather than +27.1 KB, and the WiFi part of that is down to the
last two static buffers (~3.2 KB) — the rest moved into the baseline.
Re-run `tools/rx-stress.mjs` before changing either number.

**Accepted costs**, both measured on the Athom under this profile:

- **~10% of cold browser navigations are refused** (18/20 clean over two
  `web/tools/coldload.mjs` runs; the failure is `ERR_CONNECTION_REFUSED`
  on the navigation itself, before any body). Chromium wants ~3 sockets at
  a cold nav and the pool has 2 — this is the known, deliberate tradeoff
  from the 2026-08-15 pool decision, not an RX-buffer effect. A reload
  always succeeds.
- **Concurrency beyond ~2 in-flight HTTP requests is refused, not queued**
  (a 6-worker API hammer got 1,605 served and 5,841 refused over 180 s,
  with *zero* body-level failures). Sustained throughput is fine; parallel
  fan-out is not.

Everything else held: 321/322 hw-bench (identical to the default build —
the one failure is a pattern-side array OOB), 44 k DDP frames at 245 pkt/s
× 300 px concurrent with the API hammer, a 629 KB streaming asset upload,
`heap_free` floor 99 KB, no panic and no boot-loop rollback.

Follow-ups tracked in docs/ideas.md ("Small-chip profile + more board
features"): WROVER PSRAM as an array arena for the classic line. (The
S3/C6 board features are done — see the tier-2 row — and so is the
small-chip profile, documented just above.)

## Adding a board (a five-minute diff)

Three files, no other code paths involved — plus a one-line case in
`firmware/board-target.sh` if the board is a chip we don't build yet
(that file is the single board → chip / rust target / toolchain map,
shared by build-esp32.sh and tools/stack-check.sh):

1. **`firmware/Cargo.toml`** — add the feature, selecting the chip:

   ```toml
   [features]
   board-my-thing = ["esp32"]      # or ["esp32c3"]
   ```

2. **`firmware/src/board.rs`** — add the identity block:

   ```rust
   #[cfg(feature = "board-my-thing")]
   mod def {
       use super::*;
       pub const NAME: &str = "My Thing rev A";
       pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
       pub const DEFAULT_PIXEL_COUNT: u32 = 60;
   }
   ```

   Also add the feature to the two `#[cfg(...)]` lists at the bottom of the
   file (the `compile_error!` guard and the `pub use def::*;` gate).

3. **`firmware/src/main.rs`, the `BOARD WIRING` section** — the only
   pin-specific code in the tree. At minimum the SPI pins:

   ```rust
   #[cfg(feature = "board-my-thing")]
   let spi = spi.with_sck(p.GPIO18).with_mosi(p.GPIO23);
   ```

   Anything the board needs held at a level to function goes here too,
   *before* rendering starts — see the Athom strip-power relay or the PB v3
   status LED for the pattern:

   ```rust
   #[cfg(feature = "board-my-thing")]
   let _relay = esp_hal::gpio::Output::new(
       p.GPIO2, esp_hal::gpio::Level::High,
       esp_hal::gpio::OutputConfig::default(),
   );
   ```

   A **HUB75 panel board** skips step 3 entirely: main.rs has one wiring
   line for every panel board (`board::hub75_pins!(p)`) and the pin map is
   an arm of that macro back in board.rs, next to the def block. Such a
   board also enables the driver from its own feature —
   `board-my-panel = ["esp32s3", "hub75"]` — so nothing has to be passed
   at build time.

Then build it (`BOARD=board-my-thing ./build-esp32.sh`, whatever the chip)
and add a row to the table above. If the board should also get a hermetic
`nix build` image and a release artifact, add a `luxel-fw-my-thing` entry
to `firmwareVariants` in flake.nix (a four-line attrset — copy a
neighbor) and its short name to the board loop in
`.github/workflows/release.yml`. A *new chip* additionally needs its
rustup target in the flake (both the devshell's `targets` list and
`riscvRust`) and its chip-feature block in firmware/Cargo.toml.

The installer page (web/flash.html) has its own board list in
`web/src/flash/lib/releases.ts` — it is a WLED-takeover flow, so only add
boards there that correspond to real WLED products, and re-run
`web/tools/flash-e2e.mjs`. Unknown board ids in a release manifest are
skipped by the page on purpose, so leaving a board out is safe.

If the board's output has a different pixel ceiling than a 2048-px strip
(a panel, say), give it a `MAX_PIXELS` arm in board.rs too — see "Pixel
caps are per board" above.

Pins are esp-hal *types*, not data — that's why wiring lives in code behind
`cfg` rather than in the `def` table (the HUB75 map is a macro for the same
reason). Defaults only seed the first boot;
after that the persisted settings win, so picking the "wrong" default
protocol or count is harmless.

[esp-hal]: https://github.com/esp-rs/esp-hal
