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
# from the nix devshell automatically. The BOARD IS AN ENV VAR: the one
# positional is the ACTION (flash | image | log), and passing a board
# there is rejected outright since Gitea #389 — it used to be ignored
# silently and you got the default board's image under the right-looking
# command line.
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
`luxel-fw-seengreat-hub75`, and the hosted-UI variant
`luxel-fw-c6-devkit-hosted`); see docs/firmware.md for the
credential-baking caveats.

## Supported boards

`WLED` = the `wled-takeover` feature (install by uploading Luxel to WLED's own
`/update` page — docs/wled-migration.md). It costs ~25 KB of app image, so it
is on only where that install path exists.

| feature | chip | strip pins | defaults | pixel cap | WLED | status | notes |
|---|---|---|---|---|---|---|---|
| `board-c3-devkit` (default) | ESP32-C3 | CLK GPIO6, DATA GPIO7 | SK9822, 60 px | 2048 | yes | supported (hardware-verified) | bare devkit |
| `board-pixelblaze-v3` | ESP32 | CLK GPIO18, DATA GPIO23 | SK9822, 300 px | 2048 | no | supported (the dev unit) | official PB v3 Standard schematic; onboard 5 V level shifter; status LED GPIO12 (lit at boot = Luxel alive); button GPIO32 (unused) |
| `board-athom-music` | ESP32 | CLK1 GPIO5, DATA1 GPIO18; **CLK2 GPIO16, DATA2 GPIO17** | WS2812, 60 px | 2048 | yes | on the bench (the rig) | Athom music-reactive WLED controller; strip-VCC relay on GPIO2 must be driven high or the strip stays dark. **The only two-output board** (`board::OUTPUTS` = 2, Gitea #474): `POST /api/layout` `out 0`/`out 1` lines split the one pixel space across both channels — see "Two outputs on the Athom" below. Mic + IR unused |
| `board-esp32-generic` | ESP32 | CLK GPIO18, DATA GPIO23 | WS2812, 60 px | 2048 | yes | builds, untested on hardware | VSPI defaults — most WROOM/DevKitC boards break these out |
| `board-s3-devkit` | ESP32-S3 | CLK GPIO12, DATA GPIO11 | WS2812, 60 px | 2048 | yes | **builds, UNTESTED ON METAL** | ESP32-S3-DevKitC-1; SPI2/FSPI IO_MUX pins (direct DMA route), clear of the octal-PSRAM pins GPIO33–37 |
| `board-c6-devkit` | ESP32-C6 | CLK GPIO6, DATA GPIO7 | WS2812, 60 px | 2048 | yes | **builds, UNTESTED ON METAL** | ESP32-C6-DevKitC-1; SPI2/FSPI IO_MUX pins (same numbers as the C3 by coincidence of the IO_MUX tables), clear of the onboard RGB LED on GPIO8 |
| `board-s3-devkit` + `hub75` | ESP32-S3 | HUB75 (14 pins, `board::hub75_pins!`) | HUB75 64x64 panel, 4096 px | **4096** | yes | **builds, UNTESTED ON METAL** | LCD_CAM + circular-DMA BCM rescan via patched esp-hub75 (firmware/patches/); pin map = the esp-hub75 S3 example's (a panel on jumper wires); strip SPI not wired at all; protocol switches rejected (fixed wire format); nix variant `luxel-fw-s3-hub75` |
| `board-seengreat-hub75` | ESP32-S3 | HUB75 (14 pins, `board::hub75_pins!`) | HUB75 64x64 panel, 4096 px | **4096** | no | **on metal** (first light 2026-09-05; master re-verified 2026-09-06 — see "First light" & "Second light" below) | Seengreat "RGB Matrix HUB75 S3" (ESP32-S3-WROOM-1-N16R8): a purpose-built panel driver board, so the feature turns `hub75` on itself. Pin map transcribed from the [vendor wiki](https://seengreat.com/wiki/214/) — R1 IO5, G1 IO4, B1 IO6, R2 IO15, G2 IO7, B2 IO17, A IO8, B IO18, C IO10, D IO9, E IO16, CLK IO12, LAT IO11, OE IO13; both panel outputs (ribbon + plug-in header) share those pins. Codec/mics (Gitea #142), microSD, RTC and PSRAM unused (see below); nix variant `luxel-fw-seengreat-hub75` |

All eight combos build clean (verified compile + image-size check +
`tools/image-check.sh` + `tools/stack-check.sh`). "Untested on hardware"
means the wiring is reviewed against the vendor pinout but the board has
never been lit up. The **S3 is on the bench since 2026-09-05** (the
Seengreat panel board, "First light" below — the first metal validation of
the S3 codegen, WiFi, OTA and the HUB75 driver); `board-s3-devkit` itself
is still only reviewed, and **no C6 exists on the bench**, so its pin
choices, heap sizing and radio behaviour stay unverified — Gitea #56.
Both protocols run over
SPI: SK9822/APA102 uses CLK+DATA; WS281x uses DATA only (encoded
bitstream), so a WS2812 board simply leaves CLK unconnected — the pin
still gets claimed.

Two classic-ESP32-only wiring bits are deliberately *not* extended to the
new boards: the PB sensor-expansion UART (GPIO3, `#[cfg(feature =
"esp32")]` in main.rs — devkits have no such header) and the Athom strip
relay.

## The OTA-slot ceiling

(This section was "The 1 MiB OTA-slot ceiling" until the 2026-09-20
repartition, Gitea #501. Other docs may still call it that.)

The app image — what `espflash save-image` emits and `/api/ota` writes —
must fit the board's OTA slot or OTA rejects it (crossed once at v0.1.17;
opt-level "s" bought it back — history and diet options in
docs/size-report.md). **The slot is per board**, from the partition table
the board's flash size selects (docs/firmware.md, "Partition tables"):

| table | boards | slot |
|---|---|---:|
| `firmware/partitions.csv` | every board but the Seengreat | **1,310,720 B** (1.25 MiB) |
| `firmware/partitions-16mb.csv` | `board-seengreat-hub75` | **3,145,728 B** (3 MiB) |
| *(pre-#501, still on un-migrated devices)* | — | 1,048,576 B (1 MiB) |

`board_ota_max` in `firmware/board-target.sh` is the single source of that
number per board; `tools/image-check.sh` resolves the slot through it and
prints which rule it used on every size line, because a margin percentage
is meaningless without knowing which slot it is a fraction of.

**`board-seengreat-hub75` has two tiers, and the gate only knows the upper
one.** Since Gitea #634 a 16 MB board whose *bootloader* was flashed for a
smaller part migrates to the 4 MB table rather than refusing, and then its
live `ota_0` is **1,310,720 B**, not the 3,145,728 B `board_ota_max` reports:

| the panel is… | live table | its real slot | gated against |
|---|---|---:|---:|
| on the fallback (bootloader flashed for 4 MB) | `partitions.csv` | 1,310,720 B | 3,145,728 B |
| on its own table (bootloader re-flashed for 16 MB) | `partitions-16mb.csv` | 3,145,728 B | 3,145,728 B |

`image-check.sh` deliberately keeps gating against the nominal slot — the
image it gates is a *release artifact*, and which tier a given device is on
is a property of that device, not of the build. The device is the backstop:
`/api/ota` sizes every push against the table on flash and refuses an
over-size one up front, naming the bootloader as the reason
(docs/api.md). So the practical rule for this board is **1.25 MiB until its
bootloader is re-flashed**, and `partitions.upgrade_available` on
`/api/status` is how a tool tells which tier it is looking at. Today's image
is 991,456 B, inside both tiers with 319,264 B (24.4 %) of the smaller one
free.

### The migrating release: every image weighed against the OLD slot

For exactly one release, the gate runs with `MIGRATING_RELEASE=1` and
weighs every board's image against the **1,048,576 B pre-#501 slot** with
the margin floor at **0 %** — "it fits" is the whole requirement. The
reason is that a device which has not repartitioned yet is what installs
this image, into a 1 MiB slot, with its own running firmware; and holding
any floor against the old slot would block the very release that makes the
slot bigger. Measured 2026-09-20 (credless flake builds,
`nix build .#luxel-fw-<v>` → `luxel-fw-ota.bin`, `origin/master` `b9c0632`
as the baseline column):

| variant | master | migrating release | Δ | free of the OLD 1 MiB slot | % |
|---|---:|---:|---:|---:|---:|
| `c3-devkit` | 973,120 | 985,984 | +12,864 | 62,592 | 5.96 |
| `pixelblaze-v3` | 1,005,424 | 1,023,056 | +17,632 | 25,520 | 2.43 |
| `athom-music` | 1,030,576 | 1,040,560 | +9,984 | **8,016** | **0.76** |
| `esp32-generic` | 1,026,608 | 1,036,496 | +9,888 | 12,080 | 1.15 |
| `s3-devkit` | 973,680 | 983,600 | +9,920 | 64,976 | 6.19 |
| `c6-devkit` | 1,033,296 | 1,045,584 | +12,288 | **2,992** | **0.28** |
| `s3-hub75` | 981,776 | 991,680 | +9,904 | 56,896 | 5.42 |
| `seengreat-hub75` | 969,984 | 987,792 | +17,808 | 60,784 | 5.79 |
| `c6-devkit-hosted` | 1,017,168 | 1,029,264 | +12,096 | 19,312 | 1.84 |

The delta varies because the boards **without** `wled-takeover`
(`pixelblaze-v3`, `seengreat-hub75`) gain the whole table-writing layer,
which the takeover boards already carried; the rest gain only
`migrate.rs`. Those are not comfortable numbers and they are not meant to
be — but three of the nine (`athom-music` 1.71 %, `esp32-generic` 2.09 %,
`c6-devkit` 1.45 %) were **already** under the 3 % floor on master before
this branch added a byte. `tools/ci.sh` gates three variants and
release.yml gates all nine, which is the only reason master was green: a
release cut would have failed on them. That squeeze is what the
repartition ends, not something it introduced.

`MIGRATING_RELEASE` had to come back out in the release after this one
(Gitea #635) — and it did, on 2026-09-24; see the closer below and
docs/releases.md.

Re-measured 2026-09-23 for the `/api/ota` invariant work (Gitea #655,
`origin/master` `4adee91` as the baseline column): `pixelblaze-v3`
1,022,336 → 1,024,928 (+2,592, 23,648 B of the old slot left);
`athom-music` 1,039,792 → 1,042,336 (+2,544, 6,240 left); `c6-devkit`
1,045,008 → 1,046,928 (+1,920, **1,648 left**); `c6-devkit-hosted`
1,028,736 → 1,030,384 (+1,648); `c3-devkit` 985,328 → 987,248 (+1,920);
`seengreat-hub75` 1,078,464 → 1,080,704 (+2,240). That last one was
**already 29,888 B over the old 1 MiB slot on master** (the JIT phase-3
image) — Gitea #669 — so a Seengreat still on the pre-#501 table cannot
take the migrating release at all, and every `migrate-s3-*` QEMU case fails
at its fixture check until the image fits again.

**Retired 2026-09-24 (Gitea #676).** The gate is gone: `tools/ci.sh`
defaults `MIGRATING_RELEASE=0` and `.github/workflows/release.yml` no longer
sets it, so every release image is weighed against its own board's slot at
the normal 3 % floor. What forced the decision is the classic-ESP32 JIT tier
("JIT: which boards compile patterns to native code" below) — with the
emitter those images are ~1,120–1,138 KB, comfortable inside 1.25 MiB and
hopeless against 1 MiB — and the trade Jeremy accepted is that a device
still on the pre-#501 4 MB table can no longer take a normal release over
the air at all. Its migration is two OTAs instead of one: a
`JIT_OFF=1 BOARD=<board> firmware/build-esp32.sh` build first (~1,040 KB, so
it fits the old slot, and it still carries the migrator, so the device
repartitions on that boot), then the normal release. `MIGRATING_RELEASE=1`
stays available by hand for exactly that — gating such a build against the
old slot — and the tables above are the history of the release it was
written for. Only the switch-removal half of #635 is done; `migrate-off` is
not, because devices still need the migrator.

### After the migration: the payoff

The same images, against the slot they land in the moment the device
reboots into the new table. 4 MB boards, 1,310,720 B slot:

| variant | image | free | % |
|---|---:|---:|---:|
| `c3-devkit` | 985,984 | 324,736 | 24.8 |
| `pixelblaze-v3` | 1,023,056 | 287,664 | 21.9 |
| `athom-music` | 1,040,560 | 270,160 | 20.6 |
| `esp32-generic` | 1,036,496 | 274,224 | 20.9 |
| `s3-devkit` | 983,600 | 327,120 | 25.0 |
| `c6-devkit` | 1,045,584 | 265,136 | 20.2 |
| `s3-hub75` | 991,680 | 319,040 | 24.3 |
| `c6-devkit-hosted` | 1,029,264 | 281,456 | 21.5 |

`board-seengreat-hub75` on the 16 MB table gets a 3,145,728 B slot for a
987,792 B image: **2,157,936 B, 68.6 % free.**

The 3 % floor returns in the next release, where there is finally room
under it. The C6 stays the fleet's tightest board — it is ~92 KB fatter
than the C3 for identical source — it just stops being the board that
decides whether a feature can ship.

### Store capacity, before and after

The 512 KiB the app slots gained comes out of `storage`. Inside it the
geometry is unchanged — key area (the `sequential-storage` reserved blobs)
= the first `0x20000`, the ad-hoc read-back slot `0x20000..0x49000`, the
packed pattern log from `LOG_OFF` `0x49000` to the end of the partition —
so only the **log** shrinks:

| | pre-#501 | 4 MB layout | 16 MB layout |
|---|---:|---:|---:|
| `storage` partition | `0x100000` (1 MiB) | `0x80000` (512 KiB) | `0x400000` (4 MiB) |
| pattern log | `0xB7000` (732 KiB, 183 pages) | `0x37000` (220 KiB, 55 pages) | `0x3B7000` (~3.7 MiB) |
| library-sized patterns it holds | ~119 (the Athom fill reached 118) | **38** | far more than `MAX_RECS` |

Measured on the host migration suite (`cargo test -p patlog-check`,
2026-09-20): a churned 12-pattern device stages 61,820 B of live records
(31,180 B dead) and repacks to 65,536 B — 29.1 % of the new 225,280 B log —
so a normal device crosses with ~70 % of its new log free. The ceiling is
that **38**; a device above it refuses to migrate and says so on
`/api/status` rather than dropping patterns (docs/firmware.md, "Layout
migration"). 38 is the number to quote at anyone who asks what the store
cost; the answer to wanting more is the 16 MB table, not a bigger 4 MB
`storage`.

Worth a ticket rather than a scramble: the ad-hoc read-back slot spends
2 × 64 KiB on bytecode sides against a 40 KiB `MAX_BC` cap. Sizing them to
the cap would move `LOG_OFF` from `0x49000` to `0x1D000` and give the 4 MB
log 396 KiB instead of 220 KiB (Gitea #636) — but `LOG_OFF` being shared by the old and
new geometries is exactly what makes the migration a byte move, so it needs
its own release and a format bump.

### On metal: the fleet's migration status

| device | board | migrated | live slot | `ota_slot_bytes` | `storage_bytes` | margin now |
|---|---|---|---|---:|---:|---|
| Athom rig `192.168.0.183` | `board-athom-music` | **yes, 2026-09-20** | `ota_0` | 1,310,720 | 524,288 | 103,488 B (7.9 %) |
| Seengreat panel `192.168.0.238` | `board-seengreat-hub75` | **yes** — serially re-flashed 2026-09-23 onto its own 16 MB table, so the self-applied 16 MB path stays emulator-only (Gitea #655) | `ota_0` | 3,145,728 | 4,194,304 | 2,007,856 B (63.8 %) |
| dev unit `192.168.0.205` | `board-pixelblaze-v3` | not yet (offline) | — | — | — | — |

The Athom is the first device on the new table (Gitea #634). It went across
on one reboot in **under 8.6 s** end to end — including the 512 KiB store
erase and the WiFi rejoin — with its 3 patterns, its playlist, its layout,
its name, its brightness and its untouched asset bundle all intact, and
`store.dead` 31,024 → 0 because the staged log is a packed image. The run
also showed that a device already live on `ota_1` takes the **one**-reboot
path: `ota-push` writes the new image into `ota_0`, which is the offset the
new table also calls `ota_0`, so `settle_into_ota0` neither copies nor
reboots. Budget ~10 s of silence for such a device, not the ~90 s a
self-copying one needs.

One thing that bit and is not the migration's fault: a device OTA'd across
an LXBC format bump comes back with every stored blob unreadable
(`vmerr: bytecode format v5 (this build reads v6)`) and cannot recompile
itself — Gitea #643, which the migrating release makes near-certain
fleet-wide.

#### The 16 MB half declined — and the bootloader is why (2026-09-21, Gitea #634)

The Seengreat took the migrating image cleanly and **did not migrate**, on
two consecutive boots: `partitions.migrated` stayed `false`, the live table
stayed the pre-#501 one (`ota_slot_bytes` 1,048,576, `storage_bytes`
1,048,576), and the store, the patterns, the playlist, the layout, the name
and the brightness were all exactly as found. The device stayed healthy
throughout — `vmerr: null`, 116 fps, `rescan_hz` 115 — and `/api/status`
could not say why, because four of `migrate.rs`' failure paths returned with
only a `println!`. Those now `block()` (#654), and the same session's
diagnostic OTA wedged before it could report anything.

**The cause was found off the bench, under emulation.** The 16 MB image now
runs on QEMU's `esp32s3` machine (five emulator bugs fixed in
`tools/qemu/patches/`, guest-side untouched), and with a clean fixture the
whole 16 MB migration passes — from either slot, cut at every stage, with
the 960 KiB bundle arriving byte-identical at `0xa10000`. What reproduces
the panel's decline exactly is one byte of the fixture:

> **`g_rom_flashchip.chip_size` comes from the BOOTLOADER's image header,
> and an OTA never replaces the bootloader.**

The panel was serially flashed when `board-seengreat-hub75` still used
`partitions.csv`; `firmware/board-target.sh` only started passing
`--flash-size 16mb` for it with #501. Its bootloader therefore tells the ROM
the part is **4 MB**, on 16 MB of silicon — and every `esp_rom_spiflash_*`
op (which is every op esp-storage makes) is bounds-checked against that
number. `write_new_store`'s erase of the new `storage` region at `0x610000`
failed on its *first* sector, which is why the decline was both instant and
total:

```
migrate: new storage 0x610000 + 4096 KiB — erasing
partitions: erase failed at 0x610000
```

esp-storage's own `capacity()` reads the JEDEC RDID and correctly says
16 MB, which is why `flash_fits` waved it through.

**The tempting fix is a brick.** `g_rom_flashchip.chip_size` can be raised at
runtime — ESP-IDF's `bootloader_flash_update_size()` does exactly that — and
doing so *does* let the entire migration run to completion. The device then
reboots into a bootloader that will not load the table it just installed:

```
E flash_parts: partition 4 invalid - offset 0x310000 size 0x300000 exceeds flash chip size 0x400000
E boot: Failed to verify partition table
E boot: load partition table error!
```

…forever, on a board with no serial console. Both halves of that are
emulated and asserted (`migrate-test.py --board s3 --old-bootloader 4mb`).

So `parttab::flash_refusal` now reads the bootloader's ceiling as well as the
chip's and refuses when the new table runs past it, naming the fix. A panel
in this state reports

```json
"migration_blocked":"bootloader was flashed for a smaller part — reflash it over serial",
"blocked_need_bytes":14680064,"blocked_have_bytes":4194304
```

every boot, changes nothing, and keeps working.

#### …and then it stopped refusing: the fallback (Gitea #634, second half)

Refusing outright left the panel on the pre-#501 1 MiB slots for no reason.
The 4 MB table fits under a 4 MB ceiling perfectly well, and it is the same
layout — and the same code — the Athom migrated to. So a board that embeds
the 16 MB table now embeds the 4 MB one as well, `parttab::target_table()`
picks **the largest embedded layout that fits under
`min(chip size, bootloader ceiling)`**, and the panel migrates to
`partitions.csv`: 1.25 MiB slots, a 512 KiB store, `assets` staying put at
`0x310000` with no copy. `/api/status` then reads

```json
"partitions":{"layout":"partitions.csv","migrated":true,
              "ota_slot_bytes":1310720,"storage_bytes":524288,"assets_bytes":983040,
              "ceiling_bytes":4194304,"upgrade_available":true}
```

and the boot log carries the same sentence every boot. **Getting the panel
onto the 16 MB table still needs a one-time serial flash of the bootloader**
(Jeremy's hands, the panel's USB port) — `BOARD=board-seengreat-hub75
firmware/build-esp32.sh flash` writes bootloader + table + app together with
`--flash-size 16mb`. What changed is that this is now an *upgrade* rather
than a rescue, and that it is no longer the only way forward.

If instead the bootloader is re-flashed on a device already on the fallback,
the migrator runs a **second** time on the next boot — `storage` `0x290000`
→ `0x610000`, `assets` `0x310000` → `0xA10000`, staged in the 4 MB layout's
`ota_1` — because `migrated:true` is defined as "the live table is the best
one available today", never as a terminal state. Both hops, from either OTA
slot and cut at every re-runnable stage, are emulated:
`tools/qemu/migrate-test.py --board s3 --old-bootloader 4mb
[--reflash-bootloader 16mb]`, 8 cases in `tools/qemu/run-all.py`.

The cost of all this on the boards that will never use it is **0 B**: the
second table and the selection are behind the same board feature that picks
the first, so every 4 MB image is byte-identical to one built without the
mechanism. What the 4 MB boards do pay is the table-to-table generality and
the two new `/api/status` fields — measured 2026-09-21, `+320 B` on
`board-c6-devkit` (1,044,608 → **1,044,928 B**, `MIGRATING_RELEASE=1` margin
3,968 → **3,648 B**) and `+368 B` on `board-athom-music`. `board-
seengreat-hub75` pays `+4,608 B` (986,848 → 991,456 B) for the whole of it,
against 57,120 B of old-slot margin.

### Measurement history (the 1 MiB era)

Everything below was measured against the 1,048,576 B slot, and the
margins quoted in it are fractions of that. The deltas are still the
interesting part — what a feature costs does not change with the
partition table — and this repo keeps its measurement history. Per-board
app images at v0.1.39, remeasured 2026-08-29 (devshell builds with WiFi
creds baked in — a credless build strips the WiFi stack and reads ~1.5 KB
smaller, which is what CI measures):

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

2026-08-30, map-aware blur/glow (Gitea #140): +2,272 B on `board-c6-devkit`
(997,328 → 999,600 B measured against that same revision, 48,976 B of slot
left) and +2,208 B on `board-pixelblaze-v3` (949,712 → 951,920 B). The grid
itself is six bytes — the image cost is the two extra kernels plus the map
detector. `.stack` on pixelblaze-v3: 29,244 → 29,228 B.

2026-08-30, device output palette (Gitea #139): +5,120 B on
`board-c6-devkit` (999,584 → 1,004,704 B against its own merge base,
**43,872 B** of slot left) and +4,256 B on `board-pixelblaze-v3`
(951,936 → 956,192 B). Whole-fleet re-measure at that revision:

| board | app image | slot margin |
|---|---:|---:|
| `board-c3-devkit` | 912,784 B | 135,792 B |
| `board-pixelblaze-v3` | 956,192 B | 92,384 B |
| `board-athom-music` | 956,048 B | 92,528 B |
| `board-esp32-generic` | 956,032 B | 92,544 B |
| `board-s3-devkit` | 896,688 B | 151,888 B |
| `board-s3-devkit` + `hub75` | 891,952 B | 156,624 B |
| `board-seengreat-hub75` | 891,968 B | 156,608 B |
| `board-c6-devkit` | 1,004,704 B | **43,872 B** |

The cost is the palette blob (serialize + validate + boot load), the wire
parser, the cooked-LUT cache in `apply_outpipe`, and the two new routes;
factoring the three nvs writers onto one `config::write_record` helper paid
about 600 B of it back. The C6 is now at 4.18 % margin — see the ceiling
note below. `.stack` on pixelblaze-v3: 29,228 → 29,196 B; the largest new
frame is `apply_outpipe` at 1,120 B (the cooked LUT is a heap `Box`, not a
stack array).

2026-08-30, fmt diet in the JSON builders (Gitea #168): **−2,640 B** on
`board-c6-devkit` (1,004,112 → 1,001,472 B against its merge base,
**47,104 B** of slot left) and −1,360 B on `board-athom-music`
(955,376 → 954,016 B; the classic-ESP32 variants track within a few
hundred bytes). First negative entry in this table. The win is smaller
than #168's 5–10 KB estimate for a structural reason recorded in
docs/size-report.md: `core::fmt` itself never leaves the image
(`println!` and `Debug` keep it linked — the fmt bucket only dropped
~0.6 KB), and a naive `format!`→`push_str` conversion actually GREW the
image by 8.6 KB because `push_str` inlines a reserve-and-copy at every
call site. The savings come from routing every literal append through
one `#[inline(never)]` `jsonview::push_piece` funnel. No static or
buffer changes; stack-check clean.

The three classic-ESP32 variants differ only by a few hundred bytes (same
chip feature set; only board.rs strings and the wiring lines change), so
checking one of them per release is enough — but the *chips* are not
interchangeable for size purposes: the C6 is ~92 KB fatter than the C3 for
identical source, and at
**67,792 B / 6.46 %** it owns the tightest margin in the fleet by a wide
gap (the next tightest, `board-pixelblaze-v3`, has 114,064 B / 10.87 %). It
is the board that will hit the 1 MiB ceiling first; check
`board-c6-devkit` on any release that grows the image. Measure with:

```sh
# chip/target for $BOARD come from firmware/board-target.sh
espflash save-image --chip esp32c6 \
  target/riscv32imac-unknown-none-elf/release/luxel-fw /tmp/ota.bin && stat -c %s /tmp/ota.bin
```

2026-08-31, easing builtins batch 7 (the 21 remaining standard easings,
review-pass follow-up): `board-c6-devkit` 1,001,472 → **1,005,488 B**
(+4,016 B), margin **43,088 B / 4.11 %** — measured on the rebased tree
after the #168 fmt diet, still above the 3 % CI floor but inside the 6 %
warn band; the next feature that grows the VM should re-measure the C6
first.

2026-09-01, curl noise (`curl2`/`curl3` + analytic simplex derivatives):
**+8,512 B** on `board-c6-devkit` (983,472 → **991,984 B**, margin
**56,592 B / 5.40 %**) and **+8,272 B** on `board-pixelblaze-v3`
(937,200 → 945,472 B). Devshell builds with creds, A/B against this
branch's own merge base in the same worktree (`git checkout HEAD~1`),
which is the only way to get a comparable baseline — master had already
moved ~13 KB below the batch-7 numbers above. The cost is two extra
monomorphizations of the simplex kernels: `simplex2`/`simplex3` and their
`_grad` twins share one function parameterized by `const GRAD: bool`, so
the value path stays bit-identical but the image carries both copies, plus
the derivative arithmetic, the two gradient-component tables and the two
builtin arms. `#[inline(never)]` on the `_grad` entry points was measured
and changes the image by **0 bytes** (LLVM already declines to inline them
into `curl2`/`curl3`), so it is not in the tree. No statics or buffers:
`.stack` on pixelblaze-v3 is 27,484 B with no new frame in the top
fifteen and nothing over the 12 KB budget.

2026-09-05 (later), LXBC v5 — the fixed-width word bytecode the VM
executes in place (docs/spec/bytecode.md; Gitea #260): the decoder lost
its byte walker, its pre-pass and the import-slot rewrite. Same-day A/B
of devshell builds WITH creds (so ~6–7 KB above the credless flake rows
elsewhere in this section; compare the deltas, not the absolutes):
**−2,160 B** on `board-c6-devkit` (1,005,200 → 1,003,040 B, margin
45,536 B / **4.34 %** — up from 4.13 % on the same-methodology
baseline), −2,000 B on `board-c3-devkit`, −2,128 B on the C6 hosted-ui
image; Xtensa +368…+752 B (`board-pixelblaze-v3` 983,392 → 983,792 B,
`board-athom-music` +400 B, `board-s3-devkit` +752 B, both HUB75 images
+512/+528 B) — inside the ±0.7 KB noise floor. `.stack` on
`board-pixelblaze-v3` 26,732 → 26,764 B; every flake variant passes
`tools/image-check.sh`. Firmware source untouched (it still calls the
copying `deserialize_lean`; the borrowing path is the store side's
switch).

2026-09-05, cache-MMU flash mapping of the assets partition
(`firmware/src/flashmap.rs`, docs/research/flash-mmap.md): **−1,392 B** on
`board-c6-devkit` (1,000,512 → **999,120 B**, margin **49,456 B /
4.72 %**), −2,720 B on `board-pixelblaze-v3` (979,312 → 976,592 B) and
−2,224 B on `board-athom-music` (979,152 → 976,928 B) — credless flake
builds of `origin/master` vs this branch, same day. Negative despite a new
module with four per-chip register drivers: the asset response body no
longer carries a `Timer::after` future and a staging `Vec` on its hot
path (the mapped slice goes straight to the socket), and the TOC parse
lost its per-field `read_chunk` calls. No statics beyond two
`AtomicUsize`s; `.stack` on pixelblaze-v3 26,732 B (devshell build,
−48 B), stack-check clean on pixelblaze-v3, s3-devkit and c6-devkit.

2026-09-06, borrowed program words (Gitea #260 — the firmware decodes
mapped patterns with `deserialize_lean_static`, so a running `Program`'s
code and constant pool are flash, not heap; the lifetime contract this
creates is the pin set in docs/firmware.md, "The borrowing invariant and
the pin set"): **+1.6 to +2.0 KB on every board.** Devshell builds with
the same `creds.env` on both sides, `origin/master` df0b547 vs the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 951,648 | 953,328 | +1,680 | 95,248 B (9.08 %) |
| `board-pixelblaze-v3` | 1,007,648 | 1,009,504 | +1,856 | 39,072 B (3.72 %) |
| `board-athom-music` | 1,007,664 | 1,009,488 | +1,824 | 39,088 B (3.72 %) |
| `board-esp32-generic` | 1,007,552 | 1,009,328 | +1,776 | 39,248 B (3.74 %) |
| `board-s3-devkit` | 950,192 | 951,872 | +1,680 | 96,704 B (9.22 %) |
| `board-s3-devkit` + `hub75` | 942,800 | 944,416 | +1,616 | 104,160 B (9.93 %) |
| `board-seengreat-hub75` | 942,832 | 944,448 | +1,616 | 104,128 B (9.93 %) |
| `board-c6-devkit` | 1,020,624 | 1,022,608 | +1,984 | **25,968 B (2.47 %) — FAILS, already did** |
| `board-c6-devkit` + `hosted-ui` | 1,004,192 | 1,006,128 | +1,936 | **42,448 B (4.04 %)** |

The image cost is the pin plumbing (`pins`/`pinned` and the extra
`ARENA.lock` closures, the `contains` in `next_move`/`compacted_free_run`)
plus the `Words::Static` construction path in `bytecode::decode`, which
nothing linked before — the firmware only ever called the copying
`deserialize_lean`. What it buys is RAM, not flash: the resident cost of a
running pattern drops from the decoded `Program` (12–35 KB for the big
gallery patterns) to its header tables (4–11 KB) — docs/research/
flash-mmap.md "RAM accounting". `.stack` on pixelblaze-v3 25,644 B,
stack-check clean; QEMU flashmap + both heap-regions cases pass. The C6
full-UI build was already under the 3 % floor and not a release artifact
(#291); the shipped hosted variant keeps 4 %.

2026-09-06, one mappable extent store (Gitea #330 — the half/half partition
became a 128 KiB `sequential-storage` key area plus an 896 KiB mapped extent
region; source text joined bytecode as an extent and the chunk store went
away entirely; docs/firmware.md "The pattern store: one mapped extent region
+ a small key area"): **−5.3 to −7.1 KB on every board.** Flake builds with
the same `creds.env` on both sides, `origin/master` c7e0266 vs the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---|
| `board-c3-devkit` | 952,128 | 945,024 | −7,104 | 9.20 % → 9.88 % |
| `board-pixelblaze-v3` | 1,005,312 | 999,584 | −5,728 | 4.13 % → 4.67 % |
| `board-athom-music` | 1,005,360 | 999,456 | −5,904 | 4.12 % → 4.68 % |
| `board-esp32-generic` | 1,005,120 | 999,280 | −5,840 | 4.14 % → 4.70 % |
| `board-s3-devkit` | 947,664 | 942,320 | −5,344 | 9.62 % → 10.13 % |
| `board-c6-devkit` | 1,020,384 | 1,013,312 | −7,072 | **2.69 % → 3.36 %** |
| `board-c6-devkit` + `hosted-ui` | 1,002,784 | 996,912 | −5,872 | 4.37 % → 4.93 % |
| `board-s3-hub75` | 940,032 | 934,400 | −5,632 | 10.35 % → 10.89 % |
| `board-seengreat-hub75` | 940,064 | 934,272 | −5,792 | 10.35 % → 10.90 % |

A store that stores each blob once is simply less code: `write_pattern`,
`read_source`, `read_bc`, `remove_chunks`, the two chunk-key functions and
`cache_code` all disappeared, and with them the chunk loops in every read
path. The extent table grew (`MAX_EXTENTS` 28 → 72, `MAX_PAGES` 128 → 256,
+~740 B in `.bss`, not in the image) and the shared `PageStateCache` shrank
with the map's range (128 → 32 pages). `.stack` on pixelblaze-v3
25,484 → **24,932 B** (−552), stack-check clean; the biggest frames are
unchanged (picoserve's response future, the main and render tasks).

**`board-c6-devkit` is back above image-check's 3 % floor** — 2.69 % →
3.36 % — which was #310's problem, fixed here by the store getting simpler
rather than by touching the UI. It is still the tightest board by a wide
margin and still ships as `luxel-fw-c6-devkit-hosted`; restoring the
full-UI build as a release artifact is Gitea #291.

2026-09-06, pattern extent allocator (Gitea #281 — the arena's 7 fixed
40 KiB slots became a page-granular extent allocator; `patterns.rs` +
`extents.rs`, docs/firmware.md "The pattern store: one mapped extent region + a
small key area"): **+4.1 to +5.3 KB on every board.** Devshell builds with the
same `creds.env` on both sides, `origin/master` 8b478f0 vs the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 946,256 | 951,568 | +5,312 | 97,008 B (9.25 %) |
| `board-pixelblaze-v3` | 1,003,424 | 1,007,600 | +4,176 | 40,976 B (3.90 %) |
| `board-athom-music` | 1,003,424 | 1,007,584 | +4,160 | 40,992 B (3.90 %) |
| `board-esp32-generic` | 1,003,248 | 1,007,488 | +4,240 | 41,088 B (3.91 %) |
| `board-s3-devkit` | 946,016 | 950,128 | +4,112 | 98,448 B (9.38 %) |
| `board-seengreat-hub75` | 938,592 | 942,768 | +4,176 | 105,808 B (10.09 %) |
| `board-c6-devkit` | 1,015,488 | 1,020,736 | +5,248 | **27,840 B (2.65 %) — FAILS** |
| `board-c6-devkit` + `hosted-ui` | — | 1,004,256 | — | **44,320 B (4.22 %)** |

Where it goes (`nm -S` diff on the C3 ELF, +5,031 B of named symbols, of
which 328 B is the directory static and lives in `.bss`, not the image):
`cache_code` +2,132 B (it now allocates, compacts and re-publishes instead
of picking one of seven slots), the `extents.rs` planning code ~742 B
(`order` 262, `insert` 230, `remove` 152, `first_fit` 98 — the rest
inlines), `Mutex::lock` +610 B from the extra `ARENA.lock` closure types,
`live_gens` +380 B, `cache_code`'s print machinery +354 B, the render task
+208 B. Two trims were taken before landing: the directory static is
initialized all-zero so it sits in `.bss` instead of `.data` (−472 B of
image; the real page count is installed by `arena_init`), and the
boot/full/write log lines lost their surplus format arguments.

**This is what spends the C6's margin.** The lever the 2026-09-05 entry
below recorded is now pulled: `board-c6-devkit` with the on-device
playground is **under image-check's 3 % floor** (3.15 % → 2.65 %) and is no
longer a release artifact. The C6 ships as `luxel-fw-c6-devkit-hosted` only
(.github/workflows/release.yml, flake.nix, docs/releases.md); the full-UI
build still compiles and is still the C6 build to develop against.
Restoring it as an artifact is **Gitea #291**. stack-check clean on
pixelblaze-v3 / s3-devkit / c6-devkit; the largest new frame is
`arena_init` at 1,520 B, at boot on the main task, well under the 12 KB
per-function budget.
2026-09-05, pattern code arena (library patterns execute from the flash
mapping; `patterns.rs`, docs/firmware.md "The pattern store: one mapped
extent region + a small key area"): **+10,528 B** on `board-c6-devkit` (1,002,720 →
**1,013,248 B**, margin **35,328 B / 3.37 %**), +9,232 B on
`board-pixelblaze-v3` (981,952 → 991,184 B), +9,408 B on
`board-athom-music` — credless flake builds vs `origin/master` 0f84707,
same day. Named-symbol growth is 6.7 KB (`nm -S` diff on the PB ELF: the
`Msg::Library` swap arm +1.9 KB in the render task, `cache_code` 1.2 KB,
the arena table/verify/persist code ~1 KB, `check_asserts` +0.6 KB now
outlined behind `with_code`, small new accessors), the rest alignment;
`encode_envelope` and the four envelope-building activation sites
disappeared (−1.9 KB) but did not cover it. **The C6 is now 3.9 KB above
the 3 % release floor.** The accepted lever when the next feature lands
is `EXTRA_FEATURES=hosted-ui` for the C6 variant (`luxel-fw-c6-devkit-
hosted` already exists; −14 KB), not shrinking the store. `.stack` on
pixelblaze-v3 26,396 B (−336 B: the render task future grew by the new
arm); stack-check clean on pixelblaze-v3 / s3-devkit / c6-devkit.
(The lever was pulled on 2026-09-06 — see the entry above.)

2026-08-30, picoserve response collapse (Gitea #167): **−23.0 to −24.4 KB
on every board** — the largest single reduction since the opt-level switch,
and the one that takes the C6 back out of CI's warn band. `server.rs`'s
thirteen response tuple shapes became one concrete `Reply` type, so
picoserve stops monomorphizing `IntoResponse::write_to` (22 instantiations
→ 1), its header machinery (37 symbols → 4) and a `Display` shim per
header-value type. Whole-fleet re-measure, **credless flake builds**
(`nix build .#luxel-fw-<board>` → `luxel-fw-ota.bin`; ~1.5 KB under a
devshell build with creds, so these are not comparable to the tables
above — both columns here are measured the same way):

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 913,024 | 888,688 | −24,336 | 159,888 B (15.24 %) |
| `board-pixelblaze-v3` | 957,520 | 934,512 | −23,008 | 114,064 B (10.87 %) |
| `board-athom-music` | 957,296 | 934,288 | −23,008 | 114,288 B (10.89 %) |
| `board-esp32-generic` | 957,424 | 933,920 | −23,504 | 114,656 B (10.93 %) |
| `board-s3-devkit` | 898,592 | 874,576 | −24,016 | 174,000 B (16.59 %) |
| `board-s3-devkit` + `hub75` | 893,520 | 870,480 | −23,040 | 178,096 B (16.98 %) |
| `board-seengreat-hub75` | 893,488 | 870,432 | −23,056 | 178,144 B (16.98 %) |
| `board-c6-devkit` | 1,005,168 | 980,784 | −24,384 | **67,792 B (6.46 %)** |

Both columns are an A/B with **only `firmware/src/server.rs` differing** — the
right way to measure, because the image carries a **~±0.7 KB noise floor** that
has nothing to do with your code: the flake source hash feeds rustc's
`.Lanon.<hash>` local-symbol naming, and renaming those reshuffles
`.L_MergedGlobals` packing. Editing only this documentation moved the same
firmware ~600 B (twice, in both directions, while writing these entries). Hold
everything else constant when you diet, and never read a sub-1 KB delta as
real. See docs/size-report.md. (The *other* half of that floor — the build
directory's own length, embedded in every panic `Location` — is gone since
Gitea #441; see the 2026-09-08 entry. What survives is the symbol-naming
repacking described here, measured at 496 B between two builds of identical
source 47 characters of path apart.)

All eight pass `tools/image-check.sh` (markers + margin). The cost is RAM,
not flash: the single `write_to` future is the union of every body type, so
`.stack` on `board-pixelblaze-v3` went 29,124 → **27,828 B** (3,828 B above
the 24 KB floor) and the flat dispatcher's poll frame 2,128 → **4,928 B**
against a 12,288 B budget. That frame is the one to watch when adding
routes — it is why `server.rs` must keep its hand-written flat-match
`PathRouterService` rather than picoserve's `MethodRouter` (whose HEAD arm
introduces a second writer type and would undo the whole collapse).
Details and symbol tables: docs/size-report.md.

2026-08-30, hosted-UI build mode (Gitea #11): **−13,936 to −14,544 B on
every board**, and the assets partition is never written at all. Fleet
A/B, devshell builds with creds so the absolute numbers sit ~1.5 KB above
the credless flake column just above — both columns here were taken the
same way, at the same revision:

| board | app image | slot margin | + `hosted-ui` | margin | saved |
|---|---:|---:|---:|---:|---:|
| `board-c3-devkit` | 889,136 B | 159,440 B | 875,200 B | 173,136 B | 13,936 B |
| `board-pixelblaze-v3` | 934,928 B | 113,648 B | 920,432 B | 128,144 B | 14,496 B |
| `board-athom-music` | 934,848 B | 113,728 B | 920,336 B | 128,240 B | 14,512 B |
| `board-esp32-generic` | 934,768 B | 113,808 B | 920,352 B | 128,224 B | 14,416 B |
| `board-s3-devkit` | 875,424 B | 173,152 B | 860,976 B | 187,600 B | 14,448 B |
| `board-s3-devkit` + `hub75` | 870,880 B | 177,696 B | 856,368 B | 192,208 B | 14,512 B |
| `board-seengreat-hub75` | 870,896 B | 177,680 B | 856,352 B | 192,224 B | 14,544 B |
| `board-c6-devkit` | 981,696 B | 66,880 B | 967,728 B | **80,848 B** | 13,968 B |

The shipped variant, measured the way CI measures (credless flake build,
`nix build .#luxel-fw-c6-devkit-hosted`): **966,832 B, 81,744 B / 7.79 % of
the slot free** against the same board's 980,784 B / 67,792 B / 6.46 % — so
the mode is also what takes the C6 clear of `image-check.sh`'s 6 % warn line
with room to spare.

Strikingly flat across chips (1.42–1.67 % of the image) — the code that
leaves is plain logic with no chip-specific codegen. It also hands back
DRAM: `.stack` on `board-pixelblaze-v3` goes **27,828 → 32,524 B**,
because the response future the web-task arena is sized for loses its
asset arm (largest frame 9,552 → 7,504 B). No new frame anywhere; the
12,288 B budget is untouched. What the mode is and when to use it:
"Hosted-UI builds" below.

**The VM is compiled at opt-level 3 where the slot allows** (Gitea #260,
2026-09-05). The image is opt-level "s" for the ceiling above, but at 4096
px the interpreter (crates/luxel-core) *is* the frame time, so
`firmware/board-target.sh` carries a per-board `CORE_O3` flag and
build-esp32.sh / tools/stack-check.sh / flake.nix (`coreO3`) pass
`--config profile.release.package.luxel-core.opt-level=3` when it is set.
Measured on the same tree, devshell builds: +17,584 B on
`board-seengreat-hub75` (margin 158,208 → 140,624 B) and +17,920 B on
`board-pixelblaze-v3` (91,840 → 73,920 B); on `board-c6-devkit` it would
have been +20,320 B (50,800 → 30,480 B, under the 3 % floor), so the C6
variants build with `CORE_O3=0` / `coreO3 = false` and keep the profile
default. Adding a board means choosing this flag against its margin.

**The render task runs on the second core on dual-core boards** (Gitea
#259/#260, 2026-09-05; docs/firmware.md "Cores & tasks"). Cost is the
second-core bring-up, the cross-core flash fence, the RTC watchdog and the
RTC-memory black box, and it only exists where `multi_core` is set (esp32,
esp32s3) — the C3/C6 deltas are the ±0.7 KB noise floor. Fleet A/B on the
same base (master 731ce81, devshell builds with creds, CORE_O3 as shipped):

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 945,200 | 945,184 | −16 | 103,392 B (9.86 %) |
| `board-pixelblaze-v3` | 992,128 | 1,000,512 | +8,384 | 48,064 B (4.58 %) |
| `board-athom-music` | 992,240 | 1,000,496 | +8,256 | 48,080 B (4.59 %) |
| `board-esp32-generic` | 991,744 | 1,000,272 | +8,528 | 48,304 B (4.61 %) |
| `board-s3-devkit` | 933,600 | 942,416 | +8,816 | 106,160 B (10.12 %) |
| `board-s3-devkit` + `hub75` | 926,464 | 934,864 | +8,400 | 113,712 B (10.84 %) |
| `board-seengreat-hub75` | 926,384 | 934,912 | +8,528 | 113,664 B (10.84 %) |
| `board-c6-devkit` | 1,015,040 | 1,014,416 | −624 | **34,160 B (3.26 %)** |

The classic-ESP32 boards are now under the 6 % warn line (they crossed it
with the flash mapping + code arena the same day; the fence was already
trimmed once — its spin-waits out of line — after an inlined first cut cost
25 KB), and the C6 sits 2.7 KB above the 3 % floor on master's own account.
`.stack` on `board-athom-music`: 26,044 → 26,396 B (the AppCpu stack is
heap-allocated, not a static, so the main-task stack does not pay for it;
idle `heap_free` pays the 20 KB instead: 105,456 → 84,960 B).


2026-09-06, **superinstructions** (Gitea #261 — fourteen fused opcodes at
`0x41..0x4E`, arms in `Vm::run`; the compiler-side peephole is host code
and costs the image nothing): **+5.5 to +7.6 KB on every board.** Devshell
builds with the same `creds.env` on both sides, `origin/master` df0b547 vs
the branch — same methodology as the entry above, so its "after" column is
this one's "before":

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 951,616 | 959,248 | +7,632 | 89,328 B (8.52 %) |
| `board-pixelblaze-v3` | 1,007,632 | 1,013,216 | +5,584 | 35,360 B (3.37 %) |
| `board-athom-music` | 1,007,632 | 1,013,216 | +5,584 | 35,360 B (3.37 %) |
| `board-esp32-generic` | 1,007,536 | 1,013,104 | +5,568 | 35,472 B (3.38 %) |
| `board-s3-devkit` | 950,160 | 955,664 | +5,504 | 92,912 B (8.86 %) |
| `board-seengreat-hub75` | 942,816 | 948,320 | +5,504 | 100,256 B (9.56 %) |
| `board-c6-devkit` (not shipped) | 1,021,392 | 1,027,552 | +6,160 | 21,024 B (2.01 %) |
| `board-c6-devkit` + `hosted-ui` | 1,004,912 | 1,011,088 | +6,176 | 37,488 B (3.58 %) |

The release gate is the credless flake build, which runs ~2.5 KB lighter:
`luxel-fw-c6-devkit-hosted` (the C6's shipped image since the entry above)
**1,009,696 B, 38,880 B / 3.70 % free** and `luxel-fw-pixelblaze-v3`
**1,012,272 B, 36,304 B / 3.46 %** — both over the 3 % floor,
`tools/ci.sh` green. **The three classic-ESP32 boards are now the tightest
shipped images in the fleet** (3.4–3.5 % credless, ~5 KB over the floor),
which is a change: the C6 held that title until it went hosted-UI. They
have no equivalent lever left — `hosted-ui` on a Pixelblaze v3 would take
away the on-device playground on the one board people actually own — so the
next feature that grows the VM has to bring its own diet
(docs/size-report.md).

Two dispatch-loop lessons from getting +8.5 KB (the first cut) down to
+5.5, both of them the SAME trade in opposite directions, so measure both
sides before believing either:

- Merging several opcodes into ONE match arm with an inner `match opcode`
  saves ~4 KB and costs ~14 % of interpreter throughput, because the two
  candidates were `LoadIdx` and `CallBuiltin` — the hottest opcodes there
  are.
- Moving a shared BODY out of line into an `#[inline(never)]` helper saves
  nearly as much for free: `index_read`, `call_builtin_slow`, and
  `err_static` for the ~45 `fail!` sites, whose inlined `String`
  construction had been quietly bloating the loop since long before this
  change. The exception is `builtin_fast` — taking the in-loop hot-builtin
  path out of line with `call_builtin_slow` cost 15 %, so that half stays
  in the loop as a macro.

`.stack` unchanged; `tools/stack-check.sh` clean on pixelblaze-v3,
s3-devkit and c6-devkit.

2026-09-06, **the #312 dispatch work gives some of that back — every board
shrinks.** Widening `Value`'s payloads to 32 bits (which drops a
literal-pool load and a mask from every `match` on a `Value`), the in-place
binary/store arms, and `fuel`/`insn_start` moving out of `Vm` into locals
take ~1.5 KB out of `Vm::run` alone. Devshell builds, same `creds.env` both
sides, `origin/master` 1b1ed45 vs the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 962,704 | 961,632 | −1,072 | 86,944 B (8.29 %) |
| `board-pixelblaze-v3` | 1,017,360 | 1,014,272 | −3,088 | 34,304 B (3.27 %) |
| `board-athom-music` | 1,017,472 | 1,014,384 | −3,088 | 34,192 B (3.26 %) |
| `board-seengreat-hub75` | 952,224 | 949,120 | −3,104 | 99,456 B (9.48 %) |
| `board-c6-devkit` (not shipped) | 1,031,072 | 1,023,472 | −7,600 | 25,104 B (2.39 %) |
| `board-c6-devkit` + `hosted-ui` | 1,014,560 | 1,006,960 | −7,600 | 41,616 B (3.96 %) |

Worth knowing why the C6 gains twice what anyone else does: it is the one
board `board-target.sh` does NOT build luxel-core at `CORE_O3`, so its
`Vm::run` is opt-level "s" and every instruction removed from the dispatch
loop is removed once per arm instead of being folded away. The classic-ESP32
boards mattered most here — measured with dev creds baked in they were
**below** `image-check.sh`'s 3 % floor on master (2.97 %/2.96 %) and are back
over it at 3.27 %/3.26 %. `.stack` unchanged (46,572 B on the Seengreat,
25,484 B on pixelblaze-v3); `tools/stack-check.sh` passes on both, and the
worst frames are still the picoserve response future and the embassy main
task, not anything in the VM.

2026-09-06, **the debugger check leaves the dispatch loop (#312) — every
Xtensa board gets ~580 B back.** `debug_stop` and the `pos_at` binary search
it calls were being inlined between the loop head and the instruction fetch;
moving them behind one `#[cold]` call is worth −4.5 % per loop iteration on
the Athom, and removes the register pressure that was making LLVM tail-
duplicate inside `Vm::run`. Two more things shrink with it:
`jsonview::push_u64` (LLVM was unrolling all twenty digit positions of a
`u64` decimal formatter around an inline 64-bit magic multiply: 1,909 B →
~370 B) and the two 128-byte `[Value; MAX_ARGS]` call buffers, which leave
`Vm::run`'s stack frame entirely (432 → 256 B). Devshell builds, same
`creds.env` both sides, `origin/master` 66a94f7 vs the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-pixelblaze-v3` | 1,006,704 | 1,006,112 | −592 | 42,464 B (4.05 %) |
| `board-athom-music` | 1,006,800 | 1,006,208 | −592 | 42,368 B (4.04 %) |
| `board-esp32-generic` | 1,006,528 | 1,005,952 | −576 | 42,624 B (4.06 %) |
| `board-s3-devkit` | 949,056 | 948,480 | −576 | 100,096 B (9.55 %) |
| `board-seengreat-hub75` | 941,648 | 941,072 | −576 | 107,504 B (10.25 %) |
| `board-c3-devkit` | 953,248 | 953,664 | **+416** | 94,912 B (9.05 %) |
| `board-c6-devkit` (not shipped) | 1,020,816 | 1,020,784 | −32 | 27,792 B (2.65 %) |
| `board-c6-devkit` + `hosted-ui` | 1,004,272 | 1,004,240 | −32 | 44,336 B (4.23 %) |

The C3 is the one board that grows: it and the C6 are RISC-V, where the
inlined debug blob was not costing the dispatch loop registers in the first
place, so the out-of-line call is a small net add. Everything passes
`tools/image-check.sh` except the C6 full-UI build, which already failed and
is not shipped (the released C6 variant is `+ hosted-ui`, at 4.23 %).
`.stack` unchanged; `tools/stack-check.sh` clean.

**There is a 3.0 KB size lever on this code that was measured and
deliberately not taken**: merging the 93 `fail!` sites in `Vm::run` into one
`break 'frame <msg>` epilogue removes 2.5 KB of tail-duplicated prologues
and costs **5 % of dispatch throughput**, because LLVM then hoists the
commonest message's pointer and length into the hot preamble to feed the
phi. See the #312 comment before trying it again.

2026-09-06, **the #312 op-body work takes another ~7.5 KB off every board**
(PR #323). Not the dispatch this time but what each instruction *does*:
`fmath`'s transcendentals rewritten from `i64`/`i128` to 32-bit widening
multiplies, `time()`'s scaled divide moved into 32-bit registers, and
`builtin_fast`'s argument array passed by value. That removed 93 of the 141
ROM 64-bit libcall sites in `luxel-core` — `fmath` alone went from 19 ×
`__udivdi3` + 22 × `__divdi3` to zero — which is where most of the bytes
came from (`fmath` 3,248 → 2,102 Xtensa instructions, `Vm::run` 5,446 →
5,111, `Vm::call_builtin` 7,742 → 6,951). Devshell builds, same
`creds.env` both sides, `origin/master` 96f9833 vs the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 961,712 | 953,232 | −8,480 | 95,344 B (9.09 %) |
| `board-pixelblaze-v3` | 1,014,352 | 1,006,688 | −7,664 | 41,888 B (3.99 %) |
| `board-athom-music` | 1,014,448 | 1,006,784 | −7,664 | 41,792 B (3.98 %) |
| `board-esp32-generic` | 1,014,160 | 1,006,496 | −7,664 | 42,080 B (4.01 %) |
| `board-s3-devkit` | 956,592 | 949,040 | −7,552 | 99,536 B (9.49 %) |
| `board-seengreat-hub75` | 949,184 | 941,632 | −7,552 | 106,944 B (10.19 %) |
| `board-c6-devkit` (not shipped) | 1,023,728 | 1,020,800 | −2,928 | 27,776 B (2.64 %) |
| `board-c6-devkit` + `hosted-ui` | 1,007,168 | 1,004,256 | −2,912 | 44,320 B (4.22 %) |

The three classic-ESP32 boards move from just over `image-check.sh`'s 3 %
floor to just under 4 %; the C6's un-shipped full-UI build is still below
the floor (#291), improved by 0.28 pp. `.stack` and the largest frames are
byte-identical to master on both the default board and the panel
(25,484 B / 46,572 B), and `tools/ci.sh` is green.


2026-09-06, **the frame pipeline** (Gitea #306 — the HUB75 compose and the
output pipeline move to an output task on core 0; docs/firmware.md "The
frame pipeline"). It is `hub75`-gated, so only the panel board pays for the
second task; everywhere else the change is the sink refactor that made room
for it (one `PipeState` struct instead of three loose locals in the render
task), which is worth a few hundred bytes back. Devshell builds, same
`creds.env` both sides, `origin/master` 4ccbcde vs the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 959,040 | 958,192 | −848 | 90,384 B (8.62 %) |
| `board-pixelblaze-v3` | 1,007,072 | 1,006,368 | −704 | 42,208 B (4.03 %) |
| `board-athom-music` | 1,006,960 | 1,006,400 | −560 | 42,176 B (4.02 %) |
| `board-esp32-generic` | 1,006,848 | 1,006,336 | −512 | 42,240 B (4.03 %) |
| `board-s3-devkit` | 953,440 | 953,008 | −432 | 95,568 B (9.11 %) |
| `board-seengreat-hub75` | 945,840 | 947,344 | **+1,504** | 101,232 B (9.65 %) |
| `board-c6-devkit` (not shipped) | 1,025,216 | 1,024,848 | −368 | 23,728 B (2.26 %) |

**RAM cost: zero, deliberately.** A pipeline needs one more live frame than
a serial loop, and 12 KB at 4096 px is not there to spare — the first cut
of #306 cost exactly that and pushed `library/snake-2d.js` at 4096 px below
`RUNTIME_FLOOR`, so the panel refused to load it. The shipped version pays
for the travelling buffer by deleting `shared::PIXELS`, the `/api/pixels`
snapshot, which was a second copy of the frame that had just been composed:
`pipeline::preview` reads the travelling buffer instead. Measured idle
`heap_free` on the panel is identical to master row for row (51,704 /
51,652 / 51,640 / 49,180 / 28,928 B across the five bench patterns), and
the AppCpu stack high-water is unchanged at 10,464 B of 20,480.

2026-09-07, **bulk render re-measured on the merged tree** (Gitea #336, the
on-device verification of #335). docs/bulk-render.md's size table was taken on
the branch; these are `tools/image-check.sh` on `espflash save-image` app
images built from three commits in one worktree, same `creds.env` throughout —
the merge base `974b3b3`, the #335 merge `eedabc8`, and the master of the day
`ed2ac3f` (which also carries #306 and #320):

| board | `974b3b3` | `eedabc8` (#335) | Δ #335 | `ed2ac3f` | margin at master |
|---|---:|---:|---:|---:|---:|
| `board-pixelblaze-v3` | 997,648 | 1,006,480 | +8,832 | 1,006,432 | 42,144 B (4.01 %) **warn** |
| `board-athom-music` | 997,536 | 1,006,352 | +8,816 | 1,006,480 | 42,096 B (4.01 %) **warn** |
| `board-seengreat-hub75` | 935,904 | 945,216 | +9,312 | 947,424 | 101,152 B (9.64 %) |
| `board-c6-devkit` (not shipped) | 1,015,376 | 1,024,608 | +9,232 | 1,024,912 | 23,664 B (**2.26 %**) **fail** |

So the branch's own numbers hold up: **+8.8–9.3 KB of flash on every board**,
the two shipped classic-ESP32 boards stay just over 4 % (warn, not fail), the
panel keeps 9.6 %, and the C6 is the one board #335 puts under the 3 % floor
— its base margin was 3.16 %, and the branch reproduces the 2.29 % the
evaluation predicted. That board is not in CI and its margin is tracked by
**#291**; the honest statement remains "this change costs the C6 its margin",
not "the C6 was already under".

**CI enforces a margin floor, not just the ceiling** (Gitea #160).
`tools/image-check.sh` now also takes the app image's size: it FAILS below
**3 %** of the slot free (31,458 B) and WARNS below **6 %** (62,915 B).
The release workflow runs it for all eight board variants, so an image
that would leave a device un-updatable red-lights a release build instead
of being discovered by `/api/ota` on a C6 that nobody here can serial-
recover (#56). The floor sits ~12 KB under today's tightest board, which
is deliberate: it costs roughly two more medium features before the gate
trips, and by then the diet in docs/size-report.md is genuinely overdue.
Both thresholds and the slot size are env-overridable
(`MIN_MARGIN_PCT` / `WARN_MARGIN_PCT` / `OTA_MAX`) — raise them for a
one-off, but changing the default is a decision to record here. The size
half is skipped for ELF inputs (build-esp32.sh's local call), since an ELF
is not the artifact that has to fit.

**The C6 penalty is the vendor radio blob, not our codegen** (profiled
2026-08-30 with `tools/size-report.py` on credless flake builds of
`board-c3-devkit` 912,208 B and `board-c6-devkit` 1,003,824 B — same
source, same opt settings). Of the 91,616-byte gap, ~51 KB is Espressif
blob symbols and another ~11 KB is `.rodata.wifi`; `.rwtext.wifi` alone
goes 33,768 → 55,060 B. Our Rust is essentially chip-independent:
`luxel-core` is byte-identical at 76,628 B on both, `picoserve` identical
at 32,948 B, `luxel-fw` differs by 3,644 B. The consequence is that
**there is no C6-specific diet** — every byte we can win is a fleet-wide
win, and the only C6-only lever is dropping a feature from that board's
profile. (The "riscv32imac codegen" explanation that used to sit here was
a guess; the measurement does not support it.)

Also note: the big NOBITS alignment holes (`.text_gap`, ~58 KB on the C6;
`.rotext_dummy`, 128 KB on the C3) and `.eh_frame` (~63 KB) are *not* in
the app image — the PROGBITS sections plus headers account for the image
size to within ~700 B on both chips. Don't chase them.

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

2026-09-07, packed pattern files (Gitea #340 — the page-granular extent
allocator and its one-item directory became a packed, append-only log of
exact-sized self-describing files; `patterns.rs` + the new `patlog.rs`,
docs/firmware.md "The pattern store: a packed file log in a mapped region +
a small key area"): **+2.6 to +3.3 KB on every board.** Devshell builds with
the same `creds.env` on both sides, `origin/master` fca04e7 vs the branch,
all measured after the rebase:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-c3-devkit` | 958384 | 961664 | +3280 | 86912 B (8.28 %) |
| `board-pixelblaze-v3` | 1006528 | 1009264 | +2736 | 39312 B (3.74 %) |
| `board-athom-music` | 1006560 | 1009360 | +2800 | 39216 B (3.73 %) |
| `board-esp32-generic` | 1006512 | 1009248 | +2736 | 39328 B (3.75 %) |
| `board-s3-devkit` | 953056 | 955792 | +2736 | 92784 B (8.84 %) |
| `board-c6-devkit` | 1024736 | 1027408 | +2672 | 21168 B (2.01 %) |
| `board-c6-devkit-hosted` | 1008224 | 1010784 | +2560 | 37792 B (3.60 %) |
| `board-s3-hub75` | 947264 | 950048 | +2784 | 98528 B (9.39 %) |
| `board-seengreat-hub75` | 947456 | 950064 | +2608 | 98512 B (9.39 %) |

Walking a log costs more code than reading a table: the boot scan, the
compaction planner and its page-gather executor, the two `Arena`
implementations (mapped and the `flashmap-off` read buffer), and a save that
now writes six ordered steps instead of two extents. What comes back is
RAM — `.stack` on pixelblaze-v3 **24,828 → 25,988 B**, because the 72-entry
extent table and its page bitmap were a ~1.2 KB `.bss` static and the log
has no directory to hold.

Three trims were taken before landing, all of them monomorphization:
`patlog::scan`'s and `pack`'s callbacks are `&mut dyn FnMut` rather than
generic (three copies of a whole arena walk, ~2.6 KB), the RAM index is
ordered by a hand-rolled insertion sort rather than three
`sort_unstable_by_key` instantiations of pdqsort (~1.3 KB), and the
firmware does not `{:?}`-print `patlog::Step` (710 B to name six variants).
Without them the change was **+7.9 KB** on the C3.

`board-c6-devkit` goes 2.27 % → 2.01 %, still under image-check's 3 % floor
— it was already under it on master (#310) and is not a release artifact
(#291); the shipped `board-c6-devkit` + `hosted-ui` variant keeps 3.60 %.

2026-09-07, the compaction data-loss fix (Gitea #379 — the boot scan no
longer steps over a torn record by its own claimed length, the compaction
planner checks its plan before anything is erased, and a save that compacts
re-resolves the generation it is about to retire): **+368 B** on
`board-pixelblaze-v3` (1,009,328 → 1,009,696 B, margin **38,880 B /
3.71 %**), **+336 B** on `board-athom-music` (1,009,424 → 1,009,760 B,
margin 38,816 B / 3.70 %) and **+496 B** on `board-c6-devkit` (1,027,472 →
1,027,968 B, margin 20,608 B / 1.97 %). Devshell builds, same `creds.env`
both sides, measured after the rebase onto b08bbd4 against that same
revision. The cost is the plan check plus the resync arm in `scan`; folding
the check into `patlog::plan` rather than wrapping a separate verification
pass around it halved it (+624/+800 B before the fold).
`.stack` on pixelblaze-v3 unchanged at 25,988 B — nothing here is a static.

2026-09-08, **no `{:?}` on foreign error types** (Gitea #438 — the C6 image
release.yml ships had fallen 50 B under image-check's 3 % floor): **−2,896 B**
on the shipped C6 variant and −0.8 to −1.8 KB everywhere else. Ten `println!`
sites stopped formatting somebody else's error with `Debug`; the messages
stayed. Credless flake builds (`nix build .#luxel-fw-<board>` →
`luxel-fw-ota.bin`), both columns at `f62a45e` vs this branch. **Read the
"after" column with the ±0.7 KB noise floor this section already warns
about** — the flake image embeds its own build directory path
(`/nix/var/nix/builds/nix-<pid>-<rand>/…`) in every panic `Location`, so
even a docs-only edit can move these numbers by a few hundred bytes (the C6
row read 1,015,152 B on one intermediate tree and 1,014,400 B on the merged
one). Gitea #441 is the fix, and it is worth ~9–10 KB a board on its own:

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `c6-devkit` + `hosted-ui` *(shipped)* | 1,017,296 | **1,014,400** | **−2,896** | **34,176 B (3.25 %)** |
| `pixelblaze-v3` | 1,016,320 | 1,014,528 | −1,792 | 34,048 B (3.24 %) |
| `athom-music` | 1,016,272 | 1,014,592 | −1,680 | 33,984 B (3.24 %) |
| `esp32-generic` | 1,015,776 | 1,014,128 | −1,648 | 34,448 B (3.28 %) |
| `s3-devkit` | 962,864 | 961,232 | −1,632 | 87,344 B (8.32 %) |
| `s3-devkit` + `hub75` | 969,008 | 967,856 | −1,152 | 80,720 B (7.69 %) |
| `seengreat-hub75` | 978,416 | 977,600 | −816 | 70,976 B (6.76 %) |
| `c3-devkit` | *did not build* | 966,160 | — | 82,416 B (7.86 %) |
| `c6-devkit` *(not shipped)* | 1,033,792 | 1,031,152 | −2,640 | 17,424 B (1.66 %) |

What actually cost the bytes, from an `nm -S` diff on the C6 ELF:
`picoserve::Error<E>`'s `Debug` −448 B (one `serve error:` line),
`ConnectedInfo` −324, `smoltcp::wire::ip::Address` −248, `esp_hal::spi::Error`
−156, `AuthenticationMethod` −188, `embassy_net::udp::SendError` −76,
`Option<T>`/`[T]`/`[T; N]`/`u8` `Debug` −322, `Upper`/`LowerHex` −212, and
−554 B of the `<&T as Debug>` shims that glue them to `Arguments`. The
`{:?}` that were formatting a `&'static str` (`pipeline::set_protocol`'s
error type) are the cheapest of the lot to fix and the most expensive to
leave: `str`'s `Debug` pulls the `DebugStruct`/`DebugTuple` builders.

**#438's stated lever does not exist.** It reported ~1.26 KB of
`esp_hal::gpio::OutputSignal` / `esp_radio::wifi::DisconnectReason` `Debug`
switch tables as newly linked by #424. Both tables are present, byte for
byte, in the `604bd6a` image from *before* any of that day's merges — the
symbol diff that found them was reading rustc's `.NNNN` local-symbol suffix,
which changes on every build, as a symbol appearing and disappearing. Neither
is reachable from Luxel code either: `OutputSignal`'s comes from an `assert!`
inside esp-hal's `gpio::interconnect::connect_to`, which every SPI build
links, and removing it would mean patching esp-hal. The whole +3,968 B the C6
gained that day is the #373 bulk ops (`Vm::builtin_cold` +2,450,
`bulk::canvas_fill` +928, `paint_canvas` +550) — real features, correctly
measured, that simply did not fit. **When a symbol diff says a `Debug` impl
appeared, strip the `17h<hash>E` and `.NNNN` suffixes first.**

RAM moves the other way, slightly: `.stack` is **−296 to −336 B on every
board** (pixelblaze-v3 25,996 → 25,668 B, 1,092 B above the 24 KB floor;
s3-devkit 33,812 → 33,516; seengreat-hub75 29,340 → 29,012). Nothing here is
a static — that is the `.L_MergedGlobals` repacking this section already
warns about, in the direction that costs. `tools/stack-check.sh` clean on
`board-pixelblaze-v3` and on `board-c6-devkit` + `hosted-ui` (largest frame
`budgeted_engine` at 1,680 B against the 12 KB budget, unchanged).

`board-c6-devkit` with the on-device playground is still at 1.66 % and still
not a release artifact — Gitea #291 / #426 are unchanged by this.

2026-09-08 (later), **absolute build paths out of the image** (Gitea #441 —
`--remap-path-prefix`): **−7.7 to −9.2 KB on every board, the largest single
saving in this table**, and the reason it is more than a diet: the image no
longer carries the directory it was built in, so the same commit weighs
(nearly) the same on every machine. Every dependency file containing a
panicking construct contributes one `core::panic::Location` string, and the
useful part of it is the tail — `esp-hal-1.1.0/src/system.rs`. The 55-70
character prefix in front of it (`/nix/var/nix/builds/nix-<pid>-<rand>/
cargo-vendor-dir/`, `/home/…/.cargo/registry/src/index.crates.io-<hash>/`,
`<xtensa-rust>/lib/rustlib/src/rust/library/` under `-Zbuild-std`) was
build-machine trivia repeated ~140 times: **13.5-14.7 KB of path strings per
image, now 5.4-5.9 KB.** Credless flake builds (`nix build .#luxel-fw-<v>` →
`luxel-fw-ota.bin`), `07b922b` vs this branch. Re-measured on the rebased
tree (`7aa94ca`, library/web/docs commits only): the shipped C6 row is
identical and `pixelblaze-v3` moves 144 B on the *before* side, i.e. inside
the noise floor described below.

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `c6-devkit` + `hosted-ui` *(shipped)* | 1,015,024 | **1,006,752** | **−8,272** | **41,824 B (3.98 %)** |
| `pixelblaze-v3` | 1,014,544 | 1,005,344 | −9,200 | 43,232 B (4.12 %) |
| `athom-music` | 1,014,464 | 1,005,376 | −9,088 | 43,200 B (4.12 %) |
| `esp32-generic` | 1,014,144 | 1,004,912 | −9,232 | 43,664 B (4.16 %) |
| `s3-devkit` | 961,232 | 952,208 | −9,024 | 96,368 B (9.19 %) |
| `s3-devkit` + `hub75` | 967,984 | 959,040 | −8,944 | 89,536 B (8.53 %) |
| `seengreat-hub75` | 977,600 | 968,608 | −8,992 | 79,968 B (7.62 %) |
| `c3-devkit` | 966,160 | 958,464 | −7,696 | 90,112 B (8.59 %) |
| `c6-devkit` *(not shipped)* | 1,031,152 | 1,023,200 | −7,952 | 25,376 B (2.42 %) |

All eight shipped variants pass `tools/image-check.sh`; the shipped C6 is off
the warn line's doorstep at 3.98 % for the first time since the extent
allocator. `board-c6-devkit` with the on-device playground gains 7,952 B but
is still under the 3 % floor (2.42 %) and still not a release artifact —
Gitea #291 / #426 are unchanged.

**What this does to the ±0.7 KB noise floor this section keeps warning
about.** The floor had two components and this removes the larger one. Same
source, same creds, built twice under directory names 47 characters apart
(devshell `board-c6-devkit` + `hosted-ui`): **1,006,912 B vs 1,006,416 B**,
and the `.rs` path strings in the two images are byte-for-byte identical
(5,354 B each, zero absolute paths in either). The residual 496 B is the
`.L_MergedGlobals` / `.Lanon.<hash>` repacking that rustc's `-C metadata`
hash drives — still real, still not your code, but half a kilobyte instead
of the 2.6 KB swing that made the same commit read 1,014,400 / 1,015,568 /
1,017,168 B on three machines and fail the 3 % gate on one of them. A
devshell build with creds and the credless flake image now agree to within
160 B on the C6, where they were ~2.5 KB apart.

Where the flags live: `link_rustflags` and `remap_rustflags` in
`firmware/board-target.sh`, read by `firmware/build-esp32.sh`,
`tools/stack-check.sh` and `flake.nix`'s `buildPhase`. They have to be in one
place because `RUSTFLAGS` **replaces** `firmware/.cargo/config.toml`'s
`[target.*] rustflags` rather than merging with them, so every caller that
exports it must re-supply the linker args too. Adding a board changes
nothing here; adding a *build entry point* means reading those two functions.

Two things that did NOT work, so nobody re-tries them:

- Remapping `/rustc/<commit-hash>/library/` (the dozen surviving `core`
  Locations on the RISC-V boards, 576 B) is a no-op. Those paths are rustc's
  own upstream virtualization of the prebuilt `core`;
  `--remap-path-prefix` matches the *real local* path, not a virtual name
  already baked into the metadata. Only `-Zbuild-std` — via the rust-src
  rule — can reach them, which since 2026-09-19 (#501) both arches do.
- Nothing is lost from `tools/decode-backtrace.sh` or `espflash monitor
  --elf`. The release profile carries no line tables (`[profile.release]` has
  no `debug`), so `addr2line` printed `luxel_fw.<hash>-cgu.0:?` before this
  change and prints exactly that after it; symbol names are untouched. The
  only visible difference is in a *panic message*, which now reads
  `esp-hal-1.1.0/src/system.rs:42` — prefix it with the registry or vendor
  root to open the file.

`.stack` on `board-pixelblaze-v3` 25,652 B (−16 B, i.e. unchanged);
`tools/stack-check.sh` clean on `board-pixelblaze-v3` and on
`board-c6-devkit` + `hosted-ui` (largest frame still esp-storage's 4,144 B
flash bounce buffer).

2026-09-19, **`geom` + `caps` on `/api/status`** (Gitea #464 — the effective
geometry and the capability block the v2 UI gates every screen on):
**+1.5 to +1.8 KB**, credless flake builds against `origin/master` `becc115`.

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `c6-devkit` + `hosted-ui` *(the tightest shipped image)* | 1,007,168 | 1,008,976 | +1,808 | **39,600 B (3.77 %)** |
| `athom-music` | 1,006,624 | 1,008,080 | +1,456 | 40,496 B (3.86 %) |
| `seengreat-hub75` | 969,712 | 971,168 | +1,456 | 77,408 B (7.38 %) |

A strip board and a panel board move by exactly the same amount because none
of it is board-conditional: it is `luxel_core::caps`'s two derivations and two JSON
writers, `Engine::pattern_dims`, `devicemap`'s source/shape accessors and the
render task's publish call (the C6's extra 352 B is RISC-V codegen, not a
different feature set). The shipped C6 is back on the warn line's doorstep at
3.77 % — see the ceiling note above. `.stack` on `board-athom-music` 25,580 B,
`tools/stack-check.sh` clean (largest frame unchanged — esp-storage's 4,144 B
flash bounce buffer; the new `publish_geom` does not appear in the top table).

2026-09-19 (later), **the outpipe scratch is released when every stage is
off** (Gitea #476/#446) and `caps.blur_glow` goes per board: **+80 B on
`board-athom-music` (1,008,080 → 1,008,160), +32 B on
`board-seengreat-hub75` (971,168 → 971,200), 0 B on `c6-devkit` +
`hosted-ui` (1,008,976, still 39,600 B / 3.77 % of slot)**, credless flake
builds against the merge base. `.stack` on `board-athom-music` 25,580 B (unchanged),
`tools/stack-check.sh` clean.

The heap it buys back, measured on metal the same day:

| board | px | idle | one stage on | every stage off again |
|---|---:|---:|---:|---:|
| `seengreat-hub75` | 4096 | 41,612 | 29,324 (−12,288) | **41,612** |
| `athom-music` | 2048 | 55,176 | 49,032 (−6,144) | **55,176** |

Exactly 3 B/px both times, and exactly back. Before this the second column
was where the board stayed until a reboot.

And the number behind `caps.blur_glow = false` on a panel (proposal D12) —
`/api/status` `pipe_us` on the Seengreat panel at 4096 px, against its
`pass.nominal_us` rescan of **8,665 us**:

| chain | `pipe_us` |
|---|---:|
| every stage off | 49 |
| blur 50 % | 4,508 |
| blur 50 % + glow 50 % | **8,780** |

The compose stage alone overruns the rescan window, so the panel shows the
previous frame every time — it halves the refresh rather than dropping
frames. The same pair on the Athom at 2048 px costs 2,443 us against no
display clock at all, which is why strips keep both.

2026-09-19, **projection** (Gitea #473 — the engine mechanism for showing a
pattern of one dimensionality on a Layout of another): **+2,800 B** on the
tightest shipped image, `c6-devkit` + `hosted-ui` credless, measured against
`b7e0226` (1,009,104 → 1,011,904 B, **36,672 B / 3.50 % of the slot free**,
was 3.76 %). Over the 3 % floor by ~5.2 KB, under the 6 % warn line as it
already was.
The firmware wires nothing yet — ticket A4 (#465) does — so this is the
frame loop linking the plan machinery unconditionally:
`Engine::sync_plan`+`compute_plan` ~1.1 KB, `project_replicate` 606 B,
`axis_len` 190 B, `Engine::frame` +460 B (the hoisted coordinate selector),
`drop_in_place<Engine>` +48 B. The §5.4d table as a `const [[[u8;3];7];9]`
lookup instead of a `match` was tried and is **496 B WORSE** on riscv32imc
(the rodata plus two bounds-checked index chains beat nothing the match
was doing) — the match stayed. `tools/ci.sh` green on all three CI images.

2026-09-19 (later still), **the device output chain moves into `luxel-core`**
(Gitea #466 — `outpipe::DeviceChain`, so the wasm playground runs the same
chain), re-measured on the tree rebased over #473:

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `c6-devkit` + `hosted-ui` *(the tightest shipped image)* | 1,011,904 | 1,011,776 | **−128** | 36,800 B (3.51 %) |
| `athom-music` | 1,011,568 | 1,011,664 | +96 | 36,912 B (3.52 %) |
| `seengreat-hub75` | 974,640 | 974,640 | **0** | 73,936 B (7.05 %) |

Credless flake builds against `origin/master` `01ea1ea`. `.stack` on
`board-athom-music` 25,476 B, `tools/stack-check.sh` clean.

**Free, to within the noise floor** — which is the point worth recording: the
chain is the same code either way, and moving it across a crate boundary
behind a generic `FnOnce` palette-stop hook and a `ChainSettings` struct (in
place of six inline atomic loads) costs the Xtensa panel board nothing, the
Xtensa strip board 96 B, and the RISC-V C6 −128 B. An earlier measurement of
this same branch against `61ad087` read +608 B on the C6; re-measured against
`01ea1ea` it reads −128 B. Nothing in the firmware changed between those two
measurements (#462 is web-only) — that ±736 B swing IS the
`.L_MergedGlobals`/`-C metadata` repacking the noise-floor note below
describes, and it is the reason a single-board single-measurement delta under
~1 KB should not be quoted as a cost.

**Watch the C6 row anyway.** `c6-devkit` + `hosted-ui` is the tightest SHIPPED
image, and the four PRs of 2026-09-19 (#464, #476, #473, #466) together took it
from 1,007,168 B to 1,011,776 B — **4.00 % → 3.51 %** of slot free. Above
image-check's 3 % hard floor and CI is green on all three release images, but
that is ~5.9 KB of runway and two more days like this would spend it.

2026-09-19, **the OTA-slot diet** (Gitea #501 — the survey that priced eight
candidates on real builds; this is the three that were low-risk): **−4.3 to
−26.4 KB on every board**, and the tightest shipped image, `c6-devkit` +
`hosted-ui`, goes **3.45 % → 4.70 %** of slot free. Three independent pieces:

- **`core`/`alloc` built from source with `optimize_for_size` on both
  arches** (`-Zbuild-std=core,alloc -Zbuild-std-features=optimize_for_size` in
  firmware/build-esp32.sh, tools/stack-check.sh and flake.nix). Measured on
  its own on `c6-devkit` + `hosted-ui`: `-Zbuild-std` alone **−6,992 B** — a
  from-source `core` joins the binary's own fat LTO instead of arriving
  prebuilt at opt-level 3 with an optimization boundary in front of it — and
  `optimize_for_size` a further **−5,952 B**. The Xtensa boards were always
  `-Zbuild-std` (there is no prebuilt `core` for the Espressif fork), so only
  the size half is new there: **−4,368 B** on `pixelblaze-v3`. This is the
  whole delta on every board that keeps the takeover.
- **`wled-takeover` is per board**: **−24,656 B** (`pixelblaze-v3`) /
  **−25,344 B** (`c6-devkit` + `hosted-ui`) on the boards that drop it, which
  are `board-pixelblaze-v3` (a stock PB v3 is flashed over serial) and
  `board-seengreat-hub75` (ships XiaoZhi). See "Supported boards" above and
  docs/wled-migration.md.
- **The driftsort family, for one `sort_unstable_by_key`** on a 0/1 key over a
  handful of partition entries in takeover.rs: `quicksort` + `sort4_stable` +
  `bidirectional_merge` + `heapsort` + `median3_rec` + `ipnsort`, **2,415 B**
  in a devshell A/B, replaced by a hand-rolled stable partition. This is the
  one piece the boards that KEEP the takeover also get (`nm` on the athom
  image finds none of those six symbols now), but it is not separable from
  the build-std delta in the table below: those rows move −4.3 to −4.4 KB
  total, so the two contributions together are that, not each.

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `c6-devkit` + `hosted-ui` *(shipped)* | 1,012,384 | 999,200 | −13,184 | 49,376 B (4.71 %) |
| `pixelblaze-v3` † | 1,011,648 | 985,248 | −26,400 | 63,328 B (6.04 %) |
| `athom-music` | 1,011,680 | 1,007,248 | −4,432 | 41,328 B (3.94 %) |
| `esp32-generic` | 1,011,264 | 1,006,896 | −4,368 | 41,680 B (3.97 %) |
| `c3-devkit` | 964,832 | 953,200 | −11,648 | 95,376 B (9.10 %) |
| `s3-devkit` | 958,240 | 953,920 | −4,320 | 94,656 B (9.03 %) |
| `s3-hub75` | 965,360 | 960,928 | −4,432 | 87,648 B (8.36 %) |
| `seengreat-hub75` † | 974,640 | 948,896 | −25,744 | 99,680 B (9.51 %) |
| `c6-devkit` *(not a release artifact)* | 1,029,120 | 1,016,224 | −12,800 | 32,352 B (3.09 %) |

Credless flake builds (`nix build .#luxel-fw-<v>` → `luxel-fw-ota.bin`), both
columns against `origin/master` `eeb6e03`; † = `wled-takeover` dropped on this
board; every other row is the build-std change plus the driftsort removal.
`.stack`
(`tools/stack-check.sh`, no function over the 12,288 B budget in any of the
four):

| | before | after |
|---|---:|---:|
| `board-pixelblaze-v3` | 25,492 | 25,556 |
| `board-pixelblaze-v3` + `small-chip` | 26,892 | 26,956 |
| `board-c6-devkit` | 137,656 | 137,552 |
| `board-c6-devkit` + `small-chip` | 147,248 | 147,144 |

**The full-UI C6 is back over the floor, and that is not enough to restore it
as a release artifact.** `c6-devkit` without `hosted-ui` goes 1.86 % → 3.08 %,
which clears image-check's 3 % gate by 832 B — inside one swing of the ±0.7 KB
noise floor this section warns about. Gitea #291 stays open and the release
matrix is unchanged; what changed is that the gap it has to close is now
~0.5 pp instead of ~1.5 pp.

**Measured and rejected, so nobody re-tries them.** Per-package
`[profile.release.package.X] opt-level = "z"` across eight dependency crates
(smoltcp, rust-mqtt, sequential-storage, picoserve, embassy-net, edge-dhcp,
esp-radio, esp-hal) made the `pixelblaze-v3` image **11,504 B BIGGER** — "z"
costs the inlining that fat LTO then cannot recover. `ESP_LOG` in
firmware/.cargo/config.toml is a **runtime** filter, not a compile-time one:
`info,esp_rtos::task=debug` → `error` moved the image 64 B. And **`.rodata` is
not free**: a 16 KiB live `#[used]` array cost **exactly +16,384 B** of image
on `pixelblaze-v3` AND on `c6-devkit` + `hosted-ui`. A sub-KB table can still
land inside whatever segment-alignment slack exists at that moment, but that
window is a one-off of unknown size — trading code for tables is not a size
strategy.

**Still on the table, unspent** (both cost a shipped feature, so they are
per-board profile levers like `hosted-ui`, not fleet diets): MQTT behind a
cargo feature is **−36,992 B** and the DDP/E1.31/sync inputs behind features
are **−9,520 B**, both measured on `pixelblaze-v3`. The durable answer to the
slot remains the repartition — #501 option 3.
2026-09-19 (last of the day), **per-item projection on the playlist** (Gitea
#470 — a playlist item may carry a `P <mode>` line beside its `C` values, and
the scheduler applies it when the item activates):

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `c6-devkit` + `hosted-ui` *(the tightest shipped image)* | 999,200 | 1,000,128 | **+928** | 48,448 B (4.62 %) |
| `athom-music` | 1,007,248 | 1,008,032 | +784 | 40,544 B (3.86 %) |

Credless flake builds against `origin/master` `184fedc` — i.e. on top of the
#501 diet above, which is why the absolute numbers are ~12 KB below the #466
row. `.stack` on `board-pixelblaze-v3` 26,956 → **26,892 B** (−64;
`small-chip` profile), `tools/stack-check.sh` clean at both profiles. The same
branch measured against pre-#501 master read +544 B on the C6 rather than
+928; both are inside the sub-kilobyte noise band the #466 entry describes, so
read this as "well under a kilobyte", not as a figure to three digits.

Under a kilobyte because the expensive half was already linked: #473 put the
whole projection plan machinery in `Engine::frame` unconditionally, so the
firmware pays only for `ProjectionMode`'s `FromStr`/`as_str` (two small
matches it had never called), one `Msg::Projection(u8)` arm in the render
task, and an `Option<u8>` on the playlist `Item`. `Msg` is sized by
`Code { Vec, String }`, so the new variant does not grow the 8-deep message
channel. Deliberately no `format!` and no new generic instantiations — the
`P` line is parsed by the same `split_whitespace` walk the `C` line uses, and
emitted with `push_piece`.

2026-09-19 (A4), **`/api/layout`** (Gitea #465 — the one geometry object),
measured on top of the #501 diet against `origin/master` `184fedc`:

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `c6-devkit` + `hosted-ui` *(tightest gated image)* | 999,200 | 1,014,144 | +14,944 | 34,432 B (3.28 %) |
| `pixelblaze-v3` † | 985,248 | 1,001,296 | +16,048 | 47,280 B (4.51 %) |
| `c3-devkit` | 953,200 | 970,080 | +16,880 | 78,496 B (7.49 %) |
| `athom-music` | 1,007,248 | 1,022,912 | +15,664 | **25,664 B (2.44 %)** |

† `wled-takeover` dropped on this board by #501.

**The three CI-gated variants clear the 3 % floor; `athom-music`, which is
published but not gated, does not.** That is not a #465 fact so much as a
`CI_VARIANTS` fact that #465 exposed: docs/releases.md has `pixelblaze-v3` in
the gate because it "stands in for athom-music, esp32-generic, s3-devkit,
s3-hub75 and seengreat-hub75" — and #501 stopped that being true by dropping
`wled-takeover` on `pixelblaze-v3` and keeping it on `athom-music`. The two
now differ by ~21.6 KB, so the stand-in reads 4.51 % while the board it
stands for reads 2.44 %. Tracked as Gitea #513; the same shape as the
breakages docs/releases.md already records for gating one board.

Where the +14.9 KB sits on the C6 (`nm --print-size`, riscv32imc,
`opt-level="s"` + fat LTO + build-std `optimize_for_size`):
`luxel_core::layout::parse` ~4.9 KB (the line grammar),
`Layout::push_json` ~1.3 KB, `luxel_fw::layout::store` ~1.4 KB, `json` 836 B,
`init` 798 B, `set_from_wire` 686 B, `note` 432 B, `push_output` 328 B,
`current` 272 B — plus ~1 KB in the server's flat dispatcher and ~800 B in
`main`. Two size fixes that paid and are worth reusing: persisting the Layout
as **its own POST wire** re-parsed at boot rather than a binary record (one
codec instead of a serializer plus a deserializer, **−1.9 KB**), and a
hand-rolled decimal `num()` in place of `str::parse` (**−1.3 KB** —
`from_str_radix` is ~700 B per integer width and the grammar wanted three).
`sort_by_key` over the output table was dropped for an ordered insert:
driftsort is a **4,144 B stack frame** in the web task, which
`tools/stack-check.sh` surfaced — the same family #501 removed from
takeover.rs, found independently on the stack side rather than the image side.

2026-09-19 (A13), **multiple outputs** (Gitea #474 — each output drives a
consecutive run of the one Layout), credless flake builds against
`origin/master` `9ea68f9`:

| variant | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `athom-music` *(the only 2-output board)* | 1,022,912 | 1,027,232 | **+4,320** | **21,344 B (2.03 %)** |
| `pixelblaze-v3` | 1,001,296 | 1,002,064 | +768 | 46,512 B (4.43 %) |
| `c6-devkit` + `hosted-ui` *(tightest gated image)* | 1,014,144 | 1,014,944 | +800 | 33,632 B (3.20 %) |

The second driver instance itself is behind a `multi_output` cfg (build.rs,
set from the board feature and asserted against `board::OUTPUTS`), so it is
**+3.5 KB on the Athom and nothing anywhere else**: a second `SpiDma` +
`EncodeBuf`, the boot wiring for SPI3/DMA_SPI3, and the per-frame second
`write_run` + colour-order fix-up. The +576/+272 every strip board does pay is
the split ARITHMETIC, which is unconditional on purpose — `Layout::run_of` and
`Run::clamped`/`index` in `Protocol::encode_run`, plus the parser's
duplicate-pad check — and it buys **`rev` on a single output** (a strip wired
from the far end) and a driver that clamps instead of indexing past a stale
table on every board, not just the Athom. (+768 vs +800 on two different
arches for the same code is the ±0.7 KB repacking noise this section warns
about further down; the honest reading is "under a kilobyte".)
Reusing the SAME backend (SPI, already linked) rather than adding RMT beside
it is what keeps this at 4 KB: the #474 survey priced a second protocol
backend at 8–12 KB.

`athom-music` is back under image-check's 3 % floor at 2.03 %, which it was
already under before this change (2.44 %, Gitea #513 — it is published but not
CI-gated, so nothing runs image-check on it). The two gated images stay over
the floor.

**`.stack` needed a kilobyte of heap to stay legal, and master was already
under.** The second driver's task statics cost 120 B of the classic ESP32's
leftover stack, and `board-athom-music` was **already 68 B below**
`tools/stack-check.sh`'s 24,576 B floor on `origin/master` `9ea68f9`
(24,508 B) — nothing catches it because CI stack-checks `pixelblaze-v3`,
which sits at 24,580 B, four bytes over. So this change takes 1 KB out of the
classic-ESP32 heap **on the two-output board only**
(`80 * 1024 - SECOND_OUTPUT_RAM` in main.rs), which puts `board-athom-music`
at **25,412 B** (27,004 B with `small-chip`), both clean. Measured, all
`tools/stack-check.sh`:

| build | master | this change |
|---|---:|---:|
| `board-athom-music` | 24,508 **FAIL** | **25,412 ok** |
| `board-athom-music` + `small-chip` | — | 27,004 ok |
| `board-pixelblaze-v3` | 24,580 ok | 24,580 ok (unchanged) |
| `board-c6-devkit` + `hosted-ui` | ok | ok |

The four-byte margin on `pixelblaze-v3` is the real finding here and is NOT
fixed by this change — Gitea #515.

### Two outputs on the Athom

The board breaks out two clocked LED channels — DATA1/CLK1 on GPIO18/5 and
DATA2/CLK2 on GPIO17/16 — and since #474 the firmware drives both. Channel 1
is SPI2 (HSPI) as always; channel 2 is SPI3 (VSPI) with `DMA_SPI3`, built at
boot only when the stored Layout has an `out 1` line, so a board with one
strip touches neither the peripheral nor the pad. GPIO16 is in the board's
`RESERVED_PINS` (that SPI's clock), which is why it is no longer offered by
`/api/config`'s `data_pins` list; GPIO17 is not reserved — the DATA pad is
whatever `out 1` names, exactly like output 0's.

```text
strip 120
out 0 18 ws2812 rgb 60
out 1 17 ws2812 rgb 60 rev
```

One pixel space, one pattern, one brightness: output 0 lights pixels 0–59 and
output 1 pixels 60–119, the second run wired backwards. Reboot to apply (the
table is built once, at boot); `out none` goes back to the single implicit
output. Full semantics in docs/api.md, the driver in docs/firmware.md.

**Measured on the rig** (192.168.0.183, v0.1.40, slot ota_0, 60 px WS2812 on
DATA1, 2026-09-19), `/api/status` `out_us`:

| configuration | `out_us` | `heap_free` |
|---|---:|---:|
| 1 output, 60 px (as found) | 2,519–2,531 | 83,972 |
| `out 0 … 30` posted, BEFORE the reboot | 1,495–1,507 | — |
| `out 0 … 30` + `out 1 17 … 30 rev`, after the reboot | **2,825–2,830** | 83,836 |
| `out 0 18 ws2812 rgb 60 rev` (one reversed output) | 2,540–2,547 | 83,832 |

Three things fall out of that. The split is **live** — posting the table
halved output 0's wire time immediately, before any reboot, because the runs
are re-read from the Layout per frame; only the second DRIVER waits for the
reboot. The outputs are **sequential**: two 30 px runs cost 2,827 us against
one 60 px run's 2,524 us, and the +302 us is exactly the second WS2812 latch
tail (90 B at 2.4 MHz = 300 us) — splitting a fixed pixel count does not halve
wire time. And a **reversed** run costs ~20 us at 60 px, i.e. nothing.

The second encode buffer costs 136 B of heap at 30 px; the 1 KB step between
this board's heap before and after #474 is `SECOND_OUTPUT_RAM` (above), not
the buffers.

**Not verified: what the LEDs actually do.** Nothing is wired to DATA2 on the
bench and the agent has no eyes on the strip, so "the first 30 LEDs show the
first half of Rainbow" is inferred from `out_us` and the readback, never seen.
Gitea #518 has the 10-minute bench procedure.

Since Gitea #328 the hot half of the interpreter can execute from internal
SRAM (`.rwtext`) instead of through the flash instruction cache. What each
board takes is `IRAM` in `firmware/board-target.sh` (and `iram` in flake.nix's
`firmwareVariants` — the two must agree); `IRAM_OFF=1` builds the same board
without it. The mechanism, the measured wins and the rule for adding hot code
are in docs/firmware.md, "Code placement".

**The budget is not the same kind of thing on every chip.** On the classic
ESP32, IRAM is a dedicated 128 KB region (SRAM0) that the stack never comes
out of, so the only ceiling is the region itself. On the ESP32-S3 and the
C-series it is the *same* SRAM as `.data`/`.bss`/`.stack`: every byte of
`.rwtext` is a byte off the main-task stack, which `tools/stack-check.sh`
floors at 24 KB.

Measured on master `e08b2b2` + #328 (devshell builds, WiFi creds baked in):

| board | `IRAM` | `.rwtext` | + WiFi's | of region | `.stack` | `.stack` with `IRAM_OFF=1` |
|---|---|---:|---:|---:|---:|---:|
| `board-pixelblaze-v3` | vm + builtins + math | 42,644 | 94,444 | 130,048 | 24,932 | 24,932 |
| `board-athom-music` | vm + builtins + math | 42,644 | 94,444 | 130,048 | 24,932 | 24,932 |
| `board-esp32-generic` | vm + builtins + math | 42,644 | 94,444 | 130,048 | 24,932 | 24,932 |
| `board-s3-devkit` | vm | 24,468 | 56,592 | 302,080 | 32,732 | 46,300 |
| `board-seengreat-hub75` | vm | 24,360 | 56,484 | 302,080 | 32,452 | 46,020 |
| `board-c3-devkit` | — | 4,968 | — | — | 35,448 | 35,448 |
| `board-c6-devkit` | — | 6,364 | — | — | 137,056 | 137,056 |

- **The three classic-ESP32 boards take everything.** 35,604 B of the region
  is still free afterwards and `.stack` does not move at all. The win is the
  largest in the fleet: 2.86× on `perlin-fire-wind-tunnel`, 4.17× on
  `kaleidoscope-2d` (docs/firmware.md).
- **The S3 boards take `iram-vm` only.** Adding `iram-builtins` on top
  measured 25,148 B of `.stack` — 572 B over the floor — and bought about
  1 %, because the S3's cache is not the bottleneck there. Not worth the
  margin.
- **The RISC-V boards take nothing yet.** There is no C3 or C6 on the bench,
  so their placement is unmeasured, and the C6 owns the fleet's tightest
  OTA-slot margin. `RISCV_IRAM="iram-vm …" BOARD=board-c3-devkit
  ./build-esp32.sh` is the lever for whoever gets one on metal (Gitea #337).

Slot cost: the classic-ESP32 app image is ~3.4 KB **smaller** with the
placement on (997,472 vs 1,000,848 B on `board-athom-music`) — the bytes move
from the 64 KB-page-aligned flash text segment into the IRAM segment. Every
board stays over image-check's 3 % floor; `board-c6-devkit`, which carries no
IRAM features at all, still pays ~1.1 KB for the builtin hot/cold split
(3.27 % → 3.17 % of slot free).

## JIT: which boards compile patterns to native code

Since Gitea #658 a board CAN compile the running pattern to Xtensa machine
code at activation instead of interpreting it (docs/jit-design.md,
docs/firmware.md "JIT"). Since #665/#666 there is a backend for **every
Xtensa board in the fleet**, and since #676 every one of them **ships it,
ON** — the two halves differ only in where the code goes:

- **The two S3 boards.** Their code lives in the PSRAM arena, so it costs no
  internal SRAM at all. `POST /api/jit {"on":false}` is the kill switch for
  one session; a reboot comes back native.
- **The three classic-ESP32 boards.** A 24 KB `.rwtext` static in SRAM0,
  split into two 12 KB halves so a crossfade can hold both images; SRAM0 is
  a dedicated instruction region, so it costs flash image and not one byte
  of stack. Built and verified on metal (1.9–5.2× on the Athom, "JIT on
  metal" below), and shipped since #676: `board-target.sh` sets `JIT=1`
  there like everywhere else, and the `CLASSIC_JIT=1` lever #666 carried is
  gone. The arithmetic did not change, the slot it is weighed against did —
  these images are ~1,120–1,138 KB, which clears the 1,310,720 B slot the
  #501 repartition gave the 4 MB boards with 13–15 % to spare and misses the
  pre-#501 1 MiB slot by ~72 KB. #676 retired the migrating-release gate
  rather than the tier; the price is that a device still on the old table
  needs a `JIT_OFF=1` build over the air first ("The migrating release"
  above).

`LUXEL_JIT_ENABLED` defaults to `true` in every image that carries the
backend. It is a cargo feature — `JIT` in `firmware/board-target.sh`,
mirrored by `extraFeatures = [ "jit" ]` in flake.nix's `firmwareVariants`,
and the two must agree — and `JIT_OFF=1` builds the same board without it,
which is both the A/B lever and the image a stale device migrates on.

| board | JIT | exec memory | per image | of `library/` | app image | free of 1.25 MiB | `.stack` |
|---|---|---|---:|---:|---:|---:|---:|
| `board-seengreat-hub75` | ships, **on** | PSRAM arena | 128 KB | all 307 | 1,137,872 | 63.8 % † | 26,268 |
| `board-s3-devkit` | ships, **on** | PSRAM arena, or heap alias | 128 KB / 8 KB | all 307 / — | not rebuilt | — | not measured |
| `board-athom-music` | ships, **on** | 24 KB `.rwtext` | 12 KB | 96 % | 1,207,232 | 7.9 % | 24,380 |
| `board-esp32-generic` | ships, **on** | 24 KB `.rwtext` | 12 KB | 96 % | 1,136,256 | 13.3 % | 23,476 |
| `board-pixelblaze-v3` | ships, **on** | 24 KB `.rwtext` | 12 KB | 96 % | 1,122,768 | 14.3 % | 23,524 |
| `board-c3-devkit` | no backend | — | — | — | not rebuilt | — | unchanged |
| `board-c6-devkit` | no backend | — | — | — | not rebuilt | — | unchanged |

The **two bench-board rows were re-measured on master `6855bf5`** and
deployed to metal (2026-09-24, the Phase B+C deploy); the other rows are
still the 2026-09-24 pre-Phase-B/C branch measurement and were not rebuilt.
`.stack` from `tools/stack-check.sh`, image bytes from
`tools/image-check.sh`. Phase B+C cost the Athom ~67 KB of slot margin
(13.0 % -> 7.9 % free) and the panel ~71 KB, which the 3 MiB slot does not
notice. The three classic rows share a feature list, so read the
`esp32-generic` and `pixelblaze-v3` rows as ~67 KB optimistic until they
are rebuilt. † the Seengreat's slot is
3,145,728 B, not 1,310,720 B, so its column is free-of-3-MiB. Read the
classic rows twice: they clear the **post**-migration 1.25 MiB slot with
8–14 % to spare, and they miss the old 1,048,576 B slot by ~72 KB. Since
#676 that second number gates no release — but it is still the whole of what
a device on the pre-#501 table can accept, which is why such a device takes
a `JIT_OFF=1` build before it can take one of these.

**`board-s3-devkit` was not built or measured tonight** — it is the same
target and the same feature list as the panel minus `hub75`, so its numbers
should track, but nothing here is a measurement of it. Likewise the two
RISC-V images were not rebuilt: they link no emitter at all (see the last
bullet), and `tools/image-check.sh` in `tools/ci.sh` is what catches it if
that ever stops being true.

- **Xtensa only.** `luxel-jit` has one backend, LX6/LX7; the RISC-V boards
  have none and the `jit` feature refuses to compile for them. `jit.state`
  is `"off"` on a C3 or C6 forever, not "not yet".
- **The classic tier exists now, and the reason it exists is a
  measurement.** The design (§4, decision 4) put the S3 architecture first
  and deferred the classic part because nobody had measured the win on one.
  Now someone has — 1.9–5.2× on the Athom — and since #676 that measurement
  ships: all three classic variants in flake.nix carry
  `extraFeatures = [ "jit" ]`. `nix build .#luxel-fw-esp32-generic-jit` is
  therefore the same image as `luxel-fw-esp32-generic` now; the name is kept
  because QEMU models the classic ESP32 and not the S3, so that output is
  what `tools/qemu/jit-test.py` asks for and the only image on which emitted
  code can be executed without hardware.
- **The S3 no longer buys its buffer from `iram-vm`.** Phase 3 had to: its
  exec buffer was a 14 KB `.rwtext` static, and `.rwtext` and `.stack` are
  one budget on this chip (see "IRAM budget" above), so the interpreter's
  per-pixel loop was traded away for it. #665 moved the code into the PSRAM
  arena — which costs no internal SRAM at all — and `iram-vm` came back:
  `.stack` 26,268 B with both, against 25,484 B in phase 3 (static, no
  `iram-vm`) and 27,364 B interpreter-only. That matters precisely because a
  pattern the JIT refuses runs on the interpreter, on the same board.
- **The classic ESP32 still uses a static, and that is the right answer
  there.** SRAM0 is a dedicated 128 KB instruction region, separate from the
  DRAM `.stack` comes out of, so 24 KB of `.rwtext` costs flash image and
  not one byte of stack: `.rwtext` 67,224 + `.rwtext.wifi` 51,800 of 131,072
  on all three boards, ~12 KB spare. There is no PSRAM on these parts and
  SRAM0 is the only instruction-bus RAM, so there is nothing else to use.
- **"of `library/`"** is how many of the 307 patterns fit the per-image cap
  (`cargo test -p luxel-jit --test compile_all -- --nocapture` prints the
  curve). At the S3's 128 KB the cap binds nothing — the largest image in
  `library/` is a tenth of it. At the classic 12 KB the tail refuses with
  `too-large` and is interpreted: a correct outcome, visible in
  `/api/status`'s `jit.reason`, not a failure. The S3's 8 KB internal-alias
  fallback is tighter still, and is a fallback, not a target — no PSRAM-less
  S3 is on the bench to measure its share.
- **The non-JIT boards link no emitter.** The firmware depends on luxel-core
  with `default-features = false` and names neither `jit` nor `luxel-jit`
  unless a board asks, so a C3 or C6 image carries none of it (the same
  property #642's phase-1 zero delta had). Nothing was rebuilt to re-confirm
  that tonight; CI's image-check is the standing gate.

## Hosted-UI builds (no on-device web app)

`hosted-ui` is a cargo feature, not a board — combine it with a board
feature to build a device that carries **no playground at all** and points
at the hosted one instead (Gitea #11):

```sh
EXTRA_FEATURES=hosted-ui BOARD=board-c6-devkit firmware/build-esp32.sh
EXTRA_FEATURES=hosted-ui BOARD=board-c6-devkit tools/stack-check.sh
nix build .#luxel-fw-c6-devkit-hosted     # the one shipped variant
```

Normally the playground lives in the `assets` partition (0x310000, 0xF0000)
as a LUXA archive, and `/` serves `/index.html` out of it. With `hosted-ui`:

- `src/assets.rs` keeps only `read_chunk` — the tree's stack-safe flash
  reader, which ota.rs and takeover.rs use and which is not asset-specific.
  The TOC, the archive parser and `AssetWriter` are gone.
- `server.rs` loses the streaming `FlashAsset` body, the ETag/`If-None-Match`
  304 path, `HVal::Owned`, the `/assets/` cache-control policy and the
  `POST /api/assets` installer (which answers a plain "this image has no
  on-device web app" instead of 404ing, so `tools/deploy.sh --assets-only`
  says something useful).
- `/` always serves the embedded fallback page — the same one `/min` serves —
  which links to `https://googlebot42.github.io/luxel/?device=http://<this
  host>`, built client-side. `firmware/build.rs` swaps that page's
  "install the UI with tools/deploy.sh" paragraph for a hosted-build one, so
  it never advertises a route the image doesn't have.
- `build-esp32.sh` neither packs nor writes the bundle: a `flash` leaves the
  assets partition alone and an `image` composes a full-flash binary without
  it. The release workflow does the same for `luxel-c6-devkit-hosted`.

What it buys: **~14 KB of the OTA slot on every board** and ~4.7 KB of
DRAM back into the main stack (numbers in the ceiling section above), plus
the ~641 KB bundle never having to be written to a device at all. The
`assets` partition's 983,040 B stays allocated-to-nothing; reclaiming it
needs a third partition table and the migration to carry devices onto it —
the mechanism now exists (#501), the table does not. Still Gitea #199, not
this mode.

What it costs: the device is **useless without internet** (or at least
without a copy of the playground hosted somewhere), which is the opposite of
the product's normal promise — hence a feature and not a default. The hosted
copy is https and devices are http, so the browser's mixed-content / Local
Network Access handling matters more here than anywhere else (Gitea #162).

**Hardware-verified 2026-08-31** (Gitea #198, Athom rig, `board-athom-music`,
OTA onto a device whose assets partition still physically held the previous
image's LUXA archive): boots to `assets: hosted-ui build, no on-device web
app`, no panic and no boot-guard rollback across a 303-pattern soak, `/` and
`/min` serve the embedded page while the stale on-flash bundle stays
invisible, `/assets/…` 404s, `POST /api/assets` refuses with the explanatory
body without wedging a socket-pool slot, and the API is at parity with the
normal build (302/303 clean, ~16 KB more free heap). The one leg that could
not be closed from the container is the **https** Pages copy reaching the
http device — Gitea #162, a browser-permission gap that needs a headful
browser, not a firmware one; the same app served over plain http drives the
device fine. `tools/image-check.sh` asserts the mode in both directions when
`EXPECT_FEATURES` names it: the `assets: hosted-ui build` boot line must be
present *and* the LUXA reader's strings must be absent, so a hosted image
that silently kept the asset code fails the build rather than quietly giving
back the saving.

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

A `const` assertion in board.rs fails the build if the **default** panel's
area ever exceeds its board's cap, so the half-dark panel that shipped
between #72 and #74 cannot come back silently. Since #401 the panel is a
stored setting, so the same cap is also checked at boot and by the layout
parser: a CONFIGURED panel over 4096 pixels is refused, and the boot falls
back to the board default rather than shipping a half-dark panel (see "Panel
driver settings"). Raising the cap so a real chain fits is a follow-up ticket
with #255.

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

## Scene layers: how many a board affords (Gitea #479)

A scene's **pattern layers** each cost a resident engine, and `caps.layers`
on `/api/status` is what the editor budgets against. The number is derived,
not per-board configuration, by `luxel_core::caps::layers_for_headroom`:

```
layers = clamp(1, min(tier, (headroom − stage) / layer_cost), ceiling)
  tier       = caps::layers_for(pixel_count)    — 3 at ≤512 px, else 2
  headroom   = budget::load_headroom(shared::HEAP_BASE_MAX)
             = (max since boot of the render task's measured load_base)
               − RUNTIME_FLOOR (20 KiB)
  stage      = budget::compositor_scratch(pixel_count) = pixel_count × 3
  layer_cost = budget::LAYER_BASE (4 KiB) + pixel_count × 3
             = budget::LAYER_BASE alone on a `psram-arena` board
  ceiling    = 2 on a `small-chip` board, else caps::MAX_LAYERS (4)
```

**On a `psram-arena` board a layer's frame is not internal DRAM** (Gitea
#709). Each engine's per-frame RGB888 buffer comes from the PSRAM arena
alongside its pattern arrays, so `layer_cost` is the flat `LAYER_BASE` and
nothing more — 4 KB instead of 16.4 KB at 4096 px. `budget::layer_cost`,
`layer_fits[_with]` and `caps::layers_for_headroom` all take that as a
`frame_external` argument, which every host passes as
`luxel_core::arena::frames_external()`; on every board without an arena it
is `false` and the arithmetic is byte-for-byte what it was. The staging
frame is NOT moved — see "Engine frames in PSRAM" below for why.

**The staging frame comes off the top** (Gitea #704). Every scene
composites into the sink's staging buffer — 12.3 KB at 4096 px — and that
is spent before a single layer engine is built, so it is not headroom a
layer can have. Leaving it out is what let the panel advertise 2 and refuse
the second layer at activation.

**The headroom is a high-water mark, not a live reading.** Measured on the
Seengreat panel 2026-09-24, four *identical* pattern activations reported
`heap_free` 18,904 / 23,000 / 33,332 / 37,508 — ±18 KB of WiFi and HTTP
transient against a ~16 KB per-layer cost. An advertised capability derived
from the instantaneous number flapped 1 ↔ 2 with nothing but poll traffic.
`shared::HEAP_BASE_MAX` folds each reading into a maximum, which converges on
the idle figure in a few samples and cannot over-promise on a board that
never reaches it.

`layer_cost` is a frame buffer plus a flat base rather than a per-pattern
model, because it sizes an *advertised* number and a *pre-flight* refusal.
The real gate stays the post-build `RUNTIME_FLOOR` check in
`try_budgeted_engine`, which measures the engine that was actually built; a
layer that fails it is reported as `scene: layer N does not fit` on activate
and renders nothing, leaving the rest of the scene up.

`LAYER_BASE` = 4 KiB comes from the measured fleet
(`docs/design/webui-v2/research/engine-constraints.md` §2): a rainbow-class
engine on the S3 panel costs ~17 KB of which 12.3 KB is its own frame; an
arena-backed Aurora 2D costs 14 KB of which 12.3 KB is its frame.

Taking `headroom` against `heap_free + engine_heap` rather than bare
`heap_free` is deliberate: the advertised number would otherwise drop every
time a scene loaded, which is exactly when a UI is reading it.

**Two pattern layers fit the Seengreat panel at 4096 px since Gitea #709.**
They did not before it: `RUNTIME_FLOOR` 20,480 + two `layer_cost` 16,384 +
the 12,288 staging frame is 65,536 B against a measured steady `load_base`
of 47–49 KB, so the board advertised 2 (off a boot-time `HEAP_BASE_MAX`
~16 KB above steady state) and refused layer 2 at activation. With the
engine frames in the PSRAM arena the same stack is 20,480 + 2×4,096 +
12,288 = **40,960 B**, which the low end of that steady range affords
outright — the advertised 2 no longer depends on the high-water mark being
generous. Measured on metal 2026-09-24: a two-pattern scene activates,
`engines: 2`, both layers JIT-native.

### What compositing a scene actually costs

Measured on the panel 2026-09-24 at 4096 px, `/api/status` `frame_us`
averaged over four one-second samples, Aurora 2D native on the JIT:

| what is running | frame_us | compose |
|---|---:|---:|
| `Aurora 2D` bare (`emit!`, no compositor) | 51,114 | — |
| a scene of `pat(Aurora 2D)` + a colour band | 51,768 | **654 µs** |

Gitea #705 reported that second row at 106,679 µs and read the 54 ms
difference as compositing cost. It was not: the scene's staging buffer had
made the board 12.3 KB poorer (#704), the JIT then refused the base layer
(`jit: interp/no-memory`) and Aurora 2D ran interpreted. **A doubled
`frame_us` on this board is a JIT fallback until `/api/status` `jit.state`
says otherwise** — read that field before attributing a frame to any stage.

The 654 µs is a full-layout pattern layer plus a colour band over 4096 px.
A full-layout, opaque, unkeyed, unmirrored `normal` layer is a
`copy_from_slice` since #705; everything else walks rows, not cells.

| board / layout | load_base | layer_cost | layers |
|---|---:|---:|---:|
| Seengreat S3 @4096 px (design's 2026-09 figure) | 68.7 KB | 16.4 KB | **2** (tier) |
| Seengreat S3 @4096 px, MEASURED 2026-09-24, frames in DRAM | 46.5 KB | 16.4 KB | **1** |
| **Seengreat S3 @4096 px, MEASURED 2026-09-24, frames in PSRAM (#709)** | **47.0 KB** | **4.1 KB** | **2** |
| Seengreat S3 @4096 px, device blur+glow on | 26.9 KB | 16.4 KB | **1** |
| Athom / classic ESP32 @300 px | 122.8 KB | 4.9 KB | **3** (tier) |
| classic ESP32 @1024 px | 108 KB | 7.2 KB | **2** (tier) |
| c3-devkit / c6-devkit (`small-chip`) | — | — | **2** (ceiling) |

**The panel advertises 2 and delivers 2** (since Gitea #709; on metal
2026-09-24). Until then it advertised 2 and delivered 1: a real 4096-px
engine cost 15.3 KB (`_Fairies`) to 19.7 KB (`Aurora 2D`) against a
steady-state pool of ~46.5 KB, so the *second* one landed under the 20 KiB
`RUNTIME_FLOOR` and `budget::layer_fits` refused it at activation:

```
"vmerr":"scene: layer 2 does not fit"   "engines":1
```

That refusal is still the authoritative gate, and it is still a split rather
than a failure: the refused layer becomes a no-op slot and every other layer
draws. `caps.layers` is a board-shaped estimate a UI budgets against; only
the per-layer heap check at activation knows what the pattern actually
costs. Being one too optimistic costs a clear message; being one too
pessimistic would make scenes unusable on the flagship board, so the
estimate leans optimistic on purpose.

Text, sprite and colour layers are free — they need no engine — so the
flagship "clock over a pattern" scene fits comfortably.

Two further things bound a stack in practice, both documented in
docs/firmware.md "Scenes: the layer compositor in the render loop": the JIT
has exactly two exec halves, so a third resident engine falls back to the
interpreter (softly — it is logged, never refused); and a transition whose
two stacks together exceed `caps.layers` is a hard cut rather than a
crossfade.

## Big-flash and PSRAM modules (the Seengreat board)

The Seengreat board carries an ESP32-S3-WROOM-1-**N16R8**: 16 MB of flash
and 8 MB of octal PSRAM. The PSRAM is the pattern-array arena (Gitea #253);
since 2026-09-20 the flash is its own partition table too.

**Flash: `firmware/partitions-16mb.csv`, this board only** (Gitea #501,
reversing the #73/#143 decision recorded below). 3 MiB app slots, a 4 MiB
`storage`, a 3.9375 MiB `assets`, and the top 2 MiB deliberately
unallocated — the layout is in docs/firmware.md, "Partition tables". The
board's 987,952 B image leaves **68.6 %** of its slot free. Nothing about
this board *needed* the space; what changed is that the cost of a second
table went to near zero once the 4 MB boards had to be repartitioned
anyway, so the per-board partition file, the build-time table selection and
the migrator all exist for other reasons and this board just names a
different csv.

The 16 MB table is **opt-in per board, never per chip**:
`board-s3-devkit` is the same silicon and stays on the 4 MB table, because
generic S3 devkits ship 4, 8 or 16 MB modules indistinguishably at flash
time and a 16 MB table on a 4 MB part is a serial-recovery brick. The
Seengreat qualifies because its module is known by inspection.

The reasoning this replaces, kept because the conditions it named are the
ones that actually changed: **the standard 4 MB table stays** (decision for
Gitea #73) — a 16 MB module runs it fine with the last 12 MB unallocated,
the OTA slots are capped at 1 MiB either way, the storage partition is
nowhere near full, and a second table would need a per-board partition file
threaded through `build-esp32.sh`, `flake.nix`, the release workflow,
`build.rs`'s `esp-idf-part` serialization *and* `src/takeover.rs`, forking
the "one image, one layout" property. Three of those four premises are
gone: the slot cap moved, the threading was built, and takeover.rs's table
half became `parttab.rs`, shared. Gitea #143's conditions were met by #501
rather than by this board.

What did **not** change is the assets margin, which is still the number
worth watching on every board: the 4 MB `assets` partition
(0xF0000 = 983,040 B) holds an **843 KB bundle as of 2026-09-24 — 12.2 %
headroom** (863,167 B packed; `tools/ci.sh` fails the build over 983,040 B,
and `POST /api/assets` refuses an oversized install outright). One bundle
ships to every board, so the *small* partition is the bound even though this
board's is four times the size.

That figure is the result of the Gitea #683 diet and it is not a standing
surplus — it had fallen to **0.73 %** by 2026-09-24 (the 11.5 % recorded
here on 2026-09-20 was already stale) with three Phase B surfaces and the
Phase C font blobs still to land. What bought it back, all measured on
that day's master:

| lever | gzipped saving |
|---|---:|
| `[profile.wasm-release]` (opt-level "s", fat LTO, panic=abort, strip) + `wasm-opt -Oz` + zopfli, on `luxel.wasm` | 70.4 kB |
| `build.minify: "terser"` (2 passes) instead of esbuild, + zopfli | 23.2 kB |
| zopfli instead of zlib level 9, on `gallery.json` | 17.0 kB |
| zopfli + terser on the two entry HTMLs and the small chunks | 2.1 kB |

None of it is repeatable — the three levers are spent. The next 100 kB has
to come from what is *in* the bundle: `gallery.json` is 354 kB of the 843
(307 pattern sources, which ship verbatim on purpose) and the CodeMirror
editor is most of the 288 kB JS chunk. After that the only lever left is
growing `assets` past 0xF0000, which is another migration — Gitea #691 has
the options and what each costs.

Brotli and zstd are **not** available however much they would help: a
browser only advertises `Accept-Encoding: br`/`zstd` on a secure origin,
and the device is plain http on a LAN IP. gzip is the ceiling, which is why
zopfli — a harder-searching encoder for the *same* format — is what there
was to take.

## First light: the Seengreat board on metal (2026-09-05)

Gitea #75. What the first evening established, so nobody re-derives it:

- **Panel**: a 64x64 with **FM6124EJ** drivers (plain shift-register — no
  FM6126A init, esp-hub75 drives it as-is). The vendor pin map above is
  right: all 64 rows light and the colours are correct, so both the E line
  and the by-name transcription are verified.
- **USB**: the board's data USB-C is the S3's **native USB-Serial/JTAG
  (303a:1001)**, not a bridge chip. It enumerates as `/dev/ttyACM0` once the
  container is given that id (it is a different id from the Athom's FTDI).
  Two things bite: (1) **opening the port from the host resets the chip** —
  the peripheral treats the line-state change of a termios setup (a baud rate — `stty`, `b115200`; a bare `open()` alone does NOT do it, verified 2026-09-05) as a reset request
  (`rst:0x15 USB_UART_CHIP_RESET`), so a `cat`/`stty`/`socat` loop that
  reopens the port reboots the board on every reopen (and repeated fast
  resets risk the boot guard's slot rollback). One long-lived reader costs
  exactly one reset at open; there is no passive tap. Prefer `/api/status`
  for anything that must not disturb the device. The upside: a port open
  is a **remote reset** for an unresponsive board (used on 2026-09-05
  during the soak, when a 1–2 fps pattern made `/api/status` take 11 s and
  every client timeout read the board as dead — #259) —
  `HW_BENCH_RESET_CMD` in hw-bench. (2) A chip reset
  re-creates the node with `root:dialout 660` — `doas chmod 666` again.
- **Flashing**: `espflash write-bin --chip esp32s3 -p /dev/ttyACM0 0x0
  firmware/target/luxel-full.bin` (from `BOARD=board-seengreat-hub75
  ./build-esp32.sh image`) — 30 s. Reads are slow: ~12 KB/s with espflash
  4.4, so a full 16 MB dump is ~24 min. espflash's own post-flash reset
  cannot leave download mode if BOOT is held; a physical EN press with
  BOOT released was needed for the first boot.
- **Stock firmware**: XiaoZhi 2.2.6 (IDF 5.5.3, 16 MB layout: two 4032 KB
  OTA slots + 8 MB spiffs). Dumped in full before the first flash
  (`seengreat-stock.bin`, gitignored, two reads sha256-identical); the
  OTA-takeover install path is Gitea #256. No secure boot, no flash
  encryption.
- **Numbers** (v0.1.40, 4096 px, WiFi up, one idle client): boot heap
  100,240 B free, idle with rainbow 68,044 B, with the 2D snake game
  46,008 B. **fps: rainbow 18, 1D snake 8, 2D snake 4; an empty `render`
  56 (18 ms of per-frame overhead outside the VM), one `rgb()` call per
  pixel 29.** The 8 ms frame pacing in main.rs caps everything at 125.
- **Soak** (`docs/bench-report-seengreat-hub75.md`, hw-bench on the #275
  build, ~35 min): 299 gallery patterns, **184 clean, 115 with errors**
  (VM errors plus "pattern too large for this device" rejections — at
  4096 px many 2D patterns can't fit their arrays next to a 48 KB map),
  184 under 30 fps; **median 7 fps at 4096 px, p10 2, p90 17**; heap floor
  17,984 B; one "crash" row (after "Synchronized Random Numbers", back in
  106 s via the reset hook) that serial showed was NOT a crash — the
  pattern runs at 1 fps and starves the web task (#259). No panic in the
  whole run. Rainbow curve: 125 fps to 300 px, 116 at 600,
  68 at 1024, 35 at 2048, 18 at 4096.
- **Map**: the board installs a 64x64 grid map at boot (`POST /api/map`
  `grid W H`, docs/api.md) — procedural, zero heap — so every pattern
  renders 2D; before #258 landed, 2D-only patterns depended on the engine's
  48 KB default grid, which the panel's idle heap can no longer afford.
- **Consequences ticketed**: the render loop starves the web server at this
  pixel count (228 KB bundle: 2 s from the Athom, 31–62 s here — #259;
  `hosted-ui` is the practical variant for this board until then);
  per-pixel cost / codegen (#260); no 2D map by default and a 64x64 map
  can neither be POSTed (4 KB request buffers) nor afforded (48 KB per-pixel
  storage on a 46 KB heap) — #258; E1.31 multicast joins past group 4 fail
  with `GroupTableFull`, so 21 of a 4096-px board's 25 universes are dead
  over multicast (#257). The board's other peripherals are #249–#255
  (thumb-wheel, RTC, microSD, audio out, PSRAM, I2C header, chaining) and
  #142 (mics).

**PSRAM is the pattern-array arena** (Gitea #253, `psram-arena` cargo
feature, `firmware/src/psram.rs`). It is initialised on this board only.
Two things live there: `ArrRepr::Owned` element storage — the pattern
arrays — and, since Gitea #709, each engine's per-frame RGB888 pixel buffer
(see below). Everything the VM touches per *instruction* stays in internal
DRAM, and so does every buffer the output path shares within a frame.

The arena is a **second, separate `esp_alloc::EspHeap`**, never a third
region of the global one. That matters more than it sounds: esp-radio's
`malloc` shim asks the global `HEAP` with no capability filter, so a PSRAM
region added there could serve a WiFi-blob allocation that has to be
internal and DMA-reachable. Keeping it separate also means
`esp_alloc::HEAP.free()` still means exactly what it always meant, so
`RUNTIME_FLOOR`, the post-load floor check and `/api/status`'s `heap_free`
keep their old semantics on every board.

Consequences on this board:

| | before | with the arena |
|---|---|---|
| array BYTE budget | `HEAP.free() − 24 KiB` (~28 KB at 4096 px — less than ONE pixel-sized array) | arena free − `ARENA_RESERVE` (256 KiB), ~7.7 MB |
| array ELEMENT ledger | PB's 10,236 units | `bytes / 8`, i.e. the byte budget is the only thing that binds |
| arena slot count | bounded by the element ledger | `vm::MAX_ARENA_SLOTS` (2,559 — the same bound, now explicit) |
| DRAM cost of a big pattern | arrays + program + engine | program + engine only |

Raising the element ledger is a deliberate, board-scoped divergence from
Pixel Blaze: a pattern PB rejects with "array element budget exceeded" can
run here. Every board without the arena keeps the PB number exactly.

Because it is board-scoped, the hosts that stand in for a board have to read
it off the board rather than assume PB's: the console's preview engine and
capacity model take it from `/api/status`'s `psram_free`
(`lx_array_elements_for` / `lx_set_array_elements`, `stores/pattern.ts`'s
`previewArrayElements`) and `luxel serve --board panel` enforces the panel's.
Until then both kept the PB count: `library/fairies.js` (15,104 elements at
4096 px) ran on the panel while the editor previewed it BLACK and the banner
warned about a pattern the hardware was happily showing.

`/api/status` reports `psram_free` / `psram_total` on this board (and only
on this board — the fields are `#[cfg]`-gated, so no other image or JSON
changes). `heap_free` does NOT include them.

**Measured on the panel, 2026-09-08** (4096 px, brightness 3). The arena came
up first try — `psram_total` 8,388,608, `fence_timeouts` 0 and `pass.skips` 0
across four OTAs and a dozen pattern swaps, no boot-guard rollback.

*What PSRAM costs:* a 2048-element array read once per PIXEL (4,096 array
reads per frame) — the same pattern either way, so the only variable is where
its 16,416 B lives:

| | DRAM (master `d44ff4a`) | PSRAM arena |
|---|---|---|
| fps / out_fps | 36 / 36 | 36 / 36 |
| `vm_us` | 27,808 · 27,821 | 27,861 · 27,858 · 27,814 |
| `heap_free` | 32,572 | 42,812 |
| `engine_heap` | 29,746 | 13,683 |

**+0.16 % of VM time** against 0.05 % run-to-run noise, and the engine hands
essentially the whole array (16,063 of 16,416 B) back to internal DRAM. The
working set of a pattern array fits the data cache, so the per-access cost
does not show up at panel frame rates. Live Aurora 2D reads the same: 9 fps
both ways, `engine_heap` 19,748 -> 14,068.

*What it unlocks:* at 4096 px on the old build even a SINGLE
`array(pixelCount)` is refused — by the post-load `RUNTIME_FLOOR` check, not
by the element ledger (`left only 15 KB of heap free`) — and the panel goes
dark. With the arena that probe runs at 29 fps;
`library/color-bands-buffered.js` (3 x `array(4096)`, over PB's 10,236-unit
ledger AND four times the old byte budget) goes from refused-and-dark to
9 fps using exactly 98,304 B of arena; `heatshivers` 11 fps, `coolaura`
7 fps, `novas` 3 fps. Those are the Gitea #420 ceilings, and they no longer
bind on this board.

**The arena raises the ARRAY ceiling, not the PROGRAM ceiling.** An ad-hoc
`POST /api/code` of `music-sequencer-for-v3-only.js` still refuses with
*"not enough free memory on the device for this 46 KB upload (about 53 KB
free)"* — the upload envelope plus program decode is an internal-heap
transient that PSRAM does not touch. Activating from the store (the borrowing
path) is the route for a program that big.

### Engine frames in PSRAM (Gitea #709)

The engine's per-frame pixel buffer joined the arrays there on 2026-09-24.
It is 3 B/px — **12,288 B at 4096 px**, the largest single thing a resident
engine owns — and two pattern layers plus the staging frame plus
`RUNTIME_FLOOR` needed 65,536 B of internal DRAM against a steady
`load_base` of 47–49 KB. With the frames external the same stack is 40,960 B
and the panel's second layer became real.

The old doctrine said the frame was too hot for PSRAM. Measured, it is not:
the VM writes it once per pixel per frame and the compositor or the outpipe
reads it straight through, which the S3's data cache carries. Arrays were
the same story (+0.16 % above). What stays internal is anything read and
written *within* a frame beside those layer frames — the compositor's
scratch, the pipeline's travelling buffer, the crossfade stage, the strip
output buffer — and the HUB75 DMA framebuffers, which the panel refresh
reads continuously and no cache can help. (The S3's GDMA *can* reach PSRAM
— see #521 below — so that one is a bandwidth call, not a reachability one.)

Measured on the panel 2026-09-24, `/api/status` `frame_us` averaged over
four one-second samples, both images built from the same tree:

| what is running | frames in DRAM | frames in PSRAM |
|---|---:|---:|
| `Aurora 2D` bare, JIT native | 51,784 µs | **51,702 µs** (−0.2 %) |
| a scene of `pat(Aurora 2D)` + a colour band | 51,950 µs | 52,182 µs (+0.4 %) |
| a two-pattern scene (`Aurora 2D` + `_Fairies`) | `scene: layer 2 does not fit`, `engines:1`, 51,657 µs | **72,042 µs, `engines:2`**, both layers native |
| `load_base` idle / after five scene activations | 46.5 / 49.9 KB | 47.0 / 49.8 KB |

`_Fairies` alone is 17,725 µs native, so the two-layer 72.0 ms is
51.7 + 17.7 + ~2.6 ms of compositing — arithmetic that only closes with both
layers on the JIT. A 60 s soak of the two-pattern scene held `heap_free`,
`psram_free` and `engine_heap` flat with `vmerr` null.

One reporting consequence: `/api/status` `engine_heap` is an **internal-DRAM**
figure, and on this board it now reads 0–5 KB rather than 13–20 KB, because
the frame it used to be dominated by is no longer internal. `load_base`
(`heap_free + engine_heap`) is unchanged in meaning — it was always the
internal pool.

Two ordering rules the code depends on, both documented in
`firmware/src/psram.rs`:

- PSRAM init runs **before** `esp_rtos::start`, `core1::start` and any
  `flashmap::map`. `map_psram` suspends the data cache while it programs
  MMU entries, and PSRAM shares the S3's DBUS MMU table with flash
  mappings — esp-hal maps PSRAM after the LAST valid entry, so a flash
  mapping made first would push it towards the end of a 512-entry table.
  (`flashmap::find_free_run` already treats a bit-15 entry as occupied, so
  this order is the safe one.)
- `esp_hal::psram::Psram::new` reprograms the SPI0/SPI1 **flash** clock
  divider from `PsramConfig::flash_frequency`. `psram::init` reads the real
  value out of the image header at flash offset 0 rather than trusting
  esp-hal's 80 MHz default, and falls back to the slowest setting when it
  cannot: slowing flash down is safe, speeding it up is not.


## Second light: master on the panel (2026-09-06)

Gitea #75 / #260 / #266 / #271 / #298. The bring-up build was v0.1.40; this
run put master on the board — the cache-MMU flash mapping (#274), the pattern
code arena (#276/#293), LXBC v5 (#278/#288), the second-core render executor
plus cross-core flash fence (#280), the procedural grid map (#284), the
borrowed program words (#300) and the interpreter work (#263/#268 with
`CORE_O3=1`, then the #261 superinstructions). Two builds were measured, an
hour apart, because master moved under the session: **e5935e6** and then
**0f83975** (superinstructions + borrowed words). Every table below names
which. What changed on metal:

- **Per-frame cost, 4096 px** (`/api/status` per-stage timers, µs/frame, on
  `0f83975`; both OTA slots carried the same image for the run):

  | pattern | fps v0.1.40 → e5935e6 → **0f83975** | frame | vm | pipe | out |
  |---|---|---:|---:|---:|---:|
  | empty `render(index) {}` | 56 → 77 → **77** | 12,934 | 7,591 | 35 | 5,304 |
  | one `rgb()` per pixel | 29 → 45 → **50** | 20,308 | 14,021 | 36 | 6,246 |
  | rainbow (default) | 18 → 30 → **33** | 30,361 | 24,079 | 36 | 6,243 |
  | `library/snake.js` | 8 → 12 → **16** | 65,478 | 59,112 | 52 | 6,303 |
  | `library/snake-2d.js` | 4 → 8 → **10** | 102,467 | 96,061 | 81 | 6,303 |
  | `snake-2d.js`, Smartness 100 | — → 8 → **10** | 102,899 | 96,504 | 77 | 6,295 |

  **1.4–2.5× over the bring-up build.** The VM is ~95 % of a heavy frame; the
  HUB75 compose (`out`) is a flat **5.3–6.4 ms** whatever runs (1.4 µs/px of
  bitplane packing, the board's hard ceiling of ~155 fps), and the output
  pipeline (`pipe`) is noise. An empty render still costs 12.9 ms — 7.6 ms of
  that is per-pixel dispatch around a `render` with no body (**1.85 µs/px ≈
  440 cycles**), the floor #265 has to attack, since #261 has now taken its
  share.
- **Superinstructions are worth 14–18 % of VM time here** (#261/#298), which
  is the on-metal answer the host bench could not give (x86 saw a wash). Same
  firmware, two blobs from `luxel compile [--no-fuse]`: rainbow 27,970 →
  24,084 µs vm (−13.9 %), snake-2d 116,752 → 95,541 µs (−18.2 %). Blobs also
  shrink (rainbow 456 → 436 B, snake-2d 10,528 → 8,848 B) and the saving
  shows up as free heap on an ad-hoc push.
- **`CORE_O3` is worth its 19 KB on this board** (measured on `e5935e6`, same
  tree both ways): `CORE_O3=0` 918,496 B vs `=1` 937,680 B — rainbow 26 → 30
  fps (vm 32,732 → 28,379 µs), empty render 74 → 77 (vm 9,803 → 7,372),
  snake-2d vm 130,737 → 120,006. 7–33 % of VM time, largest where dispatch
  dominates.
- **Heap at 4096 px** on `0f83975`: 51,192 B free with rainbow running,
  50,412 B with a 2D pattern (`aurora-2d`) — a pattern now costs ~800 B of
  RAM beyond its arrays, because the program words are borrowed from the
  flash mapping (#300). Uploads are refused by a pre-flight free-memory check
  rather than OOMing (`frogger-2d`: "not enough free memory on the device for
  this 35 KB upload (about 26 KB free)").
- **Web serving is fixed** (#259): the 228 KB playground bundle downloads in
  **1.2–2.7 s** instead of 31 s (rainbow) / 62 s (2D snake), and the running
  pattern no longer changes the rate — the render loop is on the AppCpu and
  the asset reader streams out of the mapping. `hosted-ui` is no longer the
  recommended variant for this board.
- **The flash mapping works on the S3** (#271): `flashmap: assets
  0x310000+0xf0000 -> 0x3c0e0000 (15 x 64 KiB pages from entry 14),
  self-check ok` and `pattern code 0x290000+0x80000 -> 0x3c1d0000 (8 x
  64 KiB pages from entry 29), self-check ok` on every boot, from both OTA
  slots. Predicted entries were 16/31; the app's rodata/text is two pages
  shorter, and the vaddr arithmetic (window base + entry × 64 KiB) holds.
  Code-arena lifecycle — fill, wrap-on-save, never-evict-on-activate,
  re-save into a new slot, delete, survive a power cycle — all behave as
  specified (checked against the 7-slot arena on `e5935e6`; `0f83975` boots
  the extent allocator's `code arena 87 pages, 0 extents valid (0 dropped),
  0 pages used` — #293 — which has not had the same lifecycle pass).
- **The second core is live** (#266): `core1: AppCpu scheduler up, 20480 B
  stack, flash fence armed` + `render task: AppCpu`; AppCpu stack peak
  10,848 / 20,480 B under the pattern sweep; `fence_timeouts` 0 across
  ~340 pattern pushes, 9 OTAs and 3 asset installs. Note the park latency:
  `fence_wait_us` peaks at **3,171 µs** here, not the tens of µs seen on a
  strip — a HUB75 compose can hold the render core for milliseconds before
  it takes the park interrupt. AppCpu stack peak 10,848 of 20,480 B.
- **OTA is a coin flip on this board — #294** (measured 2026-09-05; the
  cause was found on the Athom the next day and fixed — see Gitea #292 and
  docs/firmware.md "Cores & tasks" rule 4 — but this board has not been
  re-measured since, so treat the numbers below as the pre-fix state).
  Four of nine `POST /api/ota`
  pushes wedged the ProCpu *inside* a flash op (`core1.last` =
  `SysRtcWdt` with ProCpu fence phase 3, fences begun = completed + 1,
  `fence_timeouts` 0, and no serial output at all between `ota: writing
  ota_0 …` and the reset). The RTC watchdog recovers it every time and the
  board comes back on the old slot, so pushing is safe — it just has to be
  retried. It is **not OTA-specific**: the 299-pattern sweep (~33 k fences
  over 90 min) wedged once the same way, silently — the probability tracks
  how many flash ops you do. Read `core1.last` after any long session on
  this board; a watchdog reset mid-soak leaves no other trace, and if the
  two slots hold different builds it also moves you onto the other one
  (push the same image to both before measuring anything). Same wedge, same
  black box, on the classic ESP32 — **#292** — where it is instead
  deterministic on a `POST /api/assets` install; on the S3 asset installs
  ran 3/3 clean at 727 KB, so whatever the boards share, it is the fence
  window itself and not the esp32-only SPI2-DMA wait (which the S3 does not
  even perform).
- **Pattern performance sweep**: `docs/perf-sweep-s3.md` (299 gallery
  patterns at 4096 px with the per-stage timers, sorted by VM time) is the
  baseline the interpreter work is measured against; regenerate with
  `node tools/hw-bench.mjs <ip> docs/perf-sweep-s3.md --perf-only`.
  **225 of 299 patterns render** at this pixel count (the other 74 are
  refused or fault on their arrays); of those, on `0f83975`, vm µs/frame is
  median **111,211**, p90 382,809, max 1.42 s, and fps is median **9**,
  p10 3 — i.e. the panel is an interpreter benchmark, not a driver one. The
  same sweep on `e5935e6` an hour earlier: median vm 128,437 µs, p90
  468,966, max 2.38 s, fps median 8 — so the superinstructions moved the
  whole distribution by ~13 %, not just the three patterns in the table.
- Bug found and fixed on the way: `blur1D`'s infallible 32 KiB prefix-sum
  allocation aborted the firmware at 4096 px (#295); `library/comets.js` in
  a playlist crash-looped the board five times until the boot guard rolled
  the slot back. The 32 KiB itself is gone as of #296 — `blur1D` now slides
  a window over a `min(radius + 1, len)` ring (16 B at the radius 1 the
  gallery uses, against 8 B *per element* before), so a full-panel blur no
  longer needs a transient allocation at all.

## Bulk render (`renderFrame`) on metal (2026-09-07)

Gitea #336 — the on-device half of #335 (whole-frame render entry + sixteen
bulk builtins), which shipped host-measured only. Three builds went to both
rigs: the merge base **`974b3b3`**, the #335 merge **`eedabc8`**, and the
master of the day **`ed2ac3f`** (which also carries #306's frame pipeline and
#320's store/read elision, so it is *not* a clean one-change delta — the
`974b3b3` → `eedabc8` pair is). Every row was taken with one worktree's
`web/public/luxel.wasm`, so the bytecode pushed to the device is byte-identical
across builds and only the firmware differs. Both S3 slots carried the same
image before any measurement (#294), and `core1.last` was clean after every
push.

### The I-cache question: no regression on either board

`Vm::call_builtin` gained sixteen arms, and #318/#325 showed layout alone is
worth tens of percent, non-monotonically. `tools/patbench.mjs` on
`perlin-fire-wind-tunnel` (the stateless ±0.3 % probe — three repeats of nine
samples on the Athom, of five on the panel) plus `tools/opbench.mjs`:

| board | build | patbench µs/px | Δ | opbench cycles/op |
|---|---|---:|---:|---:|
| Athom, 256 px | `974b3b3` base | 58.039 | — | 100.9 |
| Athom, 256 px | `eedabc8` **#335** | **57.973** | **−0.11 %** | **100.7** |
| Athom, 256 px | `ed2ac3f` master | 58.121 | +0.14 % | 100.8 |
| Seengreat, 4096 px | `974b3b3` base | 44.360 | — | 83.4 |
| Seengreat, 4096 px | `eedabc8` **#335** | **44.382** | **+0.05 %** | **83.4** |
| Seengreat, 4096 px | `ed2ac3f` master | 44.413 | +0.12 % | 83.4 |

Every delta is inside the probe's own ±0.3 % repeatability, and opbench's
fitted slope is flat to three figures on both chips (Athom 968.2 → 966.5 →
967.4 µs per K; panel 12,813.5 → 12,815.0 → 12,812.2). **The sixteen arms cost
existing patterns nothing measurable**, which is what #328's placement work
predicted: they land in `Vm::builtin_cold` and every `bulk.rs` symbol links
into flash `.text`, so `Vm::run` / `Vm::call_builtin` / `Vm::builtin_hot` are
byte-for-byte where master put them. Nothing to file against #328.

### Bulk patterns on the panel

`ed2ac3f`, 4096 px, brightness 31, medians of seven `/api/status` samples
after a nine-second settle.

> **Correction (2026-09-07, Gitea #378).** The original text here said to
> read `out_fps` for "what the panel actually showed". That is wrong on this
> board. `out_fps` counts every `write_frame` **call**, and the HUB75 driver
> returns without drawing when the previous buffer swap has not landed yet.
> The panel's real rescan rate was 77 Hz at the 20 MHz clock these rows were
> taken at, so the two 125/126 rows below were **composing** 125 frames a
> second while the panel displayed 77 of them. `/api/status` now reports
> `rescan_hz` directly; the displayed rate is `min(out_fps, rescan_hz)`.

| pattern | fps | out_fps | frame | vm | pipe | out | vm µs/px | heap free |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `rainbow` (per-pixel) | 52 | 52 | 19,470 | 19,417 | 3 | 5,931 | 4.740 | 51,488 |
| **`bulk-rainbow`** | **125** | **125** | 2,825 | **2,772** | 5 | 3,261 | **0.677** | 50,660 |
| **`bulk-comet-trails`** | **125** | **126** | 480 | **431** | 3 | 3,237 | **0.105** | 50,320 |
| **`bulk-bouncing-balls-2d`** | **125** | **125** | 5,539 | **5,477** | 6 | 3,486 | **1.337** | 48,456 |
| **`bulk-sprite-scroll-2d`** | **125** | **124** | 5,205 | **5,125** | 16 | 3,556 | **1.251** | 48,220 |
| **`bulk-canvas-ripples-2d`** | **100** | **100** | 9,979 | **9,900** | 13 | 3,599 | **2.417** | 38,988 |

`rainbow` vs `bulk-rainbow` is the one genuine like-for-like pair here (both
paint one hue ramp across the strip): **VM time 19,417 → 2,772 µs, 7.0×**, and
the frame goes 52 → 125 fps on the wire. The host bench read 4.4× for the same
shape (`b-rainbow`, docs/bulk-render.md), so the device win is **1.6× the host
ratio** — real, but well short of the "understates by ~3×" rule of thumb that
page offers; take 1.5–2× as the measured multiplier for a *fill*-shaped
rewrite, where the per-pixel side was cheap to begin with.

The nearest per-pixel library analogues for the other four are not matched
rewrites (different maths, different entity counts), so these are context, not
ratios:

| pattern | fps | out_fps | frame | vm | out | vm µs/px | heap free |
|---|---:|---:|---:|---:|---:|---:|---:|
| `bouncing-balls-2d` | 25 | 25 | 41,233 | 41,159 | 6,062 | 10.049 | 45,744 |
| `2d-canvas-example` | 17 | 17 | 60,936 | 60,839 | 6,476 | 14.853 | 46,884 |
| `ripples-2d` | 5 | 5 | 234,553 | 234,402 | 6,173 | 57.227 | 49,752 |
| `rainbow-comet` | — | — | — | — | — | — | **refused** |

`rainbow-comet` is the entry that makes the frame-persistence argument
concrete: its `array(pixelCount)` trail buffer is refused outright at 4096 px
(*"pattern too large for this device — it left only 16 KB of heap free (the
firmware needs 20 KB to keep running)"*), while `bulk-comet-trails` draws the
same shape out of six scalars in **431 µs**, the cheapest frame on the board.

Four of the five bulk patterns are **frame-cap bound, not VM bound**: at
2.8–5.5 ms of VM they sit on the engine's 125 fps ceiling with `out` at
3.2–3.6 ms. So on this panel a bulk rewrite's payoff stops at the cap — past
that it buys headroom (and core-0 slack), not frames. **Unexplained, worth a
look:** `out` is not the flat 5.3–6.4 ms "Second light" recorded — it tracks
how hard core 1 is working, 6.5 ms under `ripples-2d` down to 3.2 ms under
`bulk-comet-trails`, monotonically across nine patterns spanning 431 µs to
234 ms of VM time. It is not a bulk-vs-per-pixel artifact: the stored
`Infinite Snake v2` is a cheap *per-pixel* pattern (vm 7,419 µs) and gets a
cheap `out` too (3,419 µs). HUB75 bitplane packing is content-independent, so
something about a saturated render core costs the output task ~2.5 ms a frame
— memory-bus contention is the obvious suspect. It shifts the panel's compose
ceiling by nearly 2×, so it matters as soon as the frame cap above is lifted.
Filed as Gitea #367.

### `blit` (keyed) and `fillCanvas`, asserted pixel by pixel

Neither rig can be eyeballed from here, so both were checked through
`GET /api/pixels` (the pipeline's own frame buffer) with deterministic probe
patterns — a fixed sprite at a fixed cell, no motion.

On the **panel** (real 64×64 grid, 4096 px):

- **keyed blit** — a 4×2 checkerboard sprite (`v` alternating 1/0, hue 0) over
  a green `hsv(); fill()`, mode 3 at grid (0,0): exactly **4 red pixels of
  4096**, at (0,0) (0,2) (1,0) (1,2), and **4,092 green**. Transparent cells
  are transparent; opaque ones are not blended.
- **blit clipping** — an 8×4 all-opaque sprite at `col = gridWidth() - 4`,
  mode 0: exactly **16 red pixels**, columns 60–63 × rows 2–5. The four
  columns hanging off the right edge clip silently.
- **`fillCanvas`** — a 2×2 canvas of red / green / blue / black nearest-sampled
  across the panel: exactly **1024 pixels of each**, quadrants square and in
  the right corners.

On the **Athom 60 px strip** (the over-provisioned `ceil(√60)` = 8×8 default
grid — the case the ‡ note in docs/bulk-render.md is about):

- the same keyed blit lands red at indices 0, 2, 8, 10 with green everywhere
  else, and `/api/pixels` is still exactly 180 bytes;
- an 8×1 sprite blitted onto grid **row 7** paints indices 56–59 and the four
  cells past the end of the frame (60–63) clip — no `vmerr`, no truncation;
- `fillCanvas` gives the same four exact quadrants;
- `library/bulk-sprite-scroll-2d.js` shows its `ffc600` face over the dim
  two-tone wash, and `library/bulk-canvas-ripples-2d.js` a smooth field — i.e.
  the "cover, not match" grid rule works on a real non-rectangular strip.

**Left for Jeremy's eyes** (a programmatic check cannot judge these): whether
the scrolling sprite reads as a *face* rather than a blob at 8×8 on a 64×64
panel, and whether `bulk-canvas-ripples-2d`'s 16×16 field upscaled by
`fillCanvas`'s nearest sampling looks blocky enough to want bilinear.

### Heap under a bulk pattern is flat

40-minute soak on the Athom on `ed2ac3f` (`/api/status` every 25–30 s): 20 min
holding `bulk-comet-trails`, then 20 min rotating all five `bulk-*.js`
patterns a minute apart. `renderFrame` hands the frame `Vec` to the VM by move
and takes it back on every exit path, so the claim under test is that no
allocation happens per frame — and none does: **`heap_free` read 83,616 B on
38 of the 40 samples in the 20-minute hold** (the other two 83,424 B, a 192 B
in-flight HTTP allocation), first sample and last both 83,616, and the
rotation phase returned to the same per-pattern values each time it came round (83,616 /
83,692 / 82,528 / 81,808–82,036 / 72,596 B). AppCpu stack high-water 11,136 →
11,520 B of 20,480 across the run; no `vmerr`, no missed sample, no reboot.

The full gallery soak on the same build agrees: `tools/hw-bench.mjs`,
**305/305 patterns clean, 0 errors, 4 under 30 fps**, median 123 fps at 60 px
(docs/bench-report.md) — against 299/299 clean with 12 slow and median 118 on
2026-09-02, so the interpreter work plus #335 took two thirds of the slow tail
out. The sweep's lowest `heap_free` moved the other way, 83,448 → 59,668 B;
still far above the ~20 KB floor and nothing errored, unexplained, Gitea #368.

## JIT on metal (2026-09-24, Gitea #665/#666)

The first bytes of emitted code any real chip has executed. Two boards, the
same emitter, two different places to put the image: the panel runs it out of
the PSRAM arena, the Athom out of its `.rwtext` static. `vm_us` and `fps` are
medians of `/api/status` samples after a settle, JIT off then on across the
same blob; docs/jit-design.md §7.3 is the plan these rows answer.

**Seengreat HUB75 S3**, 4096 px, v0.1.40, `partitions-16mb.csv`. Found
brightness 4, read and restored, never set.

| pattern | interp `vm_us` (fps) | native PSRAM `vm_us` (fps) | speedup | code B | `compile_us` |
|---|---:|---:|---:|---:|---:|
| `rainbow` | 19,861 (50) | 7,276 (115) | **2.73×** | 160 | 3,968 |
| `snake` | 47,617 (21) | 9,277 (105) | **5.13×** | 1,172 | 10,083 |
| `perlin-fire-wind-tunnel` | 187,369 (6) | 62,010 (16) | **3.02×** | 2,536 | 14,335 |
| `aurora-2d` | 104,045 (10) | 50,082 (20) | **2.08×** | 5,612 | 9,631 |
| `bulk-canvas-ripples-2d` | 8,191 (113) | 3,983 (114) | **2.06×** | — | — |
| `snake-2d` | 79,350 (13) | refused `no-memory` | — | 11,256 | — |



`rainbow`'s 115 fps is the panel's 115 Hz rescan ceiling, not the engine's:
past that point the JIT is buying headroom, not frames, which is the same
wall the bulk rewrites hit above. `bulk-canvas-ripples-2d` shows the other
edge of the same coin — a `renderFrame` pattern is already one call per
frame, so the 2.06× is the win on the *non-bulk* part of its work and
nothing more.

`snake-2d` is the honest failure: at 4096 px the engine leaves ~26 KB of
internal heap, the emitter's bookkeeping rule wants 31.6 KB for its 1,085
words and 19 functions, and the compile is refused before it starts. It runs
interpreted at 13 fps, and it compiles and runs natively on the Athom at 144
px, where the heap is not the constraint. `aurora-2d` is the case that sits
just the other side of the line, and it is the reason `COMPILE_FLOOR` is a
separate, smaller number than `RUNTIME_FLOOR` (docs/firmware.md "The
compile's own heap"): 31,148 B free against 14,848 B needed clears 12 KB and
not 20 KB, and with the shipped gate it compiles and runs 2.08× — verified,
not inferred.

The arena side has one cost worth stating plainly: **an image is allocated at
the full `JIT_MAX_CODE` cap, so `psram_free` drops by ~128 KB per native
image regardless of how big the code actually is** (160 B for `rainbow`).
Irrelevant against 8 MB and two images in flight, but shrinking the block to
the emitted length once the size is known is an obvious follow-up.
Internal `heap_free` does not move either way — which is the arena's entire
reason for existing.

**Athom music-reactive**, classic ESP32, 144 px ws2812, a `CLASSIC_JIT=1`
image — that lever is how the tier was built the night these rows were
taken; since #676 the shipped `board-athom-music` image is the same build
with `JIT=1` as its default ("JIT: which boards compile patterns to native
code" above). Found brightness 6 and a playing playlist, read and restored,
never set.

| pattern | px | interp `vm_us` (fps) | native `.rwtext` `vm_us` (fps) | speedup | code B | `compile_us` |
|---|---:|---:|---:|---:|---:|---:|
| `rainbow` | 144 | 1,055 (122) | 570 (122) | **1.85×** | 160 | 3,465 |
| `snake` | 144 | 2,287 (121) | 660 (122) | **3.47×** | 1,172 | 5,345 |
| `perlin-fire-wind-tunnel` | 144 | 8,810 (68) | 2,765 (115) | **3.19×** | 2,536 | 9,707 |
| `snake-2d` | 144 | 2,738 (117) | 765 (122) | **3.58×** | 11,256 | 29,086 |
| `snake` | 2048 | 28,948 (11) | 5,618 (14) | **5.15×** | 1,172 | 5,246 |
| `perlin-fire-wind-tunnel` | 2048 | 120,099 (6) | 34,964 (10) | **3.43×** | 2,536 | 7,664 |

At 144 px four of these are frame-cap bound at 122 fps with or without the
JIT, so read the `vm_us` column and not the `fps` one — the 2048-px rows are
where the frame rate has room to move. 1.9–5.2× across the set, on the part
the design deferred for want of exactly this measurement.

### PSRAM instruction fetch costs about 1 %

The §7.3 question: the S3 fetches native code through a 32 KB instruction
cache from octal SPI PSRAM, and nobody knew what that cost. `POST /api/jit
{"place":"internal"}` forces the same image into a main-heap block executed
through SRAM1's instruction alias, so the same function can be timed out of
both.

| pattern | PSRAM `vm_us` | internal SRAM `vm_us` | PSRAM cost |
|---|---:|---:|---:|
| `rainbow` | 7,276 | 7,305 | −0.4 % |
| `perlin-fire-wind-tunnel` | 62,010 | 61,353 | +1.1 % |

**About 1 %, and inside the noise on the smaller image.** These patterns'
images are 160 B and 2,536 B, so both are resident in the instruction cache
after the first frame and the bus never comes into it again — which is the
whole argument for the arena: a place to put code that costs no internal
SRAM, for a price the frame timer cannot really see. It also retires the
risk docs/jit-design.md §10 carried against it.

`compile_us` is the one number that does move with placement, and not
consistently: `rainbow` 3,968 µs in PSRAM against 4,357 µs internal,
`perlin-fire-wind-tunnel` 14,335 against 6,487. The PSRAM side pays a
cache write-back, an instruction-cache invalidate and first-touch misses on
a freshly mapped block; on a 2,536 B image that is several milliseconds,
on a 160 B one it is lost in the measurement. Either way it is a
one-off at activation, not per frame.

### The crash that produced the bookkeeping gate

The **first** on-metal native run of `snake-2d` at 4096 px took the panel
down. The panic was not in generated code at all:

```text
memory allocation of 1 bytes failed
  <alloc::vec::Vec<luxel_core::kinds::Kind> as core::clone::Clone>::clone
  ← luxel_firmware::jit::try_compile ← try_budgeted_engine
resume: heap too tight (48 free)          → RTC_SW_SYS_RST
```

The emitter is a pure function that allocates only its own bookkeeping, and
the bookkeeping was enormous: `luxel_core::kinds::StackMap` was
`Vec<Option<Vec<Kind>>>` — a `Vec` header and an allocator block **per
bytecode word**, ~55 B/word on the host. Beside a resident 4096-px engine
with 33 KB free, planning a 1,085-word pattern simply ran the heap out, with
48 bytes left, on the render core.

Two fixes, and both were needed:

1. **`StackMap` went flat** — one pool of kinds plus a `(start, len)` per
   word, ~9 B/word. Measured peaks over `library/` fell from 61,455 → 24,412 B
   for `snake-2d` and 174,595 → 67,460 B for `music-sequencer-for-v3-only`,
   the largest pattern in the tree.
2. **The compile now asks first.** `crates/luxel-jit/tests/alloc_peak.rs`
   wraps the global allocator, compiles all 307 library patterns and fits a
   rule to the peak of live bytes: `words × 24 + fns × 240 + 1024`, in host
   bytes, so it overstates the device by about a third. `firmware/src/jit.rs`
   applies the same rule to the heap it actually has before claiming any exec
   memory, and refuses `no-memory` rather than running out half way. The
   constants live in both files and move together; the test is the gate.

Measured bookkeeping peaks, for scale: `rainbow` 1,054 B · `perlin-fire-wind-tunnel`
7,470 B · `aurora-2d` 10,486 B (516 words) · `snake-2d` 24,412 B (1,085 words,
19 fns) · `music-sequencer-for-v3-only` 67,460 B (3,126 words).

The remaining cost is that the bookkeeping is still *internal* heap on a
board with eight megabytes of PSRAM sitting idle beside it, which is what
keeps `snake-2d` interpreted at 4096 px. Moving it into the arena is the open
follow-up (docs/jit-design.md §10).

**Library differential on metal** (`tools/jit-diff.mjs`, 2026-09-24). Athom, 144 px, all 307 patterns: **295 ran natively** (54 pixel-identical, 241 differing only through a wall-clock input), **0 mismatches, 0 vmerr, 0 crashes**; 11 refused — 7 `too-large` over the classic board's 12 KB half (dbzbattlefinal, fireworks-finale, flash-posterize-music-sequencer-framework, multisegment-demo, snake-2d-v2, stargen-polar-2d, utility-palettes) and 4 `no-memory` (2d-fireworks-fade, frogger-2d, the two music sequencers); 1 `unstable` (beat-bounce, sound-reactive, the interpreter does not repeat itself either). Seengreat, 4096 px, 141 patterns (every third plus every 2D one — the full sweep is ~90 s a pattern at this pixel count): **131 ran natively** (10 identical, 121 clock), **0 mismatches, 0 vmerr**, 7 refused `no-memory` (bouncy-boxes, lightning-strike, snake-2d, snake-2d-v2, sound-spectrokalidamandala, sunrise-2d, stargen-polar-2d), 1 upload refused for heap fragmentation, and 2 rows (frogger-2d, music-sequencer-for-v2) whose 30–35 KB blobs the board rejects at 4096 px with the JIT off as well — that rejection leaves ~3.6 KB of heap and the next HTTP request panics, which is Gitea #678, not the JIT.
**Soak with the JIT on** (2026-09-24): Athom, Jeremy's own 4-item playlist swapping every 5 s, 55 min native — 0 resets, `fence_timeouts` 0, 118 watcher samples all `native`; Seengreat, a 2-item playlist (_Fairies, Aurora 2D) swapping every 30 s, 33 min — 0 resets, `fence_timeouts` 0, 66 samples all `native`, heap 30.6–37.6 KB, 41 native activations narrated on serial and no panic. Not `tools/hw-bench.mjs`: that pushes every gallery pattern, which on the panel is Gitea #678 waiting to happen, and the library differential had already activated every pattern natively once.

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

Three files, no other code paths involved — plus a case in **every**
function of `firmware/board-target.sh`, each of which errors on an unknown
board rather than guessing:

| function | what it decides |
|---|---|
| `board_name` | the `board::NAME` string tools/ota-push.sh greps the image for, so an image of the wrong board is refused instead of pushed (Gitea #389) |
| `board_takeover` | whether the board can be installed by uploading Luxel to WLED's own `/update` page — must match the `wled-takeover` feature in its `firmware/Cargo.toml` feature list (docs/wled-migration.md) |
| `board_partitions` | which partition csv the board's flash takes, and the matching `espflash --flash-size`. `partitions.csv` + `4mb` unless the MODULE is known to carry 16 MB — "it's an S3" is not enough (Gitea #501; `firmware/build.rs` makes the same choice from the cargo feature for the table it EMBEDS, and the two must agree) |
| `board_ota_max` | the board's app-slot size in bytes: `1310720` on the 4 MB table, `3145728` on the 16 MB one. `tools/image-check.sh`'s margin gate reads it, so the number lives in exactly one place per board |
| `board_target` | only if the board is a chip we don't build yet |

(`board_target` is the board → chip / rust target / toolchain map shared by
build-esp32.sh and tools/stack-check.sh; its `CORE_O3` flag decides whether
the VM crate gets opt-level 3 — see "The OTA-slot ceiling" — and its `IRAM`
flag which of the interpreter's hot functions execute from internal SRAM —
see "IRAM budget" — and flake.nix's `firmwareVariants` entry must say the
same for both.)

1. **`firmware/Cargo.toml`** — add the feature, selecting the chip:

   ```toml
   [features]
   board-my-thing = ["esp32"]      # or ["esp32c3"]
   # …plus "wled-takeover" if the board ships WLED and users will install
   # Luxel through its /update page (~25 KB of image; board_takeover in
   # board-target.sh must agree, or tools/image-check.sh fails the build)
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

## Runtime pins: the data-pin picker and pattern GPIO

Two things erase the pin *type* at runtime (`esp_hal::gpio::AnyPin::
steal(n)`), and both are gated by per-chip and per-board tables in
`firmware/src/board.rs` so the "one owner per pad" rule holds by
construction:

- **The strip DATA pin is a setting** (Gitea #154). `board::DEFAULT_DATA_PIN`
  is the board's wiring; `POST /api/datapin <n|default>` stores an
  override (settings record v8) and reboots, because the SPI driver binds
  its MOSI pin once at boot. `GET /api/config` reports `data_pin` (bound
  now), `data_pin_default`, `data_pin_next` (stored, waiting for the
  reboot) and `data_pins` (every pin the picker accepts). CLK stays a
  typed board constant. A WLED takeover imports WLED's LED pin when this
  board can drive it, and says so on serial either way. Panel boards
  have no strip SPI and omit all of it.
- **Pattern GPIO is real** (Gitea #177): `firmware/src/gpio.rs` syncs
  the pads a pattern names with the engine between frames (see
  docs/lang.md "Device & environment"). ADC1 is wired on ESP32, C3 and
  S3; the C6 has no ADC channel map in esp-hal 1.1, so `analogRead` reads
  0 there. `touchRead` has no device driver.

Which pins are allowed, in `board.rs`:

| layer | what it excludes |
|---|---|
| `chip::gpio_exists` / `gpio_can_output` | numbers the silicon lacks; input-only pads (classic ESP32 34–39) for OUTPUT and DATA |
| `chip::SYSTEM_PINS` | SPI flash, octal PSRAM (S3 33–37), USB-serial-JTAG, UART0 |
| `def::RESERVED_PINS` (per board) | what Luxel drives: strip CLK, the Athom relay (2), the PB v3 status LED (12), the C6 onboard LED (8), the 14 HUB75 pins |
| `shared::DATA_PIN` (runtime) | the configured strip DATA pin, kept off the pattern surface by `gpio::pin_is_free` |

Everything else is a pattern's to use — on the Athom that includes the
case button (0), the IR receiver (25) and the mic pins (32/15/36); on the
PB v3 the button (32) and the expansion header (0, 25, 26). A pattern
naming a pin outside the free set is ignored on that pin with one serial
line (`gpio: pattern named GPIO2 — reserved on this board`); the picker
refuses such a pin outright. A new board adds `DEFAULT_DATA_PIN` and
`RESERVED_PINS` to its def block (a `const` assert checks the default
passes its own tables), and a new chip adds a `chip` module.

Cost: the runtime pin plumbing is ~7 KB of image (esp-hal's per-pin
dispatch tables), which put the C6 at 5.55 % OTA-slot margin — under the
6 % warn line of `tools/image-check.sh`, above the 3 % floor.

[esp-hal]: https://github.com/esp-rs/esp-hal

## The LCD_CAM pixel clock on the panel (2026-09-07)

The panel's rescan rate had only ever been an estimate — a comment in
`firmware/src/hub75.rs` guessing "~77 Hz at 7 planes". It is now measured,
from esp-hub75's own BCM frame counter (`Hub75::frame_count()`, always armed
in circular-DMA mode) exposed as `/api/status` **`rescan_hz`**. The estimate
was right, and the rate is exactly linear in the clock:

| LCD_CAM clock | measured rescans/s | ratio | verdict |
|---|---:|---:|---|
| 20 MHz | **77.0** | 1.00× | clean (the esp-hub75 example's value) |
| 30 MHz | **115.3** | 1.50× | clean — **now the default** |
| 40 MHz | **154.0** | 2.00× | **fails**: mid-panel split, distorted colours |

Measured on the bench 64x64 FM6124EJ panel at 7 bitplanes, two
`/api/status` samples 20 s apart. Since #525 the clock is the `panel` line's
`clock_mhz` **setting** rather than a const — one of 8/10/12/15/20/24/30 MHz
since #771, which is why the 40 MHz row is no longer reachable at all — so read
this table as what one FM6124EJ panel does, not as what every panel does; see
"Panel driver settings" below.

**Why 30 MHz.** The FM6124 datasheet (v1.1) puts FCLK at max 30 MHz, and its
20 ns minimum clock high/low implies 25 MHz on pulse width alone — so 30 MHz
is the datasheet ceiling with no margin, and the board's 74HCT245 buffers add
22–28 ns of worst-case tpd on top. 40 MHz is well outside that and looks it:
the two 32-row halves mis-sample into a visible split down the middle of the
panel and the colours distort. 30 MHz was visually clean on this panel across
rainbow, Raindrops 2D, Infinite Snake v2 and bulk-comet-trails.

**The failure is invisible to the firmware.** At 40 MHz there was no swap
error, no DMA error, `vmerr` null, `fence_timeouts` 0, nothing on serial —
and the composed frame was still byte-identical to a host render of the same
pattern (a time-independent probe compared through `/api/pixels`: 12,288 of
12,288 bytes equal). Only the panel's own sampling fails. Any future clock or
geometry change needs an eyeball, not a test run.

**A faster clock buys no throughput.** fps, `out_fps` and `vm_us` were
identical at all three rates, because every pattern tested is render-bound
rather than rescan-bound:

| clock | `rainbow` fps (vm µs) | `raindrops-2d` fps (vm µs) | `snake-2d-v2` fps (vm µs) |
|---|---:|---:|---:|
| 20 MHz | 52 (19,451) | 69 (14,438) | 118 (7,437) |
| 30 MHz | 52 (19,427) | 69 (14,483) | 118 (7,495) |
| 40 MHz | 51 (19,483) | 68 (14,539) | 119 (7,509) |

What the headroom is actually for: an **8th bitplane** becomes usable (~58 Hz
at 30 MHz, against ~38 Hz at 20 MHz), and **chained panels** get the
bandwidth they need (Gitea #255).

## HUB75 panel arrangement: the boot-time remap (2026-09-19, Gitea #475)

A HUB75 chain is one ribbon. The driver shifts a single row `pw · panels`
wide and `ph` tall, and the tiles hang wherever the installer put them —
side by side, stacked, snaked, half of them upside-down. Until this change
the firmware assumed exactly one upright 64×64 tile and the engine grid was
the panel.

`Layout.matrix` (`/api/layout`, #465) now describes the real arrangement —
`pw ph cols rows start dir snake rot180 [scan]` — and
`firmware/src/hub75.rs` turns it into **one lookup table, built once at
boot**: `lut[driver pixel] = engine pixel`. The engine keeps rendering one
`pw·cols` × `ph·rows` row-major grid and never learns about the chain; the
compose path (`luxel_hub75::pack_remap`) gathers through the table instead
of walking the frame in order. Chain order, `rot180` semantics and the
identity rule are in docs/api.md "Panel arrangement"; the builder and its
27 host tests are `crates/luxel-hub75/src/arrange.rs`.

**The identity case costs nothing, and is found rather than assumed.** The
table is built unconditionally and then checked: if `lut[i] == i` for every
driver pixel it is freed on the spot and the driver composes byte-for-byte
the code it composed before. That covers one upright tile — every device
shipped so far — *and* arrangements that merely happen to come out
row-major, such as two 32-wide tiles wired `tl row`, which a
`cols == 1 && rows == 1` test would have missed.

**Measured on the bench panel** (Seengreat 64×64, 7 planes / 30 MHz,
`Aurora 2D` at 4096 px, master `9ea68f9` vs this change):

| arrangement | `out_us` (compose) | `pipe_us` | fps / `out_fps` | `rescan_hz` | `heap_free` |
|---|---:|---:|---:|---:|---:|
| `64 64 1 1 tl row 0 0` — master | 2505 – 2536 | 42 – 52 | 9 / 9 | 115 | 41,612 |
| `64 64 1 1 tl row 0 0` — this change, identity, no table | 2491 – 2679 | 72 – 80 | 9 / 9 | 115 | **41,612** |
| `32 64 2 1 tr row 0 0` — two tiles, halves swapped, table live | 3267 – 3502 | 72 – 80 | 9 / 9 | 115 | **33,420** |

So the no-op path really is a no-op: `heap_free` is byte-identical to
master and `out_us` sits inside master's own sample spread (the compose
takes one extra branch per ROW PAIR, 32 a frame, not one per pixel). A live
table costs exactly **8,192 B of internal DRAM** (2 B per driver pixel) and
**≈ 790 µs of compose** at 4096 px — 2.6 ms → 3.4 ms against the panel's
8.66 ms rescan window, so throughput is unchanged.

**Why the table is internal DRAM and not the PSRAM arena.** It is read once
per pixel inside that compose window, which is the exact class `psram.rs`
keeps out of PSRAM (the arena is pattern arrays only). It also cannot ever
be the binding allocation: at 7 planes the table is 2 B per driver pixel
against the two bitplane framebuffers' 14 B per driver pixel, and *those*
are DMA targets that must be internal. A remap big enough to matter always
comes with framebuffers seven times bigger that cannot move either.

**What this board can drive.** *(Rewritten 2026-09-26 — this paragraph
described a compile-time framebuffer until #401 landed; see "Panel driver
settings" below.)* The DMA framebuffer is **allocated at boot from the
stored `matrix` line**, so what the board can drive is no longer a constant:
a chain is still one ribbon `pw · panels` wide and `ph` tall, and the
framebuffer is sized to exactly that. What bounds it now is
`board::MAX_PIXELS` (4096 on a panel board) and the internal SRAM the two
bitplane buffers need — 14 B per driver pixel at 7 planes. A configured
panel over either bound does not brick the board: the boot attempt is
refused ("the panel is larger than this board's pixel cap" / an allocation
failure) and the firmware **falls back once to the board default 64×64**,
reporting `driver.live.fallback` on `/api/layout`. An arrangement whose
chain is wider than the framebuffer that was actually built is still
accepted, stored and reported — the board drives the leading tiles that fit
and `GET /api/layout` says so in `matrix.drive`. Verified on metal (before
#401): `matrix 32 32 2 2 bl row 1 1` (four 32×32 tiles, snaked, alternate
lines rotated) boots, reports `drive` 2 of 4 and keeps rendering. A **real**
multi-panel chain still needs the pixel ceiling raised past 4096 (#255, and
a follow-up ticket) and the framebuffers to fit, and neither can be verified
without a second physical panel.

**Estimated refresh.** The firmware reports `matrix.est_hz`, the rate the
whole configured chain would rescan at:

```text
est_hz = clock_hz / ( scan · (2^planes − 1) · pw · panels · stripes )
```

`stripes = (ph / 2) / scan`, so it is 1 on every ordinary panel and the
formula reads as it always did (it was added 2026-09-26, #764 — see below).
`scan` is the `scan` field when set, else `ph / 2`; `planes` and `clock_hz`
are the `panel` line's settings (7 and 30 MHz by default) rather than
constants since #525. It reproduces every number in "The LCD_CAM pixel clock
on the panel" above — 115/76/153 Hz against 115.3/76.9/153.5 measured — and
the #255 research's 28.8 Hz for four chained 64×64 tiles. The device's own
`est_hz` read **115** against a measured `rescan_hz` of **115** throughout the
session. A UI computing the same number in the browser (Settings, #469) has
the formula in docs/api.md and `est_hz` to check itself against; under
~100 Hz the panel flickers.

**A 1/N-scan panel does not rescan faster, and `est_hz` used to say it did.**
Fewer address rows means a proportionally LONGER row: one address row clocks
out `stripes = (ph / 2) / scan` copies of the chain's width, so the product
`scan · pw · panels · stripes` is the panel's pixel count however the rows
are multiplexed. `est_hz` multiplies `pw · panels` by `stripes` since
2026-09-26 (#764, fixed in the #401/#525 branch); before that a `scan 8`
panel read four times its real refresh. The same `stripes` factor sizes the
framebuffer (`arrange::fb_geometry`) and is folded into the remap table, so
1/N scan costs the compose path nothing beyond the gather a chain already
does. Only the **"straight"** quad mapping is implemented; other multiplex
mappings are a follow-up ticket, and no 1/N-scan panel has been on the bench.

**Image cost**, flake builds against master `9ea68f9`:

| board | master | arrangement only | + `/api/reboot` | OTA slot free |
|---|---:|---:|---:|---:|
| `seengreat-hub75` | 963,712 | +2,800 | **+3,280** | 7.78 % |
| `athom-music` | 1,022,912 | **+0** | +368 | 2.41 % |
| `c6-devkit-hosted` | 1,014,144 | **+0** | +144 | 3.27 % |
| `pixelblaze-v3` | 1,001,296 | **+0** | +384 | 4.47 % |

The arrangement itself is **zero bytes on a strip board**, and that is not
luck: the remap builder lives in `luxel-hub75`, an optional dependency
behind the firmware's `hub75` feature, and the `est_hz`/`drive` half of the
Layout JSON is behind a new `luxel-core/panel` feature that only `hub75`
turns on. Before that feature gate the three strip images each grew
96–112 B for a field they can never populate (Gitea #501/#513).

The remaining 144–384 B is `POST /api/reboot`, which every board carries
because every board can end up with a `reboot_required` Layout (#474's
output table as much as this ticket's chain wiring). It shares one match arm
— and therefore one `finalize + write_to` instantiation — with
`/api/apmode`: given its own arm it cost 624–704 B instead, a whole extra
copy of picoserve's response path, which is the same trap `Reply` exists to
avoid (docs/size-report.md, .claude/rules/firmware.md).

## Panel driver settings: the panel became runtime (2026-09-26, Gitea #401 + #525)

Every HUB75 parameter that used to be a `const` in `firmware/src/hub75.rs` is
now a **stored device setting applied at boot**. One image drives any panel
the RAM fits, which is what a second physical panel on the bench needed:
Jeremy's new 64×64 tiles are not the FM6124EJ the firmware was tuned for.

| knob | where | range | default | what it is |
|---|---|---|---|---|
| `pw` `ph` `cols` `rows` `scan` | `matrix` line | — | 64 64 1 1 (scan `ph/2`) | the arrangement, which now **sizes the framebuffer** |
| `planes` | `panel` line | 4..8 | **7** | BCM bit depth; one rescan shifts the chain `2^planes − 1` times |
| `clock_mhz` | `panel` line | one of 8 · 10 · 12 · 15 · 20 · 24 · 30 | **30** | the LCD_CAM pixel clock — a fixed list, not a range (#771) |
| `chip` | `panel` line | `shiftreg` · `fm6126a` · `icn2038s` · `dp3246` | **`shiftreg`** | the driver chip's register init, bit-banged on the pins before the DMA starts |
| `blank` | `panel` line | 0..8 | **1** | clocks with OE off at the start of each row block and again before the latch word |

Wire format, JSON (`/api/layout`'s `driver` block, with `live` = what actually
booted) and the reboot rules are in docs/api.md, "How the panel is driven".
**Every one of these is reboot-required on a HUB75 board** — including `pw`
and `ph`, which stay live on a strip-built matrix.

**And a reboot the API asked for no longer counts as a failed boot** (#771).
`ota::preboot_guard` rolls back to the other OTA slot after two boots that
never reach `ota::boot_ok`, which only ran at the 60-second mark — so two panel
edits inside a minute, each with its reboot, looked exactly like a crash loop
and rolled Jeremy's device back to firmware that had no `panel` line at all.
`reboot_task` now calls `boot_ok()` before the reset: every API-triggered
reboot goes through that one signal, so reaching it proves the image served a
request. docs/firmware.md, "The boot-loop guard".

**The bench clock table above is now this setting's meaning, not the
firmware's choice.** "The LCD_CAM pixel clock on the panel" measured 20 MHz
and 30 MHz clean and 40 MHz broken *on one FM6124EJ panel*. A clock failure is
invisible to every counter the device has (no swap error, no DMA error, a
byte-identical composed frame) — it needs an eyeball, which is why the setting
is not a free number.

**The clock is a FIXED LIST: 8, 10, 12, 15, 20, 24, 30 MHz** (Gitea #771,
`PanelDriver::CLOCKS`, reported as `driver.clocks` and rendered as a dropdown).
It first shipped as the range 2..40 with a UI warning above 30, and on
2026-09-26 Jeremy set 40 "to see what happens" and got the broken row of the
table. Two reasons for exactly these seven values:

- **They are the rates the hardware can divide to evenly.** esp-hal's i8080
  driver doubles the requested frequency (the S3 errata puts the LCD_PCLK
  divider at ≥ 2) and then divides an LCD_CAM source, so the pixel clock is
  `source / (2 · N)`; the sources on an S3 are XTAL (40 MHz) and PLL_D2
  (PLL 480 / 2 = 240 MHz — the S3's PLL is 480 at every `CpuClock` preset).
  Integer `N` gives 120/N and 20/N MHz: 30, 24, 20, 15, 12, 10, 8, 6, 5, 4, 3,
  2. **16 and 25 MHz are not on that set** — they come out of esp-hal's
  *fractional* divider, which dithers the clock period instead of dividing
  evenly. Worse, 13, 17 and 39 MHz silently clock at **10 MHz**:
  `calculate_clkm` scores its candidate sources through
  `calculate_output_frequency`, which binds the fraction's numerator and
  denominator the wrong way round, and so prefers the XTAL "source too fast"
  fallback (`div_num = 1`, i.e. /2) over a correct PLL_D2 divider.
- **Capped at 30, floored at 8.** 30 is the FM6124 datasheet's FCLK max with no
  margin; below 8 a 7-plane 64×64 rescan falls under ~31 Hz and flickers.

A device that stored a clock this list does not carry (Jeremy's 40) still shows
it in the dropdown, marked `(not supported)`, rather than being silently
re-read as something else.

**`shiftreg` is the common case.** It sends nothing at all, which is correct
for FM6124, SM16208, ICN2037 and any other plain shift-register column
driver. `fm6126a` and `icn2038s` share the two-register init of the C++
`ESP32-HUB75-MatrixPanel-I2S-DMA`'s `fm6124init`; `dp3246` has its own, and
also holds the latch for the **last 3 clocks** of every row instead of 1.
`dp3246` is **incomplete** — it additionally needs the inverted pixel-clock
phase, which is an esp-hub75 cargo feature (`invert-clock`), so the chip
setting alone will not drive one (Gitea #763). Shift-register ROW drivers
(SM5266P / SM5368 address latching) are a separate follow-up ticket.

**Nothing dark, ever, from a bad setting.** A boot attempt that cannot build
— the panel is over `MAX_PIXELS`, a framebuffer or descriptor allocation
fails, the `blank`/latch combination leaves no OE-active clock in a row
block, or `Hub75::new` refuses the clock — is retried **once at the board
default** (64×64, 7 planes, 30 MHz, `shiftreg`, `blank 1`) and reports
`driver.live.fallback: true`. Only if that fails too does panel output stay
disabled with the render loop still ticking, which is the pre-#401
behaviour. Every allocation the failed attempt made is handed back before the
retry (owned `Block`s, not `leak()` at the allocation site); the two
exceptions are spare-plane mode's staging buffer, which comes from the PSRAM
arena and cannot be freed, and everything the GDMA may already point at when
`Hub75::new` itself fails — see docs/firmware.md.

**What to look for on serial.** One boot prints, in this order:

```text
map: 128x64 grid (configured panel)          # only when the panel is not 64x64
hub75: 1x1 tiles of 64x64 from tl row, framebuffer 64x64 scan 1/32, remap off (row-major), est 115 Hz
hub75: fm6126a init sequence sent (194 clocks)   # only when chip != shiftreg
hub75: 64x64 panel, scan 1/32, 7 bitplanes, LCD_CAM @ 30 MHz, chip shiftreg, blank 1, \
       circular DMA, 254 descriptors x 2 rings = 6096 B, framebuffer 28672 B
```

(`framebuffer 28672 B` is ONE of the two — `rows · cols · planes · 2 B` at
`32 · 64 · 7` — which is the number `driver.live.fb_bytes` reports.)

A fallback adds `— FALLBACK, the configured panel would not build` to the end
of that last line, preceded by `hub75: <why> — falling back to the board
default panel`. Two dead ends: `hub75: <why> at the board default — panel
output disabled`, and `hub75: blank N + M latch clocks leave no lit clocks in
a C-word row block` (the template check, which trips the fallback rather than
shipping a black panel). The old `hub75: bulk bitplane packer active` line is
gone — the packer is the only compose path now, so a running panel IS a
running packer.

**Jeremy's new panels use the SM16208SF**, which the SM16208 datasheet brief
and the DMD_STM32 driver table both describe as a plain shift register with
built-in ghost elimination and a 35 ns OE minimum (the related SM16206 is
rated 25 MHz max). So `chip shiftreg` — no init — and the first try on the
old firmware ("kind of worked, some parts wrong, unexpected parts lit") most
likely wants the two knobs that did not exist then: **`clock_mhz 20` and
`blank 2`–`4`**. In wire terms, `panel 7 20 shiftreg 2`.

**Cost**, credless flake builds, master `e6e59cb` as the baseline:

| variant | master | this change | Δ | of its slot | `.stack` |
|---|---:|---:|---:|---:|---:|
| `seengreat-hub75` | 1,147,216 | **1,158,048** | +10,832 | 36.81 % of 3 MiB | 25,852 → **31,620** |
| `seengreat-hub75` + `hub75-spare-plane` | 1,150,928 | **1,161,888** | +10,960 | — | **31,452** |
| `s3-devkit` + `hub75` | 1,145,888 | **1,156,592** | +10,704 | **88.24 %** of 1.25 MiB | **29,020** |

`.stack` goes UP by 5,768 B on the Seengreat because the descriptor rings
left `.bss` for the heap — a runtime geometry cannot use
`hub75_dma_descriptors!`, which is a compile-time static — so that is not
free RAM, it moved. Largest frame is unchanged (picoserve's 10,512 B against
the 12,288 B budget) and `tools/stack-check.sh` is green on all three. The
~10.7 KB of image is the runtime framebuffer, the template writer, the
runtime-dimensioned packer, the four chip sequences and the fallback path;
strip boards are untouched (it all lives behind the `hub75` feature).

**Host tests**: `luxel-hub75` 47 (the control template byte-identical to
`hub75-framebuffer`'s own at `blank 1` / 1 latch clock for six geometries;
the runtime packer byte-identical to `set_pixel`; the chip sequences
step-exact against the C++ reference), `luxel-core` 314, 677 in the
workspace. The runtime packer is **0.91–0.93×** the const-generic one on
x86 — the cost of `cols` no longer being a constant.

**Nothing here has been on a panel yet.** The on-metal list is **Gitea #765**:
the chip inits, a 1/N-scan panel, chains at the new sizes, non-default
`planes`/`clock_mhz`/`blank`, the fallback path and the board-map refresh.
docs/UNTESTED.md carries the status.

## The framebuffer swap is frame-atomic (2026-09-07, Gitea #376)

Jeremy asked how tearing is avoided when the framebuffer is fetched by DMA.
Until this change it was not.

**The mechanism that tore.** The panel is refreshed by one circular DMA
descriptor ring covering the whole BCM repetition sequence of *one*
framebuffer (esp-hub75 0.14, feature `circular-dma`). Upstream's
`Hub75::swap` rewrote **every descriptor's `buffer` pointer** by the
old→new delta the instant it was called, while the DMA was mid-pass; its own
SAFETY note conceded "the worst-case visual artifact is one partially-mixed
frame". Because the ring is ordered by plane repetition — plane 0 sixty-four
times, plane 6 once — the mix is the *high bitplanes of frame N with the low
bitplanes of frame N+1*, i.e. colour corruption at moving edges rather than a
clean horizontal tear. `SWAP_DONE` was frame-boundary, but it only guarded
reclaiming the old buffer, not the switch itself. The firmware composes up to
125 frames a second against a 115 Hz rescan, so most displayed passes mixed
two frames. Filmed on the bench panel with `library/frame-rate-scan.js`: at a
sweep-column change, **two columns are lit at once, one of them partially**.

**The fix** is a local patch to esp-hub75
(`firmware/patches/esp-hub75-0.14.0-atomic-swap.patch`; the patch header is
the full write-up). **One descriptor ring per framebuffer.** Each ring's tail
`next` points at its own head, so a ring on its own loops forever over a
single image. A swap is then ONE naturally-aligned 32-bit store: the running
ring's tail `next` is rewritten to the other ring's head. The DMA reads a
descriptor's `next` only when it finishes that descriptor, and the tail is the
last descriptor of a complete BCM pass, so the switch lands exactly on a panel
frame boundary with no ISR latency in the path. **Every pass the engine makes
reads exactly one framebuffer.** No `buffer` pointer is ever touched while the
DMA is inside the ring. When the flip lands, the frame-count ISR restores the
ring it left (tail `next` → its own head) so that ring is self-contained again
for the next swap.

**Knowing when it landed** is the subtle half, because the store can lose the
race with the DMA's prefetch of the tail, in which case the engine wraps to
its own head once more and the flip lands a frame later (still never a mixed
frame — only later). Two independent proofs, either sufficient:

- `swap()` reads the DMA channel's current-outlink-descriptor register (GDMA
  `OUT_DSCR`). If the engine is still at least three descriptors short of the
  tail, the tail cannot have been fetched, so it is certain to read the `next`
  just written and the **very next `out_eof` is the switch**.
- Otherwise **two EOFs**, which is unconditionally safe.

`out_eof` means "the last byte of that descriptor has been read from memory",
so under either proof the old framebuffer is provably free when it is handed
back. This matters for throughput, not just correctness: an early version
landed only on the two-EOF rule and the compose rate settled at **52 fps**
against the 115 Hz rescan, because every swap cost two panel frames.

**Cost.** One extra descriptor ring in `.bss`: 254 descriptors × 12 B ×
2 rings = **6,096 B**, up from 3,048 (`__DESC_CELL` 0xbec → 0x17d4 in the
linked image). It comes out of the leftover `.stack` region — 33,372 →
30,268 B, and `tools/stack-check.sh` still passes with the largest frame at
9,648 B. *(Since #401, 2026-09-26, both rings are **heap**, not `.bss`:
`hub75_dma_descriptors!` is a compile-time static and the geometry is a
runtime setting. Same 6,096 B at 64×64/7 planes, and it is why `.stack` on
this board went UP by 5,768 B — see "Panel driver settings" above.)* Flash cost is +664 B (`.text` +512, `.rodata` +112, `.data` +40);
the app image goes 950,480 → 951,264 B (+784 with headers and padding),
leaving 97,312 B (9.28 %) of the OTA slot still free. Heap is untouched — the framebuffers themselves did not change.

**Measured on the panel** (`board-seengreat-hub75`, 4096 px, 30 MHz, 7
planes, `library/frame-rate-scan.js` at ComposeCap 0), before and after:

| | `fps` | `out_fps` | `rescan_hz` | `vm_us` | `frame_us` | heap free |
|---|---:|---:|---:|---:|---:|---:|
| master `0d6beda` | 124 | 119–123 | 115 | 843–857 | 893–912 | 49,280 |
| + atomic swap | 124 | 113–120 | 115 | 831–881 | 879–943 | 41,088–49,280 |

Identical within noise: the swap was already only a handful of stores, and it
still is. `vmerr` null and `fence_timeouts` 0 throughout. What changes is what
the panel *shows*, which no API field reports — that is what the camera is
for.

**What this did NOT fix**, and the next section does: composed frames were
still *dropped* (`write_frame` returned early while a swap was pending), and
`out_fps` still counted `write_frame` calls rather than displayed frames —
which is why the table above reads `out_fps` 113–120 against a 115 Hz
rescan. See "Vsync: the panel is the clock" below (Gitea #387, #378).

## Spare-plane swap: the second framebuffer becomes one plane (2026-09-20, Gitea #610)

Jeremy's target is a 128x128 wall (four chained 64x64, electrically one 256
x 64 chain). The two-framebuffer atomic swap above needs 229 KB of internal
SRAM for that — the whole reason #521/#599 called 128x128 unreachable. The
DMA ring cannot move to PSRAM (an octal-PSRAM-resident HUB75 ring caps at ~13
MHz in prior art, and the cache is off during every flash write — the option
E record on #611), but the second buffer does not have to be a whole buffer.

**Mechanism.** The ring is plane-major, plane 0 = MSB repeated 64 times at 7
planes, so the first half of every pass reads only plane 0 and planes 1..6
are idle. The driver is built against two *views* over ONE internal
framebuffer that differ only in which block is plane 0: the buffer's own, or
a spare plane. The #376 two-ring flip works unchanged on views. Per frame
(`hub75.rs`, feature `hub75-spare-plane`): compose into a staging framebuffer
in the PSRAM arena; when the output task's poll finds the DMA inside the MSB
run with room (measured per-plane copy cost against the ISR's nominal pass
length, via the new `Hub75::dma_position()` from
`firmware/patches/esp-hub75-0.14.0-dma-position.patch`), arm the flip FIRST,
copy planes 1..6 into the live buffer, then the MSB into the idle spare. The
next pass reads the new frame in full. Deadlines: plane 1 before the DMA
leaves the MSB run (~4.4 ms after the EOF), plane k before it reaches plane k
(exponentially later), the spare before the wrap. Copies go in that order,
so only plane 1's deadline is tight, and a poll that arrives late simply
defers the frame to the next pass rather than starting a copy that cannot
finish. `/api/status` `pass.spare` carries `deferred`, `torn_p1`,
`torn_wrap` (the last two must stay 0), `copy_us` and `plane_us`.

**Cost on this board (64x64, 7 planes), from the build — NOT yet measured on
metal:**

| | two framebuffers (default) | spare-plane |
|---|---:|---:|
| internal DMA memory (heap-leaked) | 57,344 B | 32,768 B (28,672 + 4,096) |
| staging (PSRAM arena) | — | 28,672 B |
| descriptor rings (`.bss`) | 6,096 B | 6,096 B |
| app image | 970,128 B | 973,744 B |
| `.stack` | 27,188 B | 27,124 B |

*(Every row of this table is a 64×64/7-plane figure, and since #401 every one
of them is derived from the stored `panel`/`matrix` settings at boot rather
than from a type — the descriptor rings included, which are **heap** now, not
`.bss`. Current image and `.stack` numbers for both variants are in "Panel
driver settings" above.)*

At the 256-column chain the same shape is 114,688 + 16,384 B internal against
229,376 B — the #611 ledger. **Off by default** until Jeremy has looked at it
on the bench (`nix build .#luxel-fw-seengreat-hub75-spare`, or
`EXTRA_FEATURES=hub75-spare-plane` with `build-esp32.sh`): the verification is `pass.spare.torn_*` at 0 and no visible
artifact through a Raindrops / Infinite Snake / comet run, pattern saves and
an OTA (Gitea #620).

## Vsync: the panel is the clock (2026-09-07, Gitea #387, #378)

With the swap made atomic (above), the panel still showed fewer frames than
the firmware composed, and `/api/status` did not admit it. The render loop
ticked every 8 ms (125 fps) against a panel that rescans every 8.7 ms
(115 Hz), so the two clocks beat.

**How much was actually being lost was worse than it looked.** `out_fps`
counted every `write_frame` **call**, including the ones that returned
without drawing because the previous swap had not landed. The give-away was
`out_us`: 3,804 µs averaged over 123 "frames", against a true compose cost of
**7,400 µs** at 4096 px. Roughly half those calls did nothing. And because a
refused call did not retry until the render loop came round again ~8 ms
later, a compose that could have started at the rescan boundary typically
started several milliseconds after it, missed the next boundary, and landed a
whole rescan later: the panel was displaying about **60** frames a second
while `out_fps` reported 123.

**The fix is to make the hand-off buffer the clock.** Three pieces, all
HUB75-only — a strip is wire-bound and keeps the 8 ms floor:

- `OutputDriver::write_frame` returns whether the frame was actually written.
  `out_fps` counts only those, so it is a displayed rate on every board
  (#378).
- The output task **holds** a frame until `ready_for_frame()` — the previous
  swap has landed — instead of composing into a buffer about to be
  overwritten. Composing from the boundary is what gets the next swap armed
  before the following boundary, which is what makes one displayed frame per
  rescan possible at all.
- The render task's `emit` **waits** for the travelling buffer to come back
  rather than dropping the frame, and takes only a genuinely free buffer —
  never `claim`'s newest-wins steal-back, which let the loop run a frame
  ahead whenever the output task's wake was late (it showed up as `fps` 118
  against `out_fps` 112). That wait is the entire back-pressure mechanism:
  there is no timer in the path. The VM still overlaps the compose, because
  the render task waits AFTER the pattern has run, not before it.

A 50 ms cap on both waits is a liveness floor, not the pacing — a dead panel
must not freeze the engine, the pattern clock or `fps`.

Measured on the panel, `library/frame-rate-scan.js` at ComposeCap 0, 4096 px:

| | `fps` | `out_fps` | `rescan_hz` | `vm_us` | `frame_us` | `out_us` |
|---|---:|---:|---:|---:|---:|---:|
| atomic swap only (#376) | 124 | 113–120 (a call count) | 115 | 831–881 | 879–943 | 3,753–4,239 |
| + vsync | **106–112** | **106–113** | 115 | 868–931 | 935–1,024 | 7,370–7,444 |

`fps` and `out_fps` now track each other frame for frame — **nothing is
composed only to be thrown away** — and both sit a few percent under
`rescan_hz`. That gap is the compose itself: 7.4 ms of an 8.7 ms window
leaves 1.3 ms of slack, so a compose occasionally overruns its rescan and the
panel repeats a frame. A repeat is not a skip: no composed frame is lost.
`out_us` doubling is the counting fix, not a slowdown — it is the same
compose, no longer averaged with the calls that did nothing.

`bulk-comet-trails` over three minutes: `fps` 107–115, `out_fps` 106–115,
`rescan_hz` 115, heap free 42,024–50,216 B flat, `fence_timeouts` 0, `vmerr`
null. `rainbow` (the render-bound case, 19.5 ms per VM frame) is unchanged at
`fps` 51 / `out_fps` 51–52: the pipeline still overlaps VM and compose, which
is why the wait lives in `emit` rather than in the pacing.

Costs: app image 951,264 → 953,648 B on `board-seengreat-hub75` (+2,384,
including #384), `.stack` 30,268 → 30,204 B, stack-check green.
`board-pixelblaze-v3`, which has none of this, pays +400 B for the
`write_frame` return alone — the `.await` is behind an `emit!` macro that
expands to a plain call off the pipelined path, because making the direct
sink async too cost every non-panel board ~864 B of state machine for a
future that never yields (Gitea #160: that board has 3.7 % of its slot left).

### The skip that survived vsync: a race in the swap-landing shortcut

Jeremy filmed the vsync build and reported: "much better. I have observed
repeats (not many). I also observed (sadly) a skip." Repeats are expected —
see above. The skip was not, and finding it took a counter, because **this
class of loss is invisible to frame accounting**.

`swap()` decides whether the very next `out_eof` is the ring switch, or
whether it has to wait for two. The shortcut tested that the DMA was at least
three descriptors short of the ring's tail: if so the tail cannot have been
fetched yet, so it must read the `next` just written. True — but "short of the
tail" is **equally true immediately after the DMA wrapped past it**. There the
tail was fetched *before* the store, this pass does not flip, and the EOF it
had already raised gets miscounted as the switch. Two things then go wrong at
once: the compose is handed a framebuffer the DMA is still scanning out, and
the ISR "restores" the old ring's tail, *undoing the flip*. One frame both
torn and never displayed. The window is exactly "an EOF has fired and its ISR
has not run yet" — wide open inside `swap()`, because `critical_section` masks
interrupts while it runs.

Nothing downstream can see it: `write_frame` succeeded, so `out_fps` counts
the frame and `dropped` sees no gap. It is only visible on the panel.

Closed by probing `OUT_INT_RAW.out_eof` alongside `OUT_DSCR` and taking the
two-EOF fallback whenever an EOF is pending. `/api/status` `swap.eof_race`
counts entries to the window so the rate is measured, not guessed:

| | measured |
|---|---:|
| `eof_race` over 725 s at 4096 px / 115 Hz | **41** |
| rate | one every **17.7 s**, 1 frame in ~**1,950** |
| `slow_path` (two-EOF fallback, any reason) | 318 — 0.4 % of swaps |

One skip every 18 seconds is exactly "I observed a skip, maybe there was more,
I stopped watching". The `slow_path` share is small enough that the fallback
costs no measurable throughput.

### Proving the pipeline lossless: `dropped`

`/api/status` gained **`dropped`** — rendered frames the fixture never showed,
cumulative since boot. It is *derived*, not enumerated: the output task adds
the gap between the sequence numbers of consecutive **displayed** frames, so
it counts every route a frame can go missing by, including routes the firmware
does not know about. `drops` breaks the known ones out (`handoff`,
`overwrite`, `refused`), bins losses by frame number mod 64 (the sweep column,
with `frame-rate-scan`), and keeps the last 16 as `[seq, route, ms]`.

Over 725 s of `frame-rate-scan` with **zero polling** from the host — ~79,750
frames — the delta was **0**. Every drop the board has ever recorded is a boot
transient: 9 of them, all route `handoff`, all inside the first 3.8 s, before
the output task publishes the driver's pacing capability and `emit` starts
waiting for the buffer instead of dropping. The mod-64 histogram holds only
those nine, spread across bins 1–8 and 10 — **no clustering, and nothing in
bins 54–63**, which is where a right-edge-specific fault would have shown.

Host-side control, ruling the pattern out: 640 rendered frames of
`frame-rate-scan` at 8.0 / 8.7 / 9.5 ms cadences give `missing = 0`,
`multi = 0`, and every column 0–63 lit exactly 10 times — including 54–63. The
sweep never fails to draw a column and never clips at the right edge.

So the accounting is clean, the pattern is clean, and the skip was the swap
race above.

## The compose, 7.3 ms → 2.2 ms: a row-oriented bitplane packer (2026-09-07, Gitea #329)

Vsync (above) left one artefact: the compose used 7.3 ms of the panel's
8.7 ms rescan, so **1.3 ms of slack**, and anything that held core 0 longer
than that pushed the swap past the wrap and the panel rescanned the previous
frame — a REPEAT. Jeremy, filming the #398 build: *"There is no more
skipping. There are still a ton of repeated frames though."* He was right,
and the counter agreed: **6.0 % of passes idle, 11 % with a playground tab
open, 31 % under load**.

**The compose was per pixel.** `DmaFrameBuffer::set_pixel` re-derives the
row/column index, bounds-checks it, extracts one bit from each of three
channel bytes and read-modify-writes a `u16` — once per bitplane. At 4096 px
and 7 planes that is 28,672 such updates, and it costs the same for every
pattern, because bitplane packing is content-independent.

**Do it per row pair instead.** One entry word carries one bitplane of one
PIXEL PAIR: column `x` of row `r` (top half, bits 9–11) and of row `r + 32`
(bottom half, bits 12–14). All seven plane words for that pair therefore come
from the same six channel bytes with only the bit position changing. So the
packer computes, once per pair, a `u32` in which every plane's six colour bits
already sit at a fixed 8-bit stride, and each plane is then one shift, one
mask and one OR. The `u32` is six table lookups — two 256-entry tables spread
a channel byte's plane bits to that stride — and **brightness scaling is
folded into those tables**, so `scale5` leaves the inner loop entirely. The
packer writes every colour bit of every entry, so it also subsumes `erase()`.

Measured on the bench panel, `library/frame-rate-scan.js`, 4096 px,
brightness 3, 30 MHz:

| | `fps` | `out_fps` | `rescan_hz` | `vm_us` | `out_us` |
|---|---:|---:|---:|---:|---:|
| master `9d71d26` | 107–113 | 107–113 | 115 | 917–969 | **7,224–7,272** |
| + #329 packer | **115–116** | **115–116** | 115 | 862–949 | **2,166–2,221** |

**3.3x**, and the panel now displays a new frame on *every* rescan — `out_fps`
has reached `rescan_hz`. The slack goes from 1.3 ms to **6.5 ms**, five times
the tolerance, which is what the repeats needed.

`tools/panel-load-bench.mjs`, same build either side, repeats read from the
driver's own per-pass ISR counter (#398) so the figure is exact:

| phase | repeats/min before | after | repeat share before → after | `out_fps` before → after |
|---|---:|---:|---|---|
| idle | 400.2 | **30.9** | 6.0 % → **0.5 %** | 107.6 → **115.5** |
| one playground tab | 742.3 | **64.7** | 11.0 % → **1.0 %** | 104.5 → **114.8** |
| busy (1 client looping the bundle) | 2,113.7 | **203.2** | 31.1 % → **3.1 %** | 80.7 → **112.5** |

Ten minutes of `frame-rate-scan` on the merged build, 69,517 consecutive
rescan passes: `pass.repeats` **295 (0.42 %)**, `pass.skips` **0**, `dropped`
0, `pass.short`/`long` 0/0, `zero_rescan` 0, `eof_race` 4, `slow_path` 44,
`fence_timeouts` 0, heap free 38,648–39,040 B flat, `vmerr` null, `fps`
114–116 / `out_fps` 114–117 against `rescan_hz` 115.

**The web got faster too**, which is the part worth remembering: freeing 5 ms
per rescan on core 0 took `/api/status` p50 from 92 → 71 ms idle, 80 → 38 ms
with a tab open and 111 → 61 ms under load, and bundle throughput from 266 →
375 KiB/s. A compose at 85 % duty was starving the web server, not just the
panel.

What it did **not** do is make load free: busy is still ~6x idle, the same
ratio as before, so the remaining repeats are still web handlers blocking
core 0 (#395 stays open for that; the residue is 3 % of passes under a load
harsher than real usage, against 31 % before).

**Correctness is a host assertion, not a device claim.** The packer lives in
`crates/luxel-hub75`, which builds on the host, and its tests construct a real
`hub75-framebuffer` `DmaFrameBuffer` through the stock per-pixel path and
through the packer and require the two buffers to be **byte-identical**:
random frames at all 32 brightness values, every combination of the edge
channel values, short and oversized frames, five panel geometries, and real
`frame-rate-scan` / `rainbow` / `snake-2d-v2` frames rendered by the engine on
the 64x64 grid map. They run in `cargo test --workspace`, so `tools/ci.sh`
gates them.

Layout safety is two-sided, because the packer indexes the framebuffer as a
flat `u16` array. A `const` assert pins the word count (any
`hub75-framebuffer` `inter-row-blank-*` / `tail-closes-latch` feature changes
`size_of` and breaks the build), and because size cannot catch column
REordering (`esp32-ordering` XORs adjacent columns on the classic ESP32), a
boot-time probe writes three pixels through the crate's own `set_pixel` and
checks they land where `pack` would have put them. A failed probe, or a failed
2 KiB table allocation, keeps the per-pixel path and says so on serial; a
healthy board prints `hub75: bulk bitplane packer active (2048 B of tables)`.

*(Both halves of that paragraph are history as of #401, 2026-09-26. There is
no third-party framebuffer left to disagree with: the firmware owns the
buffer (`DynFb`) and `luxel_hub75::format` writes the control template, and
the two are asserted byte-identical to `hub75-framebuffer`'s on the host. The
`size_of` assert and the boot probe are gone, replaced by `template_lights` —
which asks the one question the host cannot, whether the configured
`blank`/latch widths leave any OE-active clock in a row block, and trips the
fallback if not. The per-pixel compose path is gone too: the packer is the
only one, so failing to find its 2 KiB is a boot failure, and the "packer
active" serial line no longer exists. See "Panel driver settings" above.)*

Costs: app image 960,208 → 962,288 B on `board-seengreat-hub75` (+2,080;
8.22 % of the OTA slot still free), `.stack` 28,788 → 28,780 B, the output
task's frame 1,360 B against the 12,288 B budget, stack-check green.
`board-pixelblaze-v3` moves +32 B, which is section padding shifting under a
changed crate-metadata hash — no code from the new crate links there, and a
hash-normalised symbol diff of the two ELFs is identical.

## Seeing the displayed frame rate: `library/frame-rate-test.js` (2026-09-07)

`rescan_hz` is the driver's own count. `library/frame-rate-test.js` ("Frame
Rate Test") is the independent check — an instrument pattern that makes the
gap between the **composed** and the **displayed** rate visible on the panel
itself. Push it live (`POST /api/code`) and read the panel, not the API.

**Why it has to be temporal.** A dropped frame leaves no mark on any single
composed frame: every frame is a complete image, and the one that never
reached the panel simply never existed for the eye. So the pattern flips the
whole field RED/GREEN once per `renderFrame` call — the flip is driven by a
counter, never by the clock. If every composed frame were displayed exactly
once, the alternation would fuse to steady yellow; every frame the panel
misses puts two same-colour frames side by side on the retina. Those stumbles
happen `|C − R|` times a second (C = compose rate, R = displayed rate), so the
field shimmers red/green at the beat frequency and

    displayed fps  =  compose fps  −  stumbles per second

**Reading it.** The bottom 1/16 of the panel blinks blue at
`|composeFPS − DisplayedFPS slider|` Hz; turn the slider until the blink keeps
time with the field's shimmer and it reads the displayed rate. Rows 48–55 are
the pattern's own compose-fps bar (its EMA of `1000/delta`, 2 fps per column,
ticks at 60 / 77 / 115-in-magenta / 125) — the compose rate the API's `fps`
field will not give you once a cap is set. Rows 56–59 repeat the beat as a
left/right parity stutter. The definitive version is a 240 fps phone video:
each displayed frame occupies ~2 camera frames, so count the runs that are
~4 camera frames long — that count per second is exactly `C − R`.

Measured live on the bench panel, 2026-09-07, master `b08bbd4` at 30 MHz,
4096 px, brightness 31: `fps` 125, `out_fps` 125, `rescan_hz` 114–116,
`vm_us` **478–498** (0.5 ms of an 8 ms budget — it is four bulk fills), the
pattern's own `composeFPS` 123.9–124.3 and `beatHz` 8.9–9.3 at the default
slider. So ~9 stumbles a second against a ~124 fps compose rate, and the
panel is displaying ~115 — `rescan_hz` and the beat agree.

### `setFrameRate` was quantized to 125/n on the firmware, and this is why

> **Fixed 2026-09-07 (Gitea #384).** The engine now carries the accumulator
> remainder instead of zeroing it, so the long-run average is exactly the
> requested rate for any cap at or below the loop rate; individual periods
> still jitter by up to one tick. And with vsync pacing (above) the tick grid
> on this board is the rescan, not 8 ms, so a cap holds in whole rescans.
> The measurements below are the pre-fix behaviour, kept because they are
> what the quantization looks like when you meet it.

The render loop in `firmware/src/main.rs` was paced to one iteration per 8 ms.
`setFrameRate(F)` makes the engine hold frames until `1000/F` ms have
accumulated, and it used to **reset** its accumulator (no remainder carry), so
a cap fired on the first 8 ms tick at or past the period and the only
achievable compose rates were `125/n`. Measured on the panel through the pattern's
`ComposeCap` slider and its own `composeFPS` var:

| `setFrameRate(F)` | predicted | measured `composeFPS` | `/api/status` `fps` |
|---:|---:|---:|---:|
| 0 (uncapped) | 125 | **123.9** | 125 |
| 125 | 125 | **124.1** | 125 |
| 115 | 62.5 | **61.5** | 125 |
| 100 | 62.5 | **61.6** | 125 |
| 62.5 | 41.7 | **42.7** | 125 |
| 60 | 41.7 | **41.5** | 125 |

Two consequences. First, `/api/status` `fps` is the **host loop rate** and
does not follow the cap at all (docs/lang.md says so; this is it on metal) —
only the pattern's own `1000/delta` sees the real compose rate. Second, you
cannot tune the strobe to a stroboscopic null by capping the compose rate at
the display rate: asking for 115 gets 62.5. That is why the pattern strobes
uncapped and measures the beat instead. Dropping the cap *below* the display
rate is still useful: every composed frame is then shown at least once, the
beat disappears, and a gross regular flicker at `F/2` takes its place.

### `library/frame-rate-scan.js` — the camera version

Frame Rate Test above reads the displayed rate as a *beat*, which takes a
practised eye. **Frame Rate Scan** is the version to point a phone at: it
spends one visible state per composed frame and carries its own clock, so the
camera's frame rate never enters the arithmetic and does not need to be known.

Three one-column bars on the 64x64 grid: rows 0–23 the **sweep**, at column
`frameIndex mod 64`, RED on even composed frames and GREEN on odd (a counter,
never the clock); rows 28–39 the **fine clock**, one column per 10 ms, wrapping
every 640 ms, blue; rows 44–55 the **coarse clock**, one column per 100 ms,
wrapping every 6.4 s, white. Dim grey ticks every 8 columns sit under each band
and on the bottom row, with column 0 in cyan.

Reading it, from a video at any rate above about twice the display rate:

1. Pick two video frames roughly a second apart, A and B.
2. On each, read the coarse column `c` and the fine column `f`, then
   `d = (f − 10·c) mod 64` (0..9, the tens-of-ms digit) and `t = 100·c + 10·d`
   milliseconds. `dt = tB − tA`, plus 6400 ms if the coarse bar wrapped.
3. Step through every video frame from A to B, note the sweep column, and drop
   repeats (the camera sees most displayed frames two or three times). The
   count of **distinct** positions is how many frames the panel displayed:
   `displayed fps = distinct / dt`.
4. Where the column jumps by more than one, the panel skipped `jump − 1`
   composed frames; the colour is the cross-check, since two adjacent distinct
   positions sharing a colour means an even number of composed frames went by.
   `composed fps = (distinct + dropped) / dt`, which should equal
   `/api/status` `fps`.

Live on the panel 2026-09-07 (master `e4f772b`, 30 MHz, 4096 px, brightness
31): `fps` 125, `out_fps` 125, `rescan_hz` 115–116, **`vm_us` 793–803**,
`frame_us` 838–850, heap free 49,280. Host verification: over 400 frames at
both 125 and 60 fps injected, the sweep column is exactly `frame mod 64` with
exact parity colours on every row of its band, and both clock columns match
`floor(elapsedMs/10) mod 64` and `floor(elapsedMs/100) mod 64` on every frame;
running the reading procedure above over the dump returns 125.0 and 60.0 fps.

**Camera caveats.** Phone cameras use a **rolling shutter**: the sensor reads
out line by line across the frame period, so one edge of the panel is sampled
early in each camera frame and the other edge late — and at low brightness the
LEDs are lit for only a short window once per rescan (at brightness 3 just two
low bitplanes light, ~0.8 ms in the last 12 % of the 8.7 ms pass), so on video
every other camera frame looks dark. That same skew makes the sweep's wrap
(column 63 → 0) show either **no** dark frame between the two lit columns or an
**extra** one depending on which way the phone is held — rotating the phone
180° flips one into the other (confirmed on the bench); both frames were
displayed, the difference is the camera. A camera frame that shows a column
*dimmed* rather than lit or dark is the lit window straddling a camera frame
boundary, the camera's rate not being a multiple of the display rate — also
not a display artefact. So read the display rate by counting **distinct sweep
positions** against the panel's own clock rows, never by counting dark frames
or trusting the camera's nominal rate: a real skip is two adjacent lit
positions of the **same** colour (parity), and `/api/status` `pass.skips` /
`pass.repeats` (#398) are the firmware-side ground truth to compare against.

2026-09-19, **#538's three new endpoints, paid for by one integer parser.**
`GET/POST /api/name` (a device name — a reserved-key blob plus its boot
read, `firmware/src/devname.rs`), `POST /api/clock/sync` (an SNTP poke
signal) and `geom.compatible` in `/api/status`. Credless flake builds of
`origin/master` `3727234` vs the branch, on the same machine — and these are
the CI runner's own numbers for master byte for byte (run 1858), so the
`--remap-path-prefix` set from #441 really has closed the host gap:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-pixelblaze-v3` | 1,003,040 | 1,004,704 | +1,664 | 43,872 B (4.18 %) |
| `board-athom-music` | 1,028,288 | 1,029,872 | +1,584 | 18,704 B (1.78 %) |
| `board-c6-devkit` + `hosted-ui` | 1,015,440 | 1,016,832 | +1,392 | 31,744 B (3.03 %) |

**The C6 hosted image had 1,679 B of CI margin and the feature cost 2,480 B**
(3.16 % → 2.92 %, under `tools/image-check.sh`'s 3 % floor, which
`tools/ci.sh` gates). Micro-optimising the feature could not close that —
two attempts made it WORSE, recorded below. What closed it was #465's
`str::parse` lesson applied to `firmware/src/server.rs`: it was parsing
**five** integer widths (`u8` ×3, `u16`, `u32`, `i16`, `i32`) and each width
instantiates its own `from_str_radix`. Routing every unsigned one through a
hand-rolled `fn num(&str) -> Option<u32>` and narrowing with `as`, plus
reading the timezone as the `i32` the control path already instantiates, is
**−1,088 B** on the C6 — more than the whole of `/api/name`. `u8`/`u16`
remain linked via `devicemap.rs` and `outpipe.rs`, so the saving is smaller
than five-widths-minus-two would suggest; measure, do not extrapolate.

The projection half pays 480 B of its own way (`Engine::sync_plan` −340,
`main` −140). Removing the four now-unreachable modes buys no more than
that: the firmware never linked `projection_label` or the option tables'
display path, so only the plan arms were live.

Two micro-experiments that did NOT pay, so nobody repeats them: holding the
two device-name cells as `heapless::String<32>` instead of `String` cost
**+2,032 B** on this variant (1,017,904 → 1,019,936) while saving 112 B on
the Xtensa boards, and parking the board default in a `StaticCell` as a
`&'static str` cost another **+256 B**. At this size the RISC-V codegen's
response to a small shape change is larger than the change itself — measure
the image, on the board that is gated. The underlying squeeze is Gitea #543.

`.stack` (`tools/stack-check.sh`): `board-pixelblaze-v3` was already **116 B
UNDER** the 24,576 B floor on master (24,460 B) and #538's statics took
another 120 B, so the classic-ESP32 `heap_allocator!` gives 512 B back as
`STATICS_RESERVE` — pb-v3 **24,852 B**, athom-music **25,676 B**,
pb-v3 + `small-chip` **26,468 B**, all green. `tools/ci.sh` does not run
stack-check, which is how master drifted under it unnoticed (Gitea #515).

**2026-09-24, `STATICS_RESERVE` 512 → 4096.** Phase C's text-slot table
(#484/#485) and Phase B's scene routes (#478) took another ~3.3 KB of DRAM
statics between them, most of it the **web task's future** — picoserve's
whole response path, replicated `server::WEB_TASK_POOL_SIZE` times, which
four new route arms grow by ~400 B per slot (measured: `web_task::POOL`
28,368 → 29,592 B on pb-v3). Master was already under the floor before this
branch touched it:

| board | master | #478 branch | after the reserve bump |
|---|---:|---:|---:|
| `board-pixelblaze-v3` | 23,484 | 21,252 | **24,836** |
| `board-athom-music` | 24,340 | 22,100 | **25,684** |
| `board-c3-devkit` | 34,648 | 32,424 | 32,424 (unchanged — non-esp32) |

The classic ESP32 gives up 3.5 KB of heap for it, the same trade
`SECOND_OUTPUT_RAM` makes and for the same reason: on that chip `.stack` is
the DRAM left over, so a growing static eats the stack floor rather than the
heap. The RISC-V and S3 boards have DRAM to spare and are untouched.

2026-09-19, **#550's finer `reboot_required` — four shapes, one of them
nearly free.** `Layout::reboot_required` stopped comparing the whole output
table and started comparing only what a boot BUILDS. That is strictly more
code than the `self.outputs != next.outputs` it replaced (which reused the
`Vec<Output>` equality `Layout: PartialEq` already links), so the only
question was how much. Credless flake builds of `origin/master` `e0005a0`
against the branch, same machine, `luxel-fw-ota.bin`:

| shape | athom-music | c6-devkit + `hosted-ui` | pixelblaze-v3 |
|---|---:|---:|---:|
| `Option<(pin, proto, order)>` per index, `(0..lim.outputs).any(…)` | +400 | +144 | +400 |
| the same packed into one `u32`, `while` loop | +448 | +128 | +448 |
| normalise both tables into `Vec<Output>` and reuse `Vec` equality | +304 | +272 | +320 |
| **in-place: a 1-element `implicit` array, `zip`, compare 3 fields** | **+64** | **+80** | **+80** |

The winner allocates nothing and instantiates nothing: the empty table is
covered by a one-element stack array, and the comparison is three byte
compares in a `zip`. The two "clever" shapes above it are the same lesson
#538 already recorded — at this size the codegen's answer to a shape change
is bigger than the change — and the `Vec` one shows that reusing an
already-linked `PartialEq` is NOT automatically cheaper than writing the
comparison out (`<Layout>::drivers` was 162 B of new function plus 88 B
inside `parse`, read out of `nm --print-size` after stripping the mangling
hashes). The c6 hosted image keeps 31,664 B of slot — 206 B above the
31,458 B `tools/image-check.sh`'s 3 % floor demands, where master itself had
286 B. That margin, not the Xtensa boards' 18 KB, is the binding constraint
on anything Phase A adds (#543).

2026-09-20, **the RTC watchdog learns to watch the AppCpu** (Gitea #603 — the
render core's heartbeat gates the RWDT feed, so a wedged render loop reboots
within ~33 s instead of needing a hands-on power cycle; found by #601). The
gate is `#[cfg(multi_core)]`, so the RISC-V boards pay **exactly nothing** —
`core1::beat()` is an empty `#[inline(always)]` there and `appwdt.rs` is not
compiled at all. Credless flake builds (`nix build .#luxel-fw-<variant>` →
`luxel-fw-ota.bin`) of `origin/master` `cb7002f` against the branch:

| board | before | after | Δ | slot margin |
|---|---:|---:|---:|---:|
| `board-athom-music` | 1,029,952 | 1,030,704 | **+752** | 17,872 B (1.70 %) |
| `board-pixelblaze-v3` | 1,004,752 | 1,005,408 | **+656** | 43,168 B (4.12 %) |
| `board-esp32-generic` | 1,025,776 | 1,026,576 | **+800** | 22,000 B (2.10 %) |
| `board-s3-devkit` | 973,136 | 973,920 | **+784** | 74,656 B (7.12 %) |
| `board-s3-devkit` + `hub75` | 980,720 | 981,616 | **+896** | 66,960 B (6.39 %) |
| `board-seengreat-hub75` | 968,976 | 969,792 | **+816** | 78,784 B (7.51 %) |
| `board-c3-devkit` | 973,040 | 973,040 | **0** | 75,536 B (7.20 %) |
| `board-c6-devkit` | 1,033,024 | 1,033,024 | **0** | 15,552 B (1.48 %) |
| `board-c6-devkit` + `hosted-ui` | 1,016,912 | 1,016,912 | **0** | 31,664 B (3.02 %) |

The whole Xtensa cost is one task body: `watchdog_task`'s poll goes 108 →
570 B (`xtensa-esp32-elf-nm --print-size` on the athom ELF), `render_task`
grows 9 B for the heartbeat store, and the rest is `boot_blackbox`'s
`AppCpuStall/` prefix plus two black-box slots. Two shapes were measured and
dropped on the way: `String::insert_str` for that prefix plus a two-argument
trip log, instead of a format ARGUMENT and one argument (together +144 B),
and 64-bit millisecond arithmetic in the gate
instead of 32-bit wrapping (+288 B) — the chip is 32-bit and every interval
the gate measures is seconds against a counter that wraps every 49 days, so
`wrapping_sub` is both exact and smaller. `tools/stack-check.sh` on
`board-seengreat-hub75` and `board-pixelblaze-v3`: no function over the
12,288 B budget, `render_task` frame unchanged at 5,488 B, `.stack` 27,180 B
and 24,828 B. The c6 hosted image — the binding constraint on everything
Phase A adds (#543) — is byte-identical either side.

2026-09-20, **#598's live per-pattern projection — the shape that cost 192 B
instead of 1,424.** The console's editor override had no wire at all, so it
needed one. Three shapes, measured as credless flake `luxel-fw-ota.bin`
builds of `origin/master` `cb7002f` against the branch, same machine:

| shape | c6-devkit + `hosted-ui` | margin |
|---|---:|---:|
| `POST /api/projection`, `async fn` + `MSG_QUEUE.send` | +1,424 | 2.88 % — FAILS |
| the same, sync + `try_send` (no future, no drop glue) | +2,000 | 2.83 % — FAILS |
| a `proj` line on `POST /api/layout` → `Msg::Projection` | +576 | 2.96 % — FAILS |
| **a `proj` line → the render task's existing projection flag** | **+192** | **3.00 %** |

The lesson is the same one #538 and #550 recorded, one level up: at this size
a NEW ROUTE is the expensive thing, not the logic behind it. A route arm that
awaits costs a whole future type, its drop glue and a state-machine variant
in the dispatcher — ~600 B before the handler does anything — and making it
synchronous was *worse*, because `Channel::try_send` is not on the path
`send` already linked. Merging the new `proj` verb into the existing
`proj1d|proj2d|proj3d` match arm also cost more than leaving it separate
(+368 vs +192): the shared arm has to branch on `verb` twice.

The winner adds no route and no message. `Msg::Projection(u8)` is **gone**;
the playlist's `P` and the new `proj` line both write one `AtomicU8`
(`layout::PROJ_PENDING`) that the render task already consulted every frame
as `PROJ_DIRTY`, so the whole feature is a parse arm, a widened flag and a
`match` where a `bool` used to be — and it removed a `MSG_QUEUE.send().await`
from the playlist task on the way in. `.stack` unchanged (`tools/stack-check.sh`:
pb-v3 24,852 B, pb-v3 + `small-chip` 26,468 B, both as on master).

The c6 hosted image is now **1,017,040 B, 31,536 B / 3.00 % of slot — 78 B
above the floor** (measured on the rebase over #603, which is byte-identical
on this RISC-V variant), where master had 206 B. Nothing else can land here
until Gitea #543 buys room back; measure before you write, not after.
