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
