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
```

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
