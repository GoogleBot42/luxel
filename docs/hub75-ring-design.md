# HUB75 ring driver: a fixed-size internal buffer, packed just ahead of the beam

Design for Gitea #838, written 2026-09-27 for whoever implements it. The
decisions in here are Jeremy's (the shape, §2; the failure policy, §7); the
numbers are measured on the Seengreat that day or derived from the driver as
it stands. Read docs/boards.md "First light", "Faster refresh: the `lsb`
schedule" and "Spare-plane swap" first if the panel is new to you; read
`firmware/src/hub75.rs`'s module docs for today's driver.

## 1. Why

Today the panel's DMA reads a complete bitplane framebuffer out of internal
SRAM, and there are two of them (or one plus a spare MSB plane in the chase
build). That is 7 bytes per pixel per buffer, so internal RAM grows with the
wall: 59 KB for one 64x64 panel, 118 KB for the 2x1 that bricked the board on
2026-09-27 (Gitea #822), 237 KB for a 128x128 wall — on a ~216 KB heap that
also has to hold WiFi (~75 KB), the web pool and the engine.

The panel itself needs none of that. It has no memory beyond one row's shift
register; it consumes a fixed 40 MB/s at 20 MHz whatever its size, and at any
moment it needs only the next few row blocks. So the internal buffer can be a
**fixed-size ring of row buffers, sized in milliseconds of slack, filled
just-in-time by the packer straight from the engine's RGB888 frame in PSRAM**.
No packed frame is stored anywhere. This is the standard shape for the
problem — Espressif's RGB-LCD "bounce buffer" mode on the same chip,
SmartMatrix on the Teensy, pico_scanvideo/PicoDVI on the RP2040, FastLED's
ESP32 I2S driver — see #838's prior-art comment for the sources and quotes.

## 2. The shape (Jeremy, 2026-09-27)

```
core 1  render task ──RGB888 frame (PSRAM, double-buffered)──┐
                                                             │ pointer flip at the wrap
core 1  ring-refill ISR: pack ONE row pair (all 7 planes) ◄──┘
        from the current RGB frame into the slot the DMA just left
                 │  internal SRAM, colour bits only
                 ▼
        ring of N slots ──► GDMA ch0 ──► LCD_CAM ──► panel  (40 MB/s, circular chain)
```

Three stages instead of the five the driver has now. There is no compose
pass over a whole frame, no bitplane frame in PSRAM, no copy from PSRAM to
internal, no ring flip between two framebuffers.

- **Render** (core 1, unchanged): the engine writes RGB888 into a PSRAM
  frame (`luxel_core::arena::FrameVec`, 3 B/px). The frame is double-buffered
  in the arena so the packer always reads a complete one.
- **Refill** (core 1, interrupt): every k-th ring slot carries a descriptor
  EOF. The ISR packs the next row pair — both halves of the scan, all 7
  planes, `cols` words each — from the RGB frame into the slot the DMA has
  just finished. It writes colour bits only; each slot's control bits (row
  address, OE window, latch tail) are a static template written once at
  boot with `luxel_hub75::format_scheduled`.
- **Scan-out** (GDMA channel 0 → LCD_CAM, unchanged peripheral setup): a
  circular descriptor chain over the N slots, one descriptor per row block,
  with the BCM repeats expressed as repeated descriptors exactly as
  `esp-hub75`'s `fill_full_chain` does today.
- **Frame flip:** the refill ISR reads its RGB source pointer at the wrap
  (the existing frame-count EOF) and switches to the newest complete frame.
  Every row of a pass therefore comes from one frame: frame-atomic for free,
  which retires the two-ring flip of Gitea #376.

## 3. Emission order and the slot release rule

A HUB75 driver on LCD_CAM has no per-plane timer, so plane k is held on
screen by clocking the same block out 2^k times (or, with the `lsb`
schedule, by cutting OE early inside one block). E = row shifts per pass =
`t + 2^(7−t) − 1` for t truncated planes: 127 stock, 33 at ×4, 11 at ×12, 8
at ×16 (docs/boards.md, `luxel_hub75::Schedule`).

Today's order is **plane-major**: all rows of plane 6, then all rows of
plane 5… A packed row must then survive the whole pass, which is the whole
frame again. The ring needs **row-major** order: for row pair r, plane 6
× 64, plane 5 × 32, …, plane 0 × 1, then row pair r+1. A slot then holds one
row pair for exactly its E emissions and is free the moment its last
descriptor has been read. The eye integrates over the pass either way; both
orders are valid BCM, and the panel does not care (each shift+latch simply
replaces the row).

