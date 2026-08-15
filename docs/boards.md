# Boards

Luxel targets ESP32-class chips through [esp-hal]. A *board* is a cargo
feature that picks the chip, names the hardware, sets sane strip defaults,
and wires the handful of pins that differ between products. Everything else
(protocol, pixel count, color order, gamma, power cap) is a runtime setting.

Build any board with:

```sh
cd firmware
# RISC-V (ESP32-C3) — plain cargo, mainline Rust:
cargo build --release --no-default-features --features board-c3-devkit

# Xtensa (classic ESP32) — Espressif toolchain via the nix devshell:
BOARD=board-pixelblaze-v3 ./build-esp32.sh          # build only
BOARD=board-esp32-generic ./build-esp32.sh flash    # flash app + web assets
```

Hermetic images (no devshell needed) come from the flake — one package per
board: `nix build .#luxel-fw-pixelblaze-v3` (also `luxel-fw-c3-devkit`,
`luxel-fw-athom-music`, `luxel-fw-esp32-generic`); see docs/firmware.md for
the credential-baking caveats.

## Supported boards

| feature | chip | strip pins | defaults | status | notes |
|---|---|---|---|---|---|
| `board-c3-devkit` (default) | ESP32-C3 | CLK GPIO6, DATA GPIO7 | SK9822, 60 px | supported (hardware-verified) | bare devkit |
| `board-pixelblaze-v3` | ESP32 | CLK GPIO18, DATA GPIO23 | SK9822, 300 px | supported (the dev unit) | official PB v3 Standard schematic; onboard 5 V level shifter; status LED GPIO12 (lit at boot = Luxel alive); button GPIO32 (unused) |
| `board-athom-music` | ESP32 | CLK1 GPIO5, DATA1 GPIO18 | WS2812, 60 px | builds, untested on hardware | Athom music-reactive WLED controller — demoted from bench hardware, config stays maintained; strip-VCC relay on GPIO2 must be driven high or the strip stays dark; channel 2 + mic + IR unused for now |
| `board-esp32-generic` | ESP32 | CLK GPIO18, DATA GPIO23 | WS2812, 60 px | builds, untested on hardware | VSPI defaults — most WROOM/DevKitC boards break these out |

All four build clean as of v0.1.28 (verified compile + image-size check;
"untested" above means the wiring is reviewed but the board has not been
lit up). Both protocols run over SPI: SK9822/APA102 uses CLK+DATA; WS281x
uses DATA only (encoded bitstream), so a WS2812 board simply leaves CLK
unconnected — the pin still gets claimed.

## The 1 MiB OTA-slot ceiling

The partition table (firmware/partitions.csv) is pure A/B with 1 MiB
(1,048,576-byte) app slots, so the app image — what `espflash save-image`
emits and `/api/ota` writes — must stay under that or OTA rejects it
(crossed once at v0.1.17; opt-level "s" bought it back — history and diet
options in docs/size-report.md). Per-board app images at v0.1.28:

| board | app image | slot margin |
|---|---:|---:|
| `board-c3-devkit` | 838,416 B | 210,160 B |
| `board-pixelblaze-v3` | 889,376 B | 159,200 B |
| `board-athom-music` | 889,248 B | 159,328 B |
| `board-esp32-generic` | 889,232 B | 159,344 B |

The Xtensa variants differ only by a few hundred bytes (same chip feature
set; only board.rs strings and the wiring lines change), so checking one
classic-ESP32 board per release is enough. Measure with:

```sh
espflash save-image --chip esp32 \
  target/xtensa-esp32-none-elf/release/luxel-fw /tmp/ota.bin && stat -c %s /tmp/ota.bin
```

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
| 2 — add next (follow-up) | S3, C6 | Board-feature diffs + toolchains we already have. S3 (512 KB, cheap ubiquitous modules, optional PSRAM) is the "recommended hardware" pick for new builds; C6 is the C3 successor. No bench hardware — would ship as "builds, untested on metal". |
| 3 — works, giants reject | S2 | 320 KB clears the baseline with room for small/medium patterns; the heavy tail of the library rejects cleanly. Single-core is fine (the firmware is one async executor). |
| 4 — experimental only | C2/ESP8684 (4 MB-flash variants only) | ~272 KB total leaves ~20 KB pattern headroom even with the small-chip profile (web pool 2, tuned WiFi buffers — ideas.md). Runs the simple tier of the library. Only worth it with a concrete product reason. |
| no | H2, P4 | H2 has no WiFi (802.15.4/BLE only). P4 has no radio at all and the C6-companion path doesn't exist in bare-metal Rust yet. Neither is a RAM problem, so no tuning changes the answer. Watch: C5 (5 GHz), once esp-hal support matures. |

Follow-ups tracked in docs/ideas.md ("Small-chip profile + more board
features"): `board-s3-devkit` / `board-c6-devkit`, the small-chip
profile for tier 3–4, and WROVER PSRAM as an array arena for the
classic line.

## Adding a board (a five-minute diff)

Three files, no other code paths involved:

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

Then build it (`BOARD=board-my-thing ./build-esp32.sh` for Xtensa, or
`cargo build --no-default-features --features board-my-thing` for a C3-class
chip) and add a row to the table above. If the board should also get a
hermetic `nix build` image, add a `luxel-fw-my-thing` entry to
`firmwareVariants` in flake.nix (a four-line attrset — copy a neighbor).

Pins are esp-hal *types*, not data — that's why wiring lives in code behind
`cfg` rather than in the `def` table. Defaults only seed the first boot;
after that the persisted settings win, so picking the "wrong" default
protocol or count is harmless.

[esp-hal]: https://github.com/esp-rs/esp-hal
