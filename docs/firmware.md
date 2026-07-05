# Firmware targets & bring-up

## Supported chips

| feature (Cargo)  | chip | arch | toolchain | boards |
|---|---|---|---|---|
| `esp32c3` (default) | ESP32-C3 | RISC-V | mainline Rust (in the flake) | bare C3 devkits; Athom LS4P post-2026 |
| `esp32` | ESP32 (WROOM) | Xtensa | espup rustc fork | Athom music-reactive WLED controller, generic WROOM |

Build:

```sh
# ESP32-C3 (default)
cd firmware && cargo build --release            # or `cargo run --release` to flash

# classic ESP32 (Xtensa)
espup install --targets esp32                   # once
./patch-esp-toolchain.sh                        # once, NixOS only (see below)
./build-esp32.sh                                # or `./build-esp32.sh flash`
```

WiFi credentials bake in at build time until NVS provisioning lands (M3):
`LUXEL_SSID=net LUXEL_PASS=secret cargo build …`. Without them the firmware
runs offline (render-only).

### NixOS note

espup installs prebuilt binaries that expect `/lib64/ld-linux` and system
libs; on NixOS they fail with "No such file or directory" despite existing.
`patch-esp-toolchain.sh` patchelf's the interpreter and rpath on the whole
toolchain (rustc/cargo, rust-lld, and the xtensa-esp-elf GNU linker). Rerun
it after any `espup update`.

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
