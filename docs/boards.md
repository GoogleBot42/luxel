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
| `board-seengreat-hub75` | 945,840 | 947,280 | **+1,440** | 101,296 B (9.66 %) |
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
  the slot back.
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