**Release rule:** slot s is writable once the DMA's current descriptor
(`Hub75::dma_position`, the #376 probe over `OUT_DSCR`) is past the last
descriptor of slot s. The ISR that fires on slot s's EOF refills slot s
with row pair `r + N` (mod 32) — never the slot the DMA is in, never the one
it enters next. This is the one piece none of the prior art has: their
buffers are consumed once and released; ours is re-read E times first.
Write it as a pure function over (descriptor index, N, E, rows) in
`luxel-hub75` and test it on the host before it goes near hardware — the
same way `tools/ringsim.py` proved the two-ring flip.

**Trailing display block** (`Geometry::trail`, #795): in row-major order it
is one extra dark block after the last row pair's last plane; the slot
template for row 31 carries it. Keep that invariant from `format_scheduled`.

## 4. Sizing: the slack knob

One slot is `7 × cols × 2` bytes: 0.9 / 1.8 / 3.6 / 7.2 KB at 64 / 128 /
256 / 512 columns. The DMA spends `E × cols` clocks per slot, so N slots buy

    slack = N × E × cols / 20 MHz

before the DMA starves. Size the ring in **milliseconds**, not rows; the
row count follows from E and the width, and wide walls get more slack per
slot for free. Rows needed for 3 ms on a 256-column wall: 2 at stock, 8 at
×4, 22 at ×12, 30 at ×16; for 1 ms: 1 / 3 / 8 / 10. The ceiling is the
whole frame (32 row pairs), at which point this is today's driver.
Descriptors are `N × E × 12 B` and stay near 3 KB for a fixed slack.

`ring_ms` is a `panel` line setting (default 3 ms while core 0 packs alone,
see §6) and
`driver.live` reports the rows it resolved to.

## 5. Cost model — the packer sets the ceiling

The whole frame is re-packed every pass because nothing stores the result.
Pixels per pass × passes per second = `64 × 20e6 / (33 × E)`, independent of
width, so every cost is a function of E only:

| step | E | PSRAM read (RGB) | SRAM write (pack) | SRAM read (panel) | pack CPU, today's packer | at 4× |
|---|---:|---:|---:|---:|---:|---:|
| stock | 127 | 0.9 MB/s | 2.2 MB/s | 40 MB/s | 19 % of a core | 5 % |
| ×2 | 64 | 1.8 | 4.3 | 40 | 37 % | 9 % |
| ×4 | 33 | 3.5 | 8.5 | 40 | 71 % | 18 % |
| ×7 | 18 | 6.5 | 15 | 40 | 131 % | 33 % |
| ×12 | 11 | 10.6 | 25 | 40 | 215 % | 54 % |
| ×16 | 8 | 14.5 | 35 | 40 | 295 % | 74 % |

Per pixel per pass the ratio is 3 : 7 : E bytes (RGB read : pack write :
panel read). Today's bulk packer (`luxel_hub75::pack`, Gitea #329) costs
~146 cycles/px (2.5 ms per 4096 px); "at 4×" is a target, not a
measurement. PSRAM percentages are against 37 MB/s effective at `Freq40m`
(#599); a 30 MHz pixel clock multiplies every row by 1.5.

**So the packer's speed is the refresh ceiling of this design**: ×4 on one
core as it stands, which is 72 Hz on a 128x128 wall at 20 MHz — flicker.
The lever is the S3's PIE 128-bit vector unit (what esp-dsp uses for its
4–10×): `ee.vunzip.8` to deinterleave RGB888 sixteen pixels at a time, a
vector multiply-and-shift for the brightness scale, and per plane a 32-bit
lane shift + 128-bit AND to isolate bit k of every byte, shifted into the
channel's word position and OR-ed into the output, then `ee.vzip` to widen
to the 16-bit bus words. ~14 cycles/px on paper; call 3× the floor. PIE has
no Rust intrinsics (inline `asm!`; the GNU assembler for xtensa-esp32s3
knows the mnemonics), and its q-registers are not saved by the scheduler,
so the packer must own the unit from one context with interrupts masked
around the kernel — fine for a packer that runs in task context on its own core (§6). **Measure this first**
(§10 step 1): it needs no panel, and every other number depends on it.

*Step 1 status (2026-09-27, Gitea #855):* the kernel exists
(`crates/luxel-hub75/src/pie.rs`, firmware feature `hub75-pie`; bench image
`packbench`, docs/tools.md) and the on-paper estimate above was wrong in
one place: PIE has no stride-3 gather. `ee.vunzip.8` deinterleaves by TWO,
and every PIE data-movement instruction (the zips, the unzips, the byte
shifts, the lane-constant multiply) maps a lane index affinely with a
power-of-two slope, so no sequence of them turns RGB888's 3-byte pixel
stride into the bus word's 2-byte stride for more than a couple of pixels
— a SIMD gather costs an op per pixel per channel regardless. So the
deinterleave is a scalar prologue (byte loads, the brightness LUT applied
on the way — any LUT stays exact, no vector multiply approximation) at
~13 cycles/px, and the vector unit does the per-plane extraction on 16-bit
lanes holding `bottom << 8 | top` of one channel: mask the plane bit,
`ee.vmul.u16` as the (logical) shifter per channel pair, one multiply to
fold the six bits into bits 9..14, xor/and/xor to merge into the formatted
word — 17 instructions per 8 columns per plane, ~30 cycles/px in total on
paper, ~5x today's packer rather than 10x. The lever if that is not
enough is the FRAME format: a planar or RGBX frame makes the prologue a
few `vld`s. Numbers from metal go in docs/boards.md "The PIE packer".

## 6. Cores: the refill is a work queue, not a core

Which core packs is a **lever, not a decision** (Jeremy, 2026-09-27): start
with core 0 doing all of it, and let core 1 pick slots up when core 0 is
held off (WiFi) and core 1 is free anyway because it is waiting on vsync.
So the refill is designed as a claimable queue from the start:

- **The queue** is two atomics over the ring: `free_upto` (advanced by the
  slot-EOF interrupt from the DMA position: every slot the DMA has left) and
  `next_fill` (the next slot a packer will take). A packer claims a slot by
  CAS on `next_fill` while `next_fill < free_upto`, packs it, and moves on;
  it never waits. Row pair `r + N` for slot `s` follows from the claim
  index, so two packers never pack the same row and order never matters.
- **Core 0, the output task**, is the default packer: `write_frame` becomes
  "publish this frame pointer", and the task's loop is "drain the queue,
  then yield". This is where the two-buffer driver's compose already runs.
- **Core 1, the render task**, steals: between frames — where it currently
  waits for vsync/handoff — it drains the queue too. That is Gitea #833
  applied to the pack, and it is what keeps the panel fed through core 0's
  2,029 µs WiFi holds (#620) without a 3 ms ring. Its share of the engine's
  time is whatever the queue leaves it, and it is only ever work core 0 was
  too late for.
- **The slot-EOF interrupt** does no packing itself: it advances
  `free_upto`, flips the frame pointer at the wrap, counts `late` when
  `next_fill` has fallen behind the DMA, and blanks the slot the DMA is
  about to enter if it was never filled (§7). Keep it under 5 µs; bind it
  on core 0 with the existing frame-count ISR.

What this costs the packer: it must be reentrant across cores — per-core
`Scratch`, per-core PIE state (each CPU has its own q-registers, so the
vector packer needs no cross-core save, only interrupts masked on its own
core around the kernel), and the brightness tables read-only once built.
The ring slack (§4) then only has to cover the longer of the two cores'
worst hold, and the `pass.ring` counters say which core packed what
(`packed_core0`, `packed_core1`, `late`), so Jeremy can see the lever
working before he pulls it.

Every PSRAM access rule in `psram.rs` and `core1.rs` still applies: a packer
reads the RGB frame only in task context on its own core, never while the
flash fence has that core parked (§7).

## 7. Flash writes and the failure policy (Gitea #852)

A flash write disables the external-memory cache; the RGB frame is
unreadable for the write's duration (tens of ms for a store, seconds for an
OTA). Espressif's bounce-buffer docs say their LCD "CANNOT function" then
and keep the cache live with `CONFIG_SPIRAM_XIP_FROM_PSRAM`; our esp-hal /
esp-storage stack has no equivalent (#852 has the alternative).

Policy, agreed with Jeremy 2026-09-27: **blank, never garbage.** The fence
hook (`core1.rs`, where the other core is parked) points the ring's
descriptors at one static all-dark template slot before the write and
restores them after; the panel goes dark for the write and comes back on
the next pass. A refill that misses its deadline for any other reason
(ISR latency, a packer that cannot keep up at the configured step) is
counted (`pass.ring.late`), the affected slot is blanked, and after a
threshold the driver drops the schedule one step (`lsb` up) and says so on
serial and in `driver.live` — SmartMatrix's back-off, not scanvideo's
coloured line. Nothing ever shows a stale row at the wrong address.

## 8. Memory

Internal: the ring (§4) + descriptors (~3 KB) + the packer's 2 KB tables +
one static dark slot. Nothing scales with the wall except through the slack
you choose. PSRAM: two RGB888 frames (24 / 48 / 96 / 192 KB at 4096 / 8192 /
16384 / 32768 px) — the arena already holds the engine frame and the
pipeline's travelling buffer (#777), so this is one more of the same.

128x128 (four panels, 256 columns, 16,384 px) at 20 MHz, 7 planes, blank 7:

| step | E | pass Hz | brightness | ring for 3 ms | for 1 ms | PSRAM read | pack CPU today / 4× |
|---|---:|---:|---:|---:|---:|---:|---:|
| stock | 127 | 19 | 100 % | 2 rows, 7 KB | 1 row, 4 KB | 2 % | 19 % / 5 % |
| ×4 | 33 | 72 | 96 % | 8 rows, 28 KB | 3 rows, 11 KB | 10 % | 71 % / 18 % |
| ×12 | 11 | 215 | 72 % | 22 rows, 77 KB | 8 rows, 28 KB | 29 % | 215 % / 54 % |
| ×16 | 8 | 296 | 46 % | 30 rows, 105 KB | 10 rows, 35 KB | 39 % | 295 % / 74 % |

Compare today: 237 KB two-buffer, 135 KB chase — neither fits. With the
vector packer, ×12 with core 1 stealing at a 1 ms ring is 28 KB internal and half a
core.

## 9. What changes where

- `crates/luxel-hub75`: row-major `format`/`pack` variants that produce ONE
  row pair's `7 × cols` words into a slot (the packer currently walks the
  whole frame per row pair already — expose that inner step); the slot
  release predicate and the ring/descriptor arithmetic as pure functions
  with host tests; `Schedule` unchanged.
- `firmware/patches/esp-hub75-0.14.0-*.patch`: a ring-mode chain builder
  (row-major, N slots, EOF every k-th slot) beside `fill_full_chain`, and
  the second ISR hook. Patch files, never vendored trees
  (firmware/patches/README.md).
- `firmware/src/hub75.rs`: a `Hub75Ring` output behind a feature
  (`hub75-ring`), keeping `Hub75Output` as is until Jeremy's eye retires it;
  boot: allocate the ring from `ring_ms`, format the templates, build the
  chain, install the slot-EOF ISR and the fill queue (§6); `write_frame` becomes "publish this frame
  pointer"; `flush` is gone. `boot_cost` learns the ring shape so `POST
  /api/layout` refuses honestly, and the self-heal (#822) stays as the
  measured guard.
- `firmware/src/core1.rs`: the fence hook that blanks the ring (§7).
- `luxel_core::layout::PanelDriver`: `ring_ms` on the `panel` line;
  `LiveDriver` reports rows and the resolved slack; docs/api.md.
- `/api/status`: `pass.ring` = `{rows, slack_us, late, blanked, backoffs}`.
- Tools: extend `tools/ringsim.py` to the row-major chain and the release
  rule; `tools/hw-bench.mjs` rows should read `pass.ring.late` alongside
  `torn_*`.

## 10. Order of work

1. **Vector packer microbench** (new ticket): a bare S3 test image (no panel
   needed — the Athom is a classic ESP32, so use an S3 devkit or the
   Seengreat when Jeremy hands it back) timing today's `pack` against a PIE
   row-pair packer over a 256-column row pair, reporting cycles/px. Decides
   the ceiling; also speeds up today's driver.
2. **Ring maths on the host**: row-major chain builder, descriptor counts,
   release predicate, `ringsim.py` coverage. No hardware.
3. **`hub75-ring` on the bench at 64x64**: ring sized by `ring_ms`, ISR on
   core 0 packing alone, frame flip at the wrap, `pass.ring` counters, `/api/pixels`
   byte-identical to the two-buffer build for the same frame. Then the core-1
   steal (§6) as its own PR, with `packed_core1` and `late` before/after.
4. **Failure policy** (#852): fence blanking, late-slot blanking, back-off.
   Prove it with a pattern save and an OTA on the bench.
5. **2x1 and Jeremy's eye**: the chain that is on the bench, at ×4 and at
   his `lsb 2`, against the chase build; then the 128x128 when the panels
   exist.

Each step is its own PR and its own ticket under #838.

## 11. Decisions already made, and the ones still open

Made (Jeremy, 2026-09-27): one fixed-size internal buffer, the packer is the
refill, no packed frame anywhere; blank and back off on failure, never
garbage; which core packs is a lever, with core 0 first and core 1 stealing
when he asks for it. Open: none that block step 1. Step 3 will need his `lsb` step of
choice for the bench comparison, and step 5 his call on whether the ring
driver becomes the default.
