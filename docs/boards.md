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
