# Update log

## 2026-08-22 — Output-driver abstraction: the render loop no longer
## knows it's talking SPI (Gitea #71, HUB75 prereq)

The output-driver trait docs/PLAN.md promised ("so parallel drivers slot
in later without touching the render loop") now exists: new
`firmware/src/output.rs` with an `OutputDriver` trait (`set_protocol` /
`resize` / `write_frame`) and `SpiStripOutput`, which absorbed
`EncodeBuf`, `realloc_buf`, and `spi_cfg` from main.rs verbatim — the
u32-alignment DMA invariant and the lazy per-frame realloc retry moved
with them, unchanged. `render_task` now takes the `BoardOutput` type
alias (static dispatch — embassy tasks can't be generic, and there's no
`dyn` in the frame path); the engine-freeing retry policy on tight-heap
protocol switches stays in the task, where the engines live. HUB75 (#72)
becomes "new impl + alias switch" instead of "rewrite the render loop."

Zero intended behavior change. Verified: builds for c3-devkit, s3-devkit,
c6-devkit, athom-music+small-chip; stack-check on default and
athom+small-chip (.stack 30,236 B, floor 24 KB); full QEMU suite 5/5
PASS; athom OTA image 946,880 B vs master's 948,400 B (−1.5 KB, slot
margin unaffected). One log-format nit: the protocol-switch alloc-failure
message now reports pixels, not bytes.

**Athom hardware soak (default profile, OTA'd to ota_0):** targeted
exercises of the refactored paths first — live protocol switch
ws2812→sk9822→ws2812 and pixel count 60→2048→60, including the worst
case (ws2812 @ 2048 px = the 18 KB encode-buffer realloc; heap deltas
matched the math, no reboot, no vmerr). Then `tools/hw-bench.mjs`:
**321/322 clean** — the one failure is the long-standing pattern-side
array OOB in "sound - spectromatrix render2D", same as every prior soak
(see the 2026-08-22 RX-pool entry). Lowest heap_free 63,992 B (vs
65,840 B in the v0.1.37 default-profile baseline; the gap is the bigger
ws2812-vs-sk9822 encode buffer at 300 px). fps-vs-pixel-count curve
unchanged (122 @ 60 px … 5 @ 2048 px). Slot held ota_0 throughout; no
rollback. Device restored to as-found config (60 px ws2812).

Context: HUB75 support planned with Jeremy 2026-08-22 (Seengreat HUB75
S3 + 64x64 panel ordered; series = Gitea #71–#75). Pre-existing, NOT
from this change: `cargo clippy` on master fails with 7
`large_stack_arrays` errors in netin/ota/provision (clippy 1.96 bump) —
filed separately.

## 2026-08-22 — Takeover imports WLED's LED wiring: a converted device
## comes up configured, not defaulted (the open half of Gitea #3)

The takeover already mounted WLED's littlefs and parsed cfg.json for the
SSID — and threw the rest away, so every conversion booted 60 px ws2812
at default brightness regardless of what the WLED install was actually
driving. Now the same read lifts the wiring and boot defaults, and the
takeover writes a full `LXDV` device-config record right next to the
`LXCF` creds record it already wrote:

| WLED cfg.json | Luxel field | mapping |
|---|---|---|
| `hw.led.ins[0].len` (fallback `total`) | pixel_count | clamped 1..=2048 |
| `hw.led.ins[0].type` | protocol | **22** (WS281x RGB) → ws2812, **51** (APA102) → sk9822; anything else (RGBW, WS2801, analog, matrix, virtual) keeps the board default + a serial note — deliberately conservative, a wrong protocol is worse than a default one |
| `hw.led.ins[0].order` | color_order | **relative to the chip-native order** (see below) |
| `def.bri` 0–255 | brightness 0–31 | rounded, floored at 1 (a >31 write voids the whole LXDV record — config.rs:261) |
| `hw.led.maxpwr` | cap_ma | 0 = limiter off stays off; clamped 20 A |
| `light.gc.col` | gamma_tenths | 2.8 → 28; 1.0/garbage → off |
| `hw.led.ins[0].pin` | — | logged only: Luxel pins are compile-time per board (board.rs:1-5) |

**The color-order mapping is the subtle part.** WLED's `order` is the
strip's *wire* order (COL_ORDER_*: 0=GRB…); Luxel's `ColorOrder` is a
*pre-encoder* remap with identity 0="rgb", and the encoders already emit
each chip's native wire order (ws2812 GRB, sk9822 BGR — leds.rs). So
WLED "GRB" on a WS2812 maps to Luxel *identity*, not Luxel "grb": the
import solves P in native∘P = wled_order per protocol. Pinned by a
construction test (`wire_order_roundtrip`) that simulates both pipelines
byte-for-byte across all 6 orders × both protocols rather than trusting
the hand-derived tables. Order is only imported when the protocol also
mapped — remapping against a guessed protocol would be a color bug.

Mechanics: scope-aware JSON scanners in wledfs.rs (balanced-bracket
ranges with string/escape tracking) because the interesting keys collide
all over cfg.json — `hw.btn.ins[].type`, `ir.type`, `relay.pin` — where
the WiFi lift's first-match-anywhere trick would grab the wrong value.
Same best-effort posture as the creds: every field individually
optional, any failure → board default, never a retry reboot. Import
happens before anything is modified; the write lands after the config
wipe, non-fatal like `write_wifi`.

Verification (no hardware; the QEMU harness carries it):
- 7 new host tests via `tools/wledfs-check` (`cargo test`), including
  decoy-key resistance and the wire-order construction proof; the rig
  binary now prints the wiring too, and against the real configured
  Athom dump extracts exactly the bench ground truth: 30 px, pin 18,
  type 22, order 0, bri 128, maxpwr 850, gamma 2.8.
- `tools/qemu/run-all.py` all green (5/5 suites) — takeover-test.py
  gained 13 assertions: two serial lines plus the full LXDV record at 0xA000
  byte-for-byte (30 px, protocol 1, order 0 — GRB-is-native proven on
  real config data — brightness 16/31 from bri 128, cap 850, gamma 28,
  checksum, erased tail).
- athom + c3 builds, image-check ok, stack-check clean.

Not done here: WLED's multi-bus configs import bus 0 only (Luxel drives
one output); no mic/button/IR import (no Luxel consumers yet — mic is
docs/mic-bringup.md); `rgbwm` ignored (no RGBW support at all).

## 2026-08-22 — Event injection hardware-soaked on the Athom (v0.1.39)

The deferred half of the v0.1.38/39 event work ("on-device soak when a
device is back"): 15 minutes against the rig at 192.168.0.183, serial-less
(/api/status + /api/vars polling — note /api/vars returns raw 16.16).
New indexed tool: `tools/event-soak.mjs`.

- **Delivery: 30,286/30,286** — steady 1–32-event batches at ~50 ev/s all
  arrived, and even the deliberate 30×32-batch overflow bursts were fully
  drained (at ~120 fps the queue empties between sequential HTTP posts;
  the drop path stays covered by unit tests).
- **Malformed frames: 44/44 rejected** (bad magic, truncation, count 33,
  count/length mismatch) — clean `ok:false`, no crash.
- **Heap stable** (97.7 KB idle → 92.5 KB min under load → 97.1 KB after
  cooldown, no drift), **no vmerr, no reboot** (evTotal monotonic), fps
  115–119 under combined injection+polling vs 123 idle.
- Fun correctness sighting: the counter pattern's `frames` export wrapped
  i32 right on schedule past 32768.0 — the documented two's-complement VM
  semantics, live on hardware. (It also false-positived the tool's first
  reboot detector; fixed to key on evTotal only.)
- Device left as found: ad-hoc live pushes persist nothing; rainbow
  restored.

Remaining for Jeremy (docs/UNTESTED.md unchanged in scope): the playground
click-through and the real-HA MQTT hop on the wall unit — still gated on
the dev unit coming back online.

## 2026-08-22 — TODO(oracle) sweep: every probe-able marker settled,
## two bug-for-bug fixes, perlin sweeps captured

All 24 `TODO(oracle)` markers are gone from the source. About half were
stale — settled by the July probe sessions but never cleaned up (div/mod
by zero, shift edge cases, sub-epsilon truncation, hsv rounding, refs-as-0,
transform order/sign) — and the rest were settled today against the live
oracle (fw 3.67) with a new self-judging battery,
`tools/oracle/todo-probes.mjs`.

**Two real divergences found and fixed in luxel-core:**

- **Transform stack cap is silent on PB.** 40 stacked translates: dx
  stalls after 31 ops, no error, pattern keeps running. `push_op` now
  ignores ops past 31 instead of erroring
  (test: `transform_stack_caps_at_31_silently`).
- **Palette lookup past the last stop is BLACK, not a clamp** — and the
  ends are asymmetric: below-first clamps to the first color. Hard edge
  exactly at the stop; single-stop palettes agree. `palette_lookup` now
  matches bug-for-bug (test: `palette_edges_match_pixelblaze`).

**Confirmed matches** (comments updated, no code change): method-form
`a.replace()` writes from index 0; `arrayReplaceAt` exists on PB and
matches; rotateX/Y/Z all CCW right-handed; no-palette paint = grayscale
ramp; palette state does not leak across live-code reloads; `null` = 0 at
runtime; clock civil conversion exact vs America/Denver. `undefined` is
REJECTED by PB's compiler — ours stays as a documented leniency.

**Perlin family:** arities verified identical via PB's own compiler
(perlin 4, fbm 6, ridge 7, turbulence 6, setPerlinWrap 3) and 3,320 raw
samples captured into `tools/oracle/sweeps/` (1D slices, seed sweep,
wrap-4 periodicity, per-arg fbm sweeps) for offline algorithm fitting —
filed as a ticket.

**Regression:** full `run.mjs` battery 138/175 — every diff is a
documented category (PB approximation error/seam bugs, deliberate prng
divergence, map-dependent transform specials pinned by unit tests
instead). Workspace tests green. Still open, all hardware-blocked: 1D
transform coords (oracle can never be mapless), sensor-board scaling,
no-time clock behavior. Findings doc has the full session record.
## 2026-08-22 — library: the last fake-trigger controls now listen for
## real events (`readEvent`)

The follow-up v0.1.38 explicitly left on the table ("swapping the
patterns' trigger controls over where it helps"). A trigger button is a
stand-in for a poke that comes from *outside*; now that `eventCount()` /
`readEvent()` exist, the patterns whose trigger only ever emulated one
consume the real thing, following the Typing Heatmap / Crosshair Pulse
idiom: **keep the manual control, add an event drain**. Backward
compatible — nothing that worked before stopped working.

Audited all 13 library files mentioning "trigger"; 4 exposed a
trigger-style *control* (2 already converted in v0.1.38) and 2 more used
a slider/toggle as a fake momentary button. Converted three:

- **Ripples 2D** — `triggerSplash` restarted drop 0 at a random point.
  Events now splash at the poked `(x, y)`; the trigger keeps its random
  splash and both go through one `splash()` that recycles the
  furthest-expanded ring instead of always clobbering slot 0. Rain keeps
  falling during real input (unlike the reference patterns' phantom
  generators, the drops *are* the pattern — no `quiet` window here).
- **Slime mold palette** — `triggerSeedAPixel` planted at a random free
  cell. Events plant at the poked cell (repainting an already-painted one,
  so a deliberate poke always shows), which is what lets an outside source
  steer where the blobs start.
- **SaberDeploy Tutorial** — its header already admitted the UI toggle
  "stands in for a momentary pushbutton". Events are now the real button:
  any frame carrying at least one event is one press (a burst inside one
  frame deliberately doesn't flip twice and cancel out); the toggle stays
  as the by-hand path.

Left alone, deliberately: *Golden Tix*'s `sliderSlideRightToReset` (a
slider-as-button hack, but resetting a live-coding sandbox's clock is an
editor action, not an external stimulus) and the ~9 patterns whose
"trigger" is an internal edge-trigger latch on audio/physics, not a
control.

Verification: three new tests in `crates/luxel-core/tests/engine.rs` push
an event into each converted pattern and assert the poked cell / blade
direction changes against a same-seed unpoked control — negative-controlled
(all three fail against the pre-change sources). Full `luxel check` sweep
over `library/` clean at 322/322 on both the default and 16×16 grids,
400-frame soaks clean, `cargo test --workspace` green, gallery regenerates
at 322.

## 2026-08-22 — WiFi-blob buffer tuning: the `small-chip` profile's
## missing half (+9.9 KB heap, soaked on the Athom)

The 2026-07-29 agreed follow-up from docs/ideas.md is done. esp-radio's
RX buffer pools are throughput-tuned by default; the `small-chip` cargo
feature now trims them, completing the feature that until today was only
its web-pool half.

**What changed** — three `cfg(feature = "small-chip")` lines on the
`ControllerConfig` main.rs already builds:

| knob | default | small-chip |
|---|---|---|
| `static_rx_buf_num` | 10 | 4 |
| `dynamic_rx_buf_num` | 32 | 16 |
| `ampdu_rx_enable` | true | false |

TX counts stay at the defaults deliberately: dynamic TX buffers are
allocated on demand, so lowering the cap reclaims nothing at idle and
only buys TX starvation under load. `rx_ba_win` stays at 6, which still
satisfies `ControllerConfig::validate()` against the trimmed pools
(6 < 16 dynamic, 6 < 2 × 4 static), so the pairing stays legal if AMPDU
RX is ever switched back on. The default build is untouched.

Plumbing: `firmware/build-esp32.sh` and `tools/stack-check.sh` both take
`EXTRA_FEATURES=` now — there was previously no way to build or
stack-check a non-board feature at all, which is a large part of why the
small-chip half sat unfinished.

**Measured (Athom rig, v0.1.39, idle `heap_free` from /api/status):**

| build | heap_free | Δ |
|---|---:|---:|
| default (as shipped) | 98,352 | — |
| small-chip, WiFi untuned | 115,548 | +17.2 KB |
| small-chip, WiFi tuned | **125,460** | **+26.5 KB** |

So the WiFi tuning's own share is **+9,912 B (9.7 KB)**, isolated by an
A/B of the two small-chip builds rather than inferred.

**The estimate was wrong in an instructive way.** The 2026-07-29 note
predicted 15–25 KB; the truth is 9.7 KB, and essentially all of it is
`static_rx_buf_num` (6 fewer buffers × ~1.6 KB = 9.6 KB, allocated inside
`esp_wifi_init` and never freed). The dynamic RX pool and the AMPDU
block-ack buffers are *on-demand* allocations — capping them bounds the
worst case but reclaims ~nothing at idle. Anyone tempted to chase the
remaining ~40 KB of blob draw should know it isn't sitting in the
configurable pools.

**Soak** (all on the tuned small-chip build, Athom, 300 px WS2812):

- `tools/hw-bench.mjs`: **321/322 clean**, identical to the default
  build's long-standing result (the one failure is the same pattern-side
  array OOB in "sound - spectromatrix render2D"). Lowest `heap_free`
  across the whole churn: 99,036 B — still above the *default* build's
  idle figure.
- **RX-pool stress**, the thing that would actually bite: 44,172 DDP
  frames at 245 pkt/s × 300 px (~220 KB/s inbound UDP) concurrent with a
  6-worker HTTP API hammer for 180 s, then a 629 KB streaming asset
  upload. No panic, no reboot, no rollback (slot held `ota_0`
  throughout).
- `web/tools/coldload.mjs`: 9/10 and 9/10 across two runs.

**The two refusals are the pool-2 tradeoff, not the buffers.** Both are
`ERR_CONNECTION_REFUSED` on the navigation itself at ~150 ms, before any
body — picoserve having no free slot, which is exactly the
"occasionally-refused first nav" cost the 2026-08-15 pool decision
accepted (Chromium wants ~3 sockets at a cold nav). An undersized
esp-radio RX pool presents as a StoreProhibited crash or dropped frames,
never as a clean TCP refusal. Same story for the hammer's 5,841
refused/reset vs 1,605 served: **zero** body-level failures, so
sustained throughput is fine and it's parallel fan-out that's capped.
docs/boards.md now states both costs with numbers instead of adjectives.

`tools/stack-check.sh` on both builds: `.stack` 29,516 B (default) and
30,340 B (small-chip), both clear of the 24 KB floor, no frame over
12 KB.

**Caveat on this session's evidence:** `/dev/ttyUSB0` was not present in
the container, so there was no serial capture — panic detection was a
1 Hz `/api/status` poller plus post-hoc `slot` checks (a boot-loop
rollback flips slots and would have been visible; none happened). Device
was left on the default build, `ota_1`, 60 px, playlist empty and
stopped, as found.
## 2026-08-22 — The editor warns before a pattern is too big for the device (Gitea #15)

Jeremy's framing on #15: *"Different ESP32s have different amounts of memory.
A pattern can work on one device with X pixels but not another. Fortunately,
we run the exact same pixel VM engine in WASM. We can have the device report
back how much memory it supports. While we are executing the script, we see if
we go over that threshold."* Plus a near-the-threshold warning.

The gap was real and worse than it looked: the device's capacity rejection is
**asynchronous**. `POST /api/code` answers 200, and only then does the render
task fail the post-load floor check and record a `pattern too large for this
device` vmerr. The editor showed nothing at all — the strip simply kept
running the previous pattern and the user was left to wonder.

**The estimator is a measurement, not a heuristic.** `lx_device_model`
(luxel-wasm, new) replays the firmware's own load sequence under a counting
allocator — the same instrument `crates/luxel-cli/tests/heapstat.rs` used to
establish the model in the first place, moved into the wasm build: LXP
envelope resident across `deserialize_lean`, dropped, then
`from_program_budgeted` at the device's array budget, then three frames. Peak
live bytes is what the floor check sees. Two things make this *better* than
the host test it descends from — wasm32 is 32-bit like the ESP32 (structures
measure at hardware width, where the 64-bit host test inflates every pointer),
and it counts the **whole envelope**, source included, which for a
source-heavy pattern is the actual peak rather than the engine.

**One definition of the budget.** `RUNTIME_FLOOR` (20 KB) and the array-budget
arithmetic moved out of `firmware/src/main.rs` into a new
`luxel_core::budget` — firmware and wasm now import the same constants, so
the prediction cannot drift from the device that enforces it. `load_headroom()`
encodes the subtle part: `heap_free` is measured with the *current* pattern
still resident, and since the firmware builds the new engine before releasing
the old one, that number really is the incoming pattern's headroom.

**Calibration against known ground truth.** Modelled over all 322 gallery
patterns at 300 px: 322/322 clean at 90 KB free, matching the
"full-library capacity, 322/322 modeled" figure in docs/ideas.md. Drop the
device to 70 KB free and exactly one pattern goes over — *"Music Sequencer -
for V3 ONLY"*, at 54.9 KB modelled against 51.2 KB of headroom. That is the
single pattern the 2026-07-19 full-library hardware soak actually saw
rejected. The model reproduces the hardware's one real verdict.

**UI.** A banner in the editor's right-hand stack, `.banner` idiom,
`data-role="capacity-warning"` with `data-level="tight"|"over"` and the byte
breakdown in the `title`. Severity follows **certainty, not size**: the local
model is advice (amber) and the device's own vmerr is a fact (red,
`data-role="capacity-rejected"`). Non-blocking throughout — the push still
goes, because the device is the authority and the editor only says what it
expects. The last 15 % of headroom counts as "tight": the model is exact but
the device's heap moves underneath it between the status read and the load.

**Silence where we don't know.** The playground has no device and therefore no
budget to judge against, and must not sprout a device affordance to say so. A
device that reports `heap_free` 0 (native mirror, older firmware) is silent
too — an unknown budget is not a small one, and guessing would cry wolf on
every pattern.

**Verification** (no hardware touched — the rig was another session's):
- `luxel serve --heap-free BYTES` (new) lets the mirror impersonate a device
  with that much free heap; default 0 keeps it honest about being a host.
- device-e2e spawns a second mirror claiming 30 KB free — 10 KB of load
  headroom with the arena clamped at its 16 KB minimum, which puts all four
  verdicts within reach of a one-line pattern. 9 new checks: silent on
  heap_free 0, clean/tight/over bands, the array-arena path, the numbers in
  the text, the push not being blocked, and the warning clearing. **100/100
  device-mode checks pass.** Playground e2e gained a check that an
  array-heavy pattern raises nothing there; all pass. Screenshots taken in
  real chromium.
- Firmware rebuilt (`image-check: ok`), `tools/stack-check.sh` clean,
  `cargo test --workspace` clean. The one clippy error in luxel-core
  (`LN_2`) is pre-existing on master.

Deliberately left out: no firmware API change (`heap_free` was already on the
wire and suffices — a device reporting its own derived budget would be
strictly more robust but needs hardware to verify); no pixel-count sweep
("this fits at 300 px but not at 2048"); and the warning is not shown on the
Device Patterns / Playlist lists, only in the editor.

## 2026-08-22 — ESP32-S3 and ESP32-C6 board features (builds only,
## untested on metal)

`board-s3-devkit` and `board-c6-devkit` join the four existing boards,
closing part (1) of docs/ideas.md "Small-chip profile + more board
features" (half of Gitea #3). Both ship as **builds, untested on metal**
— there is no S3 or C6 on the bench, so all that is established is: they
compile, link every load-bearing feature (`tools/image-check.sh`), fit
the 1 MiB OTA slot, and keep a `.stack` well above the 24 KB floor. Pin
choices, heap sizing and radio behaviour are unverified, and every place
they appear says so (Cargo.toml, board.rs, main.rs, flake.nix,
release.yml, docs/boards.md, docs/firmware.md, docs/releases.md).

Pins are each chip's SPI2/FSPI IO_MUX set, so DMA gets the direct route:
S3 CLK GPIO12 / DATA GPIO11 (clear of the octal-PSRAM pins GPIO33–37),
C6 CLK GPIO6 / DATA GPIO7 (clear of the devkit's RGB LED on GPIO8 — and
the same numbers as the C3 by coincidence of the IO_MUX tables).

**"Five-minute diff" held for the firmware, not for the build plumbing.**
The firmware side was exactly what docs/boards.md promised — a feature, a
`def` block, one `with_sck/with_mosi` line, plus widening the SPI-DMA cfg
from `esp32c3` to `not(esp32)` (C3/S3/C6 are all GDMA). No logic changed.
What actually needed doing was the surrounding machinery, which had the
classic-ESP32 chip/target/toolchain hardcoded in three places:

- `firmware/board-target.sh` (new) is now the single board → chip / rust
  target / toolchain map. `firmware/build-esp32.sh` and
  `tools/stack-check.sh` source it, so they can't drift. Both scripts
  drive *any* board now, Xtensa (`-Zbuild-std` + the Espressif fork) or
  RISC-V (mainline rustc, no build-std) — `BOARD=board-c3-devkit
  ./build-esp32.sh` works too, which it never did before.
- The flake needed `riscv32imac-unknown-none-elf` (the C6's target is one
  ISA letter off the C3's `riscv32imc`) in both the devshell toolchain
  and `riscvRust`, plus the two `firmwareVariants` entries.
- `.github/workflows/release.yml` builds and size-gates all six now.

Per-board app image at v0.1.39 (devshell builds, creds baked in) and
1 MiB-slot margin:

| board | app image | margin |
|---|---:|---:|
| `board-s3-devkit` | 885,840 B | 162,736 B |
| `board-c6-devkit` | 987,600 B | **60,976 B** |
| `board-c3-devkit` | 894,496 B | 154,080 B |
| `board-pixelblaze-v3` | 944,832 B | 103,744 B |
| `board-athom-music` | 944,720 B | 103,856 B |
| `board-esp32-generic` | 944,688 B | 103,888 B |

The C6 is the finding worth remembering: ~93 KB fatter than the C3 for
identical source, and at 5.8% it owns the tightest OTA margin in the
fleet — it is the board that will cross the ceiling first, so it's the
one to size-check on any release that grows the image. (docs/boards.md
now says this next to the table.)

`tools/stack-check.sh` on all four devkit/PB boards: no frame over
12 KB, `.stack` = 29,412 (pb-v3) / 39,632 (c3) / 51,172 (s3) / 141,320
(c6) bytes. The S3/C6 numbers come from reusing the C3's 160 KB heap on
chips with more DRAM; when hardware exists, that slack should become heap
(pattern capacity), not stack.

Verified: all six boards build in the devshell, `nix build
.#luxel-fw-s3-devkit` / `.#luxel-fw-c6-devkit` produce hermetic
images that pass `image-check.sh`, and the four pre-existing boards still
build byte-for-byte the way they did.

Deliberately NOT done: the installer page (web/flash.html) still lists
only the four known-good boards. It's a WLED-takeover flow for real
products, adding a chip there means extending the `Chip` union, the
arch-unsupported copy and the e2e fixtures plus a real-chromium pass, and
unknown board ids in a release manifest are skipped by design — so the
new artifacts publish harmlessly without it. Tracked as Gitea #57;
hardware bring-up for the two boards is Gitea #56.

## 2026-08-22 — Builtins batch 5: canvasAdd, seedable random(),
## timeScale / setFrameRate

The three small `docs/ideas.md` items that were left in the engine/runtime
section, appended to the builtin table (ids stable, no LXBC version bump,
every stored blob still valid) and each pinned by test.

- **`canvasAdd(buf, w, x, y, v)`** — the accumulate half of the canvas
  helpers: `cell += v` at exactly the cell `canvasSet` writes (same
  edge-clamped `floor(x·w)` addressing, same "of a non-array" runtime
  error, same degenerate-canvas no-op). Particle deposits and heatmap
  splats stop hand-rolling the read-modify-write. Returns the cell's
  **new** value, the way `+=` evaluates in JS.

- **Determinism, pinned.** Both generators are now documented as contract
  and asserted by test (`random_seed_pins_the_documented_sequence`,
  `prng_pins_the_documented_sequence`, both recomputing the sequence
  independently rather than snapshotting a run): `random()` is splitmix64
  (low 32 bits), `prng()` is xorshift32 13/17/5, state ← the seed's raw
  16.16 word, scaled `(r · max) >> 32`. The new **`randomSeed(s)`** seeds
  `random()`'s stream the way `prngSeed` seeds `prng()`'s — so a synced
  installation gets an agreed-on sequence without porting patterns from
  `random()` onto `prng()`. It returns the previous *seed* (the 64-bit
  state can't round-trip through an `Fx`, where `prngSeed`'s 32-bit state
  can, and that save/restore property is now tested too). **No existing
  sequence changed** — the algorithms are what Luxel always ran; they were
  just undocumented and unpinned, with a stale `TODO(oracle)` on `prng`.

- **`timeScale(s)` / `setFrameRate(fps)`** — in-pattern timing. `timeScale`
  scales the frame delta before it advances the clock, so `time()`,
  `beat()`, sync's `time_ms` and `beforeRender`'s delta all slow (or
  freeze, or speed up) together; negatives clamp to 0. `setFrameRate`
  holds the previous frame — same pixels, no pattern code run — until
  `1000/fps` ms of **real** time have accumulated, then hands
  `beforeRender` the whole interval, so delta-driven motion lands where it
  would have uncapped. Period clamped to 60 s.

  Both are enforced in `Engine::frame`, not per host, so firmware, WASM
  playground and CLI behave identically — no host plumbing, no
  QEMU/firmware exposure, and nothing to fake. Two honest consequences,
  now in docs/lang.md and docs/spec/vm.md: the host's output stage is
  untouched (LEDs/preview still refresh at the host's cadence re-sending
  the held frame, so the reported `fps` does **not** follow the cap — the
  cap throttles pattern evaluation, which is the expensive part), and
  the clock keeps running while frames are held so sync stays continuous
  (a sync *jump* is no longer misreported to the pattern as elapsed
  delta — `set_time_ms` now moves the render mark with it).

Verification: `cargo test --workspace` green; the 322-pattern `luxel check`
sweep over `library/` clean (compile + LXBC round-trip + smoke); web
`npm run build` (svelte-check 0/0) plus `e2e.mjs`, `device-e2e.mjs` and
`sync-e2e.mjs` all green — sync especially, since the delta path moved.
Autocomplete + hover for all four names driven in real chromium
(screenshot), including a pattern using every one of them running in the
playground preview. Clippy warning set identical to master. No firmware
source touched.

## 2026-08-16 — Pre-guard heap-regions panic root-caused + fixed, and a
## one-command QEMU test harness

Closed the last open thread on the WLED-takeover arc: the intermittent
`esp-alloc: Exceeded the maximum of 3 heap memory regions` panic that hit
the first Athom takeover (2026-07-26), fired before `ota::init`, and
self-healed on reboot — the one the installer page's beta banner was
waiting on.

**Root cause** (established under the QEMU harness, disassembly-confirmed):
the athom firmware makes exactly **two** `esp_alloc::heap_allocator!` calls
→ two `add_region()`s into esp-alloc's **three**-slot region array (the
`#[ram(reclaimed)]` 96 KiB + the 80 KiB region; the other arms are
`cfg`-gated out, and nothing in esp-hal/esp-rtos/esp-radio adds a region).
A clean boot fills 2 of 3 slots, so "exceeded 3" can only happen if the
slot array already holds stale `Some` entries when the allocators run —
which is what a flash-read flake corrupting the `HEAP` static's `.data`
`[None; 3]` initializer during the **ancient WLED bootloader's `.data`
copy** produces on the takeover boot. Same flake family as issue #35's
self-copy verify flake: intermittent, pre-`ota::init`, tied to the
via-WLED boot.

**The real danger** wasn't the intermittent panic (it self-heals via
`custom_halt`'s reboot) but a *deterministic* one: the boot-loop guard ran
**after** `ota::init`, past the allocators, so a pre-guard panic rebooted
forever without ever counting toward rollback — a bricked device that
never falls back to WLED.

**Fix**: `ota::preboot_guard`, armed **before** the heap allocators
(heap-free by necessity — stack buffers only, borrowing the `FlashStorage`
that is handed to `ota::init` once the heap is up). It increments the same
`LXBG` failed-boot counter the old guard used and, on the third
consecutive boot that never reached `boot_ok`, rolls back to the other OTA
slot (→ WLED on a takeover device). It fully replaces the old post-init
`boot_guard()`; `boot_ok` still clears the counter, and the takeover-retry
logic still zeroes it on a deliberate retry. Same increment/rollback
semantics, just early enough to catch a pre-heap panic. `stack-check`
clean (main-task stack unchanged at 29,492 B — no new heap statics; the
3 KiB partition-table buffer runs at the top of `main` with no WiFi NMI in
play). Builds green on athom + c3 + pixelblaze-v3.

**Harness** (the second half of the ask): `tools/qemu/run-all.py` is now
the single entry point for every emulator-backed test — it builds the
firmware + QEMU (separate out-links so they don't clobber each other),
autodetects the gitignored Athom dumps, and runs takeover (app1/app0/
fault) + the new `heap-regions-test.py` (selfheal/rollback), printing a
pass/fail summary (~36 s warm, 5/5 green). New QEMU tests slot into its
`suite` list. Also added: `tools/qemu/gdbrsp.py`, a dependency-free
GDB-remote client for driving QEMU's gdbstub from the Python harnesses
(the flake stays free of a `gdb`/pygdbmi dependency). All indexed in
docs/tools.md; root-cause writeup in docs/research/qemu-emulation-spike.md.

## 2026-08-16 — Takeover reboot-to-retry + attributable copy diagnostics
## (Gitea #35)

The bench conversion's one rough edge is now self-healing: a takeover
that aborts on anything possibly-flaky (self-copy verify, config wipe,
descriptor read/size) reboots and re-attempts, at most 3 boots total,
before settling into the provisioning AP — with the counter in byte 6 of
the boot-guard record (`LXBG` @0xC000) and each retry reboot zeroing the
failed-boot counter so a deliberate restart can't trip `boot_guard`'s
rollback-to-WLED (which would otherwise fire after two aborts, before
the retry cap). Exhaustion clears the counter, so a manual power cycle
starts a fresh budget; a successful takeover's config wipe erases it
anyway. Deterministic aborts (flash too small, image exceeds slot,
overlap) still settle immediately.

The copy loop also now says WHICH op failed — erase / write / read-back
/ data mismatch, with a mismatch classified further (differing-byte
count, first diff, and whether the sector read back still-erased 0xFF vs
stale data). The 2026-08-16 bench flake printed only "verify failed" and
left the failing stage forever unknowable; the next occurrence will be
attributable. The table write (the brick window) additionally gained
in-place retries ×3 with full read-back verify — a reboot can't rescue
that step, so it retries without one.

Tested hardware-free: the QEMU harness (harness-side per the isolation
rule) gained an opt-in write-fault injector in the m25p80 flash model
(`LUXEL_FLAKY_WRITE=<addr>:<len>:<budget>`, inert unless set), and
`takeover-test.py --inject-fault` drops both of boot 1's program
attempts at sector 0x10000 — byte-for-byte the bench failure signature
(erase lands, program doesn't, sector reads back 0xFF). 32 assertions:
boot 1a fails exactly like the bench did, reboots itself with `1/3
attempts used`, boot 1b converts cleanly, and the final flash state is
identical to the happy path (retry counter included). Both existing
variants re-verified green on the new firmware + patched QEMU.

The underlying silicon-side flake itself remains unreproduced/un-root-
caused — if it recurs, the new per-stage log lines are the evidence to
file the follow-up with.

## 2026-08-16 — The WLED takeover now has a hardware-free
## compose→boot→assert test (Gitea #43)

`tools/qemu/takeover-test.py` builds a 4 MiB flash in the exact state a
real Athom is in the instant WLED's OTA updater accepts a Luxel upload —
stock dump, configured WLED littlefs at 0x310000, `luxel-fw-ota.bin` in an
app slot, an otadata entry selecting it — boots it under the patched
emulator from this morning's spike, and asserts on both the takeover's
serial narration and the flash bytes it leaves behind: Luxel's partition
table at 0x8000, the `LXCF` record carrying the inherited SSID, the copied
image byte-for-byte at ota_0, WLED's littlefs untouched, and the
otadata/boot-guard state. One QEMU invocation covers both boots
(`software_reset()` reboots in-process); it exits the moment boot 2 prints
`wifi: creds from flash ("…")`, which is the inheritance proof and also
the last safe moment — the PHY panic's reboot would otherwise let a third
boot's `boot_guard` rewrite otadata.

Two variants, both passing against a **stock** image (isolation rule
holds: nothing here touches firmware source):

- `--slot app1` (default) — the realistic post-upload state, exercising
  the full 920 KiB self-copy where issue #35's verify flake lives. 23
  assertions, **12 s**.
- `--slot app0` — image already at the destination; the skip-the-copy
  path. 20 assertions, **1.5 s**.

Three things the first run taught us, now encoded as comments rather than
worked around: the `software_reset()` boot doesn't survive under QEMU (ROM
banner truncates, TG0 watchdog fires, and *that* reset loads Luxel —
cosmetic, pre-bootloader); the ESP-IDF bootloader **writes otadata back**
on the erased-otadata fallback (`seq=1`, `ESP_OTA_IMG_VALID`), so
"erased" was the wrong expectation there; and only the first 0x80000 of
WLED's app1 survives boot 2, because under the new table 0x210000 is
Luxel's `storage` partition and the pattern store legitimately reclaims
it. Also newly confirmed: `esp-storage`'s `FlashStorage::capacity()`
reports the true 4 MiB under emulation, so the takeover's flash-size
preflight is exercised, not skipped.

Espressif's QEMU fork is now a proper flake output
(`nix build .#qemu-espressif`), which was the standing TODO in
`tools/qemu/qemu-espressif.nix`; the `--impure` expression still works and
the test falls back to it.

## 2026-08-16 — QEMU emulation unblocked: stock firmware boots under
## emulation, takeover-in-CI is go

Resumed the morning's spike and killed the blocker — plus the two behind
it. Three root causes, all fixed QEMU-side in the nix derivation
(`tools/qemu/`), none of them touching the firmware:

1. **CPENABLE.** ESP32 silicon resets it to 0xff; QEMU leaves it 0, and
   nothing in ROM/bootloader/app ever writes it (disassembly-verified) —
   esp-hal relies on the silicon value. So the first float trapped
   `Cp0Disabled`, xtensa-lx-rt's `float-save-restore` `save_context`
   re-faulted on its own `rur.fcr`, and the guest looped silently in the
   double-exception handler. (espressif/qemu#154; PR #155 is unmerged and
   s3-only — no merged fix exists anywhere.)
2. **DPORT `PRO/APP_INTR_STATUS_REG_0..2` were never implemented.** That
   per-source pending bitmap is what esp-hal's level-interrupt dispatcher
   reads; it returned 0, so no handler ran, nothing acked, and esp-rtos
   livelocked in an interrupt storm on its scheduler-start interrupt.
   esp-hal is the *only* guest OS dispatching from those registers
   (IDF/Zephyr/NuttX use the Xtensa INTERRUPT sreg) — which is why this
   sat unnoticed. No prior art; we're first.
3. **TIMG level interrupts gated on `TIMG_INT_ENA`**, which is inert on
   ESP32/S2 silicon — the real gate is `Tx_LEVEL_INT_EN` in the timer
   config reg, what esp-hal writes — so the scheduler tick never fired.
   Folded in espressif/qemu#69 (an alarm already behind the counter now
   fires immediately instead of disarming).

Plus `tools/qemu/make-efuse.py`: a rev-3.0 eFuse image so **stock release
images** clear esp-hal's min-chip-revision gate with no build override.

Result: a byte-identical-to-shipping firmware image boots through engine
init, pattern storage and settings to the WiFi task, where the esp-radio
PHY blob faults on an unmapped peripheral alias (radio modeling is the
next frontier, and it's large). That's well past what the takeover test
needs — the takeover path runs before WiFi init — so
**compose→boot→assert takeover testing in CI is now unblocked**. Full
writeup, run instructions, QEMU-vs-silicon divergences and
upstream-filing candidates: docs/research/qemu-emulation-spike.md.

Standing rule recorded in the doc: the harness stays strictly isolated —
fixes go in `tools/qemu/`, never into the firmware.

## 2026-08-16 — QEMU emulation spike: takeover-in-CI is ~80% viable, one
## precisely-characterized blocker left

Jeremy asked whether the takeover could be tested in an emulator. Spike
findings (full writeup: docs/research/qemu-emulation-spike.md; nix
derivation for Espressif's QEMU fork: tools/qemu/qemu-espressif.nix):
the emulator builds reproducibly under nix (meson wrap subprojects
vendored, four one-line robustness patches to their RSA/AES device
models — all worth upstreaming), boots our real bootloader + partition
table + app from a plain flash FILE (perfect for compose→boot→assert
takeover tests), and esp-hal's chip-revision gate has a build knob.
Remaining wall: a guest double fault in the xtensa-lx-rt FPU
save_context path on the main task's first float (Cp0Disabled →
save_context faults at 0x400C200C) — hardware runs the same binary
fine, so it's a QEMU-vs-silicon divergence in coprocessor handling.
Next steps + worth-it assessment in the research doc; docs/tools.md
row added.

## 2026-08-16 — image-check: linked-feature guard (the //SIZETEST answer)

Jeremy asked what prevents the takeover class of regression. Answer:
`tools/image-check.sh` — asserts that load-bearing features are actually
LINKED into every built image by grepping for their distinctive serial
strings (dead-code elimination strips a feature's rodata along with its
code, so a commented-out call makes the strings vanish). Markers:
takeover, AP-mode provisioning, boot-loop guard. Wired into
build-esp32.sh (every local build) and release.yml next to the OTA size
guard (every board, every release). Test-of-the-test done: a clean
athom build passes; rebuilding with the call re-commented fails with
"MISSING marker 'takeover: foreign partition table'". actionlint +
shellcheck clean.

## 2026-08-16 (midnight bench session) — the WLED→Luxel takeover was
## SHIPPED DISABLED since v0.1.31; fixed, and the full conversion proven
## end to end through the installer page on real hardware

The first real via-WLED install (installer page → Athom restored to
stock WLED 0.13.2) exposed it: `takeover::maybe_takeover()` in main.rs
has been commented out as `//SIZETEST` **since commit 2e5d6ff — the
v0.1.31 commit literally titled "takeover always-on"**. A size
measurement toggle that never got reverted. Every release (v0.1.36,
v0.1.37) advertises WLED takeover and ships it dead; nothing noticed
because no via-WLED install had been run since the July 26 TAKEOVER=1
feature builds. Uncommented (with a do-not-disable comment), athom
image 938,240 B (107 KB OTA margin), stack-check clean.

**Round 1 (v0.1.37 release image, takeover dead) documented what
"dead" looks like**: one silent first boot (zero serial bytes, RTC-WDT
reset), then Luxel running absurdly from WLED's app1 under WLED's
partition table, credless → provisioning AP. Also proved: WLED
**Improv-serial provisioning persists both cfg.json and wsec.json**
(WLED rejoined after a cold power cycle) — the bench can provision
stock WLED with zero button-holds (`scratchpad improv script; packet
format in the session log`; worth a tools/ script if we do this again).

**Round 2 (fixed image, via the page's bundled mode)**: takeover ran —
foreign table detected, **WiFi inherited from WLED's littlefs**
("MOMCorp Intranet"), 920 KiB self-copy... first attempt hit an
**intermittent verify failure on the first sector** (both in-boot
retries), aborted exactly as designed (WLED table untouched) into the
provisioning AP; the next power cycle's re-attempt ran clean end to
end: otadata/nvs wiped, table rewritten, reboot to ota_0, **joined the
LAN on the inherited creds at the same DHCP address**, page detected
Luxel and pushed the web app — "All done — open your Luxel 🎉". Device
end state: Athom = Luxel v0.1.38 (local build = master + this fix),
ota_0, Luxel partition table, fresh store, full web app, 122 fps.

Open (documented in docs/wled-migration.md's beta list): the
intermittent first-sector verify flake; the takeover has no
reboot-to-retry after an abort (device waits in AP mode for a power
cycle); the two first-boot anomaly classes. The installer page's
timeout guidance ("power cycle helps") turned out to be literally the
right advice.

Harness note: both bench runs crashed the puppeteer driver mid-wait
(CDP WaitTask against a live device; the fake-wled e2e has never done
this) — unexplained, low priority, the device outcome was unaffected.

## 2026-08-16 — v0.1.39: MQTT topic → pattern events (`luxel/<id>/event`)

The follow-up v0.1.38 left on the table, and the last leg of the event
surface: HA automations (or anything with an MQTT client) can now drive
`readEvent()` patterns directly.

- **Topic**: `luxel/<id>/event`, command-only (no HA entity/discovery —
  it's an automation target, not a control). Subscribed by the firmware
  MQTT task and the mirror alongside the existing three; also added to
  `hamqtt::command_topics` (which, note, neither consumer actually calls
  — both keep inline lists; all three places updated).
- **Payload**: text, one event per line — `type [x [y [value]]]`,
  whitespace-separated decimals; x/y default 0, value defaults 1 (an
  automation can publish just `"1"`), junk lines skipped, 32-event batch
  cap. Parsed by `hamqtt::parse_event_lines` with a hand-rolled
  integer-math decimal→Fx parser (keeps the no_std path off core's
  dec2flt; that ~19 KB table-heavy machinery is in the firmware image
  only incidentally today). Both feed the same queues as
  `POST /api/events`.
- **Harness**: `tools/mqtt-e2e.mjs` (docs/tools.md) — a REAL mosquitto
  (now a dev-shell flake dep) + the mirror: connect, retained
  availability, event → pixels red, value scaling, junk tolerance,
  bare-type defaults. 8/8. This finally gives the MQTT bridge an
  automated check; v0.1.19's verification was a manual procedure.
- Mirror refactor: the inline drop-oldest push in `/api/events` became
  `queue_events`, shared with the MQTT path.

Verified: full workspace tests (hamqtt gains topic/parse/decimal suites),
mqtt-e2e 8/8, serve-e2e, stack-check clean, OTA image 941,952 B
(106,624 B margin). Measured attribution: current master builds the
SAME 941,952 with or without this diff — the mapping itself costs ~0
flash. (The v0.1.38 entry's 922,496 came from that branch's own
worktree build and doesn't reproduce on master; cause not chased —
margin is ample either way.) On-device + real-HA hop: queued in
UNTESTED.md (broker details live in agent memory; the wall unit is
offline).

Same-day follow-up: **docs/mqtt.md** — the MQTT surface finally has a
user-facing reference (enabling, HA discovery entity list, the full
`luxel/<id>/…` topic/payload table, event-topic grammar + an HA
automation example, brightness-scale note, mqtt-e2e pointer). Until now
the only topic list lived in hamqtt.rs source comments; README, lang.md,
and the hamqtt module doc now link to it.

## 2026-08-15 — v0.1.38: external event injection (`readEvent`) + webui.md dedusting

The ideas.md ★★★ item, done end to end (no-device work by design; the
firmware side is mirror-verified, on-device soak deferred until a device
is back online):

- **Engine**: a 32-slot drop-oldest FIFO on the VM (fresh per pattern —
  a switch clears it) + builtins batch 4: `eventCount()` and
  `readEvent(out)` filling `out[0..4] = [type, x, y, value]` (returns
  1/0; non-array or short `out` is a clean vmerr, and only when an event
  was actually there to deliver). `Engine::push_event` try_reserves the
  queue once, dropping events instead of erroring on a starved heap.
  Semantics pinned in tests: FIFO order, drain idiom, overflow keeps the
  newest 32, error cases.
- **Wire**: `"EV1\0" + u8 count + count × 4×i32-LE raw 16.16` — parser +
  builder in `luxel_core::netin` (round-trip + reject tests). `POST
  /api/events` accepts it on the firmware AND the CLI mirror; the render
  task/loop drains a shared batch buffer between frames, same shape as
  sensor frames.
- **Web**: click/drag anywhere on the preview (strip, waterfall, grid,
  or map canvas) injects a type-1 event with normalized x/y into the
  local WASM engine (`lx_push_event`), and in device mode also forwards
  it to the strip (batched ~50 ms per POST). Crosshair cursor +
  `touch-action: none` on the canvases; hover docs + autocomplete for
  both builtins.
- **Patterns**: Typing Heatmap 2D + Crosshair Pulse 2D now consume real
  events; their phantom generators go quiet for 4 s whenever real input
  flows (both soak 300 frames clean).
- **Docs**: lang.md "External events" section; README feature bullet;
  ideas.md item marked DONE. Folded in: webui.md stale-line fixes (3D
  mapping, MQTT/HA, AP-mode were all still marked open despite shipping
  in v0.1.19/v0.1.22/Phase 4).

Follow-up left on the table: an MQTT-topic → event mapping in the
firmware bridge (the generic surface now exists), and swapping the
sound-reactive corpus patterns' trigger controls over where it helps.

## 2026-08-15 (late) — WLED→Luxel installer page (issues #2/#9): one static
## page drives the whole takeover, riding the release pipeline

`web/flash.html` — a wizard that converts a WLED device to Luxel over the
air: probe the device, pick/auto-detect the board, upload the release OTA
image through WLED's own `/update`, watch `/api/status` for the Luxel
signature (3 min budget, with AP-mode/OTA-passphrase/first-boot-panic
troubleshooting on timeout), then push the LUXA web app. Svelte entry
`web/src/flash/` (+`lib/releases.ts`, `lib/device.ts`), second Vite page
next to the playground — so it ships in the web-dist tarball AND onto
every Luxel device's assets partition (+~11 KB gz; a Luxel on the LAN is
the friction-free plain-HTTP origin for converting the next device).

The load-bearing measurement (docs/wled-migration.md): **GitHub's
release-asset downloads send no CORS headers on any hop** (checked
`browser_download_url` and the API octet-stream redirect, incl.
`Origin: null`), so a browser can only fetch firmware same-origin. Hence
two firmware-source modes: *bundled* — the release workflow now composes
a GitHub Pages site (whole web dist + `firmware/` with per-board OTA
bins, LUXA, `manifest.json` via new `web/tools/gen-flash-manifest.mjs`),
fully automatic. (Same-night follow-up after Jeremy enabled Pages: the
v0.1.37 tag predated this work, so deployment moved from a release-job
step to a standalone `.github/workflows/pages.yml` — master-push /
release-published / manual triggers, latest-release firmware via the
GitHub API — which also means installer fixes deploy without waiting
for a firmware release.) *github* mode — API metadata (CORS `*`) +
download-link + file-picker fallback for self-hosted/device-served
copies. WLED-side quirks handled per docs/wled-migration.md: 0.13 has no
CORS (opaque no-cors probe → manual board pick, pointer at `/json/info`),
multipart `/update` POST is CORS-safelisted so it sends everywhere,
esp8266 → hard stop, s2/s3 → "no builds yet", https→LAN mixed content →
Chromium LNA `targetAddressSpace` hint + manual-steps fallback. The page
wears a visible **beta** banner until the pre-guard first-boot
heap-regions panic is root-caused.

Verified per verify-webui: `web/tools/flash-e2e.mjs` in real chromium
against new `web/tools/fake-wled.mjs` (a fake WLED that "reboots into
Luxel" after /update) — 14/14 checks across 4 scenarios (CORS-less full
auto run with byte-counted upload + byte-exact LUXA landing; 0.14-CORS
arch detect + c3 board filtering; esp8266 stop; github-mode file-picker
run), screenshots reviewed. actionlint clean on the workflow. No
hardware touched (Athom in use by another session; the takeover
mechanism itself was hardware-proven 2026-07-26). Untested on real
hardware end-to-end as a *page*: needs a WLED device on the bench —
noted for the next Athom restore-to-stock window.

**Site went live the same night** (Jeremy enabled Pages):
https://googlebot42.github.io/luxel/flash.html, deploying via the new
pages.yml, serving the v0.1.37 firmware set. Smoke-testing the LIVE
site caught a real bug and a spec rename (PRs #27/#28): Chromium
hard-fails a fetch whose `targetAddressSpace` hint mismatches the
target's real address space, and the current LNA spec's value is
`"local"` (PNA's `"private"` renamed) — the hint is now derived from
the target host (RFC1918/.local only; loopback/public get none).
Measured and documented: headless chromium DENIES local-network access
outright (no prompt; CDP grant ineffective in Chromium 150), so from
the https site the page correctly degrades to its manual-steps UX —
the automatic flow from https needs a real user's headful Chrome
permission prompt, still unverified. After the Athom freed up:
current assets pushed (installer included), and the **device-served
page verified against the real device** — resolves v0.1.37 from the
GitHub API in github mode and detects the Athom as already-Luxel.
Remaining for the full conversion proof: stock-WLED restore (needs
Jeremy's button-hold) then the page end to end.

## 2026-08-15 (late night) — v0.1.37: flash-wear fix — playlist swaps
## write nothing

The wear finding from the fairness session (same day, below) is fixed:
`store_current` used to erase the same raw-slot sectors on EVERY pattern
swap, so a 5 s playlist burned ~17k erase cycles/day against a ~100k NOR
spec — days-to-weeks to spec-exhaustion on the header sector. Now
**library swaps (playlist advance, activate, MQTT select, boot resume)
write nothing at all**: their source + blob already live in the pattern
store, and read-back serves from there.

Mechanics: the library id rides IN `Msg::Code`/`Msg::Crossfade` ("" =
ad-hoc) and the RENDER task stamps id + hash + read-back location
atomically at the swap — which also fixes a latent race, since every
sender used to `set_current_pattern_id` AFTER queueing (a fast render
task could bind the previous item's id to the new content). New
`SrcLoc::Library`/`BcLoc::Library` variants: `/api/pattern` and the sync
envelope serve via `source_of`/`bytecode_of` into a transient Vec
(exact-length framing kept — truncate/pad on a mid-session re-save or
delete, mirroring `stream_flash_readback`), and engine rebuilds fetch
the store's CURRENT blob (not the snapshot length — a re-save is the
truth). The slot write remains only for ad-hoc `/api/code` pushes and
sync adoption, and now logs `slot write (ad-hoc…)` on serial — a
tripwire line: seeing it on playlist advances means the fix regressed.

Verified on the Athom (v0.1.37, serial captured): **0 slot writes across
~10+ churn swaps** and exactly one from a deliberate ad-hoc push;
`/api/pattern` byte-identical to the playing item's library copy;
`.lxp` envelope framing computed == actual; pixel-count 300→150→300
mid-playlist rebuilds live off the store; activate → OTA-reboot resumes
the activated pattern; 3/3 clean cold loads under churn; full hw-bench
soak (results in docs/bench-report.md). Stack-check clean, C3 builds.

## 2026-08-15 (night) — CI/releases (issue #8): Gitea-tags → GitHub-builds
## pipeline, modeled on open-nanokvm-pro

Jeremy created github.com/GoogleBot42/luxel and pointed at
open-nanokvm-pro as the reference; the same architecture now exists here
(docs/releases.md is the canonical writeup):

- **Gitea stays the source of truth** (Tailscale-only); the push mirror
  Jeremy configured already replicates to GitHub (verified: the mirror
  carried a merge within minutes).
- **`.gitea/workflows/cut-release.yml`** — tag-cutting from the Gitea UI.
  Unlike onkp there is NO version-bump commit: luxel's version is bumped
  in the shipping PR (firmware/Cargo.toml), so cut-release only validates
  tag == Cargo.toml and pushes the tag. `tools/release.sh` is the
  local/agent fallback (tea API path — the agent has no direct push).
- **`.github/workflows/release.yml`** — GitHub-only (server_url guard,
  since Gitea Actions also reads .github/workflows). On a mirrored
  vX.Y.Z tag: nix-builds all four board variants (the flake's existing
  luxel-fw-* packages: ELF + merged full image + OTA image), builds the
  web app + LUXA, composes full images (LUXA dd'd at 0x310000, same as
  build-esp32.sh image), guards OTA size ≤ 1 MiB and LUXA ≤ the assets
  partition, and publishes: per-board `-ota.bin` + `-full.bin`, the
  `.luxa`, a static `web-dist` tarball (issues #10/#11 fodder), an ELF
  bundle for backtrace decoding, and sha256sums.

Two properties fell out for free and are now documented invariants:
release firmware is **credless by construction** (pure nix eval can't see
creds.env → AP-mode provisioning is the setup path) and release web
bundles are **corpus-free by construction** (fresh clone has no corpus/ →
gallery builds from the clean-room library/ only; rehearsed: 5-file LUXA,
615 KB vs the dev checkout's 6-file 930 KB).

Everything rehearsed locally before the workflows were written: all four
`nix build .#luxel-fw-*` variants build (Athom OTA 916 KB, fits), a
pristine clone builds the web app (after `mkdir -p web/public` — the
fresh-worktree gotcha, now in the workflow), the dd-composition
byte-verified, actionlint clean.

## 2026-08-15 (evening) — v0.1.36: flash-access fairness under playlist
## churn — the driver never leaves the global

The "flash churn starves flash users" finding from this morning's session
is fixed, and the root cause turned out to be one design flaw with three
disguises: `patterns::store_current` (the per-swap read-back persist,
v0.1.34) **took the flash driver out of the global** (`take_flash`) for
its entire multi-page erase/write burst (~200-500 ms per swap). Every
`with_flash` user read busy for that whole window — asset pushes failed
("flash write failed", assets.rs), served assets truncated mid-body
(`read_chunk` → None mid-stream), and `/api/ota` returned the misleading
"update already in progress" (`ota::begin` finds no driver and can't tell
absent-because-store from absent-because-OTA).

**The fix** (firmware v0.1.36): `write_raw` now borrows the driver per
erase/write op via `with_flash` — the same borrow-per-op shape the OTA
and assets writers already soak-proved (2026-07-27: take-for-the-burst
crashed 5/5, borrow-per-op clean 4/4) — with the existing 1 ms yields
between ops now doubling as real windows for waiting HTTP tasks to grab
the driver. `store_current` also skips while an OTA is active (new
`ota::ota_active()` accessor; `with_flash` deliberately doesn't check it
because OTA's own writes go through it). Contention inverts correctly
now: a store transaction (`take_flash` — user-initiated save/playlist
edit) stealing the driver mid-burst aborts the background persist
(read-back degraded until next swap, never a panic), not the user's op.

**Verified on the Athom under a 5 s three-item playlist** (the exact
morning repro): asset pushes **6/6** (was 1/6), **20/20** identical
sha256 on a 315 KB served asset (was truncating), **OTA accepted and
clean-rebooted mid-churn** (was rejected), coldload.mjs **5/5 clean**,
playlist auto-resumed after the OTA reboot, zero panics on live serial
(/dev/ttyUSB0 captured throughout). Deploy tooling's
stop-playlist→push→resume dance is no longer needed on v0.1.36+
(deploy-device skill updated; keep it for 0.1.34/35 devices — the dev
unit is still on v0.1.34 and OFFLINE, push v0.1.36 when it's back).
Stack-check clean; clippy delta zero (the 5 pre-existing `bufs` macro
errors on the c3 target are untouched).

**New finding while verifying — per-swap flash WEAR** (ideas.md): the
persist erases the same slot sectors every swap; a 5 s playlist is ~17k
erase cycles/day against ~100k NOR spec. Fix sketch (carry the library
id in Msg::Code/Crossfade; Library read-back variants; slot write only
for ad-hoc pushes) is written up in ideas.md — deliberately NOT bolted
onto this change (touches all six Msg senders + resume/sync semantics).

Also repaired on the Athom: stored pattern "Doom Fire" (5eed1e55) had
lost its bytecode chunks (playlist skipped it every cycle with
"has no bytecode"; confirmed at idle flash = pre-existing data damage
from the morning's contention chaos, not the new code). Re-saved via
lxp.mjs — same id, plays clean now.

Device end state: Athom on ota_0 = v0.1.36, current assets, empty
playlist stopped (as found), zero panics.

## 2026-08-15 (later) — v0.1.35: cold-load hardening end to end — fetch
## gate + slot reclaim + a pattern-store OOM found by serial; pool stays
## 3 (Chromium needs it), 2 becomes the small-chip profile

The agreed "web pool 3→2 via webui tolerance" follow-up (2026-07-29)
ran its full course and ended somewhere better than planned: the client
tolerance shipped, three real firmware bugs fell out of the verification
gauntlet (one a crash-on-every-page-load), and the pool question got a
definitive answer — **Chromium needs 3 sockets at cold navigation**, so
2 slots is now a `small-chip` cargo feature rather than the default.
Acceptance: **10/10 clean cold chromium loads** on the Athom (fresh
profile, cache off, ~8.6 s to a fully-booted device console, zero failed
requests, zero panics), plus 5/5 with a 5-second playlist churning flash
the whole time. Harness: new `web/tools/coldload.mjs` (docs/tools.md).

**Web (both e2e suites green):** new `web/src/lib/fetchgate.ts` — one
global gate for every fetch the app fires (assets AND API): 2 in-flight
max, backoff-retry on refused (6 tries ≈ 10 s), a 30 s per-attempt
deadline, and the slot is held until the BODY completes, not just
headers — fetch() resolves at headers, and 300 KB gallery bodies kept
device sockets busy for ~6 s afterwards, starving the boot handshake
(caught by per-request tracing). Bodies are buffered inside the gate and
returned as a detached Response. DeviceSession now delegates to it; the
device probe abort went 1.5 → 8 s.

**Firmware fix 1 — pattern-store OOM panic (the big one, found via
serial):** `patterns::read_source`/`read_bc` allocated
`count × CHUNK` bytes INFALLIBLY from a stored TOC record. The Athom had
a corrupt record (playlist wire-format bytes as its name, chunk count 32
= 4× the writer's cap) → every `GET /api/patterns/<id>` tried a 120 KB
alloc → OOM panic → reboot, i.e. **the device crash-rebooted on every
web-app cold load**, on both slots, and the boot guard ping-ponged the
slots — which masqueraded as everything else for hours. Both readers now
reject counts beyond the writer's own caps (MC/MC_BC) and try_reserve.
The corrupt record was deleted (id 2112e1ab). How it got there is
unproven — likely a torn write during the flash-contention chaos below.

**Firmware fix 2 — pool-slot reclaim (`QuickCloseSocket`):** picoserve's
graceful shutdown waits for the CLIENT's FIN bounded by
`timeouts.read_request` — our 45 s, sized for OTA bodies — and browser
socket pools sit on connections after our FIN. Measured: two idle raw
TCP connections wedged BOTH slots ≥60 s. server.rs now wraps the socket
in `QuickCloseSocket`: inline staged shutdown (close → 2 s bounded
discard → 2 s bounded flush → terminal abort), per-slot lifecycle stages
exposed as `"web":[…]` in /api/status, and serial lines on grace
expiry/serve errors. Full 2-slot wedge now self-heals in ~12 s; a
teardown is typically 50 ms–2 s.

**The pool answer:** with everything above fixed, 2-slot cold loads
still failed ~randomly at the NAVIGATION: serial-correlated to Chromium
opening ~2 sockets at cold nav (speculative preconnect + the nav; the
preconnects win both slots, the nav SYN is refused — and
`--disable-features=NetworkPrediction` doesn't stop it). No page code
can fix what happens before the page exists. So `WEB_TASK_POOL_SIZE`
stays 3 by default (esp32 heap static back at 80 KB, .stack 29,716 B
measured) and the new **`small-chip` feature** takes pool 2 + 88 KB heap
(.stack 30,540 B) — reclaiming ~17 KB for the S2/C2 tier at the cost of
an occasionally-refused FIRST navigation (reload works). Both variants
stack-check clean; C3 builds.

**Finding, documented not fixed — playlist flash churn starves flash
users** (v0.1.34's per-swap flash persist): with a 5 s playlist running,
asset pushes failed 5/6 ("flash write failed"), /api/ota rejected
("update already in progress"), and served assets truncated mid-body.
Deploy procedure now: stop playlist → push → resume (in the
deploy-device skill). With the client's retry+deadline the cold-load UX
under churn is clean (5/5), but the write-path starvation stands — a
fairness mechanism (or swap-write backoff while a request is active) is
the real fix, backlogged.

Device end state: Athom on ota_0 = v0.1.35 default (3 slots, all fixes),
current assets, playlist playing as found. The dev unit was OFFLINE at
session end and still runs v0.1.34 — push v0.1.35 to it when it's back
(the pattern-store OOM fix matters everywhere). /api/status gained
`"web"` slot stages. New tool: web/tools/coldload.mjs.

## 2026-08-15 — Claude Code setup bootstrapped from session history

The repo now carries its own agent configuration, mined from six weeks of
memories, session transcripts, and UPDATES.md itself:

- **CLAUDE.md** — orientation, toolchain norms (nix-flake-only, Rust-first,
  strict TS), environment boundaries (container/no-serial, one device + one
  oracle), autonomy grants, hard rules (clean-room corpus, black-box oracle,
  no secrets in tracked files), verification norms, and tripwires.
- **.claude/rules/** — path-scoped must-know risks: `firmware.md` (stack/heap
  footguns), `vm-bytecode.md` (BUILTINS append-only), `web.md` (browser
  verification, Svelte/e2e gotchas, terminology), `corpus-cleanroom.md`,
  `oracle.md` (websocket-wedge et al.).
- **.claude/skills/** — procedures: `deploy-device`, `athom-rig`,
  `verify-webui`, `worktree-setup`, `cleanroom-port`, plus meta-skills
  `fetch-work` / `unblock` / `reflect`.
- **docs/firmware.md** gains a "Stack & heap invariants" section — the
  permanent home for the v0.1.4 / v0.1.19 / v0.1.31-33 memory-model lessons
  (leftover-DRAM stack, task-futures-are-statics, FlashStorage::read bounce
  buffer, measure-don't-estimate, heap economics, WS2812-needs-DMA).
- **docs/wled-migration.md** gains the serial-rig facts (ttyUSB0 ownership,
  single-reader rule, no-DTR/RTS → `--before no-reset --after no-reset`,
  verify dumps twice) and the restore command now carries both no-reset
  flags. Flag spelling verified against the installed esptool v5.3.1 /
  espflash v4.4.0 (hyphenated).
- Cleanup: deleted the untracked `tools/corpus/cleanroom/` scratch dir after
  verifying all 283 specs are byte-identical to the committed
  `docs/pattern-specs/`; gitignored `.claude/settings.local.json`.

Everything cited was verified against the tree during writing; writer
subagents corrected several stale memories along the way (gen-gallery
doesn't read `last-report.json`; the corpus symlink is no longer needed for
the e2e tile assertion; esptool flag spelling).

## 2026-07-27 — v0.1.34: current pattern lives in flash (~40 KB heap
## back) + decode churn fix — Music Sequencer V3 runs at 300 px

Per-allocation profiling (new host harness, below) showed the biggest
pattern's RAM footprint was 62% bookkeeping: the PATTERN_SRC/PATTERN_BC
read-back copies (22.3 + 17.8 KB for "Music Sequencer - for V3 ONLY"),
which nothing on the render path ever reads. Three changes, all
verified on the Athom at 300 px:

- **Decode pre-pass** (luxel-core): `prog_code` is reserved once from a
  header-only sum instead of per function. try_reserve_exact per
  function reallocs the whole buffer each time — 157 KB of copy churn
  for 9 KB of tables on the big pattern (measured), and the realloc
  ladder fragments the heap enough to starve later 17–22 KB contiguous
  reservations (the silent src/bc shedding seen on-device). Now: one
  9,170 B allocation; total decode churn 213 → 65 KB; resident
  byte-identical.
- **Flash-resident current pattern**: on swap the source + blob are
  written as RAW PAGES into the reserved upper half of the storage
  partition (header page written last), and only tiny location enums
  stay in RAM (shared::SrcLoc/BcLoc; boot default = rodata, zero heap
  and zero flash). GET /api/pattern and /api/pattern.lxp stream from
  flash with the FlashAsset discipline (4 KiB reads + Timer yields);
  the engine rebuild reads a transient fallible Vec. A first cut as
  reserved-key map items made every read a 512 KiB NoCache scan whose
  cache-off bursts starved WiFi — raw pages fixed that. Byte-integrity
  verified on-device (src and envelope sections cmp-exact against the
  uploaded originals). /api/status gains `"src"/"bc"` booleans and a
  serial log line when a swap's flash write fails — the shedding that
  used to be silent is now observable.
- **Envelope dropped before the engine builds** (main.rs): the ~40 KB
  upload buffer is freed after the flash persist, so it no longer
  counts against the array budget or the post-load floor check.
  On-device before/after at 300 px: Music Sequencer V3 was REJECTED
  456 B under the floor (v0.1.33 stock: 13 KB under); now it RUNS with
  ~70 KB free. Steady-state heap while running it: was impossible,
  now 70,608 B free.

Known issue (documented, not fixed): back-to-back big readbacks
(/api/pattern twice with no gap) intermittently time out (000, retry
succeeds) — NOT present on v0.1.33's RAM path (A/B'd on hardware).
Slot/keep-alive recycling under load is suspected; a live DDP stream
(LedFx?) was hitting the bench device during later tests, which muddies
attribution. Mid-stream flash failures now PAD the body to the promised
Content-Length instead of truncating — a short body desyncs the
connection and wedges the pool slot until the write timeout (observed
as cascading dead requests; the padding closed that class).

New host tooling: `crates/luxel-cli/tests/allocprof.rs` — dhat-based
per-allocation profile of the device lifecycle (resident / peak / churn
per callsite, driven by AP_SRC/AP_PIXELS/AP_BUDGET env vars), validated
against live hardware to ~2% (Doom Fire predicted 14.7 KB resident
delta, device measured 14.4 KB). `examples/mkenvelope.rs` packs LXP1
envelopes for /api/code. The playlist boot-resume path now also writes
the flash slot on its first swap — soak big-pattern resume before
trusting it hard.

## 2026-07-27 — v0.1.33: main-task stack was 18 KB, not 27 — measured,
## fixed (deterministic /api/wifi + page-load panics)

Jeremy hit a hard-reproducible stack-guard panic on the Athom: every
`GET /api/wifi` (and every load of `/`, whose page JS calls it) died
with "write to the stack guard value on ProCpu". The panic registers
told the whole story: SP = `0x3FFDBA60`, 60 bytes above the `.stack`
section's floor (`0x3FFDBA24`), PC inside
`esp_rom_spiflash_read_status` — main-task stack exhaustion during a
request-context flash read (`read_wifi` → `assets::read_chunk`), the
classic failure mode, back again.

Root cause: v0.1.31's heap retune was arithmetic on an estimate that
was wrong by ~17 KB. The comment budgeted the 3-slot web pool at
"~9 KB of static task arena" and claimed ~27 KB of leftover stack;
`readelf -S` on the shipped ELFs says `server::web_task::POOL` is
25,968 bytes (~8.6 KB **per slot** — each slot embeds picoserve's
whole response-path future) and `.stack` was 18,140 B in v0.1.31 /
17,884 B in v0.1.32. That's ~2 KB above the empirically measured
15.6 KB overflow point — one WiFi NMI frame landing on top of a flash
read at picoserve depth eats it. (This also retroactively explains
v0.1.31's 5/5 `/api/ota` crash-mid-erase-burst on this device.)

- **esp32 heap static 92 KB → 80 KB**: `.stack` measured 30,172 B in
  the new image (the "31 KB ran clean for weeks" zone). Runtime
  heap_free on the Athom: ~95 KB — comfortably above resume.rs's
  `stored×2 + 24 KB` pre-flight and the soak's observed peak.
- **Comment rewritten around the measurement**, with the rule that
  should have been there all along: `.stack` in `readelf -S` is the
  ground truth — measure, don't estimate. `tools/stack-check.sh` now
  enforces it: it prints the linked `.stack` size and fails below a
  24 KB floor (per-frame budget check unchanged).
- **Verified on the Athom**: OTA'd 908 KB to ota_0, then 5/5 clean
  `GET /api/wifi`, repeated `/` loads, api/output/clock/brightness
  all stable. Also pushed the 930 KB playground bundle (the assets
  partition was empty after the serial full-flash — the minimal
  fallback page was what Jeremy's browser was loading), which
  doubles as a clean 227-sector flash-write soak on the new stack.

## 2026-07-27 — v0.1.32: WS2812 goes DMA (fixes erratic colors) + OTA
## writer parity

Jeremy's first real WS2812b test (300 px on the Athom) showed erratic
colors on a plain rainbow. Root cause, confirmed in esp-hal source: the
blocking `Spi::write` splits every frame into 64-byte FIFO transactions
with a busy-wait between them. 64 B = 512 SPI bits, and WS2812 encodes
each LED bit as 3 SPI bits — so every chunk boundary lands mid-symbol
and corrupts a bit (43 boundaries per 300-px frame), and a WiFi
interrupt in the gap stretches it past the strip's latch threshold
(partial-frame latch, rest of the frame re-addresses from pixel 0).
SK9822 has a clock line and never cared — which is why nothing showed
until the first single-wire strip.

- **SPI output is now DMA** (`SpiDma`, blocking mode): one continuous
  gap-free transfer per frame on both chips (esp32: `DMA_SPI2`, c3:
  `DMA_CH0`). The encode buffer became a `u32`-backed `EncodeBuf` —
  the DMA driver only streams a slice zero-copy when it's 4-byte
  aligned with a length that's a multiple of 4 (classic-ESP32 rule);
  anything else bounces through a 4-byte internal buffer, i.e. the
  exact re-chunking the DMA is here to prevent. Max frame (2048 px
  WS2812 = 18.5 KB) fits one transfer (driver cap 32,736 B).
- **OTA writer rebuilt to match the assets writer** (borrow-per-op via
  `with_flash` instead of taking the driver for the whole upload; an
  `OTA_ACTIVE` flag now provides the in-progress guard). Motivation:
  on the Athom, `/api/ota` crashed the device (CPU exception or silent
  lockup mid-erase-burst, panic-reboot or power-cycle to recover)
  **5/5 attempts**, while the line-for-line-identical assets upload
  path was clean 4/4 (930 KB, 227 sector erases each — including with
  the engine frozen). Exonerated by experiment: the Freeze (assets
  push with engine provably frozen at 20 fps = clean), `ota::begin`'s
  partition-table reads (junk-image OTA runs them and rejects cleanly,
  device stays healthy), esp-storage's per-op critical section (active
  in both paths, verified via cargo tree), executor-idle/WAITI
  interleave (frozen assets push = clean), upload pacing (8 KB/s
  throttle still crashed). The one structural delta left was
  taken-bare vs borrowed-per-op flash access, so the OTA writer now
  uses the empirically-bulletproof shape. **Verified on hardware**:
  after a serial full-flash install (which also upgraded the Athom
  off the Arduino-2019 bootloader to espflash's ESP-IDF v5.5.1 one),
  a full 908 KB OTA self-push wrote all 222 sectors, activated, and
  rebooted into ota_1 cleanly — the first successful /api/ota on this
  device ever. (Caveat: bootloader and writer changed together, so
  the fix isn't isolated to one of them.) Residual cosmetic flaw:
  the success response still often dies in the reboot window, so
  ota-push.sh reports failure on a push that actually landed — check
  /api/status version/slot.
- **Known hole, documented not fixed**: `commit` activates on
  `written == Content-Length` alone, so a *truncated prefix* of a real
  image (valid header magic + app-desc) activates and hands the
  problem to the bootloader's image validation / boot-loop guard. Real
  espflash images from ota-push.sh are never truncated; my probe files
  were. A structural end-of-image check would close it.
- Rig lesson recorded: /dev/ttyUSB0 serial works fine — but only ONE
  reader at a time; a forgotten background `cat` silently steals every
  byte and looks exactly like dead serial.

## 2026-07-26 — v0.1.31: takeover always-on + WiFi inheritance + the
## browser-starvation fix

Follow-through on the takeover work below, all verified on the Athom:

- **Takeover is now always compiled in** (feature flag removed): a no-op
  256-byte table check per boot, ~17 KB of image (module + littlefs
  reader + embedded table), and it turns partition-layout changes into
  ordinary OTAs — any future partitions.csv change self-installs on the
  device's next boot. Two new guards for that generality: a flash-size
  preflight (never write a table past the end of the chip) and a
  src/dest overlap check (a resized app slot must never erase the code
  it's running from).
- **WiFi inheritance**: during a takeover the device now mounts the
  outgoing WLED's littlefs read-only (`src/wledfs.rs`, a dependency-free
  ~330-line littlefs v2 reader) and carries SSID (cfg.json) + password
  (wsec.json) into Luxel's own creds record — the device reappears on
  the user's network without provisioning. Factory-fresh WLED (nothing
  to inherit) falls through to the provisioning AP as before. The reader
  is host-tested against real device dumps via `tools/wledfs-check`
  (cfg.json read back byte-identical to an HTTP-fetched reference).
- **"Cannot reach device" in the web UI, root-caused and fixed twice
  over**: the ESP32's 2-socket HTTP pool meant a browser's parallel
  fetches TCP-refused *each other* (headless-chromium repro: 8/10
  parallel API calls refused; even `luxel.wasm` failed during page
  load). Fix 1: web pool back to 3 slots — slots cost ~8 KB heap now,
  not the old 32 KB static (the ~9 KB task-arena growth is repaid by
  trimming the esp32 heap 96→92 KB, keeping the main stack ≈ 27 KB;
  stack-check clean). Fix 2: `device.ts` routes every API call through
  a wrapper capping in-flight requests at 2 with backoff-retry on
  connection-refused. Verified: 3× cold loads on hardware in real
  chromium, zero failed requests, editor synced.
- docs/wled-migration.md: mechanism walkthrough + working notes for the
  future installer page (chip detection via WLED's /json/info, its CORS
  limitation, artifact layout, the open first-boot-panic issue).

## 2026-07-26 — WLED → Luxel OTA takeover (proven on the Athom)

New `wled-takeover` firmware feature (`TAKEOVER=1 ./build-esp32.sh`,
`firmware/src/takeover.rs`): a Luxel app image uploaded through **WLED's
own OTA updater** self-installs the Luxel partition layout. WLED writes
the image into one of its 1.5 MB app slots and boots it (ESP32 apps are
slot-position-independent; WLED's updater only checks the 0xE9 magic); on
boot the module notices the foreign partition table, locates itself by
comparing its `esp_app_desc` against each app slot, copies itself to
0x10000 (sector-by-sector, read-back verified), wipes the nvs/otadata
sectors, rewrites the table (the single ~ms non-re-runnable window), and
reboots. WLED's Arduino-era bootloader is kept and boots our image fine
(proven: "ets Jul 29 2019", DOUT). Crash-safety: everything before the
table write re-runs under WLED's intact table, and since `boot_guard`
runs first (WLED's table also has ota_0/ota_1 + otadata), a crash-looping
takeover build rolls itself back to stock WLED after 3 strikes.
build.rs now serializes partitions.csv via esp-idf-part (byte-identical
to espflash's output, MD5 row included) for the embedded table.

First live run (Athom LS8P music controller, WLED 0.13.2 → Luxel
v0.1.30): upload → self-copy (910 KB) → repartition → clean boot on
ota_0, same DHCP lease (192.168.0.183), storage self-formatted, assets
pushed via `deploy.sh --assets-only`, web UI + engine live at 123 fps.
OPEN ISSUE: the very first boot (from the WLED slot) panicked once with
`esp-alloc: Exceeded the maximum of 3 heap memory regions` *before*
`ota::init`, then self-healed via the panic-reboot handler and never
recurred. A second full takeover run later the same day did NOT reproduce
it — intermittent, 1-in-2 so far. Pre-guard panic loops would never arm
the rollback — understand before advertising this as a public migration
path.

Same-day follow-up — credential/settings inheritance VALIDATED offline:
a configured WLED 0.13.2 stores the WiFi SSID in `cfg.json` and the
password in `wsec.json`, both on littlefs v2 (4 KiB blocks) in the old
spiffs partition — mounted the real device's dump and matched both
against known-good creds. The factory-fresh dump (never provisioned) has
both empty, and WLED's captive-portal config writes NOTHING to NVS (its
`nvs.net80211` blobs stay unprogrammed — WLED runs WiFi.persistent(false)),
so littlefs is the only credential source. cfg.json also carries the
board wiring (relay/IR/mic pins, LED outputs) for the future settings
import. Migration design settled: takeover attempts littlefs inheritance
(creds + pins + name) BEFORE wiping anything; if creds are absent
(factory-default devices) it falls back to the provisioning AP — which is
therefore mandatory, not optional, for the public migration path. Local
corpus (git-ignored, contains real creds): athom-wled-fs-configured.bin,
athom-wled-nvs-configured.bin.

## 2026-07-19 — v0.1.30: boot-resume heap pre-flight (found on hardware)

The v0.1.29 hardware pass caught a real boot-loop: with 2048 px + a large
pattern persisted, boot-time resume loaded source + bytecode + envelope
(all infallible allocations) into the boot heap trough while WiFi — whose
mallocs don't null-check — was still initializing. OOM panic, three
strikes, and the boot-loop guard (correctly) flipped the OTA slot back to
v0.1.28. Two-part fix, found iteratively on the wall (a heap pre-flight
alone still flipped — measured before WiFi had allocated anything, the
heap looked deceptively roomy):
- `resume_task` now spawns **after `wait_config_up()`** — resume always
  runs against post-WiFi steady-state heap instead of racing radio
  bring-up (no network means no resume, but also nothing to persist);
- `apply_stored` pre-flights the heap using a new
  `patterns::stored_size_hint` (TOC chunk counts — no flash reads, no
  allocation), waits up to 20 s for `2× stored bytes + 24 KB` of headroom,
  and skips resume gracefully if it never appears (the default pattern
  keeps rendering; the library copy is untouched).
Verified on the wall: the same 2048 px scenario now reboots cleanly on
the same slot. The playlist task still spawns pre-WiFi as it always has
(soak-proven at 300 px) — worth revisiting if heavy-config playlists ever
misbehave at boot.

Also verified on hardware from the v0.1.29 checklist: single-pattern
resume at 300 px (pattern + controls back after power-cycle), live
sk9822↔ws2812 switches with no outage, and the back-to-back
`/api/config` + `/api/protocol` persistence race (both values survive a
reboot — the WANT_* fix works).

## 2026-07-19 — First full-library hardware soak: 321/322 clean, 0 panics — the render2D holdouts are closed

The first soak since library/ became the gallery source, and it covers the
whole thing: all **322** patterns (the old 195 plus the completed clean-room
corpus), each run on the wall unit's strip.

- **321/322 clean, 0 panics, 0 rejections, 0 reboots** (serial monitored
  end to end); lowest heap seen 53.1 KB; fps at 300 px median 56, p90 123.
  Report regenerated at docs/bench-report.md.
- **The Breakout/Crosstown/Frogger/Swirlpool OOB class is closed.** Root
  cause of the old 4 errors: the pre-clean-room gallery sources carried the
  originals' `sqrt(pixelCount)`-square-rig assumption (fails identically on
  a real PB at 300 px — oracle-verified earlier). The clean-room
  reimplementations dropped that assumption by design (fixed 16×16 virtual
  canvases / map-driven normalized coordinates), so with the oracle-derived
  default ceil(√n) grid they run at any pixel count. Verified natively
  (10 simulated minutes each + seed/fps sweeps), in the shipped playground
  wasm (6000 frames each), and on-device. No engine change was needed;
  the PB-semantics reasoning is recorded in
  docs/research/04-oracle-findings.md ("Maps and render2D" → Resolution).
- **Emoji Animation #2 confirmed on-device**: 20 fps, no vmerr, 87 KB free
  (the LXBC v3 const-array fix holding in practice).
- The one remaining holdout is new territory from the expanded library:
  **"Music Sequencer - for V3 ONLY"** — a true capacity failure, and a
  near miss. 663 lines, 17.8 KB blob, ~71 KB total engine footprint
  (heapstat's largest); it loads at idle heap but leaves **19 KB** free,
  1 KB under the firmware's 20 KB floor, and is cleanly rejected with the
  user-facing "pattern too large for this device" error (mid-soak, with
  less heap, it fails at decode instead). It runs fine in the playground.
  Closing it means flash-mapped execution or WiFi-blob tuning
  (docs/ideas.md) — not worth a risky radio-stack gamble for one pattern.
- No firmware change; device stays on v0.1.28, restored to rainbow/300 px
  and healthy (103.8 KB idle free) after the soak.

## 2026-07-19 — v0.1.29: single-pattern reboot resume + LED-protocol re-init hardening

Two device-robustness features, both off-hardware so far (compile-checked;
the hardware pass has a checklist in docs/webui.md).

- **Single-pattern reboot persistence** (the deferred half of playlist
  resume): an *activated saved* pattern + its explicitly-set slider values
  now survive a reboot. New `firmware/src/resume.rs`; record under reserved
  storage key `0x7FFF_FFFB` (next to the playlist's), line format matching
  playlist.rs (`P <id>` + `C <name> <raw…>`). Rules:
  - only library patterns persist — an ad-hoc `/api/code` push has no saved
    source, so the record is left alone and a reboot resumes the last
    *saved* state;
  - **playlist precedence**: the record is neither written while a playlist
    plays nor applied at boot when the playlist's was-playing flag resumes;
    stopping a playlist marks the item that was showing as the resume state;
  - **flash-wear discipline**: writes debounce (3 s of quiet after the last
    activation/slider event) and identical records are never rewritten;
  - resume is graceful about deleted patterns and stale-format bytecode
    (post-OTA LXBC bump) — it just skips, leaving the built-in default.
- **LED-protocol re-init edge cases** (the last Phase-4 stragglers):
  - `Msg::Protocol` reconfigures the SPI clock *first* and only commits the
    protocol if that succeeded (no more encode-format/wire-clock mismatch on
    a failed apply);
  - the encode buffer is reallocated old-buffer-freed-first and *fallibly*
    (ws2812@2048px ≈ 18 KB; the infallible re-alloc could OOM-panic → reboot
    on a tight heap); on failure the engines freeze and it retries; both
    encode paths length-check the buffer (lazily re-allocating once heap
    frees up, else skipping output) instead of indexing out of bounds;
  - `Msg::Config` frees the engines *before* resizing the buffer (peak-heap
    ordering);
  - **requested-vs-applied persistence fix**: `/api/config` + `/api/protocol`
    persist from new `WANT_PIXEL_COUNT`/`WANT_PROTOCOL` atomics (stored at
    enqueue) instead of the applied atomics, which lag until the render task
    drains the message — back-to-back POSTs could previously persist a stale
    value for the other field and lose one setting across a reboot.

## 2026-07-08 — v0.1.28: `assert()` — invariants become real code; playlists pre-flight against the config

Jeremy's redesign of the hours-old `//# require` directive, and it's
strictly better: **`assert(cond[, "message"])` is a real statement** that
runs inline in top-level init, so invariants can use anything initialized
above them — derived vars, user function calls, array contents — not just
`pixelCount` arithmetic. The comment-directive form is gone (it shipped
yesterday; nothing depended on it).

```js
var w = sqrt(pixelCount)
assert(floor(w) == w, "needs a square number of pixels")
```

- A failed assert **aborts init on the spot** (code above it ran, code
  below didn't) and blocks rendering with
  `pattern requires: needs a square number of pixels (pixelCount = 300)`.
  Changing the pixel count rebuilds the engine → re-runs init → re-checks
  every assert: the settings-page workflow is self-healing, live-verified
  both directions on the wall unit.
- Top-level only, by compile error: inside a function it would fire per
  frame; nested in a branch it isn't an invariant. The quoted message is
  the language's first (and only) string literal, legal only there. A
  runtime error inside the condition stays an ordinary vmerr.
- **LXBC v4**: deduplicated assert-message table + `Assert` opcode, so the
  message survives to compiler-less devices (lean decode keeps it — it's
  user-facing error text, not debug info). `bc-version` auto-heal covers
  v3 blobs; the dev unit's library was upsert-healed to v4.
- **Playlist pre-flight** (the workflow gap that motivated all this): the
  render task re-validates every playlist entry's asserts between frames
  whenever config or content changes (boot, playlist edit, pattern
  save/delete, pixel-count change) — free for assert-less patterns, the
  message table gates it. `GET /api/playlist` reports per-item
  `"invalid":"<msg>"` and the web UI badges the row (⚠ won't run). The
  native mirror computes the same field inline, and device-e2e covers the
  whole loop (API verdict + rendered badge in real chromium).
- One deliberate compatibility note: `assert` is the first extension that
  makes a pattern Luxel-only when used (on a real PB it's an unknown
  identifier). Zero corpus collisions (293/293 clean of `assert`); corpus
  report unchanged at 291/293.
- Hardware: v0.1.28 on the dev unit; assert + config-flip + playlist
  badge verified live; pixel-count sweep 300→600→1024→2048→300 with zero
  panics (heap ≥ 90 KB throughout).

## 2026-07-08 — `//# require` invariants + PB-faithful default grid — the last 2D failures explained

The Breakout/Crosstown/Frogger/Swirlpool class is now understood end to
end, and patterns get a language-level way to state their assumptions.

- **Default map** (PB-as-experienced, oracle-verified): a pattern that
  exports only `render2D`/`render3D` with no installed map now gets an
  automatic ceil(√n)×ceil(√n) row-major grid instead of erroring with
  "no map". Probing the real PB showed a stronger fact: a PB that has
  ever saved a map *cannot be returned to maplessness* via its public
  interface — blank map source and `[]` both keep the old compiled map
  (documented in docs/research/04-oracle-findings.md, plus
  tools/oracle/mapdump.mjs to snapshot/restore a PB's map losslessly).
- With that grid, the four holdouts fail **identically on a real PB** at
  non-square pixel counts (verified live: clean at 17×17, same
  out-of-bounds at 10×30). They're square-rig patterns; 300 isn't square.
  At 289 px Breakout runs on the device at 45 fps. Luxel's 191/195 is
  every pattern PB itself could run on this rig.
- **`//# require <expr> ["message"]`**: patterns can declare invariants in
  a comment directive — `//# require pixelCount % 2 == 0`, or the
  Breakout fix, `//# require floor(sqrt(pixelCount)) == sqrt(pixelCount)
  "needs a square number of pixels"`. Checked before `init` ever runs; a
  violation blocks rendering (black frame) and surfaces as
  `pattern requires: needs a square number of pixels (pixelCount = 300)`.
  Compiled by the frontend into ordinary hidden exported fns (name-tagged
  `require …`), so the wire format is unchanged and the compiler-less
  firmware enforces them by just calling functions. PB-compatible: on a
  real PB the directive is a comment. Documented in docs/lang.md.
- vmerr polish: errors without a source location (require violations,
  lean-decoded blobs) no longer carry a noisy `line 0:0:` prefix.

## 2026-07-08 — LXBC v3: const-array data section — 192/195, capacity failures extinct

Jeremy's idea, straight out of mainline compilers: array literals are
constants — put them in a data section instead of building them at runtime.
The measurement made it a slam dunk: Emoji Animation #2's 768 `[r,g,b]`
literals contain FOUR unique triplets.

- The compiler interns every all-numeric array literal into a
  **deduplicated const pool** in the blob (the pattern's `.rodata`); a new
  `ConstArr` opcode replaces the per-element push/NewArray stream.
- Mutability preserved by **copy-on-write**: each literal occurrence keeps
  its own arena identity as a plain index into the pool (`ArrRepr::
  Const(u32)` — no Rc/Arc, no new dependencies; the arena can never
  outlive the Program, so ids suffice, same as fn/global/builtin ids), and
  materializes an owned copy only on first write. Never written → never
  copied. Identity-preservation is unit-tested (two identical literals
  don't alias after a write).
- Element budget (PB-compat 10,240) still counts const arrays; the byte
  ledger charges only the 32 B entry.
- Emoji Animation #2: blob 17.3 → 5.8 KB, decoded program 19.3 → 8 KB,
  full engine 70.7 → 41 KB — **runs on the device at 34 fps with ~67 KB
  free**. It was the last capacity holdout.

**Certification soak: 192/195 clean, 0 panics, 0 rejections, lowest heap
observed 60.6 KB free.** The 3 remaining errors are the no-map `render2D`
index bugs (Breakout/Crosstown/Frogger — oracle question in ideas.md);
Rainbow Smiley and Rainbow Comet cleared too (their errors were array-
degradation side effects). Day's full arc: 134 → 176 → 182 → 189 → 192,
and idle free heap 50 → 107 KB.

Also: docs/tools.md — a one-page index of every script/harness (soak,
oracle, corpus, e2e, deploy, heapstat), linked from the README.

## 2026-07-08 — v0.1.26/27: the RAM reclaim — 189/195 gallery patterns run; idle free heap 50 → 107 KB

Follow-through on "find something systemic": four structural changes, each
soak-verified on the wall unit, stacking to a device that runs almost the
whole gallery where the morning's build ran two-thirds of it.

- **In-place bytecode execution (LXBC v2)**: the VM now interprets the
  flat LXBC bytes directly — `pc` and jump operands are byte offsets, and
  nothing is materialized per instruction. A decoded Program went from
  ~5× its blob to ~1.2–2.5× (Emoji Animation's 17 KB blob: 60.7 → 19.3 KB
  decoded). One interpreter everywhere (firmware, wasm, native), so
  browser preview and strip can't drift; the decoder additionally proves
  every jump lands on an instruction boundary. Same speed (fps curve
  within noise: 123/84/49 fps at 300/600/1024 px). Format bump v1→v2 —
  stored patterns auto-heal via the bc-version recompile path. The old
  `Insn` enum survives only as compiler IR.
- **Streaming pattern uploads**: /api/code and /api/patterns now stream
  their bodies like OTA/assets do, into an exact-size fallibly-reserved
  Vec. The per-connection HTTP buffer dropped 24 KB → 4 KB (big uploads
  used to dictate its size for every connection) and the upload-size cap
  is gone — the "invalid bytecode: truncated" failures with it.
- **Per-connection buffers + engine freeze**: HTTP buffers are allocated
  per connection, not held for the server's lifetime (the 3rd pool slot
  had already died with the /ws stream). If an upload can't get memory
  because the running pattern owns the heap, the engine is FROZEN (heap
  released, strip holds its last frame) and the reservation retried — and
  OTA freezes the engine up front (a reboot follows anyway). No more
  "can't reach the device because a big pattern is running".
- **Byte-accurate array budget** (elements × 8 + per-array overhead
  against free heap, PB's 10,240-element compat cap on top) and
  pattern-cell hygiene (PATTERN_SRC/_BC no longer retain a past giant's
  capacity forever).

**Definitive soak (docs/bench-report.md): 189/195 clean, 0 panics or
reboots.** The 6 remaining: Emoji Animation #2 (genuinely beyond ~107 KB
free — cleanly rejected), and five 2D patterns hitting `array index out
of bounds` with no map installed (Breakout/Crosstown/Frogger/Rainbow
Smiley/Rainbow Comet) — a map-semantics question for the PB oracle, on
the backlog, not a memory problem. Day's arc: 134 clean (with dozens of
disguised OOM reboots) → 176 → 182 → **189, zero reboots**.

## 2026-07-08 — v0.1.25: the OOM hunt — soak v5 finally runs the whole gallery with zero reboots

Soak v5 on v0.1.24 kept OOM-panicking the device ("memory allocation of N
bytes failed" → reboot). Root cause wasn't one bug but a heap-economics
problem: ~50 KB free for patterns, and several paths that allocated
infallibly while a heavy pattern legitimately held most of it. Fixed over
six soak iterations, each verified on the wall unit via serial:

- **One decoded Program at a time.** The render task kept a resident
  second copy (`current_prog`) + per-rebuild clones — a decoded Program is
  2–3× its blob. Now the only Program lives inside the engine; rebuilds
  re-decode from the running blob, and the outgoing engine is freed
  *before* the new one decodes (peak lands where the most is free).
- **`bytecode::validate()`** — every check the decoder does, near-zero
  allocation (equivalence-tested against `deserialize` on truncations and
  corruptions). Upload/activate/MQTT/sync handlers no longer build a
  throwaway Program in request context.
- **`deserialize_lean`** — devices decode without debug info (pos +
  local names): ~half the Program RAM; vmerrs keep fn/pc, lose line:col.
- **Envelope passthrough** — `Msg::Code`/`Crossfade` carry the received
  LXP1 buffer verbatim; producers (HTTP/MQTT/sync/playlist) make ZERO
  source/blob copies (a copy in the HTTP task OOM'd while amoeba ran).
- **Fallible everything in the swap path** — decoder Vecs, the request
  body copy, PATTERN_SRC/_BC updates all `try_reserve`; array allocation
  is budget-checked BEFORE reserving and the reservation itself is
  fallible (also fixes a pre-existing VM hole: `array(50000)` allocated
  first, budget-checked after = panic even pre-bytecode).
- **Heap-aware array budget + runtime floor** — arrays may fill free heap
  down to a 20 KB floor (÷12 per element, min 1024 so ordinary strip
  patterns never trip it); a pattern whose engine leaves less than the
  floor is REJECTED with a "pattern too large for this device" vmerr.
  Rejected patterns idle properly (a busy-spin showed up as "150825 fps").
- **DRAM rebalance** — the third HTTP pool slot existed for the removed
  preview websocket: pool 3→2 (each slot = 32 KB of buffers), request
  buffer 16→24 KB (envelopes are src+bytecode now), main heap region
  88→96 KB (the ~31 KB stack was sized for the deleted on-device
  compiler; the pool's static shrink pays the stack back). Idle free
  heap: ~50 → ~66 KB.
- vmerr reporting deduped per error site (was per frame: serial flood +
  format! churn at 120 fps).

**Soak v5 final (docs/bench-report.md): 176/195 clean, 0 panics/reboots**
across all 195 patterns + the pixel-count curve — vs the old committed
baseline of 134 clean/61 errors (most of those "timeouts" were OOM
reboots in disguise). The 19 remaining: 7 genuinely-too-large patterns
(amoeba, tixy, bustle, Bouncy Boxes, DBZBattleFinal, neutronorbit,
StarGen polar 2D — clean rejections), ~6 array-budget degradations, 3
2D-game patterns with map-related index errors (separate issue), and a
dozen heavy patterns now run *slow* (20–29 fps) that previously crashed
the device outright. Closing the rest wants in-place bytecode execution
(skip the `Vec<Insn>` materialization — the PB way); tracked in ideas.

## 2026-07-08 — v0.1.24: devices execute LXBC bytecode; compiler out of firmware

The browser (wasm) and CLI now compile patterns to a serialized bytecode
(**LXBC**, docs/spec/bytecode.md) and upload it alongside the source in an
LXP1 envelope; the device stores both and only ever *decodes + executes* —
the lexer/parser/compiler are no longer linked into the firmware (a
default-on `frontend` cargo feature in luxel-core).

- **Flash**: app text+data 929.4 KB → 876.8 KB (−52.6 KB net, decoder
  included) against the 1 MiB OTA slot.
- **Robustness**: the on-device compile path — the deep-recursion,
  alloc-spiky thing behind the v0.1.21 stack-overflow saga — is gone by
  construction. The LXBC decoder fully validates untrusted blobs (the VM
  trusts `Program`, so indices/jumps/argc are proven at decode time).
- **ABI**: builtins are referenced by name via a per-blob import table, so
  growing the builtin table never invalidates stored patterns; real format
  changes bump `FORMAT_VERSION` and devices answer `"code":"bc-version"` —
  the web app then recompiles from the stored source and re-saves
  automatically.
- **Sync**: followers adopt the leader via `GET /api/pattern.lxp`
  (source + bytecode envelope) instead of compiling `/api/pattern`.
- **Storage**: pattern store format v3 (bytecode chunks next to source
  chunks; bc gets 6×3840 B ≈ 23 KB — corpus max blob is 17.5 KB). ⚠️ the
  v2→v3 bump wipes the on-device library + playlist on first boot after
  OTA; re-save from the app.
- **API break**: `POST /api/code` and `POST /api/patterns` take the binary
  LXP1 envelope now — raw-source curls need `luxel compile` (new
  subcommand) or the web app. Mirror (`luxel serve`) matches, and executes
  stored bytecode via the same decode path as the device.
- Verified: corpus 291/293 compile+run with byte-identical LXBC round-trip
  and pixel-identical source-vs-bytecode rendering; luxel-core tests (135)
  incl. decoder corruption tests; device/sync/playground e2e all green;
  ESP32 (Xtensa + C3) builds with zero frontend symbols.
- Drive-bys: `<init>` now keeps line info for init-time vmerrs;
  MAX_LOCALS off-by-one (256th local wrapped a u8) fixed at 255.

## 2026-07-07 — the v0.1.23 batch (catch-up entry): HA polish, sync v2, SNTP, output pipeline, panic fix, boards

Written after the fact — this batch shipped across 2026-07-06/07 in commits
e04f661…012d71b and was verified piecemeal; collecting it here.

- **HA polish** (e04f661): diagnostics sensors (fps/heap/rssi) + playlist
  switch and next/prev buttons via MQTT discovery. Verified live on the
  real broker + wall unit — the HA integration task (#54) is closed.
- **Sync v2** (0908341): followers adopt the leader's *pattern*, not just
  its timebase — beacon carries a source hash; on change the follower
  pulls and swaps. (Since v0.1.24 the pull is `/api/pattern.lxp` bytecode.)
- **SNTP wall clock + timezone** (7fe7aa7): `clockHour()`-family builtins
  work unplugged from a browser; tz persisted, settable via /api/clock.
- **Output pipeline** (3a29bfd): wire color order, global gamma LUT, and a
  current-estimate power cap — applied between blend and protocol encode,
  all live-settable + persisted (/api/output).
- **Compile-panic fix, part 1** (61c4266): parser/compiler recursion
  bounded at depth 60 → "nesting too deep" diagnostic instead of a device
  stack overflow (the soak-v4 crash). Deepest real gallery pattern is 16.
  Part 2 (a stack byte budget) was designed but became moot — v0.1.24
  removed the compiler from the device entirely.
- **Socket hardening + soak harness** (bd8d258): 45 s read_request timeout
  (kills the pinned-socket cascade) and tools/hw-bench.mjs, the on-hardware
  gallery soak + fps/pixel-count bench behind docs/bench-report.md.
- **Board portability** (012d71b): board.rs identity module, four board
  features incl. `board-esp32-generic`, wiring isolated to one marked
  section of main.rs. All three Xtensa variants + C3 build clean as of
  2026-07-08; docs/boards.md has the add-a-board recipe.
- Plus: README front-door rewrite (9781563).

## 2026-07-08 — device recovered ✅: v0.1.21 verified on hardware + tools/deploy.sh

You serial-flashed the fix — thanks! Verified on the wall unit right after:

- **Boot + OTA path healthy**: v0.1.21 up at ~124 fps, and a fresh OTA cycle
  (ota_0 → ota_1) worked, so the wedge is fully behind us (boot guard armed).
- **Sensors on hardware**: injected a frame via POST /api/sensors — exported
  vars carried the exact values and the strip lit from `energyAverage`.
- **Sync on hardware**: put the device in follower mode and fed it beacons
  from the dev container — it hard-jumped onto the fake leader clock in <1 s
  and held **±6 ms**. (Two-device sync awaits a second Luxel.)
- **DDP/E1.31 re-verified** on the new build.
- **MQTT/HA verified on your live broker** (192.168.1.2, user root): the
  device connected, published retained discovery, and shows up as device
  `luxel-4ae0d4` in HA — a **Light** and a **Pattern** select. Exercised
  from the MQTT side against the real wall: brightness 66/255 → device 8/31
  (state echoed back), power OFF blanked the strip with the engine still
  running, ON restored, and selecting "Rainbow" from the (auto-announced)
  library options switched the running pattern with the state topic
  following. Brightness restored to your 4/31 afterwards. The device
  library was empty post-recovery — I saved "Rainbow" into it so the
  select has an option; check Settings → Devices in HA for the new device.

**Your question — does flashing firmware also flash the asset bundle? No.**
`espflash flash` writes only the app image; the web app lives in the
`assets` partition (0x310000), deployed separately via POST /api/assets.
Your recovery left the device serving a stale playground — fixed (pushed the
current bundle), and now there's **`tools/deploy.sh <ip>`**: builds + OTAs
the firmware, then builds + packs + pushes the assets, one command
(`--fw-only` / `--assets-only` to split; validated live end-to-end).

## 2026-07-08 — Mic-to-strip, oracle sweep, Luxel-to-Luxel sync (v0.1.21)

Per your picks (all without the wedged device; everything below rides the
same recovery flash):

- **Mic → device forwarding**: with the playground's *sound* toggle on in
  device mode, mic frames also stream to the strip via POST /api/sensors
  (~20 Hz) — your laptop mic IS the sensor board. e2e-verified on the mirror.
- **Oracle sweep vs the real PB (fw 3.67)**: 130/165 exact + new probes.
  Transforms fully verified (composition order, cross-frame accumulation,
  rotate direction — pinned as tests); found + fixed one real divergence
  (`pow(negative, fractional)` now returns the PB's raw 0x80000000 instead
  of 0); several TODO(oracle) markers settled (log2(≤0), refs-as-0, ref
  identity equality, builtin shadowing aborts, div/0 family). New documented
  supersets: builtins as first-class values, lenient arity.
- **Luxel-to-Luxel sync v1**: one device leads, broadcasting its engine
  timebase on UDP :4049 (4×/s, sensor frame piggybacked when fresh);
  followers hard-jump when >1 s off, then slew smoothly by stretching frame
  deltas. Same pattern on several Luxels = phase-locked, one mic drives all.
  Role select in Settings (persisted; device-settings record → v4 with a
  compatible v3 fallback). **Proven with two mirrors** — tools/sync-e2e.mjs
  desyncs them 2.5 s and watches them converge to single-digit ms + the
  sensor relay land. On-device: needs two recovered Luxels someday.

## 2026-07-07 — ⚠️ DEV DEVICE NEEDS A BENCH RECOVERY (serial reflash)

The v0.1.19 OTA (MQTT) **bricked the boot**: the first cut put ~12 KB of
MQTT/netin buffers into embassy task futures — which are *statics*, and
statics eat the DRAM that becomes the main task stack (the v0.1.4 lesson,
re-learned). Main stack fell to ~10.7 KB (known-fatal is <15.6 KB) and the
device now crash-loops before WiFi comes up, so OTA can't reach it.

**To recover at the bench** (bad image is in ota_0; USB/serial):

```
cd firmware && ./build-esp32.sh board-pixelblaze-v3
nix develop --command espflash save-image --chip esp32 \
    target/xtensa-esp32-none-elf/release/luxel-fw /tmp/luxel-fix.bin
nix develop --command espflash write-bin 0x10000 /tmp/luxel-fix.bin
```

(That writes the current fixed build — now v0.1.21, which also picks up the
sensor-board/sound/sync work above — over the bad one; otadata already
points at ota_0. The fix moves all big task buffers to the heap and trims
the heap static 96→88 KB — main stack is back to ~31 KB.)

**So this can never happen again:** the firmware now has a **boot-loop
guard** — it counts boot attempts in flash before the risky part of boot,
and on the 3rd consecutive boot that never reached "healthy" (60 s of
serving) it flips otadata back to the other OTA slot by itself. A future
bad OTA self-heals in ~30 s instead of wedging the device.

## 2026-07-07 — Sound-reactive groundwork: playground mic + sensor-board support (v0.1.20) 🔶 partly device-blocked

Per your pick (audio next, engine+playground first, sensor board too):

- **Sensor bindings live in the engine** — `export var frequencyData` /
  `energyAverage` / `maxFrequencyMagnitude` / `maxFrequency` / `light` /
  `accelerometer` / `analogInputs` now receive data (they were zero-stubs).
- **Playground "sound" toggle** (next to *debug*): feeds your microphone
  through a WebAudio analyser reshaped to the PB sensor board's 32 log-spaced
  bands (37 Hz–10 kHz) — sound-reactive patterns run in the browser today.
  e2e-covered with chromium's fake mic.
- **PB sensor expansion board support (firmware)** — the official board's
  115200-baud `SB1.0` frames are parsed off UART0 RX (the expansion header's
  RX0, where the board plugs in PB-style). Parser is shared with the mirror
  and unit-tested; 🔶 hardware verification needs the recovered device (and a
  physical board, if you have one).
- **`POST /api/sensors`** (firmware + mirror) — accepts a raw sensor-board
  frame over HTTP, so a desktop script can stream audio/motion data to the
  device with no extra hardware. e2e-verified against the mirror.
- Also: the **firmware size report** you asked for is in
  [docs/size-report.md](docs/size-report.md) — TLDR: ~⅓ is Espressif's
  closed radio stack; found + fixed one real lump (Fx printed via f64 →
  −14.4 KB); regenerate with `tools/size-report.py`. And the **C3 devkit
  build** compiles again (two rv32imc atomic-RMW slips).

The onboard PB v3 mic (your "SPI audio") stays open: closed hardware,
undocumented pinout — that's a bench session with the recovered device
(I2S/PDM driver + FFT prep can happen any time; say the word).

## 2026-07-07 — MQTT + Home Assistant discovery (firmware v0.1.19) 🔶 device-blocked

Point the device at an MQTT broker (Settings → "MQTT / Home Assistant":
host/port/user/pass, applied live, persisted in flash) and it announces
itself to Home Assistant automatically via MQTT discovery:

- **Light** — power + brightness as a normal HA light. Power off blanks the
  strip while the engine keeps running, so ON resumes mid-animation.
- **Pattern select** — the device pattern library by name; picking one runs
  it (and re-announces when the library changes).
- Availability (`luxel/<id>/status`) with an offline LWT; state topics echo
  changes made anywhere (web UI slider moves show up in HA within ~5 s).

Topics/payloads live in `luxel_core::hamqtt` (unit-tested), shared between
firmware (rust-mqtt over embassy-net, with a tiny embedded-io 0.7→0.6
adapter) and the mirror (rumqttc). **Verified end-to-end against a real
mosquitto** with the mirror: retained discovery configs, brightness
round-trip (HA 128 → device 16 → state 132), pattern select switched the
running pattern. Also: `/api/mqtt` GET/POST, broker creds in the third nvs
sector, `connected` status in Settings. e2e: 68 device checks green.

🔶 On-device verification is blocked on the bench recovery above; after
that, OTA the fixed image and point it at a broker (HA's Mosquitto add-on
or any broker; note the dev container's broker may not be reachable from
the device's subnet — use the HA one).

## 2026-07-07 — DDP + E1.31 network input (firmware v0.1.18) ✅

xLights / LedFx / Resolume can now drive Luxel as a network pixel output:
the device listens for **DDP on UDP :4048** and **E1.31/sACN on UDP :5568**
(universe 1 up, 170 px each). Incoming frames paint the strip directly,
bypassing the engine; ~2.5 s after the stream stops, the running pattern
takes back over — so a LedFx session ends and the wall just resumes its
playlist. `/api/status` gained a `live` field (`"ddp"`/`"e131"`/`null`) and
Settings shows a "Network input" status row.

Packet parsing lives in `luxel_core::netin` (unit-tested, shared by firmware
and mirror). Verified end-to-end on the wall unit: DDP red/green/blue +
offset writes, E1.31 magenta, and the timeout-resume — all over real WiFi.
Multicast sACN groups are joined at boot but only unicast was verifiable
from the dev container (bridges/APs commonly filter multicast); xLights and
LedFx default to unicast anyway. Image is 887 KB — still 161 KB under the
1 MiB OTA slot. Both e2e suites green (65 device checks); v0.1.18 OTA'd.

Also: **springy easings** — `easeOutBack`, `easeOutElastic`, `easeOutBounce`
join the builtins (that completes the entire builtins backlog in ideas.md).

## 2026-07-07 — Playlist crossfade transitions (firmware v0.1.17) ✅

Playlists can now **crossfade** between items instead of hard-cutting. The
Playlist tab has a new "crossfade" field (seconds; blank/0 = hard cut). During
a transition the render task keeps the outgoing pattern alive and linearly
blends it into the incoming one over the set time — verified on the wall unit:
a red→blue item change ramps through `a3005b`→`62009c` mid-fade rather than
snapping. The crossfade time persists in flash (playlist wire format gained an
`X <ms>` line) and applies on both auto-advance and manual next/prev.

Also: the app crossed the **1 MiB OTA-slot** boundary at this version, so the
release profile moved to `opt-level = "s"` (size) — reclaimed ~177 KB (image
1,052 KB → 875 KB) with no visible render-rate hit (still 125 fps). Both e2e
suites green; v0.1.17 OTA'd to the wall unit.

## 2026-07-07 overnight — batch: gallery search, playlist polish, 3D preview, WiFi form, device map ✅

While you slept (you picked web features + WiFi + device map, OTA authorized):

- **Gallery search** — a search box filters the 190+ patterns by name.
- **Playlist polish** — a Clear button, a "N items · loop ≈ Xm Ys" total, and
  playlist entries whose pattern was deleted now show "(deleted)" and can be
  removed (the scheduler skips past them).
- **3D map preview** — a map whose z varies now renders as a slowly
  auto-rotating point cloud (depth-sorted, nearer points larger/brighter)
  instead of flat.
- **WiFi settings form** — the Settings tab shows the network the device will
  join and lets you change the credentials (it reboots to apply).
- **Device map upload** (firmware v0.1.16) — install a computed 2D/3D map onto
  the device so its patterns render with real geometry (render2D). Verified on
  the wall unit: a reversed map really does drive the pixels (and it survives a
  reboot). "Install on device" / "clear device map" live in the map sub-tab.

All committed + deployed (v0.1.16 OTA'd; web hot-reloaded); the wall is back to
the rainbow with a clean slate. Both e2e suites green (61 device checks). Left
alone per your note: persistence of a single ad-hoc (non-playlist) pattern.

## 2026-07-06 night — Playlist + "untitled" fix ✅

**Playlist** (firmware v0.1.15): a new device tab that plays your saved patterns
in order. Each entry carries its own **parameters**, so the same pattern can
appear multiple times with different looks — "+ playlist" in the editor captures
the current slider values. Durations are flexible: a playlist-level default,
each item can override it, and default-blank = manual advance (so you get global
timed, per-item timed, or manual, even mixed). It's **saved on the device and
resumes across reboots** — verified on the wall unit (after a reboot the playlist
and its params were intact and still playing). Built across the native mirror,
the web UI (reorder / remove / inline params), and the firmware scheduler.

**"untitled pattern" fix:** on a device the header showed "untitled" because the
device streams only source, not which saved pattern it is. Now the editor
recognizes the running pattern as its saved library entry and shows the name.
(Along the way, fixed the editor spuriously marking freshly-loaded patterns as
edited — which also makes the resume-your-edits behavior more reliable.)

Re: your reboot question — **playlists** now persist and resume across reboots.
A single ad-hoc (non-playlist) pattern still resets to the default on reboot;
I can add persistence for that too — say the word.

## 2026-07-06 night — clean device load (no flash, clear "running on device") ✅

You saw the window flash the playground before a connecting spinner appeared. On
a device the app now probes for the device in parallel with the wasm load and
holds a **full-screen boot cover** until it's decided device-vs-playground and
loaded the device's running pattern — so nothing flashes first. The cover reads
**"opening the pattern running on the device…"**, making it clear the pattern is
the one already on the device. Web-only hot reload; both e2e suites green.

## 2026-07-06 night — device mode = local preview + push (no streaming) ✅

Per your call: the live pixel stream from the device wasn't helpful, so it's
gone — along with the connect/disconnect buttons (it's always connected for the
API). Device mode is now **the playground that also drives the strip**:

- The preview runs on the **local WASM engine**, instantly — no device round-trip
  and no ws/HTTP pixel polling. You watch the real strip for the real thing.
- **Editing pushes to the device**: typing recompiles locally (fast) and pushes
  the code over WiFi (throttled; a broken pattern is never sent). Controls drive
  both the preview and the strip. The **step-debugger now works in device mode**
  (it's genuine local compute).
- **No connect/disconnect/reconnect buttons or badge** — the wordmark shows the
  device; a failed connect just shows an error (reload to retry).
- It **still waits for the device on load** so it opens whatever pattern is
  running, exactly as you asked.

Web-only (the firmware still can stream — the UI just stopped asking), so it went
out as a hot asset reload, no reflash. Both e2e suites green (43 device checks);
code-push verified on the wall unit.

## 2026-07-06 night — live LED-protocol switch (firmware v0.1.14, no reboot) ✅

You asked about protocol — different strips use different ones. You can now
switch **SK9822 (APA102) ↔ WS2812 (WS281x)** at runtime, no reboot, from a
Settings dropdown. esp-hal's blocking SPI has `apply_config()`, so the render
task just changes the clock (8 MHz ↔ 2.4 MHz) and re-sizes the encode buffer
between frames — same live-swap trick as pixel count.

- `GET/POST /api/protocol` (accepts sk9822/ws2812 plus aliases apa102/ws2811/
  ws2815). Persisted alongside brightness + pixel count (settings record → v3).
- Verified on the wall unit: sk9822→ws2812→sk9822 live, no crash, SPI restored
  to full speed (a trivial pattern is back to 124 fps). Left on sk9822 (its real
  strip), rainbow running.
- Firmware v0.1.14 OTA'd; mirror + web + e2e updated (device suite now 49
  checks); assets hot-reloaded.

**All three device settings — brightness, pixel count, LED protocol — are now
live, persisted, and reboot-free.** Phase 3 is down to just the Settings WiFi
form (the endpoint already exists).

## 2026-07-06 night — live pixel-count resize (firmware v0.1.13, no reboot) ✅

You asked for `/api/config` pixel count next and said a reboot isn't ideal — it
turned out a **live resize with no reboot** is feasible, so that's what shipped.
The SPI is Blocking (no DMA) and the encode buffer is a plain heap Vec, so the
render task (which already rebuilds the engine on every code upload) just
reallocs and recompiles at the new count between frames.

- `GET/POST /api/config` — POST a pixel count (1–2048), the strip resizes
  instantly. Persisted alongside brightness in flash (the settings record went
  v2). The **Settings Pixels field is now editable** and re-anchors the preview.
- Verified on the wall unit: 300→150→300 with **no reboot** (same OTA slot, fps
  never dropped, heap freed at 150 and came back at 300), out-of-range rejected,
  and it persists. Left at 300 (the physical strip length).
- Firmware v0.1.13 OTA'd; mirror + web + e2e all updated (device suite now 46
  checks); assets hot-reloaded.

Still open in Phase 3: a runtime LED-protocol switch and the Settings WiFi form.

## 2026-07-06 night — device-editor polish + real brightness (firmware v0.1.12) ✅

Continued from your feedback, then took the plan's next step.

- **Waterfall clears on every open**, and the **save/⋯ actions moved** out of the
  header into a toolbar fixed above the editor (`1945a73`).
- **Device editor fixes** (`6db7f03`): opening a device pattern now shows a
  **loading screen** until its source is fetched (no more flash of the last
  script); **Device Patterns get live preview thumbnails**; the **strip/grid/2D
  map dropdown is back on the device** (pixel count fixed by hardware; map is a
  local preview aid); and a **dirty-aware resume** — on load we only resume the
  last file if it had unsaved changes (and then push it so the device runs it
  too), otherwise we open whatever pattern is active on the device.
- **Brightness is real** (`6143352`, **firmware v0.1.12**, OTA'd to the dev
  device): a runtime `GET/POST /api/brightness`, applied every frame in the
  encode path (SK9822 current field + a WS2812 software scale) and **persisted
  in flash** (new `LXDV` nvs record, its own sector so it never touches WiFi
  creds) so it survives reboot. The Settings **Brightness slider is now live**
  (was a placeholder). Verified on hardware: live apply, out-of-range rejected,
  flash-persist ok. Note: the web preview stays at full range — brightness dims
  the physical strip only. Remaining Phase-3 item is `/api/config` (pixel
  count/protocol), which needs a runtime pixel count + reboot-to-apply.

Both e2e suites green (device suite now 42 checks incl. layout/thumbnail/
dirty-resume/brightness); assets hot-reloaded to 192.168.0.205.

## 2026-07-06 night — navigation redesign: library-first, editor-on-demand ✅

Restructured the app around how you actually work with patterns, per your
direction:

- **The Editor is no longer a tab.** It opens **full-screen** (its own bar:
  ← back · name · save/⋯) when you create a pattern or pick one to inspect —
  which "really gives you the feel that you are editing something." On load it
  **resumes your last edit** (falls back to the Patterns Library); on a device
  it opens on the **running pattern**.
- **"Patterns" → "Patterns Library"**, with a **+ New pattern** button. It holds
  the examples, community corpus, and your saved patterns (chips). Picking a
  tile opens it in the editor.
- **New "Device Patterns" tab** (device mode) — the patterns in the device's
  memory, each opens/activates in the editor; its own **+ New pattern**.
- **The examples dropdown is gone entirely.** Pattern selection is the Library /
  Device Patterns lists now.
- **Mapping is clearly optional**: the layout selector gained a **"2D map"**
  option — choosing it is the *only* enable/disable for mapping. It reveals the
  pattern·map editor sub-tabs and runs the map program; any other layout turns
  mapping off. (Replaces the old "back to strip" button.)

Both e2e suites were rewritten for the new flow (editor entered via New / tile /
device pattern; no picker) and are green in real chromium: 58 playground + 28
device checks. Pushed to the dev device.

## 2026-07-06 night — control-picker layout, no device-URL field, no share on device ✅

Three UI fixes from your feedback:

- **Color-picker controls no longer run off-screen.** An `hsvPicker`/`rgbPicker`
  laid its three channels out in one horizontal row, so the 2nd/3rd sliders
  overflowed the narrow right rail and were unreachable — and the channels had
  no numeric field. They now **stack vertically**, each channel with a slider
  **and** an editable number box (like the scalar sliders). Verified: 0 rail
  overflow.
- **No more "device url" field.** The address is always known — a real device
  serves the UI from its own flash (auto-connect to same origin), and reconnect
  just reuses that. So disconnect → **"reconnect" button, no URL to type**. A
  hosted/standalone playground has *no* device support at all (that's what
  playground mode is for), so it shows no connect UI. (Dev/e2e point the built
  UI at a device or the mirror with `?device=<base>`.)
- **Share is gone in device mode.** Those links carry the pattern in the URL —
  on a device that's a LAN address that won't work for anyone else. Share is now
  shown only in the playground (keyed off the same "is this a playground?"
  state, so it's hidden whether you're connected or just served from a device).

The map sub-tab is likewise gated to playground mode (device map upload is a
later firmware item). device-e2e updated (auto-connect via `?device=`, asserts
no URL field / no share button in device mode, and reconnect-without-URL); both
suites green. Pushed to the dev device.

## 2026-07-06 night — clean device connect-on-load (no more waterfall garbage) ✅

Fixed the connect weirdness you flagged: on load the waterfall skipped and kept
stale/old data after the connection stabilized, because the async handshake
(status → source → controls → ws) leaked pre-stabilization frames.

- **Connection phase state** (`idle → connecting → live`). `connectDevice` enters
  **connecting** and holds the preview blank; a `markLive()` helper flips to
  **live** the moment the *real* stream delivers its first datum (ws pixels or
  status, or the HTTP-fallback poll) and clears the preview **once** at that
  transition. So leftover playground content, canvas-resize artifacts, and
  HTTP/WS cadence jitter can never end up in the waterfall.
- **Visible connecting state** — a "connecting…" pill in the header and a
  spinner overlay on the preview, so the async handshake reads as intentional
  instead of showing stale frames.

Verified in the device-e2e (against the native mirror): on connect, the
waterfall is held **blank (0 lit)** all the way through the handshake, then
streams cleanly — plus the existing 19 device checks and 55 playground checks
stay green. Pushed to the dev device.

## 2026-07-06 night — mapper is a debuggable Luxel program in its own editor tab ✅

Re-read the feedback and fixed the thing I'd under-heard: "the mapping function
should be a tab on the script editor **and debuggable as well**." Per your call,
the map is now a **Luxel program** (not JS), which makes it debuggable for free
by reusing the pattern VM + debugger.

- **New `plot(x, y[, z])` builtin** + engine "map mode": a map program exports
  `render(index)` and calls `plot()` once per pixel. A second engine runs it
  through the *same* per-pixel `drive` loop as patterns, but stores the plotted
  coordinate instead of a color. Collected coords install into the pattern
  engine as a 2D/3D map.
- **Editor sub-tab (pattern · map).** The map is edited in the same CodeMirror
  as patterns — luxel highlighting, completions, hover — not a bare textarea.
- **Debuggable exactly like a pattern.** Gutter breakpoints, step over/into/out,
  the call stack, locals, and globals all work on the map program because it's
  real VM code. (The debugger panel is now a shared `Debugger.svelte`.) A live
  scatter preview renders the pattern on the computed map.
- Spans all layers: luxel-core (`plot`, `enable_map_mode`/`run_map`/`map` +
  4 unit tests), luxel-wasm (`lx_run_map`/`lx_map_*`), luxel.ts
  (`compileMap`/`runMap`), App.svelte. Playground-only for now (installing a
  computed map on the device is a later firmware item).

Also, two small fixes you flagged: the page **`<title>` is now just "Luxel"**
(was "Luxel Playground"), and the **Patterns tab shows a loading state** — the
built-in examples render immediately and a spinner reads "loading patterns…"
while the corpus streams in, instead of a bare "0 patterns."

Verified in real chromium: 55 playground checks (incl. map runs, installs,
scatter renders, **breakpoint pauses the map run at pixel 0**, compile errors,
back-to-strip) + 19 device-mode checks, all green; 65 Rust tests pass. Pushed to
the dev device as a hot asset reload (the map tab is playground-only, so it's
hidden in device mode).

## 2026-07-06 night — web UI Phase 2: tabs + decluttered header ✅

Phase 2 of the web-UI redesign — a real tabbed app instead of one crowded
header. Web-only (no firmware / no reflash); pushed to the dev device as a hot
asset reload, so http://192.168.0.205/ is already running it.

- **Tab bar in both modes.** `Editor · Patterns` in playground; device mode adds
  `Settings`. Panels stay mounted and hide via CSS, so the render loop, editor
  state, and gallery tile-engines all survive a tab switch.
- **Header decluttered** to just wordmark · tabs · status (fps + streaming/
  polling) · connection. Everything else moved out:
  - *Editor toolbar* (above the editor): pattern picker + `save`/`delete`,
    `share` (playground only, prominent), and a `⋯` overflow with import/export.
  - *Playback bar* (below the editor): layout (strip/grid/px — playground only),
    fps, pause, debug.
- **Patterns tab** — the gallery, promoted from a modal overlay to a first-class
  inline tab. Lazy-mounted on first visit then kept alive; picking a pattern
  jumps back to the Editor tab.
- **Settings tab** (device mode) — device address, live pixel-count readout, and
  status; brightness / pixel-count editing / WiFi are honest **Phase-3
  placeholders** (labeled as needing firmware — only `/api/wifi` exists today).
- **Share** hidden in device mode (it's a playground affordance), prominent in
  playground mode — per the feedback.

Both e2e suites were rewired to the new `data-role` hooks (`pattern-picker`,
`tab-*`, `overflow`, `map-badge`, `layout-*`, `cfg-pixels`, `pause`, `fps`) and
are **green in real chromium** — 51 playground checks + 19 device-mode checks
against the native mirror. Verified the served bundle on hardware: index.html
(revalidated) points at the new immutable hashed JS/CSS.

Still ahead (docs/webui.md): Phase 3 (firmware `/api/brightness` + `/api/config`,
wire the Settings fields, fix the connect-on-load race), Phase 4 (mapper as a
CodeMirror tab + debuggable, 3D preview, Playlist tab + firmware storage,
MQTT/HA, AP-mode).

## 2026-07-06 evening — HTTP caching: assets only re-download when changed (v0.1.11) ✅

You asked the device to take advantage of browser caching — no redownload of
unchanged assets. Done and verified live on hardware:

- **Content-hashed bundle files** (`/assets/index-<hash>.js`/`.css`) now serve
  `Cache-Control: public, max-age=31536000, immutable` — the browser reuses
  them with **zero network** until their hash (and thus URL) changes.
- **The unhashed files** (`index.html`, `luxel.wasm`, `gallery.json`) serve a
  **strong ETag** + `Cache-Control: no-cache`, so the browser revalidates with
  `If-None-Match` and gets a **304 Not Modified** (empty body) when unchanged —
  a full download only when the content actually changed.

Implementation: the LUXA archive gained a v2 format (`LUX2`) carrying an
8-byte SHA-256 content hash per file (packed host-side); the firmware serves it
as the ETag and answers `If-None-Match` with a 304. The firmware still reads
legacy `LUXA` archives (those assets just revalidate to a 200). No new stack
cost (stack-check green at 12 KB; clippy clean).

Verified on the device (v0.1.11, OTA'd onto ota_0): every asset returns its own
correct ETag; a matching `If-None-Match` → `304` with no body; a stale one →
`200`; `/assets/*` carries `immutable`, the rest `no-cache`. So a second visit
to http://192.168.0.205/ re-fetches nothing unless a file changed.

## 2026-07-06 evening — web UI feedback sorted + Phase-1 fixes ✅

You gave a big batch of web-UI feedback. First, **sorted into the docs so
nothing is forgotten**: the full redesign backlog now lives in
[docs/webui.md](docs/webui.md) (two modes: device console vs. playground;
tabs; settings page; header declutter — each item tagged with effort and
firmware-dependency), cross-referenced from ideas.md. Notable finding: a real
settings page needs new firmware (brightness/pixel-count/config endpoints,
MQTT) — only `/api/wifi` exists today.

Then **Phase 1** (web-only, no reflash — both e2e suites green in chromium):

- **Not a "playground" on a device.** The wordmark now shows the device
  (name/URL) in device mode, "playground" only when standalone. A `data-mode`
  hook is in place for the Phase-2 restructure.
- **Reconnect remembers the device.** Disconnect → connect no longer needs the
  URL re-typed; the last successful base is remembered (and persisted to
  localStorage), reused automatically.
- **"ws push" → "streaming"** (and "polling · N ms" for the HTTP fallback) —
  the old jargon meant nothing to users.
- **Debugger no longer lies on a live device.** A gutter breakpoint used to arm
  debug mode even when connected (the button was disabled but the gutter path
  bypassed it); it's now fully gated off in device mode.
- **Share hidden in device mode** — it only makes sense for the hosted
  playground.
- **Pattern-browser spinners** while a tile's preview is still computing.

The heavier items (tab restructure + header overflow menu, firmware settings
endpoints, connect-on-load race, mapper-as-editor-tab, 3D preview, playlists)
are phased in docs/webui.md as Phases 2–4.

## 2026-07-06 late — chunked patterns: larger than one flash page (v0.1.10) ✅

You asked for chunking so patterns aren't capped at one 4 KB flash page.
Done and hardware-verified:

- A pattern's source now splits across up to 4 chunk items (~3.8 KB each,
  ~15 KB total — 4x the old limit, and the practical ceiling since the 16 KB
  HTTP buffer bounds a POST there anyway). Each pattern also has a small meta
  item (name + chunk count + generation).
- **Atomic updates via generation flip:** an update writes new chunks to the
  *other* generation, then rewrites the meta (the single-item commit point) —
  a power loss before that leaves the old version fully intact. A **RAM
  index** (built at boot) keeps list/lookup off the flash.
- **Format-version marker:** boot wipes `storage` if the on-flash layout
  isn't the current one, so the incompatible pre-chunking items auto-migrate
  (saw `format 0 != 2, wiping storage` on the upgrade boot).

**Two bugs caught in hardware testing, both fixed:**
- A GET of a >4 KB pattern OOM'd (`allocation of 21880 bytes failed`): the
  old `format!(json_escape(..))` path built an ~11 KB intermediate plus a
  doubling result buffer → a ~22 KB contiguous request that fragmentation
  couldn't satisfy. Now escapes into one pre-sized string; `read_source`
  pre-sizes too.

Verified on hardware: byte-exact round-trip of 2-, 3-, and 4-chunk patterns
(7.7 / 10.8 / 15.1 KB, md5-matched), upsert returns the latest, small +
large patterns coexist, persistence across reboot. Larger-than-15 KB
patterns stay in the browser library (source stays there regardless).

## 2026-07-06 afternoon — pattern library verified on hardware (v0.1.7) ✅

You serial-flashed v0.1.6 (new factory-less table). Boot was textbook:
`Erasing storage (Data(Spiffs))…`, table shows `storage @ 0x210000`,
`booted from: ota_0`, `patterns: 0 stored`, flash creds + assets both
survived. Then full pattern CRUD verified on the real device:

- list/save/get/activate/delete all round-trip through flash; activate
  drives the pixels; missing-delete → "no such pattern".
- **Found + fixed a bug in testing (v0.1.7):** sequential-storage's
  `fetch_all_items` returns superseded item versions after an upsert, so
  the list showed a duplicate id — now deduped by key (`fetch_item` already
  returns the latest for reads).
- **Persistence:** OTA'd both slot directions on the factory-less table
  (ota_0↔ota_1, clean) and confirmed saved patterns survive the reboot —
  serial `patterns: 2 stored`, NEXT_SEQ correctly reseeded from stored ids.

Left two curated patterns in the device library ("simplex aurora", "beat
pulse", both using the new builtins) with aurora running. **Task #9 done.**

## 2026-07-06 midday — firmware pattern storage (v0.1.6); dropped factory; stack guardrails

Working with you awake. Two design calls you made, both shipped:

**Dropped the factory partition → dedicated `storage` partition.** You have
no distinct golden image (serial flash = the same build that ships OTA), so
factory was 1 MB of dead weight. New table (partitions.csv): pure A/B
(ota_0/ota_1, bootloader falls back to ota_0 when both fail) + `storage`
(0x210000, 1 MB) slotted ahead of the unmoved assets region. `ota.rs`
already read the table dynamically, so no logic change. **Applying it needs
one serial flash** — your normal `build-esp32.sh flash` runner already does
`--partition-table=partitions.csv --erase-parts otadata`, so it lays down
the new table, clears otadata (clean boot into ota_0), and preserves
nvs/assets.

**Firmware pattern library — task #9, the CRUD contract's device half
(v0.1.6).** New `patterns.rs`, built on **`sequential-storage`** (your call
over my first hand-rolled blob — it's the PLAN's storage model and matches
your "established crates" preference). Each pattern is one KV item (key =
u32 id, value = name+source) in the `storage` partition: a store/remove is
atomic, so a power loss mid-save loses at most the one in-flight pattern,
never the library — and it's wear-leveled. Routes match the mirror exactly:
`GET/POST /api/patterns`, `GET/DELETE /api/patterns/<id>`,
`POST /api/patterns/<id>/activate` — POST compile-checks before storing.

- **Safety guard:** `patterns::init` resolves the `storage` partition from
  the *live* table and disables the store (writes refuse, reads empty) if
  it is absent — so v0.1.6 is safe even on the current (old) table, where
  0x210000 is the live ota_1 app slot. No corruption; it lights up after
  your reflash.
- **Async/blocking bridge:** sequential-storage is async, esp-storage is
  blocking — a small `AsyncFlash` adapter forwards to the blocking methods,
  driven by `block_on` (it never truly pends). The flash driver is *leased*
  out of the OTA module per transaction (Drop-guarded) so its critical-
  section mutex is never held across the erases.
- **One caveat (FYI):** sequential-storage items must fit one 4 KB flash
  page, so a single pattern's source is capped at ~3.5 KB (clear API error
  above that; larger patterns stay in the browser library). If you want
  unlimited device-side pattern size later, chunking across keys would lift
  it — say the word. A serial reflash clears device patterns (they survive
  OTA updates; `--erase-parts` now includes `storage` for a clean region).

**Stack guardrails (so the original OTA-crash class can't recur):**
- `#![deny(clippy::large_stack_arrays)]` in the firmware (threshold 1 KB,
  `firmware/clippy.toml`) — turns a stray `[0u8; 4096]` on the stack into a
  hard `cargo clippy` error. Caught exactly this while writing patterns.rs.
- `tools/stack-check.{sh,py}` — builds with `-Z emit-stack-sizes` and fails
  if any function's frame exceeds a budget. Unlike clippy it sees *library*
  frames too (esp-storage's `FlashStorage::read` bounce buffer — the actual
  original culprit). Added `python3` to the flake for it.

Requires one serial flash of v0.1.6 (new table). Then I'll verify pattern
CRUD, creds, assets, and an OTA round-trip on hardware.

## 2026-07-06 morning — device back on v0.1.5; full checklist verified on hardware ✅

You reflashed v0.1.5 with creds. Device came up on `factory`, joined WiFi
via **compile-time creds** (no flash record yet), 300 px, 123 fps, no
vmerr. Then I ran the promised morning checklist end-to-end — all four
steps passed on real hardware, zero panics:

1. **WiFi creds → flash** — `POST /api/wifi` stored them in the `nvs`
   partition and rebooted. It came back with
   `wifi: joining "MOMCorp Intranet" (flash-stored creds)`. **Your
   partition ask is done and live**: creds now boot from flash, so future
   OTA images need no baked-in creds. Compile-time creds remain only as a
   last-resort fallback (a flash-wipe can't lock you out).
2. **Assets push** — `POST /api/assets` streamed the 431,755-byte LUXA
   archive (5 files) into flash in ~10 s, hot-reloaded, no reboot. The
   full playground now serves gzip'd at http://192.168.0.205/.
3. **New builtins on hardware** — live-coded a `beatSin`+`simplex2`+
   `setGamma` pattern via `/api/code`; pixels animate, `vmerr:null`. (FPS
   eases to ~87–99 under per-pixel simplex across 300 px — expected.)
4. **OTA round-trip** — pushed the 926 KB app image; device switched
   `factory` → `ota_0`, rebooted, came back clean at 124 fps. This is the
   exact path that used to crash 100% of the time. Serial log: **no panic
   since the reflash** (last crash in the log predates tonight's session).

The beatSin/simplex/setGamma demo is left running on the wall. Remaining
open item: **firmware pattern-library storage** (task #9) — still a
genuine flash-layout decision; notes below.

## 2026-07-06 ~01:30 — second overnight batch: simplex, setGamma, **, mapper, pattern library

All committed, tested (106 core tests, 40 e2e checks in real chromium),
and already in the v0.1.5 image + packed assets waiting for the device:

- **`simplex2`/`simplex3`** (`f1b7abc`) — fixed-point simplex noise,
  smoother than perlin, seeded, deterministic.
- **`setGamma(g)`** (`f95d805`) — output gamma as a cached LUT (zero
  per-pixel cost); `setGamma(2.2)` makes LED fades perceptually even.
- **`**` exponent operator** (`16d6f68`) — right-assoc, tighter than `*`.
- **Mapper (M3)** (`4f157d3`) — write a PB-style JS map function, apply,
  and the playground installs a real 2D map into the engine (new
  `lx_set_map` wasm export) with a live scatter preview. The ring-map
  rainbow screenshot is worth opening: e2e-6-mapper.png.
- **Pattern library + autosave** (`11d43a9`) — edits survive closed tabs;
  named saves live in a picker optgroup. This is the prototype UI for
  device pattern CRUD.
- Corpus re-run with everything new: **288/293 ok, zero regressions**
  (user globals correctly shadow new builtin names — checked explicitly).

**Morning checklist (after your one serial flash of v0.1.5):** I'll
detect the device and, automatically: POST /api/wifi (creds → flash,
lockout-proof forever), push the ~432 KB assets archive (new playground +
gallery + mapper + library UI, /api/assets, no reboot), live-code a
beatSin/simplex/setGamma pattern to verify the new builtins on hardware,
and run one OTA round-trip. Nothing needed from you beyond the flash
command in the note below.

Also shipped since that list (all mirror/playground-verified, waiting on
the device only for the firmware storage half): **mapper**, **pattern
library + autosave**, **device pattern CRUD** (save/load/delete patterns
*on the device* — mirror + playground done, firmware storage queued as
the one remaining piece), and a real **CORS preflight fix** (cross-origin
DELETE from the hosted playground to a device by IP). Everything is in
git; `git log 94e8c52..HEAD` is the full night. Full workspace tests +
both browser e2e suites are green.

## 2026-07-06 ~11:30 — overnight progress: builtins batch 2, .epe UI, pattern browser

All hardware-independent, all committed, all verified locally (tests +
real chromium):

- **Builtins batch 2** (`6745a38`): `beat`/`beatSin` (tempo without
  audio), `hash`/`hash2` (stable per-pixel randomness, pinned lowbias32),
  `blur1D`/`feedback` (the trails/fire idioms as builtins),
  `dot`/`dot3`/`angleBetween`, and value-returning color —
  `hsv2rgb`/`rgb2hsv`/`mixColors(..., out)` with mixColors blending in
  OKLab. 101 core tests green. Also: array literals turned out to already
  work — ideas.md refreshed.
- **.epe import/export in the playground** (`463fa54`): import button +
  drag-drop anywhere + export download (PB-compatible shape). e2e covers
  a real chromium download round-trip.
- **Pattern browser** (`0d87f8f`): your gallery idea — 192 live tiles
  (built-in examples + every corpus pattern that compiles clean), 1D as
  bars / render2D as 16×16 rectangles exactly per your spec, viewport-
  lazy so it stays light. Click a tile to open it. It looks genuinely
  delightful — see e2e-5-gallery.png in the scratchpad shots.

Web bundle: 194 KB gz JS + 491 KB gallery.json (raw; ~150 KB gz over the
wire, lazy-fetched only when the browser opens — device flash region has
plenty of room). The new builtins + wasm + gallery need the next assets
push / firmware flash to reach the device; your morning serial flash
picks up ALL of it in one go.

## 2026-07-06 ~10:00 — ⚠ MY MISTAKE: device is offline on ota_0 (needs one serial touch when you're up)

Right after proving OTA works, I pushed a build of the new builtins from
MY shell — which doesn't have your `LUXEL_SSID`/`LUXEL_PASS` exported —
so the image baked **no WiFi creds** and booted into offline render-only
mode. The device is healthy (rainbow on ota_0, serial confirms "no wifi
credentials; offline mode") but unreachable over the network, so I can't
fix it remotely. This is the exact "hard lesson" from 2026-07-05 that I
had recorded and failed to apply. Sorry.

**Recovery (one step, when you're up):**
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```
That flashes the current build (**v0.1.5**) — which now includes the
batch-2 builtins, auto-baked creds (see below), AND flash-stored WiFi
credentials: the moment it's up I'll `POST /api/wifi` once and the creds
live in the nvs partition forever — after that even a credless image
joins the network, so tonight's failure mode is structurally dead. Alternative if you prefer not to flash:
`espflash erase-region 0xd000 0x2000` clears otadata → boots factory
(v0.1.4 with creds) and I'll OTA the rest myself.

**So it can't happen again (committed):**
- `firmware/creds.env` (git-ignored) now holds the dev creds;
  `build-esp32.sh` auto-sources it — every build bakes creds no matter
  whose shell runs it.
- `tools/ota-push.sh` now **refuses to push any image that doesn't
  contain the SSID string** (grep -a for it in the binary).
- The real cure stays queued: NVS-stored credentials so images never
  carry creds at all.

## 2026-07-06 ~09:40 — v0.1.4 flashed (thanks!) — OTA fully verified, remote work unblocked

After your serial flash, hardware-verified the fix end to end:
- The 190 KB JS + 87 KB wasm (the files that used to stall/crash) served
  repeatedly, cleanly, ~1.5 s / 0.8 s each. Zero panics.
- Full OTA round-trip: factory → ota_0 (904 KB, ok) → then a second OTA
  **while a loop hammered asset + status requests** → ota_1, ok. The
  worst-case combined load that used to be instant death now just works.
- Serial log: zero new panics since the flash.

Device: ota_1, v0.1.4, ~120 fps, heap_free ~72 KB. The A/B loop is
proven again and I can push everything else tonight over OTA. Carrying on
with M3 + the ideas backlog; log entries below as they land.

## 2026-07-06 ~09:00 — OTA crash ROOT CAUSE found + fixed (v0.1.4) — this supersedes the 08:15 note

You were right that something was fundamentally wrong. The serial log had
24 panics and every single one is the same signature: **stack overflow on
the main task** ("write to the stack guard value on ProCpu"), caught during
a flash read in the HTTP serving path.

Three compounding causes:

1. **The main task stack was 15.6 KB, by accident.** esp-hal gives the
   main task "whatever RWDATA is left after .data/.bss" — and our 120 KB
   heap static ate almost all of it. That one stack runs the entire
   embassy executor (every task's poll), picoserve's deep response path,
   AND the WiFi level-6 NMI frames, which land on whatever stack is
   current. Measured in the ELF: 15,596 bytes. The captured panic's SP was
   already 1.7 KB *past* the stack end.
2. **esp-storage's `FlashStorage::read` puts a 4 KiB sector bounce-buffer
   on the caller's stack** — every asset chunk we served pushed 4 KB onto
   that already-deep stack at maximum depth. (This is the "library
   function" trap: the convenient `Storage::read` API is the wrong one;
   `read_nor` with aligned offset/len/buffer reads directly, zero stack.)
3. WiFi NMI on top of 1+2 → guard hit → panic → reboot. It looked like
   "OTA crashes" because OTA sessions are exactly when the page/status/
   asset traffic and flash ops coincide; the erase-on-write fix (v0.1.2)
   was real but treated a different, secondary hazard.

The v0.1.4 fix (firmware only, two small changes): heap 120→96 KB so the
main stack is now **60 KB** (measured in the ELF: 61,712 bytes; heap_free
still ~70 KB), and `assets::read_chunk` now uses `read_nor` through a
word-aligned heap staging buffer — no more stack bounce-buffer in the
serving path.

On your "reinventing the wheel" hunch: partially right, wrong culprit.
ota.rs does hand-roll erase/write at raw offsets where
esp-bootloader-esp-idf's `OtaUpdater::next_partition()` hands you a
bounds-checked `FlashRegion` — worth cleaning up later — but that code
wasn't the crash; the reads/stack were. Queued the cleanup.

**I attempted OTA of v0.1.4 myself** (fails safe: a crash mid-upload just
reboots to the current slot). Check /api/status — if it says 0.1.4, go to
bed, nothing needed. If it still says 0.1.3, one serial flash:
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```

## 2026-07-06 ~08:15 — ⚡ PLEASE FLASH THIS before bed (v0.1.2, task-16 fix)

You asked what I need flashed to unblock M3 remote work. This is it:
**`67f2111` fixes the OTA-vs-flash hazard** (erase-on-write interleaved
with network reads instead of a watchdog-tripping pre-erase burst; plus
a Timer-yielded asset-serving path). Once this is on the device, I can
OTA everything else remotely — settings, AP provisioning refinements,
all the ideas.md work — without needing you.

Flash command (creds already baked, so it just connects):
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```
(the runner erases otadata → boots the flashed image; leave the monitor
running as usual). It's v0.1.2 — `/api/status` will show that once up.

Fails safe as always: if the erase-on-write change has any issue, a bad
OTA just reboots to the current slot; the flashed build itself connects
and live-codes regardless (compile-time creds unchanged). **AP-mode
provisioning still needs your phone to verify** (a device with no creds
leaves WiFi, so I can't reach it) — I'll build it and OTA it, but final
sign-off is yours when you're up.

Correction to my earlier note: the build you flashed a few messages ago
was HEAD *without* a task-16 fix (I'd described it but hadn't written it
yet). THIS commit is the actual fix.

Also shipped tonight while you read this: OKLCH/OKLab perceptual color
builtins (top visual-quality item from ideas.md).

## 2026-07-06 ~07:45 — extension builtins + idea backlog

Acting on your "extend beyond PB" note: wrote `docs/ideas.md` (a ranked
backlog across builtins / language / engine / audio / integration /
playground, with your animated-pattern-browser idea and its
1D-bar-vs-2D-rectangle preview distinction), and shipped the first
cheap-but-high-value batch of builtins: `map()`, `sign`, `step`,
`saturate`, `dist`/`dist3`, and the six `easeIn/Out/InOut` `Quad`/`Cubic`
curves. All pure math, so no PB pattern is affected (new names PB
doesn't have). They're in the VM, autocomplete, and language docs, with
tests. My top-5 picks are in ideas.md — `map`+easing+oklch color helpers
and array literals are the highest authoring-quality-per-effort; I2S
mic/FFT and MQTT/HA are the headline features when we want them.

## 2026-07-06 ~07:15 — const/let landed; caught a real playground bug

`const` and `let` now work (your request). `let` == `var` for now
(function-scoped; block scoping later); `const` needs an initializer and
errors on reassignment, correctly scoped (a local const doesn't lock a
same-named global). Full stack: lexer → AST → parser → compiler, 3 new
tests, editor keyword completion, and docs/lang.md updated with a
"Luxel extensions" section. All 32 semantics tests green.

Writing the e2e for it surfaced a genuine bug I'd introduced with the
device-served playground: the auto-connect probe fetched `/api/status`,
and a dev server answers that with its SPA fallback (200 HTML), so the
local playground tried to "connect to a device", failed, and left a
stuck "device unreachable" banner. Fixed to require real device JSON.
Good catch — it would've bitten the normal local playground, not just
the test.

Queued your other note (extend builtins/features/language — the "not
limited to PB" thesis) as a design task for a docs/ideas.md brainstorm.

The device is back (you re-flashed it) and healthy; I've deliberately
kept my remote testing light after the earlier WiFi-wedge, and the
flash-vs-WiFi fix (task 16) is the thing that unlocks safe remote
iteration again.

## 2026-07-06 ~06:30 — full playground serves from flash (works!) + an OTA regression to fix, and I've STOPPED touching the device

Big win and an honest problem, both real.

**Works, browser-verified against the real PB:** the whole Svelte
playground now lives in the device's flash and serves from it. New
pieces: an "assets" flash partition (0x310000, 960 KB) behind the OTA
slots; a `LUXA` archive format (`web/tools/pack-assets.mjs` packs the
gzipped `dist/` — 279 KB, 4 files); `POST /api/assets` streams it in
(independent of firmware OTA, hot-reloads the TOC, no reboot); the
server serves `/`, `/luxel.wasm`, etc. from flash with
`Content-Encoding: gzip`, embedded minimal page demoted to `/min`; and
the playground auto-enters device mode when same-origin `/api/status`
answers. Chromium loaded `http://192.168.0.205/` end to end: editor up,
device badge, 300-px preview, ~100 fps. Bumped the connection pool to 3
(safe now the stack overflow is fixed) so the preview socket doesn't
starve page/asset loads.

**Two real bugs, same root cause — the ESP32 flash-vs-WiFi hazard:**
1. Serving a *large* asset (JS 190 KB, wasm 87 KB — multi-chunk) stalls:
   interleaving esp-storage flash reads with WiFi TCP writes starves the
   executor, so the second `write_all` never drains. Single-chunk files
   (html, css) serve fine. Tried 8 KiB chunks + `yield_now` between
   writes — not enough.
2. **OTA now trips a hardware watchdog** during the erase phase when
   assets are installed: the device resets (no panic — a watchdog, not a
   Rust panic) ~14 s in, connection reset. It **fails safe every time** —
   always reboots back into the working build — but it means I can no
   longer push updates remotely, and I can't fix a running firmware's OTA
   path *via* OTA (chicken/egg).

**Decision: I stopped experimenting on the device.** It's in a stable,
fully-working state (serves the page, WS, live-code, small assets — all
good). Continuing to hammer it risked leaving it wedged for you, and I
was starting to see transient wedges under my own back-to-back tests.
The right fix for BOTH bugs is the standard one: **memory-map the assets
flash region** and read via the cached data bus instead of esp-storage
flash-controller ops — no cache-off windows, no WiFi starvation. That's
a clean next-session task (needs a serial flash to land, since it
changes the running OTA path). The source for everything above is
committed and sound; only the runtime flash-timing needs that rework.

**To get back to a fully-updatable device:** one serial flash of a build
with the mmap fix. Until then the PB happily runs what's on it.

Everything else tonight (debugger fixes, autocomplete, bidirectional WS,
language docs) is landed, hardware-verified where relevant, and
independent of this.

## 2026-07-06 ~05:15 — WS verdict + bidirectional multiplexing live on hardware (8daf311)

After your last flash the fixed build held: five status hammers, zero
panics, heap 121 KB free. Then the A/B you asked for, on the real PB:

|            | HTTP polling | WS push |
|------------|--------------|---------|
| rate       | 10.0 fps     | 9.7 fps |
| gap p50    | 87 ms        | 96 ms   |
| gap p90    | 143 ms       | 130 ms  |
| gap p99    | **394 ms**   | **185 ms** |

Same average rate, but the tail — the visible stutter — is halved.
Combined with freeing a connection slot, WS stays.

Your bidirectional suggestion then proved *necessary*, not optional:
with the push socket pinning one of the chip's two connections, extra
HTTP callers starved (reproduced: "fetch failed" under mixed load). Now
one socket carries everything — pixel push down, API calls up
(`"<id> call\nbody"` → `{"id":…,"r":…}`), playground multiplexes
transparently with HTTP fallback. The native mirror was rewritten from
tiny_http to a hand-rolled std HTTP layer so its /ws loop is
structurally identical to the firmware's (single-threaded full-duplex).

Hardware-verified end-to-end: 6 live-code pushes + control sets +
pattern fetch over one socket while pixels streamed and a concurrent
plain-HTTP request succeeded; zero panics. 12/12 device e2e, 25/25
local e2e, all tests green. Serial flashing also hardened along the
way: `flash` now erases otadata (the image you flash is the image that
boots — the mismatch that caused the 4 AM round).

## 2026-07-06 ~02:20 — INCIDENT: device down after WS OTA (my fault); recovery staged

The WS-push build OTA'd fine but the device never came back. Doing the
arithmetic I should have done first: bumping the server pool to 3 tasks ×
~32 KB buffers on top of WiFi's ~90 KB almost certainly exhausted the
ESP32's 184 KB heap at boot → allocation panic → and esp-backtrace's
default behavior is to HALT, so one panic = a brick until someone touches
power. You were asleep, serial detached; nothing I can do remotely — the
one failure mode OTA can't recover, found the hard way.

**When you're up: power-cycle the PB once.** A persistent watcher pushes
the staged recovery build (pool back to 2, smaller buffers, and — the
real fix — panics now reboot after 3 s instead of halting forever)
automatically the moment the device answers. If a power cycle alone
doesn't bring it back (crash loop), it'll still recover: each loop
iteration reboots through a WiFi window. Absolute worst case:
`espflash erase-region 0xd000 0x2000` over serial clears otadata → boots
factory.

Meanwhile: docs/lang.md (full language reference) written; continuing
with hardware-independent work (bidirectional WS protocol + remote
debugger against the native mirror, const/let).

## 2026-07-06 ~01:45 — WS pixel push implemented, hardware A/B in progress

Implemented across all four surfaces (firmware /ws, native mirror,
playground, minimal page): binary frames = pixels ~15 Hz, text frames =
typed JSON (status+controls, vars/readouts). 12/12 device e2e green.

Per your note, measuring on the real PB before keeping it. Baseline
captured first, HTTP polling against the previous firmware:
**10.0 fps, gap p50 87 ms / p90 143 ms / p99 394 ms** — that jitter is
the choppiness you saw. WS build OTA'd (861 KB, no size tricks needed
now); push measurement running.

Your bidirectional-WS point is sound and the groundwork exists:
picoserve's next_frame takes a cancel signal that safely unblocks
between frames, so one socket can carry API calls + push concurrently.
If the numbers favor WS, I'll multiplex code/control/var (and later the
remote-debug protocol) onto the same connection.

## 2026-07-06 ~01:00 — editor autocomplete (140a8c6)

Typing pops completions for all 86 builtins with full signatures and
one-line docs (new `web/src/lib/builtins.ts`, kept in sync with the VM
table — it'll also seed the language reference). Predefined constants
(PI, pixelCount, GPIO modes…), keywords, and identifiers you've already
used in the pattern complete too. Verified in chromium; both e2e suites
still green.

Also queued your `const`/`let` language-extensions request as a task —
it needs compiler work (block scoping, assignment-to-const errors), so
it's sequenced after the device-facing items tonight.

## 2026-07-06 ~00:30 — debugger notes resolved (060884b)

**Your `heat` question: the debugger is right.** In Palette Fire 2D, `heat`
is assigned inside `render2D` *without* `var` — in PB's JS-derived scoping
(and ours), undeclared assignment creates a global. `var` inside a function
would make it local. Pinned with a test so it stays true.

**Breakpoint line mismatch: real bug, fixed.** Resolution only matched
exact lines, so a breakpoint on a blank/comment/brace line silently
installed *nothing* (and the dot pointed at a line that could never stop).
Now it snaps forward to the nearest executable line and the gutter dot
moves there — dot and stop-line always agree. While hunting this I
verified the rest of the chain is correct: pause happens *before* the
stopped line executes (so its variables show pre-execution values),
stepping tracks across beforeRender→render→pixels, and the paused line
scrolls into view.

Running log of autonomous work, newest first. Started the night of
2026-07-05 when Jeremy went to sleep ("keep going with less intervention").

## 2026-07-06 — overnight session begins

Queue for tonight, from Jeremy's list + existing plan, in intended order:

1. Debugger correctness pass: `heat`-as-global question + stopped-line
   highlight (his two debugger notes).
2. Editor autocomplete for builtins.
3. WebSocket pixel push (device + native mirror + playground + minimal page),
   then OTA to the PB and measure against HTTP polling.
4. NVS-stored WiFi credentials (kills the creds-baked-image lockout risk).
5. Pattern language docs.
6. Remote debugger design (+ implementation as far as the night allows).

Device state at session start: PB v3 at 192.168.0.205, slot ota_1, v0.1.1,
124 fps, rainbow restored after the live-code green test. OTA loop fully
proven (factory → ota_0 → ota_1 + bootloader-fallback test passed).
