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
`luxel-fw-seengreat-hub75`, and the hosted-UI variant
`luxel-fw-c6-devkit-hosted`); see docs/firmware.md for the
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
| `board-seengreat-hub75` | ESP32-S3 | HUB75 (14 pins, `board::hub75_pins!`) | HUB75 64x64 panel, 4096 px | **4096** | **on metal** (first light 2026-09-05; master re-verified 2026-09-06 — see "First light" & "Second light" below) | Seengreat "RGB Matrix HUB75 S3" (ESP32-S3-WROOM-1-N16R8): a purpose-built panel driver board, so the feature turns `hub75` on itself. Pin map transcribed from the [vendor wiki](https://seengreat.com/wiki/214/) — R1 IO5, G1 IO4, B1 IO6, R2 IO15, G2 IO7, B2 IO17, A IO8, B IO18, C IO10, D IO9, E IO16, CLK IO12, LAT IO11, OE IO13; both panel outputs (ribbon + plug-in header) share those pins. Codec/mics (Gitea #142), microSD, RTC and PSRAM unused (see below); nix variant `luxel-fw-seengreat-hub75` |

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
real. See docs/size-report.md.

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
| `board-c3-devkit` | 889,136 B | 159,440 B | 875,200 B | 173,376 B | 13,936 B |
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

## IRAM budget: where the interpreter's per-pixel code lives

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

What it buys: **~14 KB of the 1 MiB OTA slot on every board** and ~4.7 KB of
DRAM back into the main stack (numbers in the ceiling section above), plus
the ~641 KB bundle never having to be written to a device at all. The
`assets` partition's 983,040 B stays allocated-to-nothing; reclaiming it
needs a partition-table fork and is Gitea #199, not this mode.

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

**PSRAM is not initialised.** Nothing in the current firmware wants it:
DMA framebuffers must live in internal SRAM regardless, and the engine's
hot per-frame buffers would be slower on PSRAM than in DRAM. Its one
plausible use is the same one already noted for WROVER modules — a
dedicated arena for large pattern arrays, letting the array budget grow
without touching the DRAM heap. That stays a future idea (docs/ideas.md),
not a v1 requirement.


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
shared by build-esp32.sh and tools/stack-check.sh; its `CORE_O3` flag
decides whether the VM crate gets opt-level 3 — see "The 1 MiB OTA-slot
ceiling" — and its `IRAM` flag which of the interpreter's hot functions
execute from internal SRAM — see "IRAM budget" — and flake.nix's
`firmwareVariants` entry must say the same for both):

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
`/api/status` samples 20 s apart.

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
9,648 B. Flash cost is +664 B (`.text` +512, `.rodata` +112, `.data` +40);
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
