# Firmware targets & bring-up

## Supported boards

| board feature | chip | arch | toolchain | notes |
|---|---|---|---|---|
| `board-c3-devkit` (default) | ESP32-C3 | RISC-V | mainline Rust (in the flake) | bare C3 devkits; SPI CLK GPIO6 / DATA GPIO7 |
| `board-pixelblaze-v3` | ESP32 (WROOM-32) | Xtensa | Espressif rustc fork (in the flake) | Pixelblaze v3 Standard — the preferred real-hardware target |
| `board-athom-music` | ESP32 (WROOM-32E) | Xtensa | Espressif rustc fork (in the flake) | Athom music-reactive WLED controller (OTA-only, riskier) |

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

# classic ESP32 (Xtensa)
BOARD=board-pixelblaze-v3 ./build-esp32.sh      # or `… ./build-esp32.sh flash`
```

Build (nix package, hermetic — reproducible images):

```sh
nix build .#luxel-fw-pixelblaze-v3     # also: luxel-fw-c3-devkit, luxel-fw-athom-music
ls result/                              # luxel-fw.elf + luxel-fw.bin
espflash write-bin 0 result/luxel-fw.bin   # full-flash image (bootloader+partitions+app)
```

The packaged image bakes no WiFi credentials (offline render-only) —
override with `.override { ssid = "net"; pass = "secret"; }` if needed.
Two lockfiles feed the hermetic build: `firmware/Cargo.lock` (our deps) and
`firmware/rust-std.Cargo.lock` (the std workspace's deps, needed by
-Zbuild-std; re-copy from
`$XTENSA_RUST_HOME/lib/rustlib/src/rust/library/Cargo.lock` on toolchain
bumps).

WiFi credentials bake in at build time until NVS provisioning lands (M3):
`LUXEL_SSID=net LUXEL_PASS=secret cargo build …`. Without them (or with
empty values) the firmware runs offline (render-only).

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
