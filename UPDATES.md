# Update log

## 2026-09-07 — chasing a skip that three counters say does not exist (#395)

Jeremy re-filmed the merged build and still saw an occasional skipped sweep
column, with a sharp observation: on his 240 fps video every other camera frame
is normally dark, but at the skip two lit columns land in ADJACENT frames. At
brightness 3 that is a real constraint, because the lit window is tiny and
precisely placed.

**Where the panel is lit.** `scale5(255, 3) = 25 = 0b00011001`, so a
full-brightness pixel sets colour bits 4 and 3, which are planes 3 and 4, which
are descriptors **224-247 of 254** — the last 12 % of the ring, 7.67-8.49 ms
into an 8.70 ms pass, ending 0.21 ms before the `suc_eof` tail where a swap
takes effect. The lit stretch is 0.82 ms, a 9 % duty cycle, immediately before
the switch point.

**What adjacency actually proves.** A rescan is 8.696 ms and a camera frame
4.167, so consecutive lit stretches normally land 2 or 3 camera frames apart,
never 1. For two to land in adjacent frames the gap must be under 8.333 ms —
i.e. the pass short by at least 363 us, about 11 descriptors. That is real
evidence of a short pass, but it bounds it far more weakly than it first
appears: the "under 4 ms" reading would need ~137 descriptors skipped, and
skipping the dark tail planes 5+6 entirely is only 205 us, not enough to show
as adjacent at all.

**New instrumentation**, `/api/status` `pass`: pass length min/max/nominal,
`short` (under 0.9x) and `long` (over 1.5x) counts, `per_frame_min`/`max`
rescans between displayed frames, `zero_rescan`, and a log of recent short
passes with flags saying whether a swap was armed in that pass, whether the
previous EOF restored a tail, whether the swap took the fast or the two-EOF
landing, and whether an `eof_race` happened within two passes.

**The first version of that instrument was wrong, and said so loudly.** It
reported 30 short passes, the shortest 3.4 ms against an 8.7 ms nominal — but
most carried flags of 0, meaning no swap was armed during them at all. The
timestamps are taken inside the frame-count ISR, so its dispatch jitter lands
straight in the interval. At an EOF the engine has just wrapped to a ring head,
so `OUT_DSCR`'s offset from that head measures the latency in descriptors;
correcting by it took `short` from 30 to **0** and `min_us` from 3,351 to
7,316. The measured worst-case dispatch latency is **210 descriptors = 7.2 ms**
— itself a finding, and the same core-0 contention #395 is about.

**Result over 660 s with zero polling — 76,141 consecutive passes:**

| metric | delta |
|---|---:|
| `pass.short` (ring truncation) | **0** |
| `pass.long` (missed/coalesced EOF) | **0** |
| `zero_rescan` (frame never scanned out) | **0** |
| `dropped` (composed, never displayed) | **0** |

`per_frame_min` 1, `fps` 112 == `out_fps` 112 against `rescan_hz` 115,
`fence_timeouts` 0, `vmerr` null. `eof_race` 27 (one per 24 s), `slow_path`
407.

**And a host simulator agrees.** A descriptor-ring model of the swap/ISR
sequence over random request phases finds zero short passes across prefetch
depths 1-8, ISR latencies to 300 ticks, and a margin of 1 — and shows that the
two mechanisms that *could* truncate a pass do not produce this signature: a
DMA restart yields LONG passes, and rebuilding a ring under the engine yields
neither. Nothing in the driver writes a mid-ring `next`, so entering a ring off
its head has no mechanism.

So every firmware-side explanation for a lost displayed frame is now excluded
by direct measurement. The next discriminator is free and lives in the video
Jeremy already has: `frameParity` flips every composed frame, so consecutive
DISPLAYED columns must alternate red/green. Two adjacent lit columns of the
**same** colour is a genuine skip; **different** colours means they were
consecutive frames and the jump is a miscount. That settles it without trusting
any timing argument.

## 2026-09-07 — `tools/panel-load-bench.mjs`, and why #395's interrupt executor is not happening

With tearing (#376) and dropped frames (#387) gone, the artefact left on the
panel is the **repeat**: a frame shown for two rescans. The cause is arithmetic
— the compose eats 7.4 ms of an 8.7 ms rescan, leaving 1.3 ms of slack, and
`pipeline::output_task` shares a cooperative executor with the web server, so
any handler that runs longer than that without yielding pushes the swap past
the wrap.

**New harness: `tools/panel-load-bench.mjs`** (docs/tools.md). Two phases
against one build — idle (status every 10 s) and busy (N clients looping
`/api/patterns` + `/api/status` + the playground bundle) — reporting
**repeats/min** as the integral of `rescan_hz − out_fps`, next to the #394
`dropped`/`eof_race`/`slow_path` deltas, `/api/status` latency p50/p95/max,
load latency and throughput, heap floor, `fence_timeouts` and a reboot check.
GET-only. Run `--label before` and `--label after` and diff.

Two things that make it trustworthy rather than merely plausible. The repeat
integral is normalised by the span it **covers**, not wall time: the phases
sample at different rates on purpose, and wall-time division silently
under-reports the sparse one — on a mock whose true rate was 300 repeats/min
for both, it read 150 for idle against 250 for busy, an error pointing the
same way as the hypothesis. And the bundle path is discovered from the served
index rather than hardcoded, so a rehashed build cannot quietly turn the
measurement into a 404 timing run. Validated end to end against a local mock
server before ever being pointed at hardware.

**#395's proposed fix — moving `output_task` to an `InterruptExecutor` — was
investigated and rejected**, before any firmware was written and without
touching the device, because getting it wrong takes WiFi down and the panel is
then only recoverable through hands-on serial. Two independent blockers at the
pinned esp-hal rev `7c7f372`:

- **"Below the WiFi driver's priority" is unrepresentable on the S3.**
  `WIFI_MAC`/`WIFI_PWR` are at `Priority1`, the minimum on Xtensa
  (`esp-radio/src/wifi/os_adapter/esp32s3.rs:56-57`), and so are the esp-rtos
  scheduler tick, the `Software0` context switch and the cross-core yield.
  Priority2/3 preempts the radio outright. Priority1 is *worse*: esp-hal's
  dispatcher never lowers `PS.INTLEVEL` before invoking a handler
  (`esp-hal/src/interrupt/xtensa.rs:462-487`), so a 7.4 ms compose every
  8.7 ms would mask every other level-1 source — the radio's own ISRs, the
  scheduler, the context switch — at **~85 % duty with 7.4 ms worst-case
  latency**.
- **No free software interrupt.** `InterruptExecutor::new` needs an owned
  `SoftwareInterrupt<0..=3>`; the S3 has exactly four and all are taken
  (`esp_rtos::start` 0, `start_second_core` 1, the two flash-fence park
  handlers 2 and 3). Freeing one means merging the fence's park handlers —
  surgery on the #294/#309 fence, needing verification on the S3 *and* the
  classic ESP32.

#395 is retitled to the goal it actually names — "web load must not cost the
panel a frame" — and its carrier is **#329**, the row-oriented packer: a
~1.5-2 ms compose turns 1.3 ms of slack into ~6.7 ms, five times the
tolerance, attacking the cause instead of redistributing the symptom, with no
radio risk.

## 2026-09-07 — HUB75: the panel is the clock (#387), out_fps counts displayed frames (#378), setFrameRate carries its remainder (#384)

With the swap made frame-atomic earlier today (#376) the panel still showed
fewer frames than the firmware composed, and `/api/status` did not admit it.
The render loop ticked every 8 ms (125 fps) against a panel that rescans every
8.7 ms (115 Hz), and the two clocks beat.

**How much was being lost was worse than it looked.** `out_fps` counted every
`write_frame` **call**, refusals included. The tell was `out_us`: 3,804 µs
averaged over 123 "frames" against a true compose cost of **7,400 µs** at
4096 px — about half those calls did nothing. And a refused call did not retry
until the render loop came round ~8 ms later, so a compose that could have
started at the rescan boundary typically started well after it, missed the
next boundary and landed a whole rescan late. The panel was displaying about
**60** frames a second while `out_fps` said 123.

The fix makes the hand-off buffer the clock, with no timer in the path. Three
pieces, all HUB75-only (a strip is wire-bound and keeps the 8 ms floor):

- `OutputDriver::write_frame` now returns whether it wrote. `out_fps` counts
  only those, so it means the same thing on every board (#378).
- The output task **holds** a frame until the previous swap has landed rather
  than composing into a buffer about to be overwritten — composing from the
  boundary is what gets the next swap armed before the following boundary.
- The render task's `emit` **waits** for the travelling buffer instead of
  dropping the frame, and takes only a genuinely free one — never the
  newest-wins steal-back, which let the loop run a frame ahead whenever the
  output task's wake was late (`fps` 118 against `out_fps` 112 in the first
  cut). The VM still overlaps the compose because the wait is AFTER the
  pattern has run, not before it.

50 ms caps on both waits are a liveness floor, not the pacing: a dead panel
must not freeze the engine, the pattern clock or `fps`.

On the panel with `library/frame-rate-scan.js` at ComposeCap 0, before →
after: `fps` 124 → **106–112**, `out_fps` 113–120 (a call count) → **106–113**
(displayed), `rescan_hz` 115 → 115, `vm_us` 831–881 → 868–931, `frame_us`
879–943 → 935–1,024. **`fps` and `out_fps` now track each other frame for
frame** — nothing is composed only to be thrown away — and both sit a few
percent under `rescan_hz`. That gap is the compose itself: 7.4 ms of an 8.7 ms
window leaves 1.3 ms of slack, so one occasionally overruns and the panel
repeats a frame. A repeat is not a skip. `bulk-comet-trails` over three
minutes: `fps` 107–115, `out_fps` 106–115, heap flat at 42–50 KB,
`fence_timeouts` 0, `vmerr` null. `rainbow`, the render-bound case at 19.5 ms
per VM frame, is unchanged at 51–52 — the pipeline still overlaps VM and
compose.

Alongside it, **#384**: `Engine::frame` zeroed the frame accumulator whenever a
capped frame fired, so the achievable rate quantized to the caller's tick rate
over a whole number — `setFrameRate(100)` against the 8 ms loop delivered 62.5,
and nothing between 62.5 and 125 was reachable. It now subtracts the period,
clamped to one period so a long stall banks at most one catch-up frame. Six
caps are covered by a new luxel-core test; docs/lang.md documents the residual
one-tick jitter rather than hiding it.

Costs on `board-seengreat-hub75`: app image 951,264 → 953,648 B (+2,384,
including #384), `.stack` 30,268 → 30,204 B, stack-check green.
`board-pixelblaze-v3`, which gets none of this, pays +400 B for the
`write_frame` return alone: the `.await` sits behind an `emit!` macro that
expands to a plain call off the pipelined path, because making the direct sink
async too cost every non-panel board ~864 B of state machine for a future that
never yields (#160 — that board has 3.7 % of its OTA slot left).

**Then Jeremy filmed it and still saw a skip** — "much better. I have observed
repeats (not many). I also observed (sadly) a skip." Repeats are expected. The
skip was a second bug, in the #376 swap-landing shortcut, and it is the kind
that no frame accounting can catch.

`swap()` decides whether the next `out_eof` is the ring switch by checking the
DMA is at least three descriptors short of the ring tail — if so the tail
cannot have been fetched yet and must read the `next` just written. But "short
of the tail" is equally true immediately **after** the DMA wrapped past it,
and there the tail was fetched *before* the store: the pass does not flip, yet
the EOF it already raised is miscounted as the switch. The compose is then
handed a framebuffer the DMA is still scanning out, and the ISR "restores" the
old ring's tail and undoes the flip. One frame both torn and never displayed.
The window is "an EOF has fired and its ISR has not run yet" — wide open
inside `swap()`, because `critical_section` masks interrupts while it runs.
`write_frame` succeeded, so nothing downstream could see it.

Closed by probing `OUT_INT_RAW.out_eof` alongside `OUT_DSCR` and taking the
two-EOF fallback whenever an EOF is pending. Measured, not guessed:
`swap.eof_race` counts entries to the window — **41 in 725 s, one every
17.7 s, 1 frame in ~1,950**. That is exactly the rate of "I observed a skip,
maybe there was more, I stopped watching". `swap.slow_path` (the two-EOF
fallback, any cause) ran 318 times, 0.4 % of swaps, at no measurable
throughput cost.

To separate a real drop from a camera that missed a frame, `/api/status`
gained **`dropped`**: rendered frames the fixture never showed, derived from
the gap between the sequence numbers of consecutive *displayed* frames rather
than by counting known loss routes — so it covers routes the firmware does not
enumerate. `drops` breaks out the known ones (`handoff`, `overwrite`,
`refused`), bins losses by frame number mod 64, and keeps the last 16 as
`[seq, route, ms]`.

Over 725 s of `frame-rate-scan` with **zero polling** from the host (~79,750
frames), `dropped` moved by **0**. Every drop the board has recorded is a boot
transient — 9, all `handoff`, all within the first 3.8 s, before the output
task publishes the driver's pacing capability. The histogram holds only those
nine, in bins 1–8 and 10: no clustering, nothing in 54–63 where a right-edge
fault would show. Host control: 640 frames of `frame-rate-scan` at three
cadences give `missing = 0`, `multi = 0`, every column lit exactly 10 times
including 54–63 — the pattern never fails to draw a column.

Harness note: `tools/ota-push.sh` failed silently four times on this board
this session (`curl -sf` to `/api/ota` returning non-2xx, `set -e` ending the
script before its status poll, so the only symptom is output that stops after
"pushing N bytes…"). A plain `curl --data-binary @app.bin` of the same image
seconds later returned `200 {"ok":true}` every time. Two of the failures were
concurrent with a background `/api/status` poll loop — worth not doing during
a push.

## 2026-09-07 — HUB75: the framebuffer swap is now frame-atomic (Gitea #376)

Jeremy asked how tearing is avoided when the framebuffer is fetched by DMA.
It was not: esp-hub75 0.14's `circular-dma` swap rewrote **every descriptor's
`buffer` pointer** by the old→new delta the instant `swap()` was called, while
the DMA was mid-pass, and its own SAFETY note conceded "one partially-mixed
frame". The ring is ordered by plane repetition, so the mix was the high
bitplanes of one frame with the low bitplanes of the next — colour corruption
at moving edges. Confirmed on camera with `library/frame-rate-scan.js`: at a
sweep-column change, two columns lit at once, one of them partially.

Fixed with a new patch file, `firmware/patches/esp-hub75-0.14.0-atomic-swap.patch`
(carried as a patch + flake derivation per firmware/patches/README.md, never a
vendored tree). **One descriptor ring per framebuffer.** Each ring's tail
`next` loops to its own head; a swap is ONE naturally-aligned 32-bit store
rewriting the running ring's tail `next` to the other ring's head. The DMA
reads `next` only when it finishes a descriptor, and the tail is the last
descriptor of a full BCM pass, so the switch lands exactly on a panel frame
boundary — every pass reads exactly one framebuffer, and no `buffer` pointer
is ever touched while the DMA is inside the ring. The frame-count ISR restores
the ring the DMA left so it is self-contained for the next swap.

Landing is *observed*, not assumed, because the store can lose the race with
the DMA's prefetch of the tail (in which case the flip lands one frame later —
still never a mixed frame). `swap()` reads the GDMA `OUT_DSCR` register: if the
engine is still ≥3 descriptors short of the tail, the tail must read the new
`next`, so the very next `out_eof` is the switch; otherwise fall back to two
EOFs. `out_eof` means "read from memory", so under either proof the old
framebuffer is free when it is handed back. This is a throughput property too:
a first cut that only had the two-EOF rule settled at **52 fps** against the
115 Hz rescan, because every swap cost two panel frames.

Costs, measured (`board-seengreat-hub75`): descriptors 254 → 508
(`__DESC_CELL` 3,052 → 6,100 B), taken out of the leftover `.stack` region
(33,372 → 30,268 B; `tools/stack-check.sh` green, largest frame 9,648 B).
Flash +784 B — app image 950,480 → 951,264 B, 97,312 B (9.28 %) of the OTA
slot free. Heap untouched.

On the panel (4096 px, 30 MHz, `frame-rate-scan` at ComposeCap 0), before →
after: `fps` 124 → 124, `out_fps` 119–123 → 113–120, `rescan_hz` 115 → 115,
`vm_us` 843–857 → 831–881, `frame_us` 893–912 → 879–943, `vmerr` null,
`fence_timeouts` 0. Identical within noise, as it should be — the swap was
already only a handful of stores. What changed is what the panel *shows*,
which no API field reports.

Still open, deliberately: composed frames are still **dropped** (at 124
composed against 115 rescans about one in fourteen never reaches the panel) —
pacing the render loop on the panel frame boundary is Gitea #387 — and
`out_fps` still counts `write_frame` calls rather than displayed frames
(Gitea #378).

Two harness lessons worth the ink. Switching the gitignored
`firmware/vendor/esp-hub75` symlink to a different nix store path does **not**
invalidate cargo's fingerprint (every store file's mtime is 1970), so an A/B
across two versions of a crate patch silently reuses the stale rlib —
`cargo clean -p esp-hub75` between builds, or the baseline is a lie. And every
OTA reboot on this board drops the live-pushed pattern and comes back on the
persisted default (Rainbow), whose 52 fps at 4096 px reads exactly like a
throughput regression until you check `GET /api/pattern`.

## 2026-09-07 — the compaction fix verified on metal, and what it did not fix (#379, #365, #363)

The #379 fix (PR #383) went onto the Athom rig — master `81eb873`,
`board-athom-music`, v0.1.40 — and was checked against the damaged store the
finding session deliberately left in place, **without wiping it first**.
About **310 saves and 86 deletes**, the full pattern name set audited after
every single one, across 20+ compactions with and without a pin: **zero
files lost, zero corrupted, every read-back byte-identical.** The data loss
is fixed.

The fixed boot scan also **recovered a file the old one had swallowed**.
The store came back with 21 patterns where 20 went in: an orphaned older
generation of `1-white-fade`, byte-identical to its `library/` source, that
had been sitting under a stale header the old `at = rec.end()` walk stepped
over. It left the store holding two live records with the same name and
different seqs — defect 2's fingerprint, since the save that should have
superseded it could not see it via `rec_by_name`.

The original reproduction is clean now: a full `library/` fill accepts
**119 saves and leaves 119 patterns** (the bug's signature was 119 accepted,
115 present), ending in the documented refusal at
`{used 742624, total 749568, dead 4096, patterns 119}`, with the packing
arithmetic exact on every non-compacting save. On a healthy log `store.dead`
behaves as the fix claims: **0** after an unpinned compaction, 0 with the
lowest or a middle file pinned, and a **428 B sub-page hole** with the
highest-offset file pinned. The pinned file read back byte-identical after
every compaction, 500+ concurrent reads through compactions never returned a
truncated body, `fence_timeouts` stayed 0 and no watchdog ever fired.

### What it did not fix — Gitea #388

On a log carrying pre-existing #379 damage, a compaction does **not** give
the dead space back. `dead` sat at a hard floor across three consecutive
compactions (348,160 B with 22 patterns; 434,176 B — 58 % of the arena —
during a fill), and while it persists the store is silently a third of its
size: **a full fill accepted only 49 patterns instead of 119** before an
honest refusal. It clears eventually (it took ~6 compactions across the
session, after which capacity was fully restored), and no data is ever lost
— the save fails loudly, which is the designed behaviour — but a user who
fills the library on a damaged log has no in-band way out except deleting. A
healthy near-full log also keeps a small 4–5 KB residue rather than exactly
0. Numbers and the suggested `tools/patlog-check` repro are on #388.

### `tools/store-audit.mjs`

The per-save name-set audit that both the finding session and the fix
session asked for is now a real tool. The invariant is
`expected = previous + saved − deleted`, re-checked after **every** mutation
rather than counted at the end — which is precisely what #379 evaded, since
every save answered `{"ok":true}` while the count went down. It also checks
the packing arithmetic, times each compaction, reports the `dead` residue it
leaves, can pin a chosen stored pattern so the compaction has to route
around its frozen pages, and can hammer a concurrent read across every save.

### #365 checklist, resumed

Steps 1–9 and the `flashmap-off` build all pass on the fixed firmware;
numbers are on the issue. Highlights: reboot and re-enumerate with a full
119-pattern library answers in **6.6 s** with a byte-identical list;
compactions run **3.0–7.2 s** on logs of 10–119 files, well inside the 20 s
watchdog window; and `flashmap-off` costs **no measurable boot time**
(5.6 s vs 6.6 s for the same 117-pattern store) while every read-back stays
byte-identical through the flash-controller path. Steps 10 and 11 (power
cuts mid-save and mid-compaction) still need Jeremy's hands.

`code_mapped` is worth a note for anyone reading that checklist: it reports
whether the *running* pattern's code is mapped, so under `flashmap-off` it
stays **true** on the built-in default and only goes false once a stored
pattern is activated.

### #363 `engine_heap` on metal

Populated from boot (1,232 B on the built-in rainbow at 60 px), tracks an
ad-hoc swap with `heap_free + engine_heap` constant to 0.5 %, grows with the
pixel count (4,128 → 9,584 at 2048 px), and is per-item stable and
repeatable across a crossfading playlist. One thing is wrong: a **stored,
mapped activation reports 192 B MORE than the same pattern pushed live**,
where the issue predicts visibly less. Recorded on #363.

Also filed: **#389** (`build-esp32.sh` silently ignores a positional board
name and builds `board-pixelblaze-v3` — a pb-v3 image reached the Athom this
way, reserving the GPIO the strip is wired to) and **#390** (a pattern save
refused for memory after ~11 identical saves, cleared by a reboot — heap
fragmentation behind a misleading "too large to run here").
## 2026-09-07 — Frame Rate Scan: point a phone at the panel and count (`library/frame-rate-scan.js`)

Frame Rate Test (earlier today) reads the displayed rate as a beat against the
compose rate, which works but takes a practised eye. This is the version to
film. It spends **one visible state per composed frame** and **carries its own
clock**, so the camera's frame rate cancels out of the arithmetic entirely —
it does not have to be 240 Hz, and you do not have to know what it is.

Three one-column bars, on a 64x64 grid: rows 0–23 the **sweep** at column
`frameIndex mod 64`, red on even composed frames and green on odd, stepped by
a counter and never by the clock; rows 28–39 the **fine clock**, one column
per 10 ms (wraps every 640 ms), blue; rows 44–55 the **coarse clock**, one
column per 100 ms (wraps every 6.4 s), white. Dim grey ticks every 8 columns
under each band and on the bottom row, column 0 in cyan, so columns can be
counted off a paused frame.

The reading: pick two video frames about a second apart; on each read
`d = (f − 10·c) mod 64` and `t = 100·c + 10·d` ms from the coarse column `c`
and fine column `f`; `dt` is the difference. Then step through the frames
between them writing down the sweep column, drop repeats, and count the
**distinct** positions — that is how many frames the panel displayed, so
`displayed fps = distinct / dt`. A column jump of more than one is `jump − 1`
dropped frames (the red/green alternation is the cross-check), and
`composed fps = (distinct + dropped) / dt` should come out equal to
`/api/status` `fps`.

Live on the panel (master `e4f772b`, 30 MHz, 4096 px, brightness 31): `fps`
125, `out_fps` 125, `rescan_hz` 115–116, **`vm_us` 793–803** of an 8 ms budget,
`frame_us` 838–850, heap free 49,280. Host verification over 400 frames at both
125 and 60 fps injected: the sweep column is exactly `frame mod 64` with exact
parity colours on every row of its band, both clock columns match
`floor(elapsedMs/10) mod 64` and `floor(elapsedMs/100) mod 64` on every single
frame (no fixed-point drift), and running the documented reading procedure over
the dump returns 125.0 and 60.0 fps. `tools/check-library.sh` 307/307 on all
five rigs.

The only slider is ComposeCap, kept for the 125/n `setFrameRate` experiments
(#384) — below the display rate the sweep must stop skipping columns, which is
a second, independent bound on the displayed rate.

## 2026-09-07 — a pattern that shows you the displayed frame rate (`library/frame-rate-test.js`)

`/api/status` reports the rate the engine *composes* at. On the HUB75 panel
that is the 8 ms pacing cap — 125 fps — while the panel rescans at ~115, so
about nine composed frames a second are overwritten before anyone sees them.
`out_fps` does not catch it (it counts `write_frame` calls, #378) and
`rescan_hz` is the driver counting itself. **Frame Rate Test** is the
independent instrument: the panel tells you its own displayed rate.

A dropped frame is invisible in any single composed frame — every frame is a
complete image and the missing one simply never existed for the eye — so the
signal is temporal. The whole field flips RED/GREEN once per `renderFrame`
call (a counter, never the clock). Displayed one-for-one that fuses to steady
yellow; every frame the panel misses puts two same-colour frames side by side,
so the field shimmers at the beat frequency `|compose − displayed|`, and
**displayed fps = compose fps − stumbles per second**. The bottom 1/16 blinks
blue at `|composeFPS − "DisplayedFPS" slider|` Hz so the beat can be read by
matching two rates rather than counting one; rows 48–55 are the pattern's own
compose-fps bar (EMA of `1000/delta`, 2 fps/column, ticks at 60/77/115/125)
and rows 56–59 repeat the beat as a left/right parity stutter. With a 240 fps
phone video the reading is exact: each displayed frame occupies ~2 camera
frames, so the runs that last ~4 are the stumbles.

Live on the panel (master `b08bbd4`, 30 MHz, 4096 px, brightness 31): `fps`
125, `out_fps` 125, `rescan_hz` 114–116, `vm_us` **478–498** — half a
millisecond of an 8 ms budget, since the frame is four bulk fills. The
pattern's own `composeFPS` reads 123.9–124.3 and `beatHz` 8.9–9.3 at the
default slider, i.e. ~9 stumbles a second against ~124 composed: the panel is
displaying ~115, which is what `rescan_hz` says independently.

**`setFrameRate` is quantized to 125/n on the firmware**, measured here for
the first time. The render loop paces to one iteration per 8 ms and the engine
resets its accumulator when a capped frame fires (no remainder carry), so a
cap can only land on a tick boundary. Through the pattern's `ComposeCap`
slider: 0 → 123.9, 125 → 124.1, **115 → 61.5**, **100 → 61.6**, 62.5 → 42.7,
60 → 41.5. Asking for 115 gets you 62.5. That is why the strobe runs uncapped
and measures a beat instead of tuning to a stroboscopic null — and it is also
why `/api/status` `fps` stayed 125 in every row above: that field is the host
loop rate, not the pattern evaluation rate (docs/lang.md said so; this is it
on metal). Both findings are written up in docs/boards.md.

## 2026-09-07 — the packed pattern store was losing files on every compaction (#379)

The #365 on-metal pass found it on the Athom, and it is the worst class of
bug this store can have: **a compaction silently dropped files.** Gone from
flash, not just from the RAM index — a reboot re-scanned the log and still
did not find them. `POST /api/patterns` had returned `{"ok":true}` for every
one. Filling the store from `library/` accepted 119 saves and left 115
patterns, the five missing being the five *lowest* arena offsets; a single
save that ran a compaction took the count 89 → 88 instead of 90. The device
agent's discriminating run — nothing pinned, the engine on an ad-hoc
`/api/code` push — still lost one of 20, which killed the obvious
frozen-page hypothesis. `store.dead` also never came back to 0 after a
compaction (it stuck at 17,996 B), while the host suite asserted it did.

### Why the host suite was green

`tools/patlog-check` tests the *format* (`patlog.rs`) thoroughly — a NOR
simulator, power cuts at every write boundary, a 600-round churn fuzz. What
it never tested is the *caller*. Its compaction tests hand `compact()` a
live list derived fresh from the flash, so nothing exercised the index
`patterns.rs` actually keeps, a compaction happening in the middle of a
save, or an address read before a repack and used after it. So the crate now
carries `store.rs`: a host replica of the store state machine — index,
cursor, seq/stamp counters, save/delete/compact in the firmware's exact
order — over the same simulator. **It reproduced the loss on the first
run**, with and without a pin.

### The cause: two independent defects, both address-trust

1. **The boot scan stepped over a torn record by its own claimed length.**
   `scan()` treated a header that parses and commits but whose payload does
   not hash as a record for the purpose of advancing: `at = rec.end()`. But
   a torn header's length field describes bytes that are no longer the ones
   it was written for. A compaction leaves exactly such headers behind —
   a frozen page keeps the stale header of a record whose payload was just
   reclaimed — and its `end()` now points past *the files that were packed
   into that space*. One stale header swallowed every record inside its old
   extent. That is the "lowest offsets" signature: the running pattern sat
   at offset 0, page 0 could not be erased, and the stale header living
   there ate whatever was repacked immediately above it. The scan now
   resyncs 4 bytes at a time through a torn record, like any other junk —
   which is what `self_off` + `hdr_hash` were designed for.

2. **A save that compacts retired the *pre-compaction* address.** `save()`
   resolves the previous generation up front (`rec_by_name`), discovers
   there is no room, compacts — which moves every unpinned file — and then
   writes the DEAD word to the record it captured before the repack. Those
   four zero bytes land in the middle of whichever file was packed over
   them, tearing it. No pin required; this is the half the device saw with
   nothing pinned. `old` is now re-resolved from the rebuilt index.

### The fix, and making the failure mode impossible to repeat silently

* `patlog::pack` became `patlog::plan`, which **checks the plan as it builds
  it** and returns `None` rather than a partial one: every record placed
  exactly once and in order, no two overlapping, nothing moving up, a pinned
  record at its own address, and a *moved* record never landing in a page
  `build_page` will skip. `compact()` refuses on `None` — prints
  `compaction refused — no plan places all N files` and erases nothing, so
  the save fails loudly instead of the library quietly shrinking.
* `DEAD_BYTES` is now `cursor − used` rather than the sum of accepted
  records minus used. That is the honest "what a compaction would give
  back" — bytes no record claims are reclaimable too — and it makes
  `store.dead` come back to **exactly 0** after a compaction with nothing
  pinned, and to exactly the hole a frozen page forces when something is.

### Coverage

Ten new cases in `tools/patlog-check/src/store/tests.rs`, every one of them
a failing test before the fix: fill-and-churn with nothing pinned and with
the lowest / a middle / the highest file pinned; the wholly-dead-log
re-seed (#379 reproduction 3); dead-byte reclamation with and without a pin;
a re-save that compacts, asserting the DEAD word landed at the record's
*new* address and tore nothing; a power cut at every one of 2,052 write
boundaries of a **pinned** compaction; the plan guard refusing an
overlapping index with nothing erased and nothing written; and a random-pin,
random-churn fuzz (8 runs × 300 rounds) that re-reads the entire live set
after every operation. Reverting either fix individually turns tests red.

`cargo test --workspace` green (26 patlog-check cases), `tools/ci.sh` green,
QEMU `flashmap-test` PASS, `.stack` unchanged at 25,988 B. Image cost
**+368 B** on `board-pixelblaze-v3` (margin 38,880 B / 3.71 %), +336 B on
`board-athom-music`, +496 B on `board-c6-devkit` (1.97 %) — docs/boards.md.

Still open: the Athom is holding the damaged store from the #365 run
deliberately, so the fix wants verifying on metal against it (#379), and
pattern ids are derived from the arena offset, so they are **not** stable
across a compaction even though `/api/patterns/<id>` is the only handle a
client has (#382).

## 2026-09-07 — the panel's LCD_CAM clock is 30 MHz, and `out_fps` never meant what it said (#255, #378)

The panel's rescan rate had only ever been a guess: a comment in
`firmware/src/hub75.rs` estimating "~77 Hz at 7 planes". It is now measured,
from esp-hub75's own BCM frame counter (`Hub75::frame_count()`, free — the ISR
is always armed in circular-DMA mode), exposed as a new `/api/status` field
**`rescan_hz`**. The estimate was right, and the rate is exactly linear in the
pixel clock: **77.0 / 115.3 / 154.0** rescans a second at 20 / 30 / 40 MHz.

**The clock default moves 20 MHz → 30 MHz.** The FM6124 datasheet (v1.1) caps
FCLK at 30 MHz, and its 20 ns minimum clock high/low implies 25 MHz on pulse
width alone, so 30 is the ceiling with no margin — the 74HCT245 buffers add
22–28 ns of tpd on top. 40 MHz was tried and **fails visibly**: the two 32-row
halves mis-sample into a split down the middle of the panel and the colours
distort. 30 MHz was visually clean across rainbow, Raindrops 2D, Infinite
Snake v2 and bulk-comet-trails, on the bench 64x64 FM6124EJ panel.

Worth remembering: **the firmware saw nothing wrong at 40 MHz.** No swap
error, no DMA error, `vmerr` null, `fence_timeouts` 0, nothing on serial — and
the composed frame was still byte-identical to a host render (a
time-independent probe through `/api/pixels`: 12,288/12,288 bytes equal). Only
the panel's own sampling failed. Clock and geometry changes need an eyeball.

The clock buys no throughput — fps and `vm_us` were identical at all three
rates, every pattern being render-bound rather than rescan-bound. It buys
headroom: an 8th bitplane becomes usable (~58 Hz rather than ~38), and chained
panels get their bandwidth (#255).

### `out_fps` is a compose rate on the panel, not a display rate (#378)

Measuring the rescan rate resolved a contradiction sitting in the data since
#336: `snake-2d-v2` reports `out_fps` 118 and `bulk-comet-trails` 125, against
a panel that rescans 77 times a second at the old clock. Both numbers were
real; the name was wrong. `pipeline.rs` counts every `write_frame` **call**,
and `Hub75Output::write_frame` returns without drawing whenever the previous
buffer swap has not landed. So a third of those frames were composed and
thrown away unseen.

`shared.rs`'s doc comment claimed the opposite ("`OUT_FPS` is what the panel
showed") and is corrected here, along with the same claim in docs/boards.md's
bulk-pattern table. The displayed rate is `min(out_fps, rescan_hz)`. On a
strip nothing changes — every rendered frame really is written. Filed as #378;
the fix that makes `out_fps` mean one thing on every board is still open.

Measured, all on the panel at 4096 px / brightness 31 / 7 planes:

| clock | rescans/s | `rainbow` | `raindrops-2d` | `snake-2d-v2` | `bulk-comet-trails` |
|---|---:|---:|---:|---:|---:|
| 20 MHz | 77.0 | 52 | 69 | 118 | — |
| 30 MHz | 115.3 | 52 | 69 | 118 | 125 (115 shown) |
| 40 MHz | 154.0 | 51 | 68 | 119 | — |

### Also this session

* **The #340 packed store, on metal.** Packing arithmetic is exact — every
  save grows `store.used` by precisely
  `48 + align4(name) + align4(source) + align4(bytecode)`, verified over 46
  consecutive saves on the Athom and all 5 on the panel. Reboot
  re-enumeration is clean (byte-identical `GET /api/patterns`, identical
  `used`/`dead`), read-back through the flash mapping is byte-identical, and
  the store correctly refuses a save when full after 119 `library/` patterns
  — matching the predicted count exactly.
* **But a compaction silently drops files (#379, data loss).** Reproduced
  three ways on the Athom; the lost files are the lowest-offset ones, they do
  not come back after a reboot, and a pinned page is **not** required. The
  #365 checklist is stopped at that point. Everything that survives is
  byte-perfect, so it is narrowly the compaction.
* **The format 5 → 6 wipe reports a scary number.** After the migration the
  boot line says `0 files (0 torn, 18849 resyncs)` where #365 expects 0
  resyncs. Benign: `patterns.rs` wipes only the sequential-storage key area
  and deliberately leaves the log arena unerased, so the scan steps over
  ~75 KB of stale format-5 bytes 4 bytes at a time. No data is at risk, but
  the narration reads like corruption and every boot pays the scan.

## 2026-09-07 — Animated Asterisks 2D: two of three device suspects cleared, and the frame rate (#371)

Jeremy reported that on the 64x64 panel the arm drawn on top where the arms
overlap changes constantly, while the playground shows one stable arm on top.
The pattern is deterministic about that — `render2D` returns on the FIRST arm
within `halfWidth`, so arm 0 wins every overlap — so a changing winner pointed
at the device path: HUB75 pipeline tearing (#269/#306), the early `return`
inside a `for` loop under the flash-mapped execution (#268/#300/#307), or a
cross-core race on `lineCos`/`lineSin`/`lineHue` (#280).

**The engine is exonerated.** Two probes, captured through `GET /api/pixels`
and hashed against `tools/verify/enginehost.mjs` renders of the same source:

* **Frozen phase** — `angle` pinned to a constant, `hsv(i / 8, 1, 1)` so every
  pixel's colour *is* the winning arm index. 20 consecutive panel frames, all
  md5-identical to each other and to the host: `7e53b17e…`. The loop picks the
  same winner for 4096/4096 pixels, 20/20 frames.
* **Frame counter** — the real pattern with `time()` replaced by
  `fcount = fcount + 1`, so frame N is a fixed image. 400 host frames hashed;
  **25/25** consecutive panel captures were byte-identical to one whole host
  frame, with the matched index advancing monotonically (10 10 10 11 11 12 …).
  The same probe on the Athom (60 px, default grid map, 123 fps) matched
  **14/14**.

That kills the early `return` and the cross-core race. It does **not** clear
the pipeline: `/api/pixels` on a pipelined board returns `pipeline::preview()`,
the composed hand-off buffer — upstream of the DMA. And the DMA swap is not
frame-atomic in the `circular-dma` build we ship: `Hub75::swap` in
`firmware/vendor/esp-hub75/src/isr.rs` applies the pointer delta to every
descriptor immediately, mid-pass, with its own SAFETY note conceding "one
partially-mixed frame (tearing)". The doc comments in `firmware/src/hub75.rs`
("queues an atomic buffer swap at the next rescan boundary", "a swap lands at a
rescan boundary (~13 ms)") describe the non-circular mode and are wrong for our
build; only `SWAP_DONE` is deferred to `out_eof`, and that just guards
reclaiming the old buffer. That is being tracked as its own ticket.

Its duty cycle bounds what it can explain here, though. The panel rescans at
~77 Hz (7 planes, 20 MHz, 64x64) and composed this pattern at 5.7 fps, so one
pass in ~13.5 carries a swap: the panel shows a *pure* frame for ~93 % of
passes, and the mixed pass is ~13 ms out of each 175 ms frame. "Which arm is on
top" is a property of the whole 175 ms — and it already differs between
consecutive clean frames, because of aliasing:

Master's defaults are 4 arms — the fan is periodic every 45° — at 0.5 rev/s,
and the panel rendered the pattern at **5.7 fps** (`frame_us` 174,746 at
4096 px = 42.6 µs/pixel). That is **31.5° of rotation per frame against a 45°
arm spacing, 0.70 arm slots per frame**: arm 0 lands somewhere else every
frame. Measured hue at panel pixel (48,31) over consecutive frames — `0.88 0.71
0.80 0.64 0.48 0.33 0.41 0.26 0.10 …` — steps of −0.17/+0.09, exactly the
predicted −0.175 per frame plus the 0.5 cyc/s hue drift. The playground (60 fps)
and the Athom (123 fps) sample the same rotation at 3.0° and 1.5° per frame and
look continuous.

Two changes, both in `library/animated-asterisks-2d.js`:

* **`render2D` inner loop, output-identical.** `px`/`py` are loop-invariant but
  were recomputed for every arm, and the segment-endpoint branches were
  evaluated before the cheap test that gates them. The perpendicular distance
  to an arm's infinite line is a lower bound on the distance to the segment, so
  testing it first rejects a pixel in four ops instead of a projection plus two
  branches — and on a panel most pixels reject against every arm. Panel
  `vm_us` **175,101 → 127,137** (mean of 5 samples each, 4096 px): **−27.4 %,
  5.7 → 7.9 fps**. Byte-identical output: 2,880 frame pairs across six rigs
  (64x64, 16x16, 8x32 grids; 60/300/512 px strips) and four control sets with
  the rotation speed pinned, **0 differing bytes**.
* **RotationSpeed default 0.5 → 0.25 rev/s.** The cap is set by the slowest rig
  this ships for, not by taste: at the new 7.9 fps that is 11.4° per frame, a
  quarter of the 45° arm spacing, versus 0.70 of it before. Mean frame-to-frame
  hue change at four fixed panel pixels: **0.151 → 0.087** (median 0.159 →
  0.066 — and 0.063 of that is the ColorSpeed drift itself, which is real
  motion, not churn). The slider still reaches 3 rev/s.

Both also shrink the *other* term: a mid-pass swap mixes frames 11.4° apart
instead of 31.5°. (It fires more often — 1 pass in 9.8 rather than 13.5 — but
each mix is 2.8× smaller.) #371 stays open until Jeremy says whether the top
arm now tracks smoothly on the panel; that observation is what separates the
residue from the non-atomic swap.

`tools/check-library.sh`: 305/305 on all five rigs. Both boards were left as
found — panel on its live Raindrops 2D at brightness 31 with all four stored
patterns intact, Athom on Rainbow at 60 px / brightness 4.

## 2026-09-07 — Raindrops 2D renders through `renderFrame` + `fillCanvas`

`library/raindrops-2d.js` converted in place, the way `snake-2d-v2.js`
converted `snake-2d.js`. The pool was always a 16x16 simulation — two water
buffers, a static sea floor, a drifting shimmer — and `render2D` only resolved
a colour out of it per LED. That readout moved to the pool's own resolution:
one `shade()` pass over 256 cells into `hC`/`sC`/`vC` and one
`fillCanvas(hC, sC, vC, 16, 16)` from `renderFrame()`.

Everything the pattern *is* is untouched — the seven dials and their `//#`
bounds, the fixed 30 Hz step, the pointer-swapped buffers, the mirrored
boundary, the whole-field flatten that makes ripples fade to exactly nothing,
the drop scheduler. The static per-cell terms (the sea floor's contribution and
the two shimmer wave arguments minus their phase) are baked at init, the hue
channel is repainted only when its dial moves, and a pool that has gone exactly
flat skips the recurrence *and* the repaint.

**Host throughput** (`luxel bench`, best of five):

| rig | before | after | ratio |
|---|---:|---:|---:|
| 4096 px, `--map-grid 64x64` | 6.90 Mpx/s, 593.8 µs/frame | **46.98 Mpx/s, 87.2 µs/frame** | **6.8x** |
| 256 px, `--map-grid 16x16` | 4.03 Mpx/s, 63.5 µs/frame | 4.52 Mpx/s, 56.6 µs/frame | 1.12x |

**Interpreted instructions** (`bench --profile`): 60.6 → **6.4 insns/px** at
4096 px, 114.0 → 102.2 at 256 px. Both profile runs of the converted pattern
report the *same* 7,847,813 instructions over 300 frames — 26,159 per frame
whatever the panel size, which is the whole point. Idle (Raindrops 0.3,
Shimmer 0, RippleFade 0.4) it drops to **2,279 instructions a frame**.

**Visual equivalence** — 60 frames at a fixed 30 fps delta and seed, byte for
byte against the pre-conversion file: max per-channel diff **1** at 16x16 (one
byte of 46,080), 10 at 32x32, 16 at 64x64, 5 on a 60 px strip. With Texture at
0 it is **0 on all four rigs**, and against the original with *only* the
shimmer's coordinates quantized to the pool's cells it is **0 on all four**.
So every non-zero delta is the shimmer, which used to be evaluated per PIXEL
and is now per cell: above 16x16 it blocks with the water instead of carrying
full-panel detail. The single byte at 16x16 is column 15 — a map normalizes
its far edge to 65535/65536 while a cell's own `cx / (W - 1)` is exactly 1.
Undriven vs every control at its declared `default=` differs by maxdiff 1 over
0.21 % of bytes *both before and after* (a pre-existing rounding of `198 / 360`
against the literal `0.55`), so dial agreement is unchanged.

**Where the frame goes now** at 4096 px (host, ablation): `fillCanvas` and the
frame's fixed overhead 32.6 µs (37 %), the water recurrence 28.5 µs (33 %,
14,461 insns), the per-cell shading 26.5 µs (30 %, 11,520 insns). Two thirds of
the frame is interpreted bytecode that no existing bulk op covers — the
mirrored 4-neighbour Laplacian and the element-wise shading map. The builtins
that would, with these numbers behind them, are filed rather than built.

Verified host-side only: `tools/check-library.sh` 305/305 on all five rigs,
`tools/ci.sh` green (web + cargo + library), and driven in real chromium — the
gallery tile renders, the pattern opens with no compile error, and it animates
as rain ripples on both a 16x16 and a 64x64 grid rig. Not yet seen on the
panel — Gitea #374 is the on-panel look check (per-cell shimmer at 4096 px, the
flat-pool skip, and the real `out_fps`), #373 the bulk-op analysis it produced.

## 2026-09-07 — `renderFrame` on metal: no I-cache cost, 7× on the panel (#336)

#335 (whole-frame render entry + sixteen bulk builtins) shipped host-measured
only — both rigs were busy the day it merged. This is the device half.

Three firmware builds went to both boards out of one worktree: the merge base
**`974b3b3`**, the #335 merge **`eedabc8`**, and the master of the day
**`ed2ac3f`**. One `web/public/luxel.wasm` compiled every pattern for every
run, so the bytecode on the wire is byte-identical across builds and only the
firmware differs. Both S3 slots carried the same image before any measurement
(#294); six OTAs, none wedged, `core1.last` clean throughout.

**The I-cache risk did not materialise.** `Vm::call_builtin` gained sixteen
arms and #318/#325 had shown layout alone worth tens of percent —
`tools/patbench.mjs` on `perlin-fire-wind-tunnel` (three repeats) plus
`tools/opbench.mjs`:

| board | `974b3b3` | `eedabc8` (#335) | `ed2ac3f` |
|---|---:|---:|---:|
| Athom @ 256 px, µs/px | 58.039 | **57.973** (−0.11 %) | 58.121 (+0.14 %) |
| Athom, cycles/op | 100.9 | **100.7** | 100.8 |
| Seengreat @ 4096 px, µs/px | 44.360 | **44.382** (+0.05 %) | 44.413 (+0.12 %) |
| Seengreat, cycles/op | 83.4 | **83.4** | 83.4 |

Every delta is inside the probe's own ±0.3 % repeatability, on the board that
pins `Vm::run`/`call_builtin`/`builtin_hot` in IRAM *and* on the one that pins
only `Vm::run`. Nothing to file against #328.

**Bulk patterns on the 64×64 panel** (`ed2ac3f`, 4096 px, medians of seven
samples):

| pattern | fps | out_fps | frame µs | vm µs | out µs | vm µs/px |
|---|---:|---:|---:|---:|---:|---:|
| `rainbow` (per-pixel control) | 52 | 52 | 19,470 | 19,417 | 5,931 | 4.740 |
| `bulk-rainbow` | 125 | 125 | 2,825 | **2,772** | 3,261 | **0.677** |
| `bulk-comet-trails` | 125 | 126 | 480 | **431** | 3,237 | **0.105** |
| `bulk-bouncing-balls-2d` | 125 | 125 | 5,539 | **5,477** | 3,486 | **1.337** |
| `bulk-sprite-scroll-2d` | 125 | 124 | 5,205 | **5,125** | 3,556 | **1.251** |
| `bulk-canvas-ripples-2d` | 100 | 100 | 9,979 | **9,900** | 3,599 | **2.417** |

`rainbow` → `bulk-rainbow` is the only like-for-like pair among shipped library
patterns: **7.0× of VM time**, 52 → 125 fps on the wire. The host bench read
4.4× for that shape, so the device multiplier is **1.6×, not the ~3×**
docs/bulk-render.md's rule of thumb suggested — that estimate is built on the
per-pixel entry cost, which is a smaller share of a fill-shaped frame than of
an entity loop. The page now says 1.5–2× for fills and "re-measure, don't
scale".

Two things only the device shows. **Four of the five bulk patterns are
frame-cap bound, not VM bound** — at 2.8–5.5 ms of VM they sit on the engine's
125 fps ceiling with the HUB75 compose at 3.2–3.6 ms, so a bulk rewrite's
payoff stops at the cap and past it buys headroom, not frames. And
**`library/rainbow-comet.js` will not load at 4096 px at all**: its
`array(pixelCount)` trail is refused by the pre-flight check ("pattern too
large for this device — it left only 16 KB of heap free"), while
`bulk-comet-trails` draws the same shape from six scalars in **431 µs**, the
cheapest frame measured on the board. Frame persistence is not a
micro-optimisation on this hardware; it is the difference between running and
not running.

One thing the sweep turned up that is not about #335 at all: **`out_us` is not
flat**. The HUB75 compose is content-independent, and "Second light" recorded
it as a flat 5.3–6.4 ms whatever runs — but it tracks how hard core 1 is
working, monotonically across nine patterns spanning 431 µs to 234 ms of VM
time: 6.5 ms under `ripples-2d` down to 3.2 ms under `bulk-comet-trails`, and
not a bulk-vs-per-pixel artifact (the cheap per-pixel `Infinite Snake v2` gets
a cheap `out` too). That is nearly 2× on the panel's compose ceiling. Filed as
**#367**.

**`blit` keyed mode and `fillCanvas` verified pixel by pixel, not by eye** —
neither rig can be seen from here, so deterministic probe patterns were pushed
and `GET /api/pixels` asserted. On the panel: a 4×2 keyed sprite over a green
`fill()` gives exactly 4 red pixels of 4096 in the right cells; an 8×4 sprite
at `gridWidth() - 4` gives exactly 16, the overhanging columns clipped; a 2×2
`fillCanvas` gives exactly 1024 pixels per quadrant. On the Athom's 60-px strip
— the over-provisioned 8×8 `ceil(√n)` grid — the same probes land at indices
0/2/8/10, an 8×1 sprite on grid row 7 paints 56–59 and clips 60–63 with no
`vmerr`, and `library/bulk-sprite-scroll-2d.js` shows its face over the wash.
"Cover, not match" holds on a real non-rectangular fixture.

**The full gallery soak is clean too.** `tools/hw-bench.mjs` on the Athom on
`ed2ac3f`: **305/305 patterns clean, 0 errors, 4 under 30 fps**, median 123 fps
at 60 px (docs/bench-report.md). Against the 2026-09-02 run on the same rig —
299/299 clean, 12 slow, median 118 — the interpreter work plus #335 has taken
two thirds of the slow tail out. One thing that moved the other way and is not
explained: the sweep's lowest `heap_free` went 83,448 → 59,668 B. Still far
above the ~20 KB floor and nothing errored, so it is an observation, not a
fault; filed as **#368**, which also asks hw-bench to name the pattern that
produced the low-water instead of just printing the number.

**Heap is flat under a bulk pattern.** 40-minute soak on the Athom (20 min holding
`bulk-comet-trails`, then all five `bulk-*.js` rotated a minute apart):
`heap_free` read **83,616 B on 38 of the hold's 40 samples**, first and last
included, the other two 192 B lower with a response in flight; each rotated
pattern returned to its own fixed value every cycle. AppCpu stack high-water
11,136 → 11,520 B of 20,480, no `vmerr`, no reboot. The `mem::take`/give-back
of the frame `Vec` costs nothing per frame, as designed.

**OTA margins re-measured on the merged tree** (docs/bulk-render.md's size
table was taken on the branch), `tools/image-check.sh` on `espflash save-image`
app images, same `creds.env` throughout:

| board | `974b3b3` | `eedabc8` | Δ #335 | `ed2ac3f` | margin at master |
|---|---:|---:|---:|---:|---:|
| `board-pixelblaze-v3` | 997,648 | 1,006,480 | +8,832 | 1,006,432 | 42,144 B (4.01 %) warn |
| `board-athom-music` | 997,536 | 1,006,352 | +8,816 | 1,006,480 | 42,096 B (4.01 %) warn |
| `board-seengreat-hub75` | 935,904 | 945,216 | +9,312 | 947,424 | 101,152 B (9.64 %) |
| `board-c6-devkit` | 1,015,376 | 1,024,608 | +9,232 | 1,024,912 | 23,664 B (**2.26 %**) fail |

+8.8–9.3 KB a board, and the C6 conclusion stands: it is the one board this
change puts under `image-check.sh`'s 3 % floor (its base margin was 3.16 %),
tracked by **#291**. Both shipped classic-ESP32 boards stay just over 4 %.

Both rigs finish on `ed2ac3f`, image in both slots, at the pixel count and
brightness they were found with: the Athom on rainbow at 60 px, the panel back
on the stored `Infinite Snake v2` it was found running (119 fps / 119 out_fps
at 4096 px — identified by matching its `/api/status` signature after the
harness had already replaced the live code with rainbow). Its four stored
patterns are untouched; nothing in the range `974b3b3..ed2ac3f` changes
`FORMAT_VERSION`. **They are deliberately NOT on the master this entry merges
into**: #340 landed hours later and bumps the store format 5 → 6, which wipes
the key area on first boot by design — so pushing today's tip would have
destroyed the four patterns Jeremy saved on the panel. Whoever OTAs that board
next should expect `patterns: format 5 != 6, wiping storage` and export them
first.

Docs: docs/boards.md gains a "Bulk render (`renderFrame`) on metal"
section plus a margin row, docs/bulk-render.md a "Results — on device" section,
and the stale "unmeasured I-cache risk" caveat is gone from both its
how-to-judge and scope sections.

## 2026-09-07 — the pattern store packs files to their exact size and stopped having a directory (#340)

Jeremy, #340: *"Right now we are limited to hold (12?) patterns total
because slots are fixed. We should have properly sized files instead (the
size required is known after all) just held sequentially in memory to exact
size. Adding a new file is easy, just add it right after the last file. It
may mean walking a linked list to get to a desired file or to enumerate the
existing files but that's ok."*

Yesterday's #330 had already killed the fixed slots, but it left two things
standing and they are exactly the two the ticket asks for. #330 allocated in
whole **4 KiB pages** — and a pattern owned two of those allocations, its
source and its bytecode — so a median pattern paid ≥ 8 KiB for 4.9 KiB of
bytes. And its whole directory was ONE `sequential-storage` item, whose
one-page cap is what pinned `MAX_PATTERNS` to 32. Both are gone.

**A file is now exactly its bytes.** One stored pattern is one file: a
48-byte header, the name, the source text and the LXBC, packed back to back
at 4-byte alignment, and the next file starts immediately after. Four is
both the floor and the ceiling of the alignment tax —
`deserialize_lean_static` borrows a blob's word region only when it is
4-aligned in memory, and `esp-storage`'s `WRITE_SIZE` is 4, so every flash
write is a 4-aligned offset and a multiple-of-4 length anyway.

**The log is self-describing — there is no directory anywhere.** Every
header carries magic, its own offset, a monotonic stamp, the seq, both
payload lengths, both payload hashes, the name and a commit marker. Boot
walks them through the flash mapping and builds a small RAM index
(`Vec<Rec>`, 32 B each, no names — those are read back out of the mapping).
The pattern count is bounded by **bytes**. What is left is `MAX_RECS` (192),
a heap guard on the index, sitting well past what the log physically holds —
and the *scan* is deliberately uncapped, because `cursor` has to be the end
of the last record in the log or an append would land on top of live ones.
If a log somehow does carry more than 192 distinct patterns (only reachable
with implausibly tiny ones), the index is incomplete, boot says so, and
every mutation refuses until some are deleted: a compaction rewrites the log
from the index and would drop what it cannot see.

Measured on the real library — `cargo test -p patlog-check`, 305 patterns,
median source 2,853 B, median bytecode 2,008 B, 219/305 sources ≤ 4 KiB:

| | patterns that fit | of the 732 KiB log |
|---|---:|---:|
| page-granular (#330) | 32 (its directory item's cap) | 328 KiB, 44.8 % |
| exact-packed (#340) | **119** | 722 KiB, **98.7 %** |

### The design calls, and why

**Append-only, with compaction as the only reclaim.** The erase unit is
still 4 KiB while a file is now byte-sized, so a delete cannot erase
anything. It writes a `dead` word into the old header — NOR clears bits
without an erase, and this build uses plain `FlashStorage`, never the
encrypted one, so a sub-page 4-byte write is legal — and the bytes stay.
That is also what makes it safe to delete or re-save a pattern an engine is
executing. Space comes back only from a compaction, which only a **save that
ran out of room** ever triggers: an activation never compacts, so playlist
churn stays wear-free (the 2026-08-15 rule).

**Compaction rewrites one destination page at a time, buffered before the
erase.** `patlog::pack` places every live file — plus any dead one a pinned
engine is still executing — at or below its current offset and never inside
a page a pinned file occupies; nothing ever moves up. The executor then
builds each 4 KiB destination page in RAM *before* erasing it. Every byte a
page needs lives at an offset ≥ that page's start, which is the whole safety
argument for an overlapping repack. Pinned pages are skipped outright, a
page whose new content equals its old one is not erased at all, and the
stale copies above the new cursor are erased last. **Bound:** at most one
erase + one 4 KiB write per page of the packed log plus one erase per swept
page — 183 + 183 flash ops on a full log, each its own fenced door with a
1 ms yield, and `core1::fenced` feeds the watchdog every 64 fences (#309).

**Power-cut safety is ordering plus a self-offset.** A save erases the pages
it will touch, writes the header prefix (magic first), the name, the source
and the bytecode, **re-reads the payload through the mapping and checks both
hashes**, and only then writes the commit word. A cut before that leaves a
record the scan refuses and the previous version untouched. `self_off` +
`hdr_hash` make recovery local: a header only validates at the offset it was
written for, so after a cut compaction a stale copy is still readable at its
old home while the new copy is readable at its new one, the scan resyncs
4 bytes at a time through the seam, and the most that can be lost is the one
file that was in flight. Duplicates (a cut between "commit the new" and
"retire the old") resolve to the highest stamp.

**FORMAT_VERSION 5 → 6 wipes the key area, and `patlog::VER` retires every
older record in the log.** No migration, by design — Jeremy's rule from
#330; the playground re-syncs. The log needs no boot-time erase for that:
the scan finds nothing and the first append erases the pages it lands on.

`/api/status`'s `store` is **bytes** now, not 4 KiB pages, and gained a
`dead` field: `{"used":18452,"total":749568,"dead":0,"patterns":3}`.
docs/api.md and docs/firmware.md updated; docs/research/flash-mmap.md gets
the "packed files, not pages" note (nothing about the XIP contract changed —
contiguous and 4-byte aligned is still all it asks for).

### Verified (host only)

`firmware/src/extents.rs` → `firmware/src/patlog.rs`, and
`tools/extent-check` → `tools/patlog-check`. **16 cases**, driven against a
NOR flash simulator that models the real semantics — an erase sets 0xFF, a
write only ever clears bits, and every 4-byte word and every page erase is a
step power can die between:

* exact-size 4-byte-aligned packing, with the bytecode landing 4-aligned for
  every combination of name and source length;
* a header that parses only at the offset it was written for, and not
  without its commit word or with one payload-length bit flipped;
* back-to-back appends with `resync == 0`, three files and 13 KB of payload
  inside four erase pages;
* 4,000-odd tiny files in a 183-page log, where the old ceiling was 32;
* a dead mark that erases nothing; a re-save superseded by stamp; a torn
  tail costing one page rather than the log;
* **a power cut at every write boundary of a save** (1,000+ cut points):
  exactly one whole version of the pattern survives every time, never a mix,
  the bystander pattern is always intact, and the store still accepts a save
  afterwards. The same sweep for a delete and **for a compaction**, where at
  most the one file in flight is lost and every survivor reads back byte for
  byte;
* compaction reclaiming dead space exactly, never touching a pinned file,
  and erasing nothing when there is nothing to reclaim;
* a 600-round churn fuzz (save / re-save / delete / compact) re-reading
  every live file every round.

Plus two cases that need the compiler, in the crate itself: all 305
`library/*.js` compiled with `luxel_core::compile` and packed (the table
above), and the whole library written into a simulated log and enumerated
back by the boot scan — 119 files, 739,616 B, `resync == 0`.

QEMU `flashmap-test.py` updated for the new boot narration and **PASS**:
`patterns: log 749568 B, 0 patterns, 0 B used, 0 B reclaimable, 0 files
(0 torn, 0 resyncs), cursor 0` on a virgin flash — a non-zero `resyncs`
there would mean the scan cannot tell erased NOR from junk. `ci.sh` green,
`tools/stack-check.sh` clean, all nine board images build.

**Image.** +2,608 to +3,280 B on every board (docs/boards.md has the
per-board table) — a store that walks a log costs more code than one that
reads a table. What comes back is RAM: `.stack` on pixelblaze-v3
**24,828 → 25,988 B**, because the 72-entry extent table and its page
bitmap were a ~1.2 KB `.bss` static and the log has no directory to hold.
Three trims were taken before landing, all monomorphization: `scan`'s and
`pack`'s callbacks are `&mut dyn FnMut` rather than generic (three copies
of a whole arena walk, ~2.6 KB), the index is ordered by a hand-rolled
insertion sort rather than three `sort_unstable_by_key` instantiations of
pdqsort (~1.3 KB), and the firmware does not `{:?}`-print `patlog::Step`
(710 B to name six variants). Without them it was +7.9 KB on the C3.
`board-c6-devkit` goes 2.27 % → 2.01 % of its OTA slot free — already under
image-check's floor on master (#310), and not a release artifact (#291);
the shipped hosted variant keeps 3.60 %.

**Not verified — hardware.** Nothing here has touched flash on a device. The
on-metal checklist is Gitea #365; #331 (the #330 pass) is superseded by it
for the store half. The device **wipes on first boot** — expect
`patterns: format 5 != 6, wiping storage` once, then an empty library.


## 2026-09-07 — Recalibrating "too large for this device": the outgoing engine's heap is part of the budget (#287)

Jeremy: *"Many patterns which do run have warnings that they won't."* Two
independent errors in the capacity model, both pushing the same way.

**1. The load base was wrong.** `budget::load_headroom` was documented as
"the firmware builds the new engine while the OLD one is still resident, so
`heap_free` at status time really is the incoming pattern's headroom." That
has not been true since v0.1.34: every load path — `Msg::Code`, a
non-crossfading `Msg::Library`, both `rebuild()` callers — opens with
`engine = None; drop_prev(&mut prev)`, precisely so peak heap lands where the
most is free. So the incoming pattern was being charged for the outgoing one.
The effect compounds: the fatter the resident pattern, the lower `heap_free`,
the louder the false alarm. At 300 px with 25 KB free and a 30 KB pattern
loaded, **all 305 gallery patterns modelled "over"** — against a real base of
55 KB, where 3 do.

`/api/status` now carries **`engine_heap`**: what the resident engine costs,
measured by the firmware across each load (`note_engine_heap`, one
`HEAP.free()` bracket per load path; `Msg::Code` adds back the still-resident
envelope's `len()`). The budget is `budget::load_base(heap_free,
engine_heap)`. A crossfade keeps its outgoing engine alive on purpose and so
records nothing. `engine_heap` 0 — pre-#287 firmware, the mirror without
`--engine-heap` — falls back to `heap_free` alone, which is the old
conservative behaviour, never an optimistic one.

**2. The wrong number was being compared.** The model reported a single
`peak` — the transient high-water of the load window, envelope included — and
tested it against the *floor* headroom. But the firmware drops the envelope
BEFORE it builds the engine (`drop(env)` sits above `engine_or_vmerr` for
exactly this reason), so `try_budgeted_engine`'s floor check never sees it.
The two tests the device actually applies are now modelled separately:
`resident` (what is left once the load settles) against the floor headroom,
and `peak` against the whole load base — the decode's fallible allocations
have to fit somewhere. Peak ran up to **2.8× resident** on the gallery, all of
it charged against the wrong budget.

**Both paths are modelled, and they are no longer the same price.** A live
push (`POST /api/code`, what the editor does on every recompile) holds the
whole LXP envelope across a *copying* `deserialize_lean`, with the pattern
store's 4 KiB write staging alive in the same window. A stored pattern
activated by id decodes with `deserialize_lean_static` straight off the flash
mapping — code and constant pool borrowed, costing no heap at all (#276/#300).
`lx_device_model` now replays both and returns `resident`/`peak`/`budget` for
each plus a `fit` verdict. The banner shows the LIVE verdict deliberately: it
is the worse path and the one about to be taken. When only the live push
fails it says so and names the stored figure, because "save it to the device's
library" is then a real fix rather than "rewrite your pattern".

Measured, 300 px, a device reporting 25 KB free with a 30 KB pattern resident
(counting allocator, whole gallery):

| pattern | envelope | live peak | live resident | stored resident | old verdict | new |
|---|---:|---:|---:|---:|---|---|
| Chasing Rainbows & HSLuv | 14,228 | 26,070 | 18,407 | 14,699 | over | **fits** |
| Fireworks Finale | 17,551 | 29,335 | 21,941 | 17,877 | over | **fits** |
| Utility: Palettes | 16,700 | 30,444 | 13,580 | 8,228 | over | **fits** |
| 2D Fireworks Fade | 30,989 | 48,242 | 29,154 | 20,710 | over | **fits** |
| Infinite Snake v2 | 21,037 | 34,307 | 32,454 | 27,666 | over | **tight** |
| Main Stage | 45,746 | 70,987 | 34,535 | 22,323 | over | **over** (the 45 KB upload, not the pattern — banner points at the library) |

Across the gallery at that operating point the old model flagged **all 305**
(its headroom was 4,520 B); the new one flags **3**. On the Athom at 60 px /
104,832 B idle, 0 of 305 warn either way — it never had the problem, which is
why this went unnoticed until the panel. On the S3 panel at 4096 px /
51,704 B idle, 9 → 4, and those four are real rejections: at 4096 px an
array of `pixelCount` is 32,800 B on its own.

`luxel_core::budget` now owns the verdict itself, not just the constants:
`load_base`, `load_headroom`, `TIGHT_PERCENT` (the 85 % band, previously a
magic number in App.svelte) and `fit()`. Firmware, wasm model and UI all
import it, so the prediction still cannot drift from the device that enforces
it.

Verification: 6 unit tests in `budget.rs` carrying the measured numbers above;
`tools/ci.sh` green; `tools/stack-check.sh` `.stack` 24,828 B, no function
over budget, image +368 B; device-e2e's four capacity bands still pass, plus
two new checks driving a third mirror that reports `--heap-free 30720
--engine-heap 30720` (the pattern the starved mirror correctly rejects fits
there, and something bigger still warns). Driven in real chromium against
mirrors with and without `engine_heap`: "Chasing Rainbows & HSLuv" warns on
the uncredited one and is clean on the credited one; "Main Stage" warns with
the library hint. Screenshots taken.

**Not verified on hardware** — no device grant this session. `engine_heap`'s
on-device behaviour (populated from boot, `heap_free + engine_heap` roughly
invariant across a swap, smaller for a mapped library activation than a live
push, refreshed by a pixel-count change, untouched by a crossfade) is Gitea
**#363**.

## 2026-09-07 — Three library patterns that only broke in the 16x16 preview (#285)

Jeremy reported three patterns rendering wrong in the browser but fine on
the panel. None of it was the preview harness: the playground and the device
run the same `luxel-core`, and the playground's grid map (`lx_set_map_grid`)
is the same map the firmware installs. What differs is the *rig* — a gallery
2D pattern previews on a 16x16 grid at 60 fps, while the bench panel is
64x64 at 100+ fps — and all three patterns had a resolution or frame-rate
assumption baked in. Reproduced host-side against the same engine wasm, then
before/after in real chromium.

* **Easing Library v1.0** — the white velocity marker was picked with
  `abs(x - mx) < 0.7 / sqrt(pixelCount)`. That tolerance (0.0437 at 256 px)
  is wider than half the pixel pitch (0.0333), so it lit **two columns in
  191 of 600 frames (32 %)** — Jeremy's "two pixel ball". It now discovers
  the grid pitch from the map (the `mapPixels` scan `us-flag-2d.js` already
  used) and lights the single nearest cell: **1 pixel in 600/600 frames**, at
  16x16 and 64x64 alike. The column is clamped to the row, so the back and
  elastic families' overshoot presses the marker against the end instead of
  hiding it off-panel (it used to vanish for whole seconds there too).

* **US Flag 2D** — the star lattice clamped its pitch to "every other pixel"
  when the canton was too small for six columns of stars. On a 16-row panel
  that is a 6x9 canton with **15 stars covering 27.8 % of it**, which reads
  as a chequerboard, not as stars. Below six columns it now spreads what fits
  at a pitch of 3: **6 stars, 11.1 % coverage**, blue visible between every
  pair. The star radius formula also went from `(pitch-1)/2` to `(pitch-2)/2`
  — a diamond of radius r needs a pitch of 2r+2 to keep a dark pixel between
  neighbours, and at an odd pitch the old form made them touch. 32x32 and
  64x64 are byte-identical to before (26.7 % / 32.0 % star coverage): the
  documented nine-rows-of-7/6 arrangement is untouched.

* **Lissajous curve tracer** — the trail was stamped at the dot's new
  position once per frame and decayed by a per-*frame* factor, while the dot
  itself is driven by `time()`. So how far it jumps between stamps is purely
  the host's frame rate: at 120 fps a continuous stroke, at the browser's
  60 fps a scattered constellation. It now stamps the **segment swept since
  the last frame** (closest-point-on-segment, with `1/|s|^2` hoisted out of
  the pixel loop and a point-stamp fallback below 0.02 canvas units where the
  16.16 projection stops being reliable) and decays per second
  (`pow(fade, delta * 0.06)`, `fade` still being the 60 fps per-frame
  factor). Bright-cell agreement between a 60 fps and a 120 fps run of the
  same two wall-clock seconds: **Jaccard 0.486 → 0.947**.

Three regression tests in `crates/luxel-core/tests/engine.rs` drive the
shipped `library/` sources over the playground's own grid map; all three fail
on the pre-fix patterns with the numbers above. `tools/ci.sh` green,
`check-library.sh` 305/305 on all five rigs.

Not touched: on a 16-row panel the wave still tears the 13 stripes into
blotches, because a stripe is 1.23 rows there — inherent to the geometry, and
not what the issue reported. No device was touched: the flag is byte-identical
at 32x32 and 64x64 over 90 frames, and a visual confirmation of the other two
on the panel is Gitea #361.

## 2026-09-07 — QEMU takeover tests green again: the pin assertion was stale, not the import (#273)

`tools/qemu/run-all.py` had been red on master since 2026-09-05 — all three
takeover tests failing on `takeover: WLED drove the strip on GPIO18` never
appearing in boot 1. It is a stale assertion, not a regression, and the serial
log says so: boot 1 still prints every other inherited field
(`settings carried over (30 px, ws2812, order rgb, brightness 16/31, cap
850 mA, gamma 2.8)`), so the wiring import plainly ran.

What changed under it is #154/PR #240 (v0.1.40), which gave the strip data pin
a runtime setting and added an arm to `takeover.rs`'s pin decision:

```rust
Some(pin) if pin == crate::board::DEFAULT_DATA_PIN as i32 => None,
```

The checked-in fixture `athom-wled-fs-configured.bin` carries
`"ins":[{...,"pin":[18],...}]`, and the Athom's `DEFAULT_DATA_PIN` **is** 18 —
so the import reads the pin, sees it already matches, and deliberately stores
no override (a stored pin would follow the config onto a board with a different
default). Boot 2's `strip data pin: GPIO18 (board default)` is the correct end
state, not evidence that nothing was imported.

Fixes, all in the harness — no firmware behaviour change (CLAUDE.md's QEMU
isolation rule):

* `takeover-test.py` now asserts the *outcome* per fixture pin instead of one
  transient message: for a pin equal to the board default it requires that no
  data-pin line is printed at all and that boot 2 says "board default", and it
  checks LXDV byte 21 (v8's `data_pin+1`, 0 = default) to match.
* The record-version assertion was stale the same way — `ver == 7` where
  `config.rs` has been at `DEV_VER = 8` since #154 took one of v7's pad bytes
  for the pin. Now keyed off a named `DEV_VER` constant.
* New `--wled-pin N`: rewrites `hw.led.ins[0].pin[0]` in the littlefs fixture
  before composing, so the one dump covers all three arms of the decision. It
  is a same-length byte poke into a CTZ **data** block — cfg.json is ~1.3 KiB,
  well over littlefs's inline threshold, and data blocks carry no CRC (only
  metadata pairs do), so every checksum, CTZ pointer and file size stays valid.
  Verified end to end: the unmodified shipping image parses the rewritten value
  out of emulated flash and writes `GPIO19` into LXDV.
* Two new suite entries covering the arms the fixture never could:
  `takeover-pin-import` (`--wled-pin 19` → imported, boot 2 "configured",
  LXDV byte = 20) and `takeover-pin-reserved` (`--wled-pin 10`, an ESP32
  SPI-flash pin → refused with the rewire hint, board default kept).

`tools/qemu/run-all.py` is now **8/8 PASS in 27 s warm** (takeover
app1 4.9 / app0 1.6 / fault 5.7 / pin-import 1.6 / pin-reserved 1.7,
heap-regions selfheal 8.6 / rollback 0.7, flashmap 0.9).

Also: an **opt-in** `qemu` step in `tools/ci.sh` behind `CI_QEMU=1`, so the
suite has a home in the gate script without going into the hosted gate — that
would want a from-source build of Espressif's QEMU fork on the runner, and five
of the eight tests need the gitignored Athom dumps CI does not have (they skip
there). `CI_QEMU=1 CI_SKIP="web cargo library firmware" nix develop --command
tools/ci.sh` is 64 s end to end here. Making it a real gate is Gitea #359.

Doc drift found on the way and fixed: `docs/wled-migration.md` still said
WLED's data pin "is only logged — Luxel pins are compile-time per board", and
`WledWiring::pin`'s doc comment in `firmware/src/wledfs.rs` said the same. Both
predate #154.

## 2026-09-07 — blur1D slides a window instead of building a prefix sum (#296)

`blur1D(arr, radius)` allocated a full `len + 1` array of `i64` prefix sums
— 8 bytes **per element** — and read windows out of it. On the 64x64 panel
that is a 32,776-byte transient allocation for a `pixelCount`-sized buffer,
more than a loaded device has spare: #295 had already made the failure
clean rather than fatal (an infallible `Vec` aborted the firmware outright,
crash-looping the board on `library/comets.js` until the boot guard rolled
the slot back), but the blur still simply did not run there.

A box blur only needs the originals still inside the sliding window, and
those are exactly the ones already overwritten — indices `i+1-r ..= i`. The
new `blur1d_inplace` keeps a running `i64` sum plus a ring of
`min(radius + 1, len)` originals, subtracting the element leaving the window
and adding the one entering it:

| radius | array | scratch before | scratch after |
|---|---|---|---|
| 1 (`library/comets.js`) | 4096 px | 32,776 B | **16 B** |
| 1 | 60 px | 488 B | **16 B** |
| 8 | 4096 px | 32,776 B | **72 B** |
| 4096 (radius ≥ len) | 4096 px | 32,776 B | **32,768 B** (ring caps at `len`) |

so the peak is O(radius), never worse than the old O(n), and a few dozen
bytes at the radius 1–8 patterns actually use.

**Bit-identical, not merely equivalent.** The window sums are the same exact
i64 integers accumulated in a different order and divided the same
truncating way, so every output raw is unchanged. Two proofs kept in the
tree: `vm::blur1d_tests::sliding_window_matches_prefix_sum_reference` runs
the new code against the old prefix-sum implementation, kept verbatim as a
test-only reference, over 14 lengths (1, 2, 3, 4, 5, 7, 8, 16, 17, 63, 64,
255, 256, 4096) × 13 radii (0 … 100,000, i.e. far past the array) × 3 input
shapes (random 0..1 16.16 values, wide signed values that exercise
truncation toward zero, a sparse impulse train), asserting raw equality
element by element. And 12 PPM frame strips — `library/comets.js` (the one
gallery pattern using `blur1D`) at 1/2/60/300/1000/4096 px and a five-radii
scratch pattern at 1/2/3/60/300/4096 px — are byte-identical between
`origin/master` and this branch.

Throughput is unchanged: best of 5 `luxel bench` runs at 4096 px on the
blur-heavy scratch pattern, 21.02 Mpx/s before against 21.37 after (within
this box's 6–19 % run-to-run spread). The ring cursors are advanced by hand
rather than with `i % cap` — the modulo is a hardware divide per element and
cost a measured ~14 % on the first version of the patch.

`blur2D` is untouched: it already reuses one `max(w, h) + 1` prefix line (65
elements on a 64x64 panel), which is O(side), not O(n).

Verified: `cargo test -p luxel-core` (144 + suites, green), `tools/ci.sh`
green end to end. No device touched — the on-panel confirmation rides the
next OTA.

## 2026-09-07 — wire-check.sh: the wall of FAILs was the argument, not the firmware (#346)

`tools/wire-check.sh` reported FAIL on nearly every check against a healthy
device, with the captured values printed as empty parentheses and
`stat: cannot statx .../wc-body`. It reads as a firmware catastrophe. It was
the argument form.

The script takes a bare IP and prepends the scheme itself. Called as
`tools/wire-check.sh http://192.168.0.238` — which is how every other device
tool in the repo is called — it built `http://http://192.168.0.238/...`,
where curl parses the authority as the host `http` and gives up with exit 6,
`Could not resolve host: http`. Every curl carried `-s` and no `-S`, so that
error went nowhere; `-D -` produced an empty header capture and `-o` never
created the body file, and each assertion then compared empty strings.
Chasing `TMPDIR` was a dead end because the temp dir was never the problem.

Fixed on both axes:

* **The argument is normalized.** `192.168.0.183`, `http://192.168.0.183`
  and `http://192.168.0.183/` all work now.
* **A capture that does not land is loud.** Every request goes through a
  `req` helper that keeps curl's exit status (with `-S`, so the message
  survives) and asserts the header capture is non-empty; body sizes go
  through `bodysize`, which aborts on a missing file. Both print
  `HARNESS BROKEN: …` and exit **2**. Connection-class curl exits (6/7/28/
  35/56) print `DEVICE UNREACHABLE` and exit **3**. So the exit code now
  says which of the three things happened: 0 all pass, 1 a real contract
  FAIL, 2 harness, 3 network. A harness fault can no longer masquerade as a
  per-check firmware regression.
* One capture is still legitimately allowed to be absent — curl never
  creates the `-o` file for a body-less 304 — and that case keeps its
  tolerant check. A 200 with no ETag now SKIPS the 304 block with a note
  instead of asserting against an empty `If-None-Match`; the missing ETag is
  already a FAIL one line above.

Verified end-to-end against the Athom rig (192.168.0.183, v0.1.40, ota_0, 60
px): **ALL PASS, exit 0** for all three argument forms, including the exact
`nix develop --command tools/wire-check.sh http://192.168.0.183` invocation
from the bug report. The guards were exercised too: unreachable host → exit
3, unwritable `TMPDIR` → `HARNESS BROKEN` exit 2, missing body capture →
`HARNESS BROKEN` exit 2. No firmware defect behind any of the original
FAILs — the device answered every check correctly the whole time.

## 2026-09-07 — Store forwarding: the stored value stays on the stack for the next statement (#320)

Assignment is an expression, so `StoreL` and `StoreG` PEEK — the assigned
value is still on the stack after them. A statement that stores a variable,
followed by a statement whose first operand reads it back, has therefore
always compiled to `StoreL a; Pop; LoadL a`: throw the value away, read it
straight back. A new pass, `compile::forward_stores`, deletes the
`Pop; LoadL a`.

It is deliberately NOT a peephole template. The #261 peephole may never
carry a value across a source position — that is what keeps the debugger
stopping once per statement — and this rewrite does exactly that, so it
stands on its own argument instead:

* **The stack is identical at every point.** The base sequence is at depth
  `d` after the store, `d-1` after the `Pop`, `d` again after the load; the
  rewrite is at `d` throughout. Peak depth is unchanged, so every
  `MAX_STACK` verdict is too — the single elided push-with-check could only
  have failed at a depth the sequence had already reached.
* **The debugger still stops on the second statement.** Only that
  statement's FIRST instruction is deleted, never its last: the pass
  requires a successor, and a statement that reads a variable always emits
  more than the read (an expression statement appends `Pop`, a `return`
  appends `Ret`). Its position run survives one instruction shorter and
  `pos_at` still answers with its line.

Guards: neither the `Pop` nor the load may be a jump target, the `Pop` must
carry the store's own source position, the load must name the slot the store
wrote, and the load may not be the function's last instruction. A read that
is not the next statement's first operand (`var v = 1 - d * 4` after
`var d = …`) never matches, because the load is not the instruction after
the `Pop`.

**Measured over all 305 library patterns**, 256 px on a 16×16 grid, 20
frames each:

| | dispatches | insns/px |
|---|---:|---:|
| `--no-storefwd` | 101,708,320 | 65.131 |
| default | 100,990,763 | **64.671** |

−0.71 % library-wide; 155 patterns get measurably faster and none gets
slower — "none" is structural, not sampled: no library pattern grows a word
with the pass on, asserted per file in both the fused and unfused
configurations. (The two clock patterns wobble by ±0.1 insns/px between
sweeps because they branch on the time of day; run A/B back to back and they
are identical.)
The heaviest single wins are `1d-aurora-borealis` and `perlin-fire-wind`
(−5.00 insns/px each), `kaleidoscope-2d` (−4.01), `color-twinkles`,
`perlin-fire`, `static-random-colors`, `xorcery-2d-3d` (−4.00).

**Why it is 0.7 % and not the ~1.5 % the ticket hoped for: #320 and #261
overlap.** With the peephole OFF the pass fires at **1,183 sites across 271
of the 305 files** — just over half the ~2,300 store-then-read pairs an AST
scan finds, the rest failing the "first operand of the next statement"
requirement. With the peephole ON that becomes **465 words saved across 197
files**: at the other 718 sites the deleted `LoadL` was already being folded
into a `LoadLL`/`LoadLIdx`/`LoadLConstOp`/`LoadLG` superinstruction, so the
window cost one dispatch either way. It never costs more — no library
pattern grows a word in either configuration, asserted per file.

The static blob shrinks too: −465 instructions, −465 words, −1,860 bytes
across the library. Firmware images are **byte-identical** — the compiler
frontend is behind luxel-core's `frontend` feature and the firmware links it
out, so the pass is free on device (`board-pixelblaze-v3` 1,006,368 B,
4.02 % OTA margin, `cmp`-equal to the same tree without the change;
re-measured after rebasing onto #306).

`--no-storefwd` on `luxel run|bench|compile` is the A/B lever, the third
independent switch alongside `--no-fuse` and `--no-fold`.

Verified: 610/610 byte-identical PPM renders (all 305 patterns × a 300 px
strip and a 16×16 grid, 24 frames, seed 7, with and without the pass);
`check-library.sh` 305/305 on all five rigs; `cargo test --workspace` 355
tests including a new `tests/storefwd.rs` (10 sources × every
fuse/fold/forward combination, LXBC round-trip, runtime-error text and
position, compile errors, the debugger's line sequence under `StepKind::Into`,
the full `debug_stack` locals trace at every stop, and a stack overflow);
`tools/ci.sh` green. The playground debugger was stepped through a
store-then-read chain in real chromium on both builds — line 2→3→4→5→6,
wrapping to the next pixel, with the locals panel showing identical values at
all 14 stops.

Not done here (no device available): the Xtensa instruction-count and
cycle measurement the ticket asks for — `STORE_L_POP` + `LOAD_L` = 94
instructions → `STORE_L` = 58 is a pre-#314 estimate and needs re-measuring
on the panel with `tools/opbench.mjs`. Left on #320.
## 2026-09-07 — the frame pipeline: compose + outpipe on core 0 (#306)

On the 64x64 HUB75 panel the frame period was `vm + out`: the render task on
the AppCpu evaluated the pattern and then composed the panel's bitplanes
itself, and that compose is a flat ~6 ms at 4096 px whatever the pattern
does. It is now `max(vm, out)` — a new `output_task` on the ProCpu does the
compose, the output pipeline and the driver write while the AppCpu is already
rendering the next frame. Measured on the Seengreat panel at 4096 px, nine
`/api/status` samples per row after a 6 s settle, medians:

| pattern | fps before | fps after | out_fps | frame_us | vm_us | pipe_us | out_us |
|---|---:|---:|---:|---:|---:|---:|---:|
| empty render | 99 | **125** | 123 | 5,531 | 5,478 | 7 | 3,396 |
| rgb-only | 60 | **87** | 87 | 11,487 | 11,430 | 8 | 4,446 |
| rainbow | 39 | **52** | 52 | 19,475 | 19,421 | 11 | 6,654 |
| snake (1D) | 19 | **21** | 21 | 48,185 | 48,109 | 21 | 6,724 |
| snake-2d | 12 | **13** | 13 | 77,478 | 77,395 | 36 | 6,791 |

`firmware/src/pipeline.rs` holds both shapes: `DirectSink` (the render task
owns the driver and runs every post-VM stage inline — unchanged, and what
every strip board and single-core chip still uses) and `RenderSide` +
`output_task` behind the new `pipelined` cfg (`hub75` + `multi_core`). ONE
frame buffer travels between the cores in a `CriticalSectionRawMutex` cell
announced by a `Signal`; the render task borrows it only for the copy and
holds nothing between frames, so a busy output task costs a dropped frame —
the best-effort contract `write_frame` already had with the DMA swap — never
a blocked VM. The frame is copied rather than swapped because `Engine::frame`
may legitimately return the PREVIOUS frame (a pattern under its own frame-rate
cap), and 12 KB is ~50 µs against a 5-77 ms frame.

**It costs no RAM.** A pipeline needs one more live frame than a serial loop,
and at 4096 px that 12 KB is not there: the first cut of this change pushed
`library/snake-2d.js` below `RUNTIME_FLOOR` and the panel refused to load a
pattern that had worked. The shipped version pays for the travelling buffer by
deleting the frame it replaces — `shared::PIXELS`, the `/api/pixels` snapshot,
was a second full copy of the frame that had just been composed. On a
pipelined build `pipeline::preview` reads the travelling buffer out of its
slot and `set_pixels` is never called. Idle `heap_free` is identical to master
row for row, and `pipe_us` fell from ~44 to ~11 µs with the memcpy gone.

`library/snake-2d-v2.js` (#335's `renderFrame` + `fillCanvas` rewrite, landed
the same day) is the pipeline's biggest beneficiary and worth recording next to
the rest: at 4096 px it goes **77 -> 109 fps on the wire** (+42 %), because at
`vm` 7.2 ms against `out` 5.5-6.8 ms its two halves are nearly balanced — the
`max(vm, out)` sweet spot, where `snake-2d` (vm 77 ms) gains almost nothing.
Its own win over `snake-2d` is the rewrite, not this change: on the same master
build `vm_us` 77,466 -> 7,194 (-90.7 %), 12 -> 77 fps. Note its `vm_us` means
something different — it exports `renderFrame` and no per-pixel `render2D`, so
that number is one call plus a 256-cell canvas repaint, with the per-pixel work
inside the native `fillCanvas`, not 4096 VM entries.

`pipeline::preview` is deliberately ONE fallible allocation, reserved outside
the slot's critical section. The first cut made two (a `Vec<[u8; 3]>` inside
the lock, then `to_vec()` to flatten it) and panicked the panel on
`GET /api/pixels` under the 2D snake — caught on serial as `memory allocation
of 12288 bytes failed`, a software reset, and after a few of them a boot-guard
rollback. Re-tested with both slots carrying the fix: 30 concurrent
`/api/pixels` requests under the 2D snake at 4096 px, zero panics, no reboot.
(The pre-#306 path had the same hazard and less defence — `get_pixels()` is an
infallible `Vec::clone` inside the same kind of critical section.)

New `/api/status` `out_fps`: frames actually written to the wire. It differs
from `fps` (frames *rendered*) exactly when the output stage is the slower
half, and on a pipelined board `frame_us` is now the render period —
`pipe_us`/`out_us` come from the other core and no longer nest inside it.

**Not gated on `multi_core`.** Measured on the Athom at 420 px with the gate
widened: the wire rate went 59 → 60 fps — a strip's `out_us` is WS2812 wire
time, not CPU work — while the playground bundle download went 1.29 → 2.60 s
and the fence's worst park wait 96 → 6,223 µs, because the ProCpu now spends
the frame busy-waiting on SPI DMA. That is the #259 starvation the second-core
split exists to prevent. `fps` read 123 on that row (the render task
free-running at its pacing cap, producing twice the frames the wire can take),
which is exactly the illusion `out_fps` was added to puncture. The strip win
wants deferred-DMA overlap on the render core instead — Gitea #343.

Verified on the panel: the five-pattern table before and after, a 736 KB asset
push and an OTA under the 2D snake (`fence_timeouts` 0, 4,424/4,424 fences
completed, no wedge), a playlist crossfade cycle, DDP live input in and out,
`/api/pixels` served from the travelling buffer, `tools/render-bench.mjs` at
4096 px (#259 bundle download 1.43-2.27 s, unregressed), and a 32-minute 2D
snake soak with no missed poll, no reboot and a flat heap. `tools/ci.sh`
green, all seven boards build with `image-check`, `tools/stack-check.sh` ok,
QEMU flashmap + heap-regions pass (the takeover trio is the known-red #273).

## 2026-09-07 — renderFrame: one call per frame instead of one per pixel (#335)

A pattern may now export `renderFrame()` instead of `render`/`render2D`/
`render3D`. The engine calls it ONCE per frame after `beforeRender`, lends it
the frame buffer (a `mem::take` of the engine's RGB888 Vec into `Vm::frame`,
never a copy), and the pattern paints with sixteen new bulk builtins
(ids 166–181, appended, no LXBC bump): `gridWidth` `gridHeight` `clear` `fill`
`fade` `setPixel` `fillRange` `fillHSV` `fillRGB` `fillGradient` `fillRect`
`fillCircle` `splat` `drawLine` `fillCanvas` `blit`. The brush for the shape
ops is whatever `hsv()`/`rgb()`/`paint()`/`oklch()` last set, so every
existing colour builtin works unchanged. Full design and evaluation:
docs/bulk-render.md; language reference: docs/lang.md "Whole-frame rendering".

Why this and not more interpreter tuning: the per-pixel entry costs
~317–440 cycles/px on the S3 panel (5.4–7.6 ms/frame at 4096 px) and #314's
−18 % dispatch win left it at exactly 5,409 µs — only a whole-frame entry
removes it. A survey of all 299 library patterns (`luxel bench --profile`
at 256 and 1024 px, fitted per-pixel vs fixed) says who benefits: 87
persistent-buffer readouts, 29 entity loops, ~20 range fills, 28 canvas
readouts, 9 constant fills. ~110 dense-procedural patterns do NOT — every
pixel is a different computation, and the doc says so rather than pretending
a `fillNoise` would help.

Three "spaces": index ops touch pixel `i` on any map; coordinate ops are a
predicate over each pixel's MAPPED (x, y) exactly as `render2D` sees it, so
sparse/irregular maps just work (unmapped space has no pixels to fill) and a
grid fast path iterates only the shape's bounding-box cells — required and
tested to be byte-identical to the generic scan (12 rounds × 4 random shapes
× 8 rigs incl. serpentine and transposed coordinate grids); grid ops (`blit`)
need a grid that COVERS the frame and clip the tail of an over-provisioned
`ceil(√n)` default grid (first cut required an exact match and silently
no-oped on every non-square strip — caught by the sprite pair on 300 px).

Host, `tools/pairbench.mjs` (new), 4096 px / 64×64 coordinate map, best of 5:

| pair | px ns/px | bulk ns/px | ratio |
|---|---:|---:|---:|
| empty render vs empty renderFrame (the entry alone) | 9.68 | 0.004 | 2201× |
| constant `hsv` vs `fill()` | 15.75 | 0.21 | 75× |
| per-pixel hue vs `fillGradient` (the default rainbow) | 20.74 | 4.76 | 4.4× |
| `hsv(hues[i],1,vals[i])` vs `fillHSV(hues,1,vals)` | 19.15 | 4.16 | 4.6× |
| array trail vs `fade`+`setPixel` (no array at all) | 17.27 | 0.67 | 25.7× |
| 3 block tests vs 3× `fillRange` | 36.85 | 0.28 | 133× |
| 6-ball `hypot` loop vs `clear`+6 `splat` | 441.56 | 14.06 | 31.4× |
| 32×32 canvas readout vs `fillCanvas` | 54.49 | 11.34 | 4.8× |
| 8×8 keyed sprite vs `blit(…,3)` | 69.51 | 0.31 | 225× |
| 8 segment distances vs 8 `drawLine` | 896.74 | 51.61 | 17.4× |
| **dense perlin control** | 138.84 | 184.36 | **0.75×** |

The control going backwards is the point: rewriting a dense pattern as a
`renderFrame` loop over `setPixel` puts the loop bookkeeping into bytecode
(27 → 52 insns/px). The host understates the device win ~3× (docs/boards.md).

Firmware: nothing in firmware/ changed, but luxel-core grew the app image by
+14.9 KB on the first cut, dropping `board-pixelblaze-v3` to a 2.58 % OTA
margin — under the 3 % gate. A size pass (dyn closure instead of four
monomorphized `paint_shape`s, shared texel helpers, out-of-line `put`/
`map_coord`/`span`, static error strings, `splat` as a zero-length
`drawLine`) took 6.5 KB back bit-identically. Rebased onto #328's placement
work (base 974b3b3) the delta is **+8,272 B: 4.02 % margin on
pixelblaze-v3**, 9.86 % on the HUB75 S3, `.rwtext` byte-identical (the
sixteen arms sit in `builtin_cold`, tier 3 — once per frame, never per
pixel). `board-c6-devkit` goes 3.13 % → 2.29 %: #328 had just lifted it back
over the floor, so this branch is now what pushes it under (#291; not the
CI board). `.stack` −64 B. The two coordinate-shape pairs pay ~10 % for the
indirect call; the doc records it. The rebase itself was a clean merge that
did not compile: #328 hands each dispatch tier a fixed `[Value; 16]`, and the
bulk arms wanted the old exact-length slice — `&args[..argc]` restored it.

Six library patterns ship with it (`bulk-rainbow`, `bulk-comet-trails`,
`bulk-bouncing-balls-2d`, `bulk-sprite-scroll-2d`, `bulk-canvas-ripples-2d`
and `snake-2d-v2`, the Infinite Snake rendered through one `fillCanvas`);
gallery tiles classify a pattern naming a coordinate op as `grid`, guarded
against patterns that define their own `splat`/`drawLine` (three do — they
still shadow the builtins and compile). 32 new tests; 346 pass;
`check-library.sh` 305/305 on all five rigs; chromium-verified.

Not done here (devices were in use): the I-cache regression check on a
NON-bulk pattern (`call_builtin` gained sixteen arms; #318/#325 showed layout
alone swings real patterns by tens of percent) and the on-device patbench of
the bulk patterns — Gitea #336. #265's dual-core split does not apply to a
`renderFrame` pattern (one call per frame); noted there.

## 2026-09-06 — Engine vs engine on builtin-heavy patterns: Luxel is at parity with the Pixelblaze (#312)

The straight-line loop microbench says the Luxel VM is 1.51× slower per
iteration than the Pixelblaze's on identical silicon (6.20 vs 4.11 µs). Nobody
had compared a *real* pattern, because the oracle is wire-bound at 77.3 fps at
420 px and a simple pattern never breaks through that cap. Builtin-heavy
patterns do.

`tools/oracle/fps-compare.mjs` (new) live-codes the same sources onto the
oracle and onto a Luxel device at the same pixel count and tabulates both. Its
`--1d` flag is the load-bearing part: the oracle has a 2D map installed and a
PB map is a one-way door, so it dispatches `render2D` while a 1D strip
dispatches `render` — `--1d` rewrites both copies identically so each engine
runs the same body on the same arguments.

**Measured at 420 px, oracle (fw 3.67) vs Athom (`6cf19b4`, v0.1.40), 25 s of
samples per pattern after a 6 s settle.** PB µs/px against Luxel `vm_us`/px:

| pattern | PB µs/px | Luxel vm µs/px | PB ÷ Luxel |
|---|---:|---:|---:|
| `perlin-fire-wind-tunnel` | 54.98 | 58.20 | 0.94× |
| `coral-plasma` | 76.41 | 75.25 | 1.02× |
| `eye-of-sauron` | 75.52 | 85.50 | 0.88× |
| `blue-holiday-star-2d` | 82.51 | 89.64 | 0.92× |
| `dire-spider-2d` | 365.31 | 311.80 | 1.17× |

Within ±17 %, mean 0.99× — parity, not 1.5×. The dispatch gap is real but it
only shows on straight-line VM work; a pattern whose time goes into `perlin`,
`hypot`, `sin` and `hsv` spends it in native code on both engines, and there
Luxel is competitive. So #312's 1.51× is a ceiling on what interpreter-dispatch
work can win back on real patterns, not a headline slowdown.

**Second result, which the comparison needed first: the Pixelblaze overlaps LED
output with rendering.** Extrapolating the K-sweep loop line back to K = 0
gives a *negative* fixed per-pixel cost (−16 to −20 µs/px) under a
`frame = wire + render` model and a plausible +10…15 µs/px under
`frame = max(wire, render)`. So a PB fps below its wire cap is engine time and
comparable to `vm_us`; an fps pinned at 77.3 is output-bound and means nothing.
Luxel on the classic ESP32 does not overlap (`frame_us ≈ vm_us + pipe_us +
out_us`), so its fps is the wrong column to compare. Written up in
docs/research/04-oracle-findings.md.

Both devices restored and verified: oracle 420 px / "rainbow melt"; Athom
60 px / ws2812 GPIO18 / rainbow / ota_0 v0.1.40 / store empty / no map.

## 2026-09-06 — Code placement for the flash instruction cache: 2.9–4.2× on the classic ESP32 (#328)

#325 showed the dominant term in `vm_us` is whether the interpreter's own
native code stays resident in the flash instruction cache: moving ~300 B out
of the dispatch loop was 1.87× on a real pattern while reading −4.5 % on the
loop microbench. This is the follow-up that acts on it, measured with BOTH
`tools/opbench.mjs` and `tools/patbench.mjs` on both Xtensa boards (#327), on
master `e08b2b2`.

**Two changes, and they only work together.**

1. **The builtin dispatch is a three-tier ladder.** `builtin_fast` (22 arms,
   in the loop) → `call_builtin` + a new `builtin_hot` (the ~30 builtins a
   `render` calls per pixel: transcendentals, the noise family, `dist`/
   `hypot`, `paint`, `canvasGet`, `pixelState`) → a new `#[cold]
   #[inline(never)] builtin_cold` holding the other 90 arms — easings,
   beziers, arrays, transforms, palette/GPIO/clock/canvas-write setters —
   reachable only through `builtin_hot`'s `_` arm. `call_builtin` goes
   **18,817 → 1,980 B**; the per-pixel dispatch footprint goes 32.0 → 20.2 KB.
   The tiering is measured, not guessed: `tools/profile-library.mjs` over all
   299 `library/` patterns leaves a **40× gap** between the least-used tier-2
   arm (≥ 0.6 calls/px in the pattern that uses it) and the most-used tier-3
   arm (≤ 0.016 calls/px); tiers 1+2 answer 99.4 % of the library's builtin
   calls. The `noise` entry points are now `#[inline(never)]` for the same
   reason — inlined, `simplex2_inner`/`simplex3_inner` put 7 KB *inside*
   `builtin_hot` and every `sin`-only pattern dragged it through the cache.

2. **The hot tiers execute from internal SRAM.** Three device-only cargo
   features on `luxel-core` add `#[link_section = ".rwtext"]` (what
   `esp_hal::ram` expands to, spelled by hand — luxel-core must not depend on
   esp-hal): `iram-vm` (`Vm::run`, 13,572 B), `iram-builtins` (`call_builtin`
   + `builtin_hot`, 7,304 B), `iram-math` (`hsv_to_rgb`, the hot `fmath`, the
   `noise` entry points, 9,412 B). Which board takes which is `IRAM` in
   `firmware/board-target.sh`, mirrored by `iram` in flake.nix's
   `firmwareVariants`, with `IRAM_OFF=1` as the A/B lever on `build-esp32.sh`
   and `tools/stack-check.sh`.

**On metal** (µs/px; Athom @ 256 px, Seengreat panel @ 4096 px; every build
OTA'd, `opbench` and `patbench` in the same run):

| | cycles/op | `rainbow` | `perlin-fire-wind-tunnel` | `kaleidoscope-2d` | `snake` |
|---|---:|---:|---:|---:|---:|
| Athom, master | 104.9 | 6.379 | 167.51 | 626.23 | 15.469 |
| Athom, shipped | **100.8** | **6.184** | **58.53** | **150.20** | **14.322** |
| | −3.9 % | −3.1 % | **−65 % (2.86×)** | **−76 % (4.17×)** | −7.4 % |
| panel, master | 84.0 | 4.752 | 45.39 | 102.36 | 11.696 |
| panel, shipped | **83.4** | **4.719** | **44.79** | **101.16** | **11.531** |
| | −0.7 % | −0.7 % | −1.3 % | −1.2 % | −1.4 % |

Same code, same features on the dispatch loop, 2.9–4.2× on one chip and 1 % on
the other: the classic ESP32's render task is cache-starved and the S3's is
not. `opbench` alone would have reported this whole piece of work as "4 %".

**The ladder that got there, and the two non-wins in it** (Athom, geometric
mean over the four probes, master = 1.00):

| variant | gm | note |
|---|---:|---|
| `iram-vm` only | 0.68 | `Vm::run` in SRAM; already most of the win |
| hot/cold split only | **1.38** | **a loss**: +80 % on `perlin-fire-wind-tunnel`, +73 % on `snake` |
| split + `iram-vm` | 0.76 | |
| split + `iram-vm` + `iram-builtins` | 0.67 | best `perlin-fire`, worst `kaleidoscope` |
| **split + all three (shipped)** | **0.52** | |
| all three without the split | 0.56 | and **13.7 KB more IRAM** (56,384 vs 42,644 B of `.rwtext`) |

`snake` executes nothing but `Vm::run`, which is **byte-identical (13,170 B)
in every one of those builds**, and it still swung 15.5 → 26.8 → 14.3 µs/px.
The effect is pure placement — which is why the split alone is a loss and only
pinning the code in SRAM makes it reproducible. Pre-#332 measurements from
earlier the same day are in the Gitea thread and are NOT comparable: the store
rewrite moved the baseline 7 % on its own.

**The budget is per chip.** On the classic ESP32, IRAM is a dedicated 128 KB
region (SRAM0) that `.stack` never comes out of — all three features fit with
35,604 B to spare and `.stack` stays 24,932 B. On the S3 it is the *same* SRAM
as the stack: `iram-vm` alone costs 46,020 → 32,452 B of `.stack`, and adding
`iram-builtins` left 572 B over `tools/stack-check.sh`'s 24 KB floor for ~1 %,
so the S3 boards take `iram-vm` only. The RISC-V boards take nothing — there
is no C3 or C6 on the bench, and the C3's 16 KB icache makes it the most
interesting untested case (Gitea #337; `RISCV_IRAM=` is the lever).

Slot cost: the classic-ESP32 app image is ~3.4 KB **smaller** with the
placement on (bytes move out of the 64 KB-page-aligned flash text segment);
`board-c6-devkit`, which takes no IRAM features, pays ~1.1 KB for the split
alone (3.27 % → 3.17 % of slot free). All boards stay over the 3 % floor.

Verification: `cargo test --workspace` green, `tools/check-library.sh`
1495/1495, **40/40 byte-identical PPMs** against master (128 px × 12 frames),
`tools/stack-check.sh` clean on all six boards, QEMU flashmap + heap-regions
green (takeover trio known-red, #273), `tools/ci.sh` green. Reference:
docs/firmware.md "Code placement", docs/boards.md "IRAM budget".

## 2026-09-06 — One mappable extent store: source and bytecode stop being written twice (#330)

The `storage` partition was half a `sequential-storage` map and half raw
mappable pages, and the seam showed: a saved pattern's LXBC was written
**twice** — as ≤3,840-byte chunk items in the map, which can never be
mapped, and again as an arena extent, which is what the VM actually
executed. The map was the source of truth for the source text; the mappable
half was a bolt-on to a layout designed around the map. Jeremy's read
(#330): design the partition around what the device does with the bytes.

**The layout now.** 1 MiB, 256 pages:

| rel | abs | size | region |
|---|---|---:|---|
| `0x00000` | `0x210000` | 128 KiB (32 pages) | **key area** — `sequential-storage` |
| `0x20000` | `0x230000` | 896 KiB (224 pages) | **extent region** — mapped read-only at boot, 14 × 64 KiB MMU entries |

and inside the extent region: 1 header page, 8 pages of ad-hoc source,
2 × 16 pages of two-sided ad-hoc bytecode, and **183 pages (732 KiB)** of
extent arena — up from 87. A stored pattern owns two extents, its source
and its bytecode. One save is one extent write per blob. The chunk copies
are gone entirely, and so is `cache_code`: there is nothing to fill in
later, because the save wrote the executable bytes in the first place.

The key area keeps what a log-structured map is actually good at — small,
hot, power-loss-safe writes: playlist, playstate, pixel map, resume record,
palette, format key, and the **directory**. ~12 KB of live data in 128 KiB
is an order of magnitude of GC headroom, and every op's page scan is 4×
cheaper than over the old 512 KiB range.

**The directory is one map item**, so the pattern list and the extent table
cannot disagree: `[ver][npat] npat × {seq, gen, name_len, name}` followed by
`extents::Dir::to_bytes` (per extent: seq, gen, **kind**, start page, len,
FNV-1a). Writing it is the store's atomic commit point — an extent is
reachable only after the item that names it landed, so a power cut before
that leaves the previous generation whole and the new extents unreferenced
(boot rebuilds the page bitmap from the directory, so their pages come back
free). Its 3,398-byte worst case is what caps `MAX_PATTERNS`, asserted at
compile time.

**What the read paths look like now.** `GET /api/patterns/:id` escapes the
source into one pre-sized response buffer straight from the mapping. The
running pattern's read-back (`GET /api/pattern`, `GET /api/pattern.lxp`)
streams the source extent as the response body — `stream_store_readback`'s
`String` fetch is gone from that path. `source_stat`, which every library
swap calls for the identity hash and Content-Length, is now two directory
fields: no flash read, no allocation. A library activation reads nothing
but the mapping.

**Caps, deliberately.** `MAX_SOURCE` 30,720 → **32 KiB** (8 pages),
`MAX_BC` 38,400 → **40 KiB** (10 pages) — both were chunk-count artifacts,
now page-rounded. `MAX_PATTERNS` 24 → **32**: a pattern used to cost up to
18 map items and now costs two extents plus one directory record, so a
larger library is genuinely cheap (32 median patterns use ~64 of 183 pages).

**No migration.** `FORMAT_VERSION` 4 → 5 wipes the key area on mismatch and
the playground re-syncs the library — Jeremy's call on #330, he is the only
user. The extent region is left alone by a wipe: with no directory nothing
references it, and every page is erased before it is written again.

**Concurrency.** The pin set (#260) is unchanged and now covers a pattern's
source extent as well as its bytecode, which is what makes streaming the
running pattern's source out of the mapping across `await`s sound. An
UNPINNED pattern's mapped bytes needed something new: `GET /api/patterns/:id`
copies from the mapping while a save on the other core might compact it, so
`patterns.rs` gained a reader/writer counter making mutations and unpinned
mapped reads exclusive. Readers hold it across a *synchronous* copy only
(microseconds), so the writer's bounded retry converges.

**Measured.**

| | before | after |
|---|---:|---:|
| extent arena | 87 pages (348 KiB) | **183 pages (732 KiB)** |
| bytes written per save | source chunks + bc chunks + a duplicate bc extent | **one extent per blob** |
| `.stack` (pixelblaze-v3) | 25,484 B | **24,932 B** |
| Σ swap peak, 299 gallery patterns | 3,856,171 B (historical) | **1,899,668 B** mapped / 2,388,757 B `flashmap-off` |

`heapstat` grew a `swap(nomap)` column for the `flashmap-off` fallback,
which is a real firmware path: one transient bytecode Vec, no source Vec, no
envelope — 38.1 % under the historical lifecycle, against 50.7 % for the
mapped path. Six of 299 patterns exceed 45 KB at swap under the historical
lifecycle; **zero** under either current path.

Every board image got **smaller** — the store lost a whole chunk layer:

| board | before | after | Δ | OTA margin |
|---|---:|---:|---:|---|
| c3-devkit | 952,128 | 945,024 | −7,104 | 9.20 % → 9.88 % |
| pixelblaze-v3 | 1,005,312 | 999,584 | −5,728 | 4.13 % → 4.67 % |
| athom-music | 1,005,360 | 999,456 | −5,904 | 4.12 % → 4.68 % |
| esp32-generic | 1,005,120 | 999,280 | −5,840 | 4.14 % → 4.70 % |
| s3-devkit | 947,664 | 942,320 | −5,344 | 9.62 % → 10.13 % |
| **c6-devkit** | 1,020,384 | 1,013,312 | −7,072 | **2.69 % → 3.36 %** |
| c6-devkit-hosted | 1,002,784 | 996,912 | −5,872 | 4.37 % → 4.93 % |
| s3-hub75 | 940,032 | 934,400 | −5,632 | 10.35 % → 10.89 % |
| seengreat-hub75 | 940,064 | 934,272 | −5,792 | 10.35 % → 10.90 % |

The C6 full-UI build was **under** `image-check`'s 3 % OTA-slot floor
(#310); it is back above it, without anyone touching the UI.

**One toolchain scar.** The rewrite made the Xtensa LLVM fork abort
instruction selection on `resume_task`'s poll function —
`rustc-LLVM ERROR: Cannot select: i32 = Constant<24576>`, tracking
resume.rs' `stored * 2 + 24 * 1024` literal (change it to `23 * 1024` and it
fails as `Constant<23552>`). `#[inline(never)]` does not help; fat LTO folds
the body back in. `resume_headroom()` now computes it with a
`core::hint::black_box` around the constant, commented as the workaround it
is. The same source built fine before this change — the constant only
exposes the backend bug once the surrounding state machine is complex
enough.

**Verified host-side only.** `cargo test --workspace` (extents grew to 25
cases: source+bytecode extent pairs, a re-save publishing both new
generations before freeing either, a pin holding both of a pattern's
extents, `kind` in the serialized record, a second 4,000-step two-kind churn
fuzz); QEMU `flashmap-test` extended for the new boot narration and the
format wipe, `run-all.py` otherwise unchanged (the takeover trio stays red,
#273); `heapstat`; `stack-check`; all nine board images. Nothing has touched
flash on a device — the write/invalidate/hash discipline, the compaction
copy and the power-cut window are hardware questions by construction. The
checklist is Gitea #331 (Athom then panel: fill, re-save churn, compaction
under a running pattern, power cut mid-write, an N > 24 library, read-back of
source through the mapping, and one `flashmap-off` build).

## 2026-09-06 — The loop microbenchmark is not the judge: #318's fix is a 38 % regression (#312, #318)

#318 asked whether the `Const c; <op>` fused arms should stop calling
`binop_const` out of line — 3 of the 9 operations the loop microbenchmark
executes per iteration pay an Xtensa window transition (`entry`/`retw`) plus a
second jump table, and the ticket said explicitly that a disassembly cannot
judge it. It cannot, and neither can `tools/opbench.mjs`. Measured on the Athom
(classic ESP32 @ 240 MHz), against `origin/master` 579082a:

| build | loop µs/iteration | `perlin-fire-wind-tunnel` µs/px | `Vm::run` | app image |
|---|---|---|---|---|
| master | 3.935 | **144.8** | 13,170 B | 1,006,208 B |
| `binop_const` `#[inline(always)]`, all three arms | **3.638** (−7.5 %) | **199.4 (+38 %)** | 14,424 B | 1,007,440 B |
| the same, one arm only | 3.813 (−3.1 %) | **227.7 (+57 %)** | 13,717 B | 1,006,944 B |

Both rejected. The window transition really is worth 7.5 % of pure dispatch —
and it is dwarfed by what `Vm::run`'s footprint does to the 21 KB
`call_builtin` sharing the flash instruction cache with it. Note the one-arm
variant is **smaller than the three-arm one and slower on both patterns**: this
is not a size law, it is layout, and it is violent — tens of percent from a
kilobyte of movement in `luxel-core`.

**Which means the same instrument, pointed at the change that shipped this
afternoon, says something much better than we reported.** PR #325 (the debugger
check out of the dispatch loop) read −4.5 % on the loop microbenchmark. On
`perlin-fire-wind-tunnel` at 256 px it is:

| build | µs/px | fps |
|---|---|---|
| `66a94f7` (pre-#325) | 270.8 | 13 |
| `579082a` (post-#325) | **144.8** | **22** |

**1.87× faster on a real, noise-heavy pattern** — ten times the effect the loop
bench could see, and it came from the same 300 bytes of inlined `debug_stop`,
which had been costing `call_builtin` its cache residency rather than costing
the dispatch loop its registers. Both readings repeat to ±0.3 %.

`tools/patbench.mjs` (docs/tools.md) is that measurement as a tool: push one
`library/` pattern, settle, report the median `vm_us` and `vm_us/px`. **Run it
alongside `opbench.mjs` for any luxel-core change** — they can point in opposite
directions, and the pattern number is the one that matters. Pick a *stateless*
pattern: `snake-2d` and friends carry game state whose per-frame work varies,
which makes them useless as an A/B probe (it swung 74 % between two builds that
differed by a kilobyte); `perlin-fire-wind-tunnel` is a pure function of time and
coordinates and repeats to ±0.3 %.

#318 stays open with the numbers. Its remaining candidate is the one that does
NOT grow the loop: give the peephole a per-operation opcode (`ConstOpAdd`,
`ConstOpMul`, `ConstOpLt`…) so the fused arms dispatch once instead of twice and
there is no call to inline in the first place. That needs `compile.rs`'s peephole
and `bytecode::walk_word`, and it must be judged on `patbench.mjs` too.

## 2026-09-06 — The debugger check was inlined into every dispatch (#312)

#312's remaining lead on the dispatch loop's own scaffolding: the shared
per-instruction preamble was reloading `code.ptr`, `code.len` and the `debug`
flag itself from stack spill slots on every dispatch. The cause was one line.

`if debug { … self.debug_stop(…) }` sat at the top of the inner loop, and LLVM
inlined `debug_stop` **and** the `pos_at` binary search it calls — some 300 bytes —
straight between the loop head and the instruction fetch. Two consequences on
the classic ESP32, both invisible in the source:

- the not-taken `debug` test became a **taken** branch on every instruction (it
  had to jump over the blob), and
- the blob's register demand pushed `debug`, `code.len` and `code.ptr` out into
  the 432-byte frame, so the preamble paid three extra loads per dispatch.

One `#[cold] #[inline(never)] debug_step` — publish the frame pc, ask whether to
pause — moves all of it out of the loop. Athom rig (classic ESP32 @ 240 MHz),
`tools/opbench.mjs` at 256 px, matched pair on `origin/master` **66a94f7** (i.e.
after #314, #317 and #323), each side rebuilt and reflashed:

| build | ops / iteration | µs / iteration | cycles / op | app image (athom) |
|---|---|---|---|---|
| master `66a94f7` | 9.00 | 4.120 | **109.9** | 1,006,800 B |
| this branch | 9.00 | **3.935** | **104.9** | **1,006,208 B** |

**−4.5 % of the time a loop iteration costs, and −592 B of image** — better on
both axes. Ops per iteration are identical on both sides, so this is entirely
cycles/op and `ops/px × cycles/op` moves by exactly that factor. (3.935 repeated
exactly on a second run.)

The ladder below was measured earlier the same day against `a3cac71`, before
#317 changed the microbench's op count and before #323 took the 64-bit ROM
libcalls off the render path. Read its µs/iteration column, not its cycles/op
column, against the table above — and note the win was larger there (−10.8 %):
#323 removed part of the same bottleneck, and on top of a cheaper `fmath` the
freed registers buy less.

| build (base `a3cac71`) | µs / iteration | cycles / op | app image (athom) |
|---|---|---|---|
| master `a3cac71` | 5.203 | 113.5 | 1,014,416 B |
| `debug_step` out of line | 4.698 | 102.5 | 1,017,344 B |
| + `fi` read inside `debug_step` | 4.671 | 101.9 | 1,016,816 B |
| + `assert_failed` / `enter_call` out of line | 4.640 | 101.2 | 1,016,704 B |
| + `push_u64` un-unrolled | 4.638 | 101.2 | 1,015,744 B |

That +1.3 KB is also gone on the post-#323 tree: `Vm::run` is 13,170 B here,
smaller than master's, because LLVM no longer has the register pressure that
made it tail-duplicate.

The rest:

- **`fi` is no longer an argument.** `debug_step` reads the running function off
  the top frame itself, which is where the loop got it from — one fewer value
  live across the whole dispatch loop.
- **`assert_failed` and `enter_call` out of line.** `format!` drags the whole
  formatting machinery in with it for a path that fires once per pattern; and
  `CallFn` and `CallValue`'s `Fun` arm each inlined a 128-byte
  `[Value::default(); MAX_ARGS]` **and its `memset`** into the dispatch loop.
  `Vm::run`'s stack frame: **432 → 256 B**.
- **`jsonview::push_u64` was 1,909 B of Xtensa** — dividing a `u64` by the
  literal 10 makes LLVM emit a 64-bit magic multiply and then unroll all twenty
  digit positions. An `#[inline(never)] divmod10_u64` makes it ~370 B. Nothing
  hot; it pays for the loop change.
- **Fixed a latent regression from #314**: `CallFn`/`CallValue` reached
  `push_frame` without publishing `insn_start`, so a "call depth exceeded" error
  was attributed to whatever instruction last wrote the field.

### Two things that measured worse than they read

- **Block alignment is not where the missing cycles are.** Every hot block in
  `Vm::run` is 4-byte-misaligned, including the `jx` targets;
  `-C llvm-args=--align-all-nofallthru-blocks=2` fixes that for **2.0 %** and
  **+13,232 B**. Dropped — 0.15 %/KB against the debug change's 3.7 %/KB.
- **Merging the 93 `fail!` sites saves 3.0 KB and costs 5 %.** LLVM tail-
  duplicates the publish-and-build prologue into every site (41 byte-identical
  copies of the stack-underflow one, 22 of the overflow one — 2.5 KB of a 16 KB
  function). Replacing `return Err(…)` with `break 'frame <msg>` and one
  epilogue reclaims it, and then LLVM hoists the most common message's pointer
  and length into the **hot preamble** to feed the phi: 101.2 → 106.4 cycles/op.
  Dropped. (It also needs the macro block moved inside the labelled loop — a
  loop label is not visible inside a `macro_rules!` body defined before it.)

Two measurement notes for anyone repeating this. The Athom rig repeats to ±0.1 %
on the same image, but **unrelated code motion moves the number ~2.6 %** — the
same source change measured 102.5 and 105.1 cycles/op depending only on what else
had shifted around it. Compare rebuilt A against rebuilt B, never against a
remembered number. And **`tools/opbench.mjs` takes ops/iteration from a freshly
built host `luxel` but the bytecode it pushes from `web/public/luxel.wasm`**: with
a stale wasm after a rebase over a compiler change, the device runs the OLD op
stream while the profiler counts the NEW one, and cycles/op comes out ~20 % high
(115.7 read as 138.7 here). Run `npm run wasm` first.

Verified: `cargo test --workspace`, `tools/check-library.sh` (299/299 on every
rig), 40 library patterns rendered to PPMs against `origin/master` — all
byte-identical except `2d-clock-with-hand-color-pickers`, which reads the wall
clock and differs against itself — `tools/stack-check.sh` clean,
`tools/ci.sh` green, all eight board images built and checked with
`tools/image-check.sh`.

## 2026-09-06 — reflect: Xtensa-shaped code is not free on the host (#312)

Three things #312's op-body pass learned the hard way, now in
`.claude/rules/vm-bytecode.md` so the next engine change doesn't re-derive
them:

- **"Xtensa is the target that matters" does not license host slowdowns in
  `luxel-core`** — the wasm playground renders every browser preview from
  the same code. A rewrite that is only a win because the target lacks a
  64-bit ALU (a 32-bit restoring loop replacing one wide divide) cost −31 %
  on x86 for a `dist`-heavy pattern. The fix is the `NARROW_WORD` pattern in
  `fmath.rs`: a `cfg!()` **value**, both forms compiled everywhere, and host
  tests asserting `narrow == wide == reference` three ways — which is what
  keeps the device's path proven by a host `cargo test`.
- **`#[inline(never)]` on a shared arm body to shrink the dispatch loop is a
  trap.** It only moves bytes out of `Vm::run`; the arms come out the same
  length or longer, and the host pays ~20 % on array-heavy patterns.
- **Host `luxel bench` on this box has 6–19 % run-to-run spread.** Median-of-3
  invented three double-digit "regressions" that vanished on re-measurement.
  Interleave the binaries, ≥ 200 frames, best of 5–11, and treat ±3 % as noise.

`docs/boards.md` gets the #323 whole-fleet size row (every board −7.5 KB;
the classic-ESP32 boards 3.26 % → 3.99 % OTA margin).

## 2026-09-06 — Op bodies: every 64-bit ROM libcall out of the render path (#312)

The sibling pass on #312 fixed the *dispatch*; this one is the other half of the
issue — what each instruction does once dispatched. Disassembling the S3 and
classic-ESP32 images turned up a specific, measurable class of waste: **64-bit
arithmetic in fixed-point code that only ever needed 32 bits**, which Xtensa
cannot do inline and hands to the ROM's `__udivdi3` / `__divdi3` / `__umoddi3`
(a windowed call plus a bit-loop, easily 150–300 cycles each).

**141 ROM 64-bit libcall sites in `luxel-core` → 47**, and the ones left are all
off the render path (`jsonview::push_u64`, `civil_from_unix`, `hamqtt::parse_fx`,
one `Fx::div` fallback). Nothing on the per-pixel path calls the ROM any more.

| where | before | after |
|---|---|---|
| `fmath` (sin/cos/sin_turns/tan/pow/exp/exp2/log2/atan/atan2/sqrt/hypot) | 19 × `__udivdi3`, 22 × `__divdi3` | **0** |
| `Vm::run` (the `time()` builtin) | 2 × `__udivdi3`, 2 × `__umoddi3` | **0** (only `Fx::div`'s i64 fallback remains) |
| `color::rgb_to_oklab` / `oklch_to_rgb` / `srgb_to_linear` / `linear_to_srgb` | 32 | **1** (they inline `pow`) |
| `vm::rotation`, `Vm::call_builtin` | 8, 18 | 0, 5 |

`fmath` is 3,248 → 2,102 Xtensa instructions (−35 %); `Vm::run` 5,446 → 5,098;
`Vm::call_builtin` 7,742 → 6,951; the whole image 305,654 → 302,597.

What changed:

- **`fmath` is 32-bit throughout.** `exp2`'s series ran in `i128` with six i64
  divisions by constants; `log2`'s mantissa loop squared a `u64`; `sin_turns`
  multiplied and divided i64s whose values never exceed 2²². Each is now a
  widening 32×32→64 multiply (`mull`+`muluh`, one instruction pair) and a 32-bit
  magic-multiply divide, with the proven bound written at the site. `isqrt64`
  (a 24-iteration 64-bit bit-loop) is a digit-pair `isqrt48`. `sqrt` 212 → 15
  instructions, `pow` 288 → 151, `tan` 426 → 46, `sin_turns` 173 → 59,
  `hypot` 224 → 25.
  **Bit-exactness is tested, not asserted**: every rewritten function keeps a
  verbatim copy of the old 64/128-bit code as a `reference_*` fn and is swept
  against it — exhaustively over the whole input space where that is finite
  (all 65,536 `exp2` fractions, all 65,536 `sin_turns` phases, all 411,775
  values `sin` can feed `div_shift16`), densely plus randomised elsewhere. The
  sweeps were mutation-tested (perturb a coefficient, flip a loop bound) to
  confirm they actually discriminate.
  **Three of the rewrites are conditional**, because they trade one wide
  machine instruction for a 32-bit loop: `div_shift16` (used by `sin`, `atan`,
  `atan2`), `isqrt48` (`sqrt`, `hypot`, `dist`, `asin`) and `sq16` (`log2`).
  That is a win on Xtensa/RISC-V32, where the wide form is a ROM call, and a
  large LOSS everywhere else — the first cut cost `dist`-heavy patterns 31 % on
  x86, which the wasm playground would have paid too. Both forms are compiled
  on every target and selected by a `cfg!()` **value** (`NARROW_WORD`), never
  by `#[cfg]` on the definitions, so a host `cargo test` still proves the
  device's path bit-exact: the sweeps assert narrow == wide == the i64
  reference, three ways.
- **`time()` never calls the ROM.** Its period is a 16.16 raw, so both it and
  `now % period` fit `u32`; the scaled divide is 16 restoring steps in 32-bit
  registers, exact and bit-identical. The 64-bit form survives only past 2³² ms
  (49 days) of uptime, in a `#[cold]` helper outside the dispatch loop.
- **`builtin_fast` takes its arguments by value.** It took `&[Value]`, so the
  caller had to materialise a `[Value; 4]` in the frame — a ROM `memset` plus a
  ROM `memcpy` on *every* builtin call, including `hsv` and `time` in the
  per-pixel path. By value they stay in registers: both ROM calls gone from
  `Vm::run`. Host `luxel bench` on `library/rainbow.js` (which is `time` + `hsv`
  and little else): **15.6–16.2 → 21.1–22.1 M px/s, +36 %**.
- **`binop` in registers, with the reference cases split off cold.** `binop_fx`
  takes and returns bare `Fx`; `binop_const` serves the three `Const c; <op>`
  fusions without wrapping the literal in a `Value` (a non-`Num` left operand can
  only make EQ false and NE true, so the cold half needs neither operand).
  `binop` 250 → `binop_fx` 107 + `binop_const`/`binop_ref` 33.
- **`Fx::wrap_unit()`** — `mod_floor(Fx::ONE)`, the unit wrap every waveform,
  `hsv` hue and phase reduction performs, is just `raw & 0xFFFF` (floored modulo
  by 2¹⁶ *is* the low half of the two's-complement word, negatives included).
  Used by `hsv_to_rgb`, `square()` and `triangle()`. `square()`'s default duty
  also stops going through `Fx::from_f64` — softfloat has no business in a
  builtin.
- **`index_read` deliberately left inlined**, with a comment saying why. Marking
  it `#[inline(never)]` to shrink the dispatch loop looks right and is wrong: it
  buys nothing on Xtensa (the three indexing arms come out the same length or
  two instructions *longer*) and costs the host badly — `colourful-fireflies`
  −21 %, an array-heavy 2-D pattern −18 %, which the wasm playground would pay
  too. Reverted, with the measurement in the comment so the next person does not
  re-derive it.

Per-op instruction counts on the taken path (S3 and classic ESP32 are
instruction-for-instruction identical, so one column serves both): `Pow`
474 → 370, `Rem` 34 → 32, `Add`/`Sub`/`Bit*` 29 → 28, `Mul` 33 → 32, `Div`
43 → 42, `LoadGLIdx` 71 → 68; the rest move by ±3 with the register allocator.
**The loop microbench's own mix barely moves** (543 → 538 instructions per
iteration over its 11.06 ops) — it is pure fused integer arithmetic and never
touches a builtin, so the wins here land on real patterns instead: rainbow +36 %,
`library/snake.js` +14 %, `colourful-fireflies` +13 %, `snake-2d` +7 % on the
host (max of 5 runs, 512 px × 200 frames, interleaved with the baseline
binary). The transcendental-heavy patterns still give back 2–7 % on x86 —
`dist`/`hypot`/`atan2` per pixel — because `isqrt48` is now an out-of-line
function where the old bitwise `isqrt64` was inlined into `sqrt`. That is the
residue after the `cfg` split, and it is the direction the project trades:
`Vm::run` 5,446 → 5,111 instructions and every board's image 8 KB smaller.

Every board's app image **shrinks** by 7.8–9.3 KB, which moves the three
classic-ESP32 boards off the edge of the 1 MiB OTA slot:

| board | base | new | margin |
|---|---|---|---|
| board-pixelblaze-v3 | 1,014,336 B | 1,006,416 B | 3.26 % → **4.02 %** |
| board-athom-music | 1,014,448 B | 1,006,512 B | 3.25 % → **4.01 %** |
| board-esp32-generic | 1,014,144 B | 1,006,240 B | 3.28 % → **4.03 %** |
| board-s3-devkit | 956,592 B | 948,768 B | 8.77 % → 9.51 % |
| board-seengreat-hub75 | 949,184 B | 941,360 B | 9.47 % → 10.22 % |
| board-c3-devkit | 961,712 B | 952,384 B | 8.28 % → 9.17 % |
| board-c6-devkit + hosted-ui | 1,007,776 B | 1,003,728 B | 3.89 % → 4.27 % |

(`board-c6-devkit` full-UI still fails image-check at 2.69 %, as it does on
master — #291, pre-existing; the shipped artifact is the hosted-ui variant.)
`.stack` and the largest frame are unchanged on both the default board and the
panel — Gitea #374 is the on-panel look check (per-cell shimmer at 4096 px, the
flat-pool skip, and the real `out_fps`), #373 the bulk-op analysis it produced.

Still open, and now the top of the list for whoever has the device: the
`Const c; <op>` arms pay an Xtensa window transition (`entry`/`retw`) plus a
second jump table per instruction, and 4 of the loop microbench's 11 ops go
through them. Inlining the handful of hot sub-opcodes into the arm would remove
the transition but *raise* the instruction count, so it cannot be judged from a
disassembly — it needs `tools/opbench.mjs` on the panel. `Fx::div`'s i64
fallback (the last ROM call reachable from a render) is Gitea #316; the `Const c; <op>` follow-up is #318.

## 2026-09-06 — What the COMPILER wastes: constant folding + the `i++` recovery (#312)

#312 is about the ~110 cycles our interpreter spends per bytecode operation.
This is the other half of `time/pixel = ops/px × cycles/op`: four things the
compiler emitted that the VM then had to execute. Library-wide **69.27 →
66.37 dynamic ops/px** (−4.2 %, 213 of 299 patterns improved, none
regressed), and the #312 loop microbench itself **11.0 → 9.0 ops per
iteration (−18 %)**.

Every one is measured on both axes, because fewer ops is not automatically
faster (Jeremy's rule): the dynamic count from `luxel bench --profile`, and
the Xtensa instruction count of the ops involved, walked out of the S3
disassembly (`xtensa-esp32s3-elf-objdump -d` of the `board-seengreat-hub75`
image — the dispatch floor every op pays is **21 instructions** after #314).

| transformation | sites in `library/` | ops/px | Xtensa instructions, per site |
|---|---|---|---|
| postfix `x++` whose value is discarded emits no old-value recovery | 470 (all of them — the library contains no prefix and no value-using inc/dec) | 69.27 → 67.40 (−2.7 %) | `StoreL` 42 + `ConstOp(SUB)` 62 + `Pop` 27 = **131** → `StoreLPop` **42** (−68 %) |
| literal arithmetic folded (`1/3`, `-1`) | 192 negated literals + 69 binary sites | 67.40 → 67.15 (−0.4 %) | `CONST_NUM` 41 + `NEG` 39 = **80** → **41** (−49 %) |
| reads of never-written predefined globals become constants, and a constant operand of a commutative operator moves to the right where it fuses | 311 frozen reads (270 in operand position, 219 of them `*`) + 137 literal-left multiplies | 67.15 → 66.37 (−1.2 %) | `x * PI2`: `LoadLG` 62 + `MUL` 50 = **112** → `LoadLConstOp` 56 + `binop MUL` 20 = **76** (−32 %); `2 * x`: **137** → **76** (−45 %) |

The loop microbench, op by op — `for (i=0;i<K;i++){ x += i*0.5 }` was eleven
operations per iteration and is now nine. The two that went were the
`Const 1; Sub` that recovers the pre-increment value of `i` nobody reads,
which also unblocked `StoreL; Pop` → `StoreLPop`.

**Considered and rejected: strength-reducing `x * 2^n` to a shift** (606
sites). It is bit-exact — `Fx::mul` is `(a·b) >> 16` on the full 64-bit
product, so multiplying by `2^-k` *is* `raw >> k`, and `Fx::shr` shifts the
raw word arithmetically — but it is **slower**. All four forms are one fused
`LOAD_L_CONST_OP` differing only in the sub-opcode, and inside `binop` the
S3's hardware multiplier does `Fx::mul` in 5 instructions
(`mull`/`mulsh`/`ssl`/`src`) while `Fx::shl/shr` must first `to_int_trunc()`
the 16.16 shift count and mask it to 0..31 — 9. Per op: `x * 2` **76**
instructions, `x / 2` 78, `x << 1` and `x >> 1` 79. So the divide variant is
not a static win either; the only argument left for it is that `quos` is an
iterative divide whose *cycle* cost the instruction count does not show, and
that needs the device — Gitea #319.

The passes live in `compile::const_fold`, ahead of the #261 peephole, and
obey its two contracts: never fold across a jump target, never across a
source position. The folded value comes from `vm::binop` and the same `Fx`
operators the interpreter's arms use, so it cannot drift from the word the
VM would have pushed — pinned by a unit test over all 17 foldable operators
× 100 edge-word pairs. A global is "frozen" only when it is predefined,
never `StoreG`-ed anywhere in the program, and not exported (only exported
globals are host-settable); `pixelCount` is excluded by name because the
engine writes it without a `StoreG`. `luxel run|bench|compile --no-fold` is
the A/B lever, independent of `--no-fuse`.

Verification: `cargo test --workspace` (15 new codegen pins in `compile.rs`,
8 new equivalence tests in `tests/constfold.rs`); `tools/check-library.sh`
1495/1495; **all 299 library patterns render byte-identically to the master
binary**, on both a 256-px strip and a 16×16 grid (598 PPMs); web build +
`npm test` 29/29 + `web/tools/e2e.mjs` (which steps the debugger);
all seven boards build and the firmware images are **unchanged** — the
device drops luxel-core's frontend, and the one thing the host compiler
puts in the image, the built-in `library/rainbow.js` blob, is byte-identical.

Host `luxel bench` throughput is **not** a usable signal for this change and
is not quoted: `rainbow`'s compiled blob is byte-identical between the two
binaries and `Vm::run`, `Engine::frame` and `call_builtin` disassemble
instruction-for-instruction identically, yet it measures +8 % — the x86
code-placement lottery #261 already recorded.

## 2026-09-06 — Where the S3's ~110 cycles per bytecode op go, and 18 % of them back (#312)

#312 asked for the ~50 cycles/op the instruction-count model could not explain.
The answer is that **the model was right about the instruction count and wrong
about the cycles per instruction**: the dispatch loop really is ~55 Xtensa
instructions per bytecode op, and an LX7 runs that branchy, load-dependent code
at roughly 2 cycles per instruction, not one. Nothing exotic — no cache
pathology — was hiding in there:

| suspect | test | result |
|---|---|---|
| 1. words executed from the flash mapping | the same K-sweep on a pattern pushed live (`deserialize_lean` → DRAM heap) vs saved + activated (`deserialize_lean_static` → the mapped code arena) | **0.0 %.** 4.322 vs 4.323 µs/iteration. A live `/api/code` push was *already* running from DRAM, so the baseline never had this cost, and the mapped path does not either. **No DRAM code cache is needed** — that closes the #260 RAM-budget question. |
| 2. instruction-cache pressure | `Vm::run` (15.7 KB) moved into `.rwtext` (IRAM) behind a new `iram-vm` feature | **−1.0 %** (110.3 → 109.2 cycles/op) for 16 KB of the scarcest memory on the board. Not the cause. Jitter did drop sharply (K=200 sample spread 4,700 → 790 µs), so the cache is *doing* something — just not costing throughput. |
| 3. per-op overheads outside the instruction count | a throwaway image with the `fuel` and `insn_start` bookkeeping deleted outright | **−7.6 %** (94.3 → 87.1 cycles/op). Real, and mostly recoverable. |
| 4. Xtensa pipeline effects | objdump of the taken path: ~20-instruction dispatch preamble + ~34-instruction `Add` arm, ~4 taken control transfers per op including a load-dependent `jx` | the residue. ~55 instructions in ~110 cycles ⇒ ~2 cycles/instruction. |

`tools/opbench.mjs` is the measurement (docs/tools.md): it pushes the loop
pattern for each K, fits the slope of `vm_us` against K — which cancels the wire,
the per-pixel entry and `beforeRender` — and prints µs and cycles per iteration
and per op, taking ops/iteration from the host profiler on the same sources
rather than hardcoding it.

**What landed**, in the order it was found, each measured on the panel:

1. **`Value`'s discriminant was a `u16`** because `Fun`/`Builtin` carried `u16`
   payloads. Every `match` on a `Value` — including the `Option<Value>` niche
   test `Vec::pop` leaves behind — therefore needed `l32r 0xffff; and` before the
   compare, and since Xtensa has no 32-bit immediate that mask is a literal-pool
   *load*, re-issued at each use under register pressure: the `Add` arm alone
   carried four. All payloads 32-bit ⇒ a `u32` tag ⇒ mask and literal gone.
   `Add` arm 34 → 22 instructions, `Vm::run` 15,670 → 14,123 B. **−12.5 %.**
2. **Binary ops and `StoreL`/`StoreG`/`ConstOp` in place.** `pop; pop; push` writes
   the Vec's length three times and checks `MAX_STACK` on a shape that shrinks the
   stack; rewriting the top slots and truncating once does it with one write and no
   check. **−2.3 %** (and +9 % *slower* on x86 — see the rule below).
3. **`fuel` and `insn_start` out of `Vm` and into locals.** A load/compare/
   decrement/store and a store on every single dispatch, for two values only the
   error paths read. They are published at each `return` and at the two sites that
   can re-enter the VM through an array callback. **−1.5 %.**
4. **Instruction fetch and the jump table.** `at` now advances unconditionally so
   the fell-off-the-end arm merges on a *value* instead of on control flow (−1 `j`,
   −1 `mov`), and naming opcode 0 in the match makes the jump table's range start
   at 0, dropping the `addi -1` that rebased it. **−2.5 %.** Preamble 20 → 16
   instructions.

**Net on the Seengreat panel** (4096 px, `board-seengreat-hub75`), with ops/pixel
unchanged at 11.00 per loop iteration throughout — the whole win is cycles/op:

| | master 7b0c2d5 | this branch | |
|---|---:|---:|---|
| loop bench, µs/iteration | 5.055 | 4.147 | −18.0 % |
| loop bench, cycles/op | 110.3 | **90.5** | (Pixelblaze ≈ 90) |
| rainbow `vm_us` | 22,034 | 21,479 | −2.5 % |
| snake 1D `vm_us` | 55,682 | 51,481 | −7.5 % (17 → 18 fps) |
| snake 2D `vm_us` | 95,392 | 84,788 | −11.1 % (10 → 11 fps) |
| empty render `vm_us` | 5,409 | 5,409 | unchanged — it executes no ops |

Rainbow moves least because it is dominated by `call_builtin` (`hsv`), which this
work did not touch; the more op-bound the pattern, the bigger the win. #312 stays
open at 90.5 vs the Pixelblaze's ~90 (or ~76 if its VM dispatches literal words as
their own pushes, #313) — the remaining leads are the ~21 KB `call_builtin`
itself, the second jump table `binop` dispatches through for the fused
constant-argument forms, and the register pressure that keeps `code.ptr`/`code.len`
spilled across the loop.

One more suspect-2 probe, prompted by the observation that the dispatch jump
table lives in flash-mapped DROM so every `jx` does a data-cache read from
flash: building with `-C llvm-args=--min-jump-table-entries=200`, which
removes the tables entirely and lowers the dispatch to a compare tree, made
it **21 % WORSE** (90.5 → 109.3 cycles/op, rainbow `vm_us` 21,479 → 24,088).
Together with the 0.0 % from suspect 1 — where the *bytecode words* come from
DROM in one arm and DRAM in the other — that settles it on the S3: a
flash-mapped data read on this chip is a data-cache hit and costs nothing
measurable, and the table load plus one `jx` is much cheaper than six or
seven unpredicted compares.

**Sizes: every board shrinks** (docs/boards.md) — −1,072 B on the C3, −3,088 B
on the classic-ESP32 boards, −3,104 B on the Seengreat, −7,600 B on the C6
(the one board not built at `CORE_O3`, so its dispatch loop pays per arm).
That matters most for the classic-ESP32 boards: with dev creds baked in they
were *below* `image-check.sh`'s 3 % floor on master (2.97 %) and are back over
it at 3.27 %. `.stack` unchanged; `tools/stack-check.sh` passes.

Also here: `iram-vm` / `iram-builtins` cargo features (off everywhere, kept because
they are how suspect 2 gets re-tested on a future board), and the S3's OTA wedge
**#294 closed** — 3/3 clean pushes of current master, which carries #309's fence fix.

## 2026-09-06 — Counting the Pixelblaze compiler's ops: `tools/oracle/opcount.mjs` (#312)

#312 measured that one iteration of `x += i * 0.5` costs **4.1 µs** on the
Pixelblaze and 5.0 µs (S3) / 6.2 µs (Athom) on ours, but its cycles-per-*operation*
row was a guess: nobody had counted what the PB's compiler emits for that source.
Now we count it.

`tools/oracle/opcount.mjs` compiles a pattern with the **Pixelblaze's own
compiler** — the existing `tools/oracle/compiler.mjs` sandbox, so the only thing
that touches the oracle is one read-only HTTP GET of its web UI page: no
websocket, no `setCode`, nothing to restore (`--cache` skips even that). It then
decodes the emitted word stream using *client-side* facts only — the compiler's
opcode table is read out of the served page and evaluated at run time rather than
transcribed into this repo, and the word encoding is the one its own encoder
states (bit 0 tags an instruction word `inline<<16 | stack<<8 | opcode<<1 | 1`
against a data word — a 16.16 literal or `addr<<1`; every instruction is exactly
one 32-bit word). Function boundaries fall out of the `<fn address>; gstore
<global>` pairs the init block emits, cross-referenced with the export table, so
init / beforeRender / render / render2D / each control handler are counted
separately, and loops come from backward branches.

**The answer for the microbench: 13 words per iteration — 11 instruction words
plus 2 literal pushes.** The K=16 and K=64 blobs are word-identical (the PB
compiler does not unroll), so the count is the loop span, not a K difference.
Luxel's dynamic profiler says 11.0 fused ops per iteration for the same source.

| device | µs/iter | cycles/iter | ops/iter | cycles/op |
|---|---:|---:|---|---:|
| Pixelblaze oracle (ESP32, fw 3.67) | 4.11 | ~986 | 11 insn words (13 with literals) | **~90** (~76) |
| Luxel, Athom (classic ESP32), master | 6.20 | ~1,488 | 11.0 | **~135** |
| Luxel, Seengreat S3, d267836 | 5.02 | ~1,205 | 11.0 | **~110** |

So #312's ~90 cycles/op for the PB was right, and the gap it describes is real
and like-for-like: the two compilers emit **the same 11 operations** for that
loop body, and ours costs 20–50 % more per operation. What we still cannot say —
and the tool says so rather than guessing — is what any of it *costs* on the PB:
cycles/op there are only ever inferred from a wall-clock measurement divided by a
counted op count, and whether its VM dispatches a literal word as its own push is
firmware behaviour we do not look at, so both figures are reported.

Whole-pattern static counts, same rig for both sides:

| pattern | PB words | PB insn-words | Lx static fused ops | Lx dyn ops/px |
|---|---:|---:|---:|---:|
| `library/rainbow.js` | 16 | 12 | 7 | 6.0 |
| `library/snake.js` | 197 | 162 | 113 | 26.9 |
| `library/snake-2d.js` | 1366 | 1152 | 909 | 39.7 |
| `library/perlin-fire.js` | rejected (`const`) | — | 290 | 102.6 |
| `library/perlin-fire-wind-tunnel.js` | 308 | 245 | 180 | 81.1 |

`perlin-fire.js` is the one benchmark the PB compiler will not take — its parser
rejects `const` declarations, nothing to do with builtins — so
`perlin-fire-wind-tunnel.js` (the same clean-room noise model, written without
`const`) stands in as the builtin-heavy case; the rejected row keeps its Luxel
columns rather than going blank.

Supporting change: `luxel compile --stats` prints one JSON line of a blob's
static shape (bytes, words, globals, per-function instruction counts, where a
multi-word instruction counts once), writing no `.lxbc` unless `--out` is given;
`--no-fuse` alongside it gives the unfused baseline. It is backed by a new
`luxel_core::bytecode::insn_count`, which walks a function's words exactly as
`validate` does.

## 2026-09-06 — The per-pixel VM entry: 323 → 231 Xtensa instructions for an empty render (#260)

An `export function render(index) {}` cost **7,591 µs of VM time per frame** at
4096 px on the Seengreat panel — 1.85 µs ≈ 440 cycles per pixel before a single
pattern instruction runs, a third of rainbow's frame and a hard ~77 fps cap on
every pattern. The dispatch loop was already tuned (#263/#268/#278/#302); this
is everything *around* one pixel's run.

**Counted, not estimated** — `xtensa-esp32s3-elf-objdump` over the
`board-seengreat-hub75` image, walking the empty-render path by hand
(`Engine::frame`'s pixel loop → `render_args` → `push_frame` → `run` → RET →
return), instructions on the taken path only:

| stage | master | this |
|---|---:|---:|
| `Engine::frame` pixel loop body | 95 | **111** |
| `Engine::render_args` (windowed call) | 33 | — |
| `Vm::push_frame` (windowed call) | 75 | — |
| `Vm::run` (windowed call) | 120 | 120 |
| **total per pixel** | **323** | **231** (−28 %) |
| `callx8` per pixel | 3 | **1** |

`Vm::run` is byte-identical before and after — the dispatch loop was not
touched. What changed:

- **`Vm::begin_pixel_pass(prog, fn_idx, argc) -> PixelPlan`** resolves the
  callee's frame shape once per frame (local-slot count, how many of them come
  from arguments) and reserves the locals/frame storage so no pixel allocates.
- **`Vm::render_pixel(prog, &plan, &args)`** replaces `start()` in the color
  pass: fuel reset, one pass filling the locals, one `Frame`, `run`. It is
  semantically `start(prog, fn_idx, &args[..argc], false)` — same defaults for
  slots past the argument count, same `clear_run` on error — and returns
  `Result<(), VmError>` so the hot return is a discriminant, not an `Outcome`.
  `start`/`resume` are untouched, so the debugger and map mode are unaffected
  (`render_pixels` only runs when `!debug_enabled && !is_map`).
- **`Engine::render_pixels` hoists the argument build.** The entry, its
  argument count, the mid-space fill and the "a plain `render(index)` never
  reads x" test are loop-invariant; only `args[0]` changes per pixel for a 1D
  entry. That removes `render_args`' 40-byte return travelling through memory
  (a 19-instruction copy) once per pixel.
- `Vm::apply_transform` is `#[inline(never)]`: it was being inlined into
  `Engine::frame` twice (the hoisted loop and `render_args`), which is where
  most of the image growth came from.

Two variants were measured and dropped: `resize` + `copy_from_slice` for the
locals (compiles to ROM `memset` + `memcpy` calls for the one word a
`render(index)` frame holds) and seeding `run`'s frame context from the plan to
skip its 63-instruction prologue — the latter costs 16–21 % on x86 because the
live seed adds register pressure to the whole dispatch loop, the same effect
#302 documented for extra opcode arms.

**Host** (`luxel bench`, 4096 px, best of 9, x86-64 — the Xtensa gain should be
larger, since x86 hides windowed calls and is not dispatch-bound):

| bench | master | this | Δ |
|---|---:|---:|---:|
| empty `render` | 76.29 M px/s | **99.56 M** | **+30.5 %** |
| one `rgb()` | 40.43 M | **50.85 M** | **+25.8 %** |
| rainbow | 26.70 M | **33.21 M** | **+24.4 %** |
| `snake.js` | 14.49 M | 14.91 M | +2.9 % |
| `snake-2d.js` | 13.28 M | 14.02 M | +5.5 % |
| `snake-2d.js --map-grid 64x64` | 9.36 M | 10.04 M | +7.3 % |

**Image cost**: +192 B on the Xtensa boards (`board-pixelblaze-v3` free
33,312 → 33,120 B, 3.17 % → 3.15 %, floor 3 %), +176 B on
`board-seengreat-hub75`, +528 B on the C6, and −16 B on the C3. Not the
shrink the ticket hoped for: the hoisted argument build and the inlined frame
setup are new code in `Engine::frame`, while `push_frame`/`render_args`
remain for the debugger, map and nested-call paths. `apply_transform` going
out of line clawed back 400 of the original 592 B.

**Verified**: `cargo test --workspace` 278/278 (incl. the `tests/engine.rs`
error-semantics pins — first error wins, fatal blanks the rest, non-fatal keeps
the pre-error color — and the debugger step tests); `tools/check-library.sh`
1495/1495; **all 299 library patterns render byte-identically** to the master
binary on two rigs (120 px strip and a 16×16 map, 12 frames each — 598/598
PPMs `cmp`-clean); wasm rebuild + `npm test` 29/29 + `web/tools/e2e.mjs` in
real chromium; nine firmware variants build with image-check; `tools/stack-check.sh`
clean; `tools/ci.sh` green.

**Not measured on hardware** — both devices were in use. The on-device check
(empty-render `vm_us` at 4096 px, expected well under 7.6 ms) is on #266.

## 2026-09-06 — The panel re-measured on today's master: superinstructions are 14–18 % on Xtensa

Master moved four times during the Seengreat session (#288 LXBC v5 restored,
#293 extent allocator, #300 borrowed program words, #302 superinstructions),
so the panel was re-flashed with **0f83975** and everything re-measured on
that. The #260 table at 4096 px, per-stage µs from `/api/status`:

| pattern | v0.1.40 | e5935e6 | **0f83975** | vm µs |
|---|---:|---:|---:|---:|
| empty `render` | 56 | 77 | **77** | 7,591 |
| one `rgb()` | 29 | 45 | **50** | 14,021 |
| rainbow | 18 | 30 | **33** | 24,079 |
| `snake.js` | 8 | 12 | **16** | 59,112 |
| `snake-2d.js` | 4 | 8 | **10** | 96,061 |

- **#298 answered on hardware**: same firmware, two blobs from
  `luxel compile [--no-fuse]` — rainbow vm −13.9 %, snake-2d vm −18.2 %.
  Xtensa is dispatch-bound in a way x86 is not, which is exactly the bet
  #261 made. `--no-fuse` now works on `luxel compile` (it only existed on
  `run`/`bench`), which is what makes a device-side A/B possible without
  building a second firmware.
- **Heap**: a 2D pattern costs ~800 B beyond its arrays now that the program
  words are borrowed from the flash mapping (#300) — 51,192 B free with
  rainbow, 50,412 B with `aurora-2d`.
- `tools/ota-push.sh` reads `$BOARD` (through `firmware/board-target.sh`) for
  the ELF path and `espflash --chip`, so `BOARD=board-seengreat-hub75
  tools/deploy.sh <ip>` finally works end to end on an S3; it used to look
  for a classic-ESP32 ELF that an S3 build never produces.
- The S3 reproduces the **#292 fence wedge** (filed as #294 before the two
  were connected): 4 of 9 OTAs and once in 33 k fences, same
  `SysRtcWdt` + ProCpu phase 3 black box. Notably, `POST /api/assets` — the
  deterministic trigger on the Athom — ran 3/3 clean at 727 KB on the S3,
  and the S3 never performs the esp32-only SPI2-DMA wait, so that wait is
  not the common cause.
## 2026-09-06 (later) — the second core's flash fence: root-caused, fixed, and 100x fewer fences (#292)

Follow-up to the entry below, on the same Athom rig. The blocker it filed is
fixed, and the fence traffic that made it fatal is gone.

**Root cause: the fencing core re-enabled its own interrupts while the other
core was still parked.** The fence did half of ESP-IDF's
`spi_flash_disable_interrupts_caches_and_other_cpu()` — it parked the other
core — and none of the other half. It is not the "garbage instruction fetch"
story the code comments assumed: a black box extended to record which side of
the driver call the ProCpu was on put both wedges *after* the flash work, on
the instruction where this core re-enables interrupts. Everything that queued
up behind a ~45 ms sector erase (esp-storage masks only levels 1-5, and only
for the ROM call) then fires at once with the other core still parked, and one
of those handlers never returns. Two windows, found one after the other:
esp-storage's own critical section dropping back to level 0 (ProCpu phase 7),
and the wait for the park acknowledgement (phase 2). `core1::fenced` now holds
interrupts masked from the moment it takes the fence lock until after the
other core is released; `park_if_asked` in the spin-waits is what keeps that
safe. Fix confirmed by A/B on metal — the same build with and without the mask,
728 KB `POST /api/assets` 5/5 wedged vs 5/5 clean.

The bisect that got there, all on this rig, all read out of the RTC black box
(no serial this session either):

| experiment | result |
|---|---|
| 10 × single fenced sector erase (`POST /api/brightness`) | 10/10 clean — it is not "erases are broken" |
| same, under sustained WiFi RX (48 KB rejected uploads in a loop) | 25/25 clean — not RX pressure alone |
| 1 MB `POST /api/ota` (~500 fences, same erase+write shape) | clean, 3/3 |
| 728 KB `POST /api/assets`, strip cut to 1 px | wedges — not the SPI2 DMA hazard |
| same, `EXTRA_FEATURES=flashmap-off` | wedges — not the cache mapping (confirms #292's report) |
| same, interrupts masked across the fence | **installs, 14.9 s** |

Also disproved a premise carried from the issue: the asset writer is NOT taking
a fence per ~16 bytes. Per-call-site fence counters showed the install spending
2 fences per 4 KiB page (one erase, one program), 358 for the whole archive, and
the board wedging **5 fences in** — the "46,800 fences/minute" figure was a
boot-time pattern-store scan, not the install.

**But that scan was the second bug.** `map::fetch_item`/`store_item` were each
handed a fresh `NoCache`, so every call re-scanned all 128 pages and every item
header in them, one fenced 8-byte read at a time. A fence costs ~0.5 ms (an
interrupt and a park round-trip on the other core, plus this core's interrupt
latency); the flash read costs ~10 µs. Measured: **one 650-byte `POST
/api/patterns` = 81,143 fenced reads, ~13 s** — long enough that the RTC
watchdog rebooted the board mid-save, which is exactly the "intermittent large
pattern saves" half of #292. There is now ONE `PageStateCache` for the store
region, kept across transactions and passed to every call (`patterns::store_cache`);
the flash lease is what makes a single shared `&mut` sound. Same save:
**929 fenced reads, 0.5 s.**

| operation | fenced flash ops before → after | wall time |
|---|---|---|
| 650 B `POST /api/patterns` | 81,143 → **929** | 6.0 s → 0.5 s |
| 8 KB `POST /api/patterns` | 54,678 → **1,895** | 15.3 s → 1.0 s |
| boot: pattern-store scan | 23,957 → ~4,100 | — |
| 728 KB `POST /api/assets` | 358 (unchanged — already one fence per page) | 13–15 s |

Third fix, and the one that made the save soak green: **a long flash burst now
feeds the RTC watchdog.** The watchdog task lives on the ProCpu executor, which
a store transaction blocks — a 728 KB asset install is ~15 s of erases and a
garbage-collecting 46 KB save was measured at 25 s, both against a 20 s timeout.
`core1::fenced` feeds every 64 fences: taking a fence is proof of progress, so
the watchdog still catches a core that stopped without rebooting one that is
merely slow.

Two smaller things worth keeping. The black box grew to 10 words — the two new
ones are the call site that holds the fence in flight (`core1::tag`) and the
AppCpu park count the ProCpu saw, and `/api/status` now reports live
`core1.fences` `[begun, completed]`, so the fence cost of any operation is a
before/after delta (`docs/api.md` documents the whole `core1` object, which it
never did). And a 4 KiB page cache in front of the store's reads was measured
and **rejected**: the store's reads jump between pages, so nearly every one
missed and paid a full 4 KiB read — that build could not finish a save at all.

Verified on the Athom, 60 px WS2812, second-core build: 728 KB asset install
5/5 with all 8 files `gunzip -t` clean on read-back, 15 consecutive 46 KB
pattern saves, firmware OTA + assets via `tools/deploy.sh`, `fence_timeouts` 0,
no watchdog reset, render 121 fps. Image cost on `board-pixelblaze-v3`:
1,015,264 → 1,016,592 B, OTA-slot margin 3.18 % → 3.05 %. Getting there needed
the new work out of `fenced_as` (which is `#[inline(always)]` at ~20 flash call
sites — 2.6 KB of image) and `PageStateCache` rather than `PagePointerCache`
(3.3 KB more, and measurably no faster here).

## 2026-09-06 — Athom hardware pass on the stacked master: the second core's flash fence wedges the board (#292)

Hardware verification of what master had stacked without ever running it together
on metal — #274 (cache-MMU flash mapping), #276/#293 (pattern code arena), #278
(LXBC v5), #300 (borrowed mapped words), #302 (superinstructions) and #280
(second-core render executor) — on the Athom rig (classic ESP32, 60 px WS2812).
No serial: `/dev/ttyUSB0` was absent all session, so everything rests on
`/api/status`, the API and a 1 Hz poller. Every build was pinned from
`/api/status` rather than assumed.

**The headline is a blocker.** Any build carrying the second core wedges the
ProCpu *inside* a fenced flash op once fence traffic is sustained; the RTC
watchdog PR #280 added reboots the board 20 s later, so the symptom reads as a
network failure. `POST /api/assets` died 5 times out of 5 after 60–70 KB of a
728 KB archive — on `8b478f0`, on `0f83975`, and identically with
`EXTRA_FEATURES=flashmap-off`, while single-core `731ce81` installs the same
archive in 17.1 s through the same code with the mapping live. Black box every
time: ProCpu fence phase 3, `fences begun − completed == 1`, park-ack timeouts 0.
The rate is what predicts it — the asset writer takes ~46,800 fences a minute,
against 600–2,500 on an idle boot. Filed as **Gitea #292**, with the decoded
black boxes, the bisect, and a candidate fix that was implemented, flashed and
**disproved** (disabling the parked core's cache via the ROM helpers, which a
comment in `core1.rs` claims is already done and is not).

Twice in 25 minutes the board silently **rolled back to the other slot** — three
watchdog resets burn the OTA boot guard's three lives, and with both slots
reporting `v0.1.40` nothing but `slot` shows it. Collateral: an **interrupted
asset install leaves the partition corrupt but parseable** — the TOC is written
early, so afterwards `assets_mapped` is `true`, every `/assets/…` returns 200
with a plausible length and ETag, and 6 of 8 bodies failed `gunzip -t`. Nothing
in `/api/status` says the on-device playground is broken.

**What did verify.** On `731ce81` (mapping + arena, pre-v5, single-core):
`assets_mapped`/`code_mapped` true and `arena [0,7]` from boot; the 228,553 B
bundle in 2.13 s at 60 px and 20.77 s at 2048 px, reproducing the second-core
"before" column to within noise; a 728 KB asset install in 17.1 s with all eight
files verifying; seven clean OTAs; `core1.fence_timeouts` 0 all session. The
7-slot arena filled from *saving* ten patterns before any of them ran —
`cache_code` claims a slot on save, so with only 7 slots the eighth and later
stored patterns could never become mapped, since activation never evicts (#293's
87-slot extent arena is the answer to that).

On `0f83975` the engine and store improvements are real and measurable, when the
board stays up long enough to measure them. Main Stage's resident heap cost fell
**21,452 → 12,096 B (−44 %)** and Frogger 2D's **18,584 → 11,416 B (−39 %)**;
activation went from 2.6 s cold / 2.7 s warm to **394 ms / 72 ms**; rainbow's
`vm_us` at 60 px went 836 (`731ce81`) → 675 (`8b478f0`, v5) → **631**
(`0f83975`, superinstructions). The v5 version guard was exercised in both
directions and refuses cleanly with `{"ok":false,"code":"bc-version",...}`.

Results are on Gitea #259, #260, #271, #277 and in `docs/research/flash-mmap.md`
"Hardware follow-up". #271 and #277 stay open: their arena-eviction and swap-soak
halves are exactly the fence traffic #292 breaks. The rig is parked on `731ce81`
(single-core) with a current asset bundle built from `0f83975`, because that is
the only build on it that can install one; `0f83975` sits on the other slot.

## 2026-09-06 — Master on the Seengreat panel: the #260/#259 numbers, the O3 A/B, and two bugs

A hardware session on the 64x64 HUB75 panel (`board-seengreat-hub75`,
master e5935e6, 4096 px) putting the last three weeks of engine work —
flash mapping (#274), code arena (#276), second-core render (#280),
procedural grid map (#284), interpreter passes 1/2a (#263/#268) — on metal
for the first time as one build. Numbers and per-issue results are in
docs/boards.md "Second light"; the headlines:

- **fps at 4096 px**: rainbow 18 → **30**, 1D snake 8 → **12**, 2D snake
  4 → **8**, empty `render` 56 → **77**. With the per-stage timers the VM is
  now ~96 % of a heavy frame; the HUB75 compose is a flat 5.4–6.3 ms.
- **Bundle download** (#259): 31 s → **2.7 s** with rainbow running, 62 s →
  **1.2 s** with the 2D snake. The running pattern no longer affects the
  rate at all.
- **`CORE_O3=1` earns its 19 KB** on this board: 7–33 % of VM time
  (rainbow 26 → 30 fps), measured by rebuilding the same tree both ways.
- **`docs/perf-sweep-s3.md`**: all 299 gallery patterns at 4096 px with
  fps + `frame/vm/pipe/out` µs, sorted by VM time. `tools/hw-bench.mjs`
  grew `--perf-only` to produce it — the baseline #261 and #265 will be
  measured against.
- **`blur1D` fixed** (#295): its prefix-sum vector was an infallible
  `Vec::with_capacity(len + 1)` — 8 bytes per element, so 32 KiB at
  4096 px — and aborted the firmware when the heap couldn't serve it.
  `library/comets.js` in a playlist crash-looped the panel five times
  until the boot guard rolled the slot back. Now fallible, like blur2D's
  (#296 tracks making it O(radius) instead of O(len)).
- **OTA on the S3 is unreliable** (#294): 4 of 9 pushes wedged the ProCpu
  inside a flash op — silent, no serial, RTC-watchdog recovered, board back
  on the old slot. The black box says ProCpu fence phase 3 with the AppCpu
  parked cleanly. Suspect is the S3 analogue of the ESP32 SPI2-DMA hang:
  the `output::transfer_busy()` wait before a flash op is esp32-only, and a
  HUB75 board's GDMA is never idle. #266 stays open for it.

## 2026-09-06 — Superinstructions: 14 fused opcodes chosen by a dynamic profiler (#261)

Jeremy's #260 ask is another 2× on per-pixel cost on the S3 at 4096 px. The
interpreter's remaining lever is **instructions executed per pixel**, so this
pass built the instrument first and let it pick the work.

**The instrument** — `luxel bench <pattern> --profile [--json]` (Gitea #261)
counts what the VM actually *executed*: per opcode, per builtin, and per
**statically adjacent** opcode pair/triple — where "adjacent" means the second
instruction is the first's fall-through successor in the same function,
reached without a jump, i.e. exactly what a compiler peephole may fuse.
`tools/profile-library.mjs` runs it over all 299 library patterns and sums
the counters into one ranked report. Counters live behind luxel-core's
non-default `profile` cargo feature; the firmware takes luxel-core with
`default-features = false` and never names it, so no device image carries a
counter, and the plain `luxel bench` binary stays a throughput measurement
(with counters on it is ~2× slower — the profile output says so).

**What the library actually runs** (299 patterns, 256 px on a 16×16 grid ×
20 frames, 163.7 M instructions over 1.53 M pixel renders — **106.9
insns/px** average). Top of the ranked tables, as % of all instructions
executed:

| opcode | % | adjacent pair | % | adjacent triple | % |
|---|---:|---|---:|---|---:|
| `LOAD_L` | 20.46 | `STORE_L POP` | 6.52 | `STORE_L POP LOAD_L` | 4.41 |
| `CONST_NUM` | 12.08 | `POP LOAD_L` | 5.20 | `LOAD_G LOAD_L LOAD_IDX` | 2.61 |
| `LOAD_G` | 11.92 | `LOAD_L CONST_NUM` | 4.29 | `POP LOAD_L CONST_NUM` | 1.71 |
| `POP` | 10.18 | `LOAD_L LOAD_G` | 4.26 | `CALL_BUILTIN STORE_L POP` | 1.68 |
| `STORE_L` | 7.38 | `LOAD_G LOAD_L` | 3.85 | `POP LOAD_L LOAD_L` | 1.56 |
| `MUL` | 6.25 | `LOAD_L LOAD_L` | 3.69 | `POP LOAD_L LOAD_G` | 1.48 |
| `CALL_BUILTIN` | 5.37 | `LOAD_L LOAD_IDX` | 2.63 | `LOAD_L LOAD_G LOAD_L` | 1.20 |
| `ADD` | 4.21 | `POP LOAD_G` | 1.84 | `LOAD_L CONST_NUM ADD` | 1.06 |
| `SUB` | 4.10 | `LOAD_G MUL` | 1.82 | `LOAD_G LT JMP_IF_FALSE` | 1.00 |
| `JMP_IF_FALSE` | 3.71 | `ADD STORE_L` | 1.82 | `LOAD_L LOAD_G LT` | 0.99 |
| `LOAD_IDX` | 3.25 | `LOAD_L MUL` | 1.73 | `CONST_NUM ADD STORE_L` | 0.97 |
| `LT` | 1.85 | `CALL_BUILTIN STORE_L` | 1.68 | `ADD STORE_L POP` | 0.97 |
| `JMP` | 1.51 | `CONST_NUM MUL` | 1.61 | `LOAD_L LOAD_L MUL` | 0.92 |
| `DIV` | 1.34 | `LT JMP_IF_FALSE` | 1.61 | `CONST_NUM CONST_NUM CALL_BUILTIN` | 0.70 |

The statement-context store (`STORE_L POP`, 6.5 %) is the single biggest
shape, the paired loads together are 11.8 %, and `hsv(h, 1, 1)` /
`time(.1)` show up as `CONST_NUM CONST_NUM CALL_BUILTIN`. Nothing here was
guessed.

**Fourteen fused opcodes**, appended at `0x41..0x4E` (`0x4F..0xFF` still
free), emitted by a peephole in `compile.rs`, validated in `bytecode.rs`,
executed in `vm.rs` — `StoreLPop`, `StoreGPop`, `LoadLL`, `LoadLG`,
`LoadGL`, `LoadLIdx`, `LoadGLIdx`, `ConstOp`, `LoadLConstOp`,
`LoadGConstOp`, `CallBuiltinC`, `CallBuiltinCC`, `CmpJf`, `PopRetNull`.
Full table with operand layouts: docs/spec/bytecode.md.

**`FORMAT_VERSION` stays at 5.** Appending opcodes keeps every stored blob
readable, which is the only compatibility direction that matters (a device
never reads a blob a newer host has not produced), and a v5 blob that uses
none of them still validates and runs — test-pinned. The rule file said a
new opcode needs a bump; it was wrong, and now says so.

**Equivalence is the whole design.** Each arm does exactly what its base
sequence did, in the same order, with the same error messages and the same
`insn_start` attribution — including the `MAX_STACK` limits that the elided
intermediate pushes would have hit. Two peephole rules carry the rest:
never fuse across a JUMP TARGET (so every branch still lands on an
instruction boundary, where it landed before), and never fuse across a
SOURCE POSITION (positions are per statement, so a fused run lies inside
one statement, the position runs are unchanged, and the debugger stops on
exactly the same lines). Fuel is the one deliberate difference: a fused
instruction costs 1 unit instead of 2–3.

**Instructions per pixel, at 4096 px** — the metric the change targets:

| bench | before | after | Δ |
|---|---:|---:|---:|
| `library/rainbow.js` | 11.0 | **6.0** | −45 % |
| `library/snake.js` | 42.2 | **26.7** | −37 % |
| `library/snake-2d.js` (mapless) | 35.9 | **22.6** | −37 % |
| `library/snake-2d.js --map-grid 64x64` | 55.9 | **34.6** | −38 % |
| whole library (299 patterns) | 106.9 | **69.3** | −35 % |

Rainbow's render is now six instructions: `CallBuiltinC time`, `LoadLG`,
`Div`, `Add`, `CallBuiltinCC hsv`, `PopRetNull`.

**Host throughput is a wash, and that is the honest headline.** `luxel
bench` at 4096 px, best of 5, x86-64, against `origin/master` df0b547:

| bench | master | this branch | Δ | same binary, `--no-fuse` |
|---|---:|---:|---:|---:|
| rainbow | 29.26 M px/s | 28.47 M | −2.7 % | 27.91 M (−4.6 %) |
| snake | 13.88 M | 14.37 M | +3.5 % | 12.18 M (−12.2 %) |
| snake-2d (mapless) | 13.06 M | 13.06 M | ±0 % | 11.26 M (−13.7 %) |
| snake-2d 64×64 | 8.85 M | 8.94 M | +0.9 % | 8.01 M (−9.5 %) |

The `--no-fuse` column is the same binary compiling the same patterns
WITHOUT the peephole — byte-for-byte the opcode streams master runs. It is
5–14 % SLOWER than master, which is the price of a bigger dispatch loop on
an out-of-order x86: fourteen more arms cost registers and code locality
even for programs that never execute one. The fused stream buys that back
and lands within ±3 % of master. So on the host, cutting instructions by
37 % is worth about nothing: **x86 is not dispatch-bound** — rainbow's cost
there is the `hsv`/`time` arithmetic, not the interpreter.

The device is the opposite, which is the whole bet: #260 measured 3,200
cycles for an 11-instruction rainbow pixel on the S3 — ≈ 290 cycles per
instruction, nearly all of it dispatch. Going 11 → 6 there should show up
close to in full, and the loop-size tax is multiplicative on a far larger
per-instruction cost. **That is a prediction, not a measurement**: no
hardware was touched (both panel and Athom were in use). The on-device A/B
is Gitea #298 and carries an explicit revert criterion — if `vm_us` on the
S3 at 4096 px does not improve by ~15 %, turn the peephole off by default.
Reverting is a one-line `CompileOpts` change plus a host recompile; nothing
on any device has to change, because the fusion happens in the host
compiler and the device only ever runs the blob.

Two dispatch-loop lessons, both measured, both now in
`.claude/rules/vm-bytecode.md` and docs/boards.md: merging several opcodes
into ONE arm with an inner `match opcode` saves ~4 KB of image and costs
~14 % of throughput (the candidates were `LoadIdx` and `CallBuiltin` — the
hottest opcodes there are); moving a shared BODY out of line
(`index_read`, `call_builtin_slow`, and `err_static` for the ~45 `fail!`
sites, whose inlined `String` construction had been bloating the loop since
long before this change) saves nearly as much for free — except that
taking the in-loop `builtin_fast` fast path out with it costs 15 %, so that
half stays inline as a macro.

**Image cost** +5.5 to +7.6 KB per board (devshell builds, same creds both
sides). Nothing needed a lever: #281 had already made the C6 ship as
`luxel-fw-c6-devkit-hosted` earlier the same day, and that image is at
1,009,696 B / **3.70 %** free credless, over the 3 % floor. What did change
hands is who is tightest: **the three classic-ESP32 boards are now the
fleet's tightest shipped images** (`luxel-fw-pixelblaze-v3` 1,012,272 B /
3.46 %), and they have no `hosted-ui` lever available — taking the
playground off a Pixelblaze v3 is not a trade anyone wants. The next
feature that grows the VM brings its own diet. Per-board table in
docs/boards.md.

**Verification.** `cargo test --workspace` green including a new
`tests/superinsns.rs` (fused vs unfused render identically across six
sources covering every fusion family; byte-identical LXBC round-trip; an
unfused v5 blob still validates and runs; runtime errors inside a fused op
report the same message AND line/col; the debugger stops on the same lines;
the decoder rejects bad sub-opcodes, a `CmpJf` target off an instruction
boundary, reserved bits, and out-of-range slots in BOTH halves of a paired
load). `tools/check-library.sh` 1495/1495. **All 299 library patterns
rendered to PPM by the master binary and by this one are byte-identical**
(8×8 map, 25 frames, fixed seed). Web: `npm run build`, `npm test` 29/29,
`tools/e2e.mjs` all green in real chromium, plus a scripted breakpoint +
step-into through a four-statement pattern where every line compiles to a
fused op — it stops on lines 2, 3, 4, 5 exactly as the unfused build does.
All seven boards build; `tools/stack-check.sh` clean on pixelblaze-v3 /
s3-devkit / c6-devkit; `tools/ci.sh` green.

Not done: per-frame hoisting of loop-invariant expressions out of `render`
(item 2 of #261) — Gitea #299, gated on #298.

## 2026-09-06 — The firmware BORROWS the mapped program words (#260)

LXBC v5 (PR #278) gave `luxel_core::bytecode` a `deserialize_lean_static`
that validates a `&'static [u8]` in place and lets `Program.words` point
straight at it; PR #276/#293 made the pattern store hand the engine
`&'static [u8]` slices out of the flash mapping. The device was still
calling the copying `deserialize_lean` at every one of those sites, so the
whole point of executing from the mapping — the code and constant pool
costing **no RAM at all** — was never realized on metal. It is now.

Switched to `deserialize_lean_static`: the boot default (`PATTERN_BC`, plus
a 4-byte alignment wrapper around the `include_bytes!` so the borrow can
actually happen — `include_bytes!` has alignment 1 and the decoder silently
copies an unaligned blob), the `Msg::Library` swap arm (`patterns::code_of`),
and all three mapped branches of the engine `rebuild()` closure
(`BcLoc::Default`, the ad-hoc slot via `current_slot_code`, and a library
pattern's arena extent — the rebuild path matters most, since a
pixel-count or map change installs an engine that then lives as long as
any swap's). The transient-`Vec` sites keep the copying decode, because
their bytes do not outlive the call: the `Msg::Code`/`Msg::Crossfade`
envelope, the chunk-store fallbacks, the playlist `check_asserts`
pre-flight, and the HTTP upload's `validate`.

From the model (`cargo test -p luxel-cli --release --test heapstat`), the
resident cost of a running pattern on the device — what it now pays
instead of the `program (copy)` column:

| pattern | blob | program (copy) | **mapped** |
|---|---:|---:|---:|
| Main Stage | 25,688 | 35,540 | **8,933** |
| Opening Act | 17,556 | 27,798 | **10,577** |
| Frogger 2D | 20,488 | 27,274 | **6,421** |
| 2D Fireworks Fade | 18,028 | 23,037 | **4,713** |
| Infinite Snake | 10,528 | 15,075 | **4,280** |
| Chasing Rainbows & HSLuv | 8,492 | 12,657 | **4,038** |

### The bug this exposed: a crossfade's outgoing engine

A borrowed `Program`'s code **is** the mapped flash, so an extent an
engine still executes from must never be written, moved or freed. The
store's rule for that was "never touch the RUNNING pattern's extent", with
"running" read from `shared::get_current_pattern_id()` — a single pattern.
Two windows have an engine executing something else:

- a swap decodes the **incoming** pattern's mapped bytes before it becomes
  the current pattern (between `code_of` and `set_current_pattern_id`), and
- a **crossfade** keeps the outgoing engine alive as the blend source
  (`prev` in the render loop) for up to several seconds *after* the current
  pattern id has moved on.

In both, a `POST /api/patterns` save that compacts the arena is free to
slide that extent out from under a live VM — and on a dual-core board the
store runs on the *other* core, in parallel, so this is not even an
`await`-granularity race. `delete` had the same shape: `arena_forget` drops
the extent from the directory, handing its pages to the next save, which
erases them under the running engine.

Fixed with an explicit **pin set** in `patterns.rs`: three slots the render
task publishes — what a decode is about to borrow (`pin_code`, set *before*
`code_of`), what the live engine borrows (`pin_running`), what the
crossfade's outgoing engine borrows (`pin_prev_from_running`, released by
`unpin_prev`) — plus the current pattern id as belt and braces. Compaction,
the stale-generation sweep, the superseded-generation free and
`arena_forget` all consult the set instead of one seq; `extents.rs`'
`next_move`/`compacted_free_run` take a pin *slice* and each pinned extent
splits the free space rather than blocking the pass. `prev = None` is gone
from the render task — every drop goes through a `drop_prev` helper that
releases the pin with it. Pins are conservative by construction: a stale
one wastes arena pages until the next swap overwrites it, and can never
free something live.

Two host tests cover the planning half (`cargo test -p extent-check`): a
crossfade layout where both pinned extents must stay put while the others
pack down, and the exhaustive compaction-vs-prediction sweep now runs over
all eight pin *sets* of three extents instead of the four single pins.

Cost: **+1.7 to +2.0 KB of app image** on every board (the pin plumbing and
the `Words::Static` construction path, which nothing linked before).
`board-c6-devkit`'s full-UI build, already under image-check's 3 % floor
and not a release artifact since #293, goes 2.66 % → 2.47 %; the shipped
`luxel-fw-c6-devkit-hosted` is 4.23 % → 4.04 %. Everything else passes.
`.stack` on pixelblaze-v3 25,644 B, stack-check clean. QEMU flashmap and
both heap-regions cases pass (the takeover trio is the known-red #273).

Drive-by: `main.rs` had two doc comments spliced into each other —
`persist_current_pattern`'s "Stamp the just-swapped pattern's identity"
block was sitting on `engine_or_vmerr`, and the sentence about the raw
slot's sectors being "erased on EVERY playlist advance" had lost its first
half. Both are back on their own functions.

The on-device confirmation — idle `heap_free` should rise by roughly the
running pattern's code + constant bytes — is on the #271 hardware
checklist; no hardware was touched here.

## 2026-09-06 — The code arena grows a real allocator: page extents, not 7 slots (#281)

The arena PR #276 shipped the day before was seven fixed 40 KiB slots in
the mapped raw half of `storage`. Jeremy called it "very temporary unless
you can explain why this is better" — and it wasn't: it cached seven
patterns out of a possible twenty-four, wasted ~90 % of its 280 KiB (the
median library blob is under 1 KB, the largest ~26 KB), and made saving a
pattern quietly evict someone else's cached one.

It is now a **page-granular extent allocator**. The unit is the 4 KiB erase
page; a stored pattern's LXBC occupies a contiguous first-fit run of them;
the directory (seq → start page, byte length, bytecode generation, FNV-1a)
persists under the same reserved map key the slot table used. Trimming the
ad-hoc source region from 96 KiB to 32 KiB — `MAX_SOURCE` is 30 KB, so
read-back never needed more — grew the pool to **87 pages / 348 KiB**,
which holds every pattern a device can store many times over. There is no
eviction any more, and no LRU bookkeeping to go with it.

The rules the flash side keeps:

- **The running pattern's extent is never written, freed or moved.** A
  re-save allocates a new extent, writes, invalidates, hash-checks and
  publishes it, and only then frees the superseded one — so a power cut
  never loses both. Re-saving the *running* pattern leaves its old
  generation in the directory (those pages are executing) to be swept once
  something else is on the strip.
- **Compaction**, only when a save finds no contiguous hole *and* the
  planner says packing would open one. Live extents slide toward page 0 one
  at a time; the running one stays put and splits the free space rather
  than blocking the pass. Each move un-publishes the extent first, copies
  one page per `ota::with_flash` op with yields between (the destination is
  strictly below the source, so ascending order is a safe overlapping
  move), then invalidates, hash-checks and re-publishes — a power cut costs
  at most the extent in flight, which re-caches from its chunks.
- Activation-time fills never compact and never evict, so playlist churn
  still writes flash at most once per pattern and then never (the
  2026-08-15 wear rule). One transaction at a time (`ArenaGuard`).
- Boot rebuilds the bitmap from the directory and drops any extent the
  index, the mapped bytes' hash, or the current layout doesn't vouch for.

The planning half — bitmap, first-fit, compaction plan, directory format —
is a new `firmware/src/extents.rs`: pure, `no_std`, allocation-free, and
**host-tested**. `tools/extent-check` is a workspace member that
`#[path]`-includes it (the `tools/wledfs-check` trick, but in the
workspace) so `cargo test --workspace` runs its 18 cases: fragmentation and
reuse, pool and table exhaustion, an exhaustive sweep proving
`compacted_free_run()` predicts exactly what compaction opens — with and
without a pinned running extent — re-save ordering, directory round-trips,
torn/foreign/overlapping directories dropped on load, and a 4,000-step
churn fuzz that re-checks the bitmap against the extents every step.

`/api/status` now reports `arena: [used_pages, total_pages]` (`[0, 0]` when
the mapping is off); `code_mapped` is unchanged. The QEMU flashmap test
asserts the new boot line (`patterns: code arena 87 pages, 0 extents valid
(0 dropped), 0 pages used` on an empty library). The RAM picture is
untouched by construction — nothing under `crates/` changed; heapstat on
the rebased tree reads 0 of 299 patterns over the 45 KB swap line under
XIP, avg saving 7,391 B / 53.7 % (that figure moved because master got
LXBC v5 back, not because of this PR).

**Cost: +4.1 to +5.3 KB of app image on every board** (docs/boards.md has
the table and the symbol breakdown), and that is what finally spends the
C6's margin: `board-c6-devkit` with the on-device playground drops from
3.15 % to **2.65 %** of the OTA slot free, under `image-check.sh`'s 3 %
floor. So the lever docs/boards.md had recorded for exactly this moment is
pulled — **the C6 ships as `luxel-fw-c6-devkit-hosted` only** (4.22 %
margin); the full-UI C6 build still compiles and is still what you develop
against, it just isn't a release artifact. Getting it back over the floor
is #291. Hardware verification of the write/compaction paths (an N > 7
playlist all cached, re-save churn, compaction under a running pattern, a
power cut mid-write) went onto #271's checklist — none of it can be
exercised under QEMU.
## 2026-09-06 — LXBC v5 restored: PR #280's merge had silently reverted it

PR #278 (LXBC v5, merged 2026-09-05 21:52) was **undone the same evening** by
the merge of PR #280 (render task on the second core). That branch was cut
before v5 and its conflict resolution took its own pre-v5 side wholesale, so
master's `bytecode.rs`, `compile.rs`, the VM's decode loop, `tests/bytecode.rs`,
`docs/spec/bytecode.md`, `docs/spec/vm.md` and `.claude/rules/vm-bytecode.md`
all went back to v4 byte bytecode — `FORMAT_VERSION` was 4 again, and nothing
failed, because the revert was self-consistent across producer, consumer, tests
and docs.

Restored here by re-applying the v5 commit onto master (only `UPDATES.md`
conflicted). `cargo test --workspace` green, `check-library.sh` 299/299 on
every rig. The v5 entry below is the original one and still describes what is
in the tree.

## 2026-09-05 — Procedural grid map: panels are grids, not 48 KB of coordinates (#258)

Jeremy installed "DNA Helix 2D" on the Seengreat panel and it rendered as a
wrapped strip. The engine's default map for 2D-only patterns was a
ceil(√n)-wide grid **materialised per pixel** — 4096 × 12 B = 48 KB,
allocated (fallibly, since #275) while the outgoing engine was still alive.
It fit the panel's idle heap by a hair until the second-core render task
took its 20 KB stack, and then it failed on every swap; nothing said so.

`MapData` now has a procedural form, `grid: Option<(w, h)>`: coordinates
are computed on read, nothing is stored, and `Engine::set_grid_map` installs
it — plus the outpipe's `GridMap`, so 2D blur/glow see the geometry — without
allocating. The default grid, the wasm `lx_set_map_grid` and the firmware
all use it. A frame rendered through the procedural grid is byte-identical
to one rendered through the explicit coordinates (test-pinned across
several sizes).

On the device: `POST /api/map` takes `grid <w> <h>` (a 5-byte flash blob),
`GET /api/map` reports `kind`/`w`/`h`, and a **HUB75 panel board installs
its own `PANEL_COLS`×`PANEL_ROWS` grid at boot** when nothing is stored (and
falls back to it on clear) — so patterns that also export a 1D `render()`
(the snake game) see the panel as a matrix too, which the engine default
never did. The playground gets an "install grid on device" button beside
the grid size inputs; the native mirror accepts the grid form. Verified in
chromium against the panel and on the panel itself (DNA Helix 2D renders as
a helix again).

## 2026-09-05 — Render task on the second core (dual-core boards) + the cross-core flash fence (#259, #260, #272)

On the classic ESP32 and the ESP32-S3 the render task now runs on the
AppCpu under its own esp-rtos scheduler and thread-mode embassy executor
(`firmware/src/core1.rs`, cfg `multi_core` from build.rs); WiFi, the network
stack, the web pool and every other task stay on the ProCpu. Single-core
boards are untouched (the module compiles to no-ops). Measured on the Athom
(60 px WS2812, same master either side, `tools/render-bench.mjs` — new,
docs/tools.md): the playground bundle (228 KB) downloads in 0.9–1.2 s at 2048 px instead of 20.7–34.8 s, in 1.0–1.1 s at 60 px instead of 1.7–2.1 s, with fps during the download equal to fps without it (96→120 at 60 px rainbow); fps itself is wire-bound at 2048 px (12/9/9, `out_us` 68 ms) and `vm_us` drops 7–8 % for want of WiFi preemption. Idle heap pays the AppCpu's 20 KB stack (105,456 → 84,960 B; high-water 10,896 B through 2048 px snake-2d). Full tables in docs/firmware.md "Cores & tasks".

The one real piece of multicore machinery is the **flash fence**: SPI flash
is shared between the cache (every code fetch and now every mapped read, on
either core) and esp-storage's SPI1 ops, so `core1::fenced(op)` parks the
other core in an IRAM spin (its park software interrupt — SWI2 for the
ProCpu, SWI3 for the AppCpu) for the duration. esp-storage's own multicore
strategies were rejected: the default makes every write fail while the
second core runs, and `auto_park` hard-stalls the other core at an arbitrary
instruction — possibly inside a spinlock the flash op's next interrupt then
spins on forever. Four doors carry every flash op (`ota::with_flash`,
`patterns::AsyncFlash`, `ota::begin`'s partition reads, `flashmap::quiesced`
— #272 closed) and `FlashStorage` is constructed `multicore_ignore()`.

Getting the park right on the ESP32 cost the evening: three distinct hard
hangs (no panic, no reboot), each reproduced within a minute of snake-2d +
a bundle download and isolated with an RTC-memory black box plus an RTC
watchdog — both of which stay in the firmware (`/api/status` `core1.last`,
`core1.fence_timeouts`). (1) A Priority3 park landing inside a level-1
handler's DPORT reads wedges the bus → the park is Priority1 and masks
INTENABLE itself. (2) An AppCpu RTC-memory access inside the park wedges
the ProCpu's next one → only the ProCpu writes the black box. (3) A ROM
SPI1 flash op while the strip's SPI2 DMA transfer is still in flight hangs
the CPU (shared SPI DMA engine; impossible on single core, where the
blocking DMA write held the only core) → the fence waits for
`output::transfer_busy()` to clear. Full story: docs/firmware.md "Cores &
tasks". Image cost: +8.3–8.8 KB on the classic-ESP32 boards; the AppCpu's
20 KB stack is heap-allocated (high-water 10,896 B), `.stack` unchanged.
Unverified on metal: the S3/HUB75 boards (build green; Gitea #266). The
two-VM pixel split is Gitea #265; single-core yield-in-frame is #267.

## 2026-09-05 — LXBC v5: execution-ready word bytecode, borrowed from memory-mapped flash (#260)

The luxel-core half of Jeremy's #260 decision (the format the VM runs
straight out of the cache-MMU mapping PR #274 landed): `FORMAT_VERSION`
4 → 5. A stale v4 blob still says `bytecode format v4 (this build reads
v5) — recompile the pattern` and the existing `bc-version` recompile path
handles it; the web UI needs nothing new.

**Format (docs/spec/bytecode.md, rewritten).** The code and the constant
pool are ONE 4-byte-aligned region of little-endian `u32` words at the
tail of the blob (`words_off`/`n_words` in the header; the tables are
padded to a word boundary). One word per instruction — bits 0..8 opcode,
bits 8..32 a 24-bit operand field read as u8 / u16 / u16+u8 argc / u24
jump target depending on the opcode; `CONST_NUM` is the only two-word
instruction (raw i32 in the next word). Jump targets, `FnDef.code_start`
/`code_len`, every `pc` (frames, breakpoints, `VmError.pc`, debug
position runs) are fn-relative WORD indices. The constant pool is raw
16.16 words in the same region with a `(start, len)` table; `ArrRepr::
Const` reads decode words on access through the new `ArrView`
(`get`/`at`/`len`/`iter`) instead of a `&[Value]`, and the copy-on-write
promotion in `arr_mut` materializes an owned `Vec<Value>` exactly as
before (every bounds/truncation/budget test still pins the semantics).
Builtin operands are the RUNTIME ids (`BUILTINS` is append-only); the
import table is kept as `(name, id)` pairs purely for validation — the
decoder rejects a blob whose names don't resolve to exactly those ids
(`builtin \`foo\` is not available on this firmware — recompile the
pattern`) and any code id outside the table. Nothing in the word region
is rewritten at load, which is what makes executing it in place legal.
Opcodes 0x41..0xFF are free for #261's superinstructions. Unused operand
bits must be zero (canonical encoding; `serialize∘deserialize` stays
byte-identical, the check-library sweep proves it on all 299 patterns).

**API.** `Program { words: Words, pool: Vec<PoolEntry>, … }` with
`enum Words { Owned(Vec<u32>), Static(&'static [u32]) }` (derefs to
`[u32]`; `Program` stays `Send + Sync`). `deserialize(&[u8])` and
`deserialize_lean(&[u8])` keep their signatures and COPY the words (the
firmware's current call sites compile unchanged); new
`deserialize_lean_static(&'static [u8])` validates in place and BORROWS
the word region when the input is 4-aligned in memory (falls back to
copying otherwise — never an alignment error). `validate` runs the same
checks over the raw bytes with no word copy in any mode. The dispatch
loop is one bounds-checked `code[at]` load per instruction (`bytecode::
enc` field accessors); the byte-decoding macros are gone, the two-level
loop and the pass-2a fast paths stay.

**Host bench** (`luxel bench --pixels 4096`, best of 3, px/s, x86-64):
rainbow 28.90 M → 28.82 M (noise), snake 11.23 M → 12.72 M (+13 %),
snake-2d mapless 10.74 M → 11.78 M (+10 %), snake-2d `--map-grid 64x64`
7.96 M → 8.23 M (+3 %). The on-device numbers are the point (the S3's
per-instruction byte decode was the cost) and are still owed with the
store side.

**RAM** (`heapstat`, merged with the code-arena entry's swap columns:
swap(xip) now decodes with `deserialize_lean_static` over an aligned
`'static` blob so the program BORROWS its words, and the new `mapped`
column is that program's resident RAM; bytes):

| pattern | blob v4→v5 | program v4→v5 (copy) | swap(vec) | swap(xip) v4-copy→v5-borrow | mapped |
|---|---:|---:|---:|---:|---:|
| Main Stage | 18,459→25,688 | 28,512→35,540 | 99,847 | 33,491→22,451 | 8,933 |
| Frogger 2D | 14,878→20,488 | 21,821→27,274 | 79,491 | —→15,061 | 6,421 |
| Opening Act | 13,293→17,556 | 23,810→27,798 | 72,289 | —→23,419 | 10,577 |
| 2D Fireworks Fade | 12,666→18,028 | 17,785→23,037 | 67,877 | 33,500→27,366 | 4,713 |
| Infinite Snake | 7,728→10,528 | 12,386→15,075 | 38,395 | 24,995→21,464 | 4,280 |
| Chasing Rainbows & HSLuv | 6,098→8,492 | 10,370→12,657 | 31,067 | 17,432→14,699 | 4,038 |

Words are 4.6 B/insn against 2.6 B for v4's bytes, so the COPYING path
(hosts, wasm, and the firmware until the store calls
`deserialize_lean_static`) costs ~25 % more program RAM — swap(vec) has
6 of 299 patterns over 45 KB (was 5). On the borrowing path the resident
program is the header tables only — Main Stage 8.9 KB instead of the
28.5 KB v4 copy — and Σ swap(xip) over the gallery drops from 2,870,875 B
(v4 copying decode) to 2,594,892 B, 40.5 % under swap(vec).

**Verification.** `cargo test --workspace` (goldens in tests/bytecode.rs
updated: the pool assertion reads `prog.pool`, the export-index
corruption test locates the export entry by name because the blob's
tail is now the word region; new tests pin the borrowing path — aligned
→ `Words::Static`, unaligned → copy, both rendering identically — and
the import-table check); `tools/check-library.sh` 1495/1495 on the five
rigs; 28 reference PPMs (24 patterns @ 64 px + 4 grid runs) byte-identical
against the master binary, the one wall-clock pattern re-run back-to-back;
web `npm run build` + `npm test` + a real-chromium compile/run/debug pass
(breakpoints, step over/into/out, error line/col) — pcs cross the wasm
boundary and the unit change is invisible there because breakpoints are
resolved by line inside the engine; all ten firmware variants build with
firmware/src untouched (it only calls `deserialize_lean`) and pass
image-check — the simpler decoder is −2.0…−2.2 KB on RISC-V (C6 margin
4.13 → 4.34 % same-methodology) and +0.4…+0.75 KB on Xtensa (noise
floor), `.stack` 26,732 → 26,764 B (docs/boards.md); `tools/ci.sh`
green.

Store side (patterns.rs/main.rs handing the engine a mapped, 4-aligned
`&'static [u8]` and calling `deserialize_lean_static`) is the other
subagent's PR; until it lands the device copies words like before.

## 2026-09-05 — Pattern code arena: library patterns execute from the flash mapping (#260 store side)

The store half of the VM consumer contract in docs/research/flash-mmap.md,
landed independently of the instruction format so the two can merge in
either order. `patterns.rs` now maps the raw upper half of `storage`
(`0x290000`, 512 KiB, 8 pages) at boot with the same self-check as the
assets partition, and `patterns::current_code() -> Option<&'static [u8]>`
hands the engine the running pattern's bytecode as mapped memory: rodata
for the built-in default, the ad-hoc read-back slot (now TWO 64 KiB
bytecode sides — a swap writes the side the engine is not executing
from), or the pattern's slot in the new **code arena**: 7 × 40 KiB
page-aligned slots holding one stored pattern's contiguous LXBC each,
with a slot table (seq, bytecode generation, length, FNV-1a) under a
reserved map key that boot verifies against the index AND the mapped
bytes before trusting a slot.

**Library swaps carry only the id.** `Msg::Library { id, ms }` replaced
the envelope-carrying `Msg::Code`/`Crossfade` for playlist, activate,
MQTT and resume: the render task decodes from `code_of(id)` (mapped) or,
for a pattern without a slot, from a transient chunk-store Vec that it
then offers to a FREE or stale slot — so playlist churn writes flash at
most 7 times per library state, then never (the wear rule). Saves fill
with eviction (LRU by activation, never the running pattern); deletes
forget the slot. Identity/read-back lengths come from `source_stat(id)`,
which streams the source out of its chunks. Every arena write is the
existing `write_raw` discipline (one `ota::with_flash` per op — the same
quiesce path as the assets writer, so #272's fence hook lands in one
place) followed by `flashmap::invalidate_slice` and a hash check of the
mapped bytes. Rebuilds and the playlist pre-flight read the mapped slot
too; `/api/pattern` read-back streams from the mapping; `/api/status`
gains `code_mapped` and `arena: [used, total]`.

**Measured (heapstat, counting allocator, whole gallery).** The
library-activation peak — source Vec + blob Vec + envelope Vec, then
Program + engine — vs the arena lifecycle (blob in flash, Program +
engine): Main Stage 85,389 → 31,819 B, Frogger 2D 68,271 → 22,272,
Opening Act 63,763 → 28,715, 2D Fireworks Fade 57,153 → 33,514,
Infinite Snake 32,795 → 24,995, novas 18,359 → 17,417; over all 299
patterns 3,568 B (27.1 %) less per activation on average, and **5
patterns over 45 KB at swap → 0**. Resident cost is unchanged until the
fixed-width format lets `Program` borrow the slice (`deserialize_lean`
still copies) — that is the parent's half of the contract.

**Verified without hardware:** every board + `c6 hosted-ui` +
`athom flashmap-off` build; stack-check clean on pixelblaze-v3 / s3 / c6
(`.stack` 26,396 B on the PB); QEMU `flashmap-test.py` extended and
green — the store's mapping lands on entry 18 (assets entry 3 + 15
pages, `0x3f520000`) with its self-check ok and `code arena 0/7 slots
valid (0 dropped), 40 KiB each` on an empty library (no activation runs
under QEMU: that needs a sequential-storage image or the network, so the
write path is a #271 item); `tools/ci.sh` green. **Image cost:** credless
flake builds vs `origin/master` (0f84707): C6 1,002,720 → 1,013,248 B
(+10,528; margin **35,328 B / 3.37 %** — above the 3 % floor, inside the
6 % warn band), PB v3 981,952 → 991,184 (+9,232), Athom 982,016 →
991,424. About 6.7 KB of that is named symbols (the `Library` swap arm,
`cache_code`, the arena table code, `check_asserts` no longer inlined);
the rest is alignment. Deduplicating the three "pattern too large"
vmerr builders into `engine_or_vmerr` clawed ~1 KB back. The next ~4 KB
on the C6 trips the release gate: the accepted lever is
`EXTRA_FEATURES=hosted-ui` for that variant (docs/boards.md).

Hardware steps for the arena (activate twice, re-save the running one,
an 8-item playlist against 7 slots, ad-hoc pushes, a power cycle) are
appended to #271.

## 2026-09-05 — Flash memory-mapping through the cache MMU (assets first, the VM next; #259/#260)

Jeremy's decision for #260: the pattern engine will execute a fixed-width
instruction stream *directly out of flash* through the cache/MMU, so a
loaded pattern costs ~zero heap for its code and constants — decoding into
RAM was overruled. esp-hal exposes no mapping API; this is the facility,
designed per chip and landed with one consumer wired end-to-end.

**Design: docs/research/flash-mmap.md.** Every chip's MMU page table is a
register block the bootloader fills with the app's own pages and otherwise
leaves invalid: the classic ESP32's DPORT tables (one per core, 64 DROM0
entries, the app uses 3), the S3/C3's shared I/D table at `0x600C5000`
(512/128 entries, the app uses 16/14), the C6's indexed `SPI_MEM0` item
registers (256 entries, page size from a register, the app uses 15). A
4 MiB flash needs 64; we map 15 (assets) now and ≤16 more (the pattern
store's raw half) next. The whole thing is register pokes mirrored from
esp-idf's `mmu_ll.h` and esp-storage's private `mmu.rs`, plus ROM cache
maintenance (`Cache_Flush_rom` on the ESP32 — also what Espressif's QEMU
needs to re-sync a page; suspend/resume + `Cache_Invalidate_Addr`
elsewhere). No esp-hal patch. The doc works through the SPI0/SPI1
contention rule (a mapped read is a cache miss; none may happen during an
esp-storage op on another core — the second-core branch's flash fence
already gives that; the one thing it must add is routing `flashmap`'s
table ops through the fence, #272), WiFi (esp-radio never touches flash
at runtime), OTA (the slot is never mapped), writes under a mapping
(invalidate before reading back), the store changes the VM needs (a
page-aligned code arena in `storage`'s raw half; library chunks are not
contiguous), and the RAM accounting from the heapstat model (Infinite
Snake: 7.7 KB blob + most of 12.4 KB program off the device; Main Stage:
18.5 KB + most of 28.5 KB).

**Facility: `firmware/src/flashmap.rs`** — `map(offset, len) ->
Result<Mapped, Error>` (page-aligned offset, first-fit run of invalid
entries above the app's), `unmap`, `invalidate`/`invalidate_slice`,
`Mapped::bytes`/`leak`. Per-chip `chip` modules behind the existing chip
features; the programming functions are `#[esp_hal::ram]` with inlined
table accessors because the S3/C3/C6 sequence suspends the caches. A
`flashmap-off` cargo feature makes `map` fail so every consumer's
read_nor path can be forced.

**Consumer: the web assets partition.** `assets::map_region` maps
`0x310000+0xF0000` at boot (after `ota::init` and the takeover check),
reads the first 4 KiB both ways and refuses the mapping on any
disagreement, then leaks it; `init()` parses the TOC through it;
`FlashAsset::write_content` hands the socket 4 KiB slices of the mapping
with a `yield_now` between them — no staging Vec, no critical section per
chunk, and the 1 ms `Timer::after` that existed only to give WiFi
airtime between cache-off windows is gone from that path;
`AssetWriter::commit` invalidates the region before re-parsing.
`/api/status` gains `assets_mapped`; `tools/image-check.sh` asserts the
mapping is linked into every non-hosted image.

**Verified without hardware** (the S3 was soaking, the Athom in use —
nothing here touched a device): all six boards plus `c6-devkit +
hosted-ui` and `athom-music + flashmap-off` build; stack-check clean on
pixelblaze-v3 / s3-devkit / c6-devkit (`.stack` 26,732 B on the PB,
−48 B); credless flake images **shrink** — C6 1,000,512 → 999,120 B
(margin 49,456 B / 4.72 %), PB v3 979,312 → 976,592 B, Athom 979,152 →
976,928 B (docs/boards.md); `tools/ci.sh` green. **QEMU proves the
ESP32 path**: new `tools/qemu/flashmap-test.py` (in `run-all.py`, needs
no dumps — espflash's merged image + a synthetic LUX2 archive) sees
`flashmap: assets 0x310000+0xf0000 -> 0x3f430000 (15 x 64 KiB pages from
entry 3), self-check ok` — entry 3 is exactly the app's three DROM pages
— and `assets: 2 files installed` parsed through the mapping. The same
line shows up in boot 2 of the takeover test over WLED's littlefs
(real data, self-check ok). The three takeover tests fail on a pristine
`origin/master` build identically (a boot-1 pin-import marker that never
appears — #273, pre-existing).

**Follow-ups filed:** #271 hardware bring-up (Athom then the Seengreat
S3: the serial line to expect per chip, `assets_mapped`, re-measuring
#259's 2.1 s / 31–62 s bundle download, upload-while-serving, OTA with
assets, soak), #272 the fence hook for the second-core branch, #273 the
stale takeover assertion. The VM consumer contract (who maps, who
invalidates, what the engine may do with the slice) is written down in
the doc's "The VM consumer" section for the parallel #260 format work.

## 2026-09-05 — Engine: per-pixel performance pass 2a — hot builtins in the loop, batched pixel pass (#260)

Two structural costs pass 1 left alone, both device-free to fix:

- **Hot builtins straight off the stack.** `hsv`/`rgb`/`time`/`wave`/
  `square`/`triangle`/`sin`/`cos`/`sqrt`/`abs`/`floor`/`ceil`/`round`/
  `trunc`/`frac`/`clamp`/`min`/`max`/`mod`/`mix`/`random`/`prng` now live
  in `Vm::builtin_fast`, an `#[inline(always)]` match the dispatch loop
  calls with the top-of-stack values in place: no 16-slot args array to
  zero, no `Result<Value, VmError>` through memory, no `call8` into the
  20 KB `call_builtin` (which still delegates to the same function first,
  so the semantics exist exactly once). On Xtensa a call into a function
  that size costs a register-window spill each way on top of the
  bookkeeping; the empty-render → rgb-only delta on the panel (#260:
  ~940 cycles for five ops and one call) is the number this is aimed at.
- **Batched pixel pass.** `Engine::render_pixels` runs every pixel of a
  non-debug, non-map frame in one tight loop — reset pixel, args, `start`,
  quantize — instead of one trip per pixel through the resumable
  `drive()` state machine (stage/outcome matching, `run_stage` updates).
  Error semantics are unchanged: first error wins, asserts/resource
  guards blank the rest of the frame, a non-fatal error keeps the
  pre-error color. The debugger and map programs keep the old path.

Host `luxel bench` at 4096 px (x86 hides most of the call overhead, so
these understate the Xtensa gain): rainbow 26.2 → 28.4 M px/s, snake-2d
on a 64×64 map 7.1 → 8.5 M px/s. All 245 luxel-core tests pass unchanged.

Next in the queue for #260: the execution-ready instruction-word format
executed directly from a memory-mapped flash region (Jeremy's decision
2026-09-05 — RAM is the constraint; design + facility in progress on
agent/luxel/flash-mmap), superinstructions on top of that format (#261),
and the two-core pixel split once the core-1 executor (#259) has landed.

## 2026-09-05 — Engine: per-pixel rendering performance, pass 1 (#260)

Jeremy's ask after the HUB75 panel's first evening: 4096 px at 18 fps for
the default rainbow (~3,200 cycles per pixel on a 240 MHz S3) and 4 fps
for the 2D snake. This is the interpreter-level pass — measure first, no
code generation; the second-core work runs in parallel (#259).

**What the S3 image told us before touching anything.** The
disassembly has no local 64-bit division at all: every `i64 /` is a call
into the mask ROM's libgcc (`__divdi3` at 0x4000225c and friends), and
rainbow was doing four of them per pixel — `index / pixelCount`
(`Fx::div`), the 1D `x` coordinate the engine computed for a
`render(index)` that never reads it, and two inside `time()`
(`time_ms % period`, `(t << 16) / period`). Multiplies are fine
(`mull` + `muluh`, the S3 has MUL32_HIGH).

**Changes, all bit-exact with the old arithmetic** (new tests pin each
one against the 64-bit form, edges included):

- `Fx::div`: two 32-bit shortcuts — integer divisor (`a / B` exactly) and
  small dividend (`|a| < 0.5`, `a << 16` fits) — hit the hardware `quos`;
  the i64 form stays for the rest.
- `time()`: u32 remainder/divide while the period is ≤ 65536 ms (every
  interval ≤ 1.0) and the clock is under 49 days.
- The 1D pixel coordinate uses a u32 divide below 32768 px and is not
  computed at all for a one-parameter `render(index)`.
- `quantize` and `hsv_to_rgb` lose their i64 intermediates (they fit i32).
- The dispatch loop is now two-level: function, code slice, locals base
  and pc live in locals for the whole frame and the frame's `pc` is
  written back only before a call and at a debug stop, instead of
  `frames.last_mut().pc = …` plus re-deriving `prog.fns[..]`/the slice on
  every instruction. Builtin/user-call args are popped into a
  caller-provided buffer (the by-value 128-byte return was measurably
  worse on the host).
- Firmware: luxel-core is compiled at **opt-level 3** inside the
  opt-level-"s" image on boards whose OTA margin allows it (`CORE_O3` in
  board-target.sh, `coreO3` in flake.nix; the C6 keeps "s" — +20 KB would
  cross the 3 % floor). docs/boards.md has the measured sizes.
- Firmware: **per-stage frame timers** in `/api/status` — `frame_us`,
  `vm_us`, `pipe_us`, `out_us` (average µs per pattern frame over the
  last second; docs/firmware.md), reported by tools/hw-bench.mjs. The
  empty-render floor measured in #260 (18 ms/frame outside the VM) needs
  this split before anyone touches the HUB75 compose. `set_pixels` copies
  the frame in one memcpy instead of a 3-byte extend per pixel.

**Host (x86, `luxel bench`, 4096 px, pixels/s)** — the only numbers
available while the panel was busy soaking; the on-device table follows
once it is free:

| pattern | before | after |
|---|---:|---:|
| rainbow | 21.3 M | 26.2 M (+23 %) |
| snake (1D) | 7.6 M | 11.8 M (+55 %) |
| snake-2d, bare strip | 7.5 M | 11.0 M (+46 %) |
| snake-2d, 64×64 map | 5.4 M | 7.1 M (+32 %) |

The Xtensa gain should be larger than the host's: the ROM divides and the
opt-level switch don't exist on x86. Superinstructions / per-frame
hoisting are the next interpreter-level step if the panel numbers still
fall short (Gitea #261); AOT/JIT stays the last resort.
## 2026-09-05 — Seengreat HUB75 S3 on metal: first S3, first panel (Gitea #75)

The Seengreat RGB Matrix HUB75 S3 and its 64x64 panel arrived and were
brought up in one evening. This is the first ESP32-S3 and the first HUB75
panel Luxel has run on real hardware; everything S3-side had been
"builds, untested" since #56.

**Bring-up.** Panel driver IC read as FM6124EJ (plain shift-register, so
esp-hub75 works as-is). The stock XiaoZhi 2.2.6 firmware was dumped in
full (16 MB, two sha256-identical reads) before the first flash — a
WLED-style OTA takeover path is #256. Flashed over the S3's native
USB-Serial/JTAG (303a:1001, `/dev/ttyACM0` once passed into the
container); the first boot needed Jeremy's EN press because espflash's
reset cannot leave download mode with BOOT held. WiFi, OTA baseline
(ota_0 → ota_1 and back), the on-device web app and the panel all worked
first time — Jeremy confirmed all 64 rows and correct colours, so the
vendor-wiki pin map transcription is verified.

**What the panel taught us (all ticketed, numbers in docs/boards.md
"First light"):**

- **Opening the USB port from the host resets the chip** — the S3's
  USB-Serial/JTAG treats the open's DTR/RTS toggle as a reset request, so
  there is no passive serial tap; a reader loop reboots the board on every
  reopen (it did, for a few minutes, before this was understood). The
  upside is a free remote reset for a hung board, which the soak harness
  now uses (`HW_BENCH_RESET_CMD`).
- **Per-pixel cost is the ceiling at 4096 px**: rainbow 18 fps, the 2D
  snake game 4 fps, an EMPTY `render` 56 fps (18 ms/frame of overhead
  outside the VM). #260 (profile → interpreter fast paths → host-side AOT →
  second core).
- **The render loop starves the web server** at this frame time: the 228 KB
  playground bundle takes 2 s from the Athom and 31–62 s from the panel.
  #259; `hosted-ui` is the practical variant for this board meanwhile.
- **No 2D map by default for patterns with a 1D fallback**, and a 64x64
  map can neither be POSTed (4 KB request buffers) nor afforded (48 KB
  per-pixel storage on a ~46 KB heap). #258 — the fix is a procedural grid.
- **E1.31 multicast joins fail past group 4** on the S3 (`GroupTableFull`),
  so 21 of a 4096-px board's 25 universes are dead over multicast. #257.
- Occasional tearing on some patterns — swap atomicity to verify, #269.
- The board's other peripherals (thumb-wheel, RTC, microSD, audio out,
  PSRAM, I2C header, panel chaining) are #249–#255; the mics were #142.

**The soak found a real bug (#275, fixed here).** The first hw-bench run
crash-looped three times and then went unreachable for four minutes.
Reproduced with a serial reader attached: `memory allocation of 49152 bytes failed` inside
`Engine::set_map`, called from the pattern swap. At 4096 px the default
ceil(√n) grid map that 2D-only patterns get is 48 KB; the map installer
copied it into a SECOND 48 KB buffer with an infallible `vec!`, while the
outgoing pattern still held its heap (25 KB free after a heavy one) →
panic → reboot, and the boot guard's slot flips on top. `set_map` is now
fallible (`try_reserve_exact`, returns `bool`, the engine stays on 1D
fallback and the firmware logs it), and a new in-place `set_map_vec`
normalises an owned buffer so the default grid costs one allocation, not
two. Regression test in `crates/luxel-core/tests/engine.rs`. Verified on
the panel: the same two pushes now end in a clean "pattern too large for
this device — it left only 9 KB of heap free" with the board still
serving. The four-minute "hang" turned out to be #259 at its extreme —
a pattern at 1–2 fps makes `/api/status` take 11 s, so every client
timeout reads the board as dead while serial shows it rendering.

**Tooling.** `tools/hw-bench.mjs` no longer dies when the device does: an
unreachable device after a push is a `crashed` row, it waits for the
reboot (or runs the reset command), the report is written even if the
restore phase fails, and `HW_BENCH_FROM` resumes a run. The soak report
for this board is `docs/bench-report-seengreat-hub75.md`: 299 patterns,
184 clean, 115 with errors (VM errors plus "too large" rejections), median
7 fps at 4096 px, heap floor 18 KB, one "crash" row recovered by the reset
hook — which serial showed was a 1 fps pattern starving the web task
(#259), not a panic. No panic in the whole run.
A gotcha found while making that hook work: the S3's USB reset is
triggered by the termios setup (a baud rate), not by opening the port —
`socat … ,b115200` resets, a bare open does nothing.

## 2026-09-02 — CI: the test gate runs on Gitea now

Gitea #233. Until today nothing ran the test suite except a human
remembering to. `.gitea/workflows/ci.yml` now runs the standing acceptance
gates on every push to `master` and every pull request.

**The gate is a script, not YAML.** All four steps live in `tools/ci.sh`, so
`nix develop --command tools/ci.sh` runs byte-for-byte what CI runs: the web
build + `npm test`, `cargo test --workspace`, `tools/check-library.sh`, then
a `board-pixelblaze-v3` firmware build. The order is load-bearing — the web
build writes `web/public/gallery.json`, which `luxel-cli`'s `heapstat` test
reads, so a cargo-first gate fails on a fresh checkout for reasons that have
nothing to do with the change under test (the same trap fresh worktrees fall
into).

**The OTA-slot gate needed one extra step.** `build-esp32.sh` already ends in
`tools/image-check.sh`, but it hands it the ELF, and image-check's
size-margin half only applies to app images (magic 0xE9). So the tripwire
CLAUDE.md advertises — the app must fit the 1 MiB OTA slot — was not actually
being checked by any build anyone runs day to day, only by the release
workflow. ci.sh now makes an app image with `espflash save-image` (the same
call the flake makes) and runs image-check over that too. Current
pixelblaze-v3 margin: CI prints it every run (957,040 B used, 91,536 B
free, 8.72 % of the slot as of this commit).

**One build at a time.** A workflow-level `concurrency` group named
`luxel-ci` — global, not per-ref — with `cancel-in-progress: true`, so a new
build cancels whatever is in flight instead of queueing behind it. With
sessions merging PRs continuously that matters, and the single-slot runner is
shared with every other repo on the server — a superseded build is somebody
else's queue time.

**Runner: `nixos`,** the legacy host-mode runner — it shares the host nix
store, so the flake toolchain closures are already there and entering the
devshell is nearly free on a second run. It is slated for retirement in
favour of `nixos-podman`, which was broken on 2026-09-01;
a rehearsal there did pass end to end (run 1441, 10 min 45 s, ~8 min of it
realizing the devshell) and taught the workflow three things worth writing
down for whoever flips the switch: that container has no `/usr/bin/env`, no
`HOME` (nix then computes a *relative* cache dir and refuses it), and no
build-users group. All three, plus "drop the `PATH:` env" and "every run is
cold there", are recorded on the `runs-on:` line and in docs/releases.md.
The optional attic steps — no-ops until an `ATTIC_TOKEN` secret exists — are
the lever that buys some of that cold start back. CI writes a placeholder
`firmware/creds.env`; real credentials never reach it, and it never
publishes an image.

**Measured** on the `nixos` runner: 5 min 43 s for the first ever run (1443)
and 2 min 58 s for the next one (1448). The whole warm saving is the nix
store — entering the devshell goes 3 min 44 s → 21 s — and *none* of it is
the build tree: `actions/checkout` cleans (`git clean -ffdx`) the gitignored
`target/` and `web/node_modules` away at the top of every job, so the gate
itself is ~110–145 s either way. Left that way on purpose; a correctness gate
should not be able to go green on stale artifacts. Queue time dwarfs both
numbers — the runner is single-slot and shared with every repo on the
server, and both runs waited half an hour behind a nix-config flake check.

Docs: a "CI (the test gate)" section in docs/releases.md, `tools/ci.sh` in
docs/tools.md. Making the check *required* on master needs repo-admin
rights the bot doesn't have — filed as Gitea #246, together with the
podman migration and the attic switch-on.

## 2026-09-02 — Real GPIO behind the pin builtins (#177 closed) and a runtime strip data pin (#154 closed)

Two of the three items Jeremy picked from the 2026-09-01 fetch-work list
(the third, "push master to the Athom and soak", is their deploy step).
v0.1.40.

**Pattern GPIO is real (Gitea #177 item 4).** `pinMode` / `digitalWrite` /
`digitalRead` / `analogRead` now reach the pads on a device. The engine
still never touches hardware — it gained the two things a host was missing
(`Vm::pin_mode`, the verbatim Arduino/ESP32 mode byte per pin, and
`pins_out_high`, the last `digitalWrite` level per pin; `DigitalWrite` was a
no-op builtin) — and the new `firmware/src/gpio.rs` is that host: between
frames the render task syncs every pin the pattern named (configure the pad
from its mode, copy written levels out, read the pad back in through the
same `Engine::set_pin` injection ABI the playground's Pins panel uses).
Pins are esp-hal *types*, so this is the one place the firmware erases the
type at runtime (`AnyPin::steal` → `Flex`), gated by new per-chip and
per-board pin tables in `board.rs`: what exists, what is input-only,
flash/PSRAM/console pins, and what Luxel itself drives (strip CLK, the
Athom relay, the PB v3 status LED, the HUB75 bus, plus the runtime DATA
pin). A pattern naming a reserved pin is ignored on that pin with one
serial line; nothing else changes. ADC1 is wired on ESP32/C3/S3 through a
per-chip typed `slots!` table (esp-hal's ADC API is typed per GPIO; the
bank is rebuilt when the sampled set changes, 11 dB, 12-bit → 0..1); the C6
has no ADC channel map in esp-hal 1.1 and reads 0. `touchRead` has no
driver — #237. Cost when unused: one mask test per frame.

Two esp-hal facts cost a deploy each and are now in code comments:
switching a pad to its analog function (`set_analog`) drops the pulls and
output enable esp-hal finds there — a pot on `INPUT_PULLUP` read 0.08, not
1.0, until the digital config was re-applied on top (the RTC pull
resistors work in analog mode); and on Xtensa the RTC mux that analog mode
switched in has to be routed back by hand (`rtc_set_config(…, Digital)`)
when the pad leaves the ADC set, or the next pattern's `digitalRead` of it
reads 0 forever (`teardown_analog`).

**Verified on the Athom (v0.1.40, 60 px), no wiring, via `/api/vars`:**
`INPUT_PULLUP` idles HIGH and `INPUT_PULLDOWN`/`INPUT` LOW on real pads
(GPIO33/26/27); the case button on GPIO0 reads unpressed under its pull-up;
a 1 s square wave `digitalWrite`n to GPIO27 reads back through the pad's
input buffer in step; `analogRead(33)` reads **1.000 under the internal
pull-up and 0 under the pull-down** (full scale / floor), a floating ADC pad
~0.06; alternating analog and digital patterns on the same pad keep
working after the teardown fix; a pattern naming the relay (2), CLK (5),
DATA (18) and a flash pin (6) is refused per pin, the strip keeps
rendering, 100–120 fps and `vmerr:null` throughout. What no headless
session can see — pixels on a moved data pin, a finger on the button —
is #238 for Jeremy.

**The strip DATA pin is a setting (Gitea #154).** `board::DEFAULT_DATA_PIN`
per board; settings record v8 stores an override (`data_pin+1` in a v7 pad
byte, so v7 records read as "default" and the body length is unchanged);
`POST /api/datapin <n|default>` validates against the pin tables,
persists, and **reboots** — the SPI driver binds its MOSI pin once at boot
(`with_mosi(AnyPin::steal(pin))`, CLK stays a typed board constant). `GET
/api/config` gains `data_pin` / `data_pin_default` / `data_pin_next` /
`data_pins`; Settings → Device gains a **Data pin** select with an
"apply & reboot" button behind a confirm (deliberately not live: a
mis-click darkens the strip); a WLED takeover now IMPORTS WLED's LED pin
when the board can drive it instead of logging that it can't. Panel boards
have no strip SPI and omit all of it.

One boot-order bug found on the rig: the settings record was first read
BEFORE `ota::init`, which installs the flash driver `assets::read_chunk`
reads through — the read silently answered "no record" and the device
booted on the default pin after a successful apply (UI said "rebooting
with data on GPIO33", the device came back on 18). Moved the strip wiring
after flash init; the stored 33 then took effect on the very next boot,
which also proved the write had been fine all along.

**Verified:** `cargo test --workspace` green (new engine test
`pin_modes_and_digital_write_are_recorded_for_the_host`),
`tools/check-library.sh` 298/298 on all rigs, svelte-check clean, firmware
builds for athom-music / pixelblaze-v3 / c6-devkit. Measured after the
change: PB v3 `.stack` 27,484 → 26,860 B (the `PinHost` lives in the
render task's future; floor is 24 KB), app image 937,600 → 947,776 B;
**C6 983,264 → 990,400 B = 5.55 % OTA-slot margin, under the 6 % warn
line of `tools/image-check.sh` for the first time (floor 3 %)** — the cost
is esp-hal's per-pin dispatch tables, and it is documented in
docs/boards.md "Runtime pins". Data-pin picker driven in real chromium
against the Athom: pick → confirm → "rebooting with data on GPIO33" →
device back on 33, reload shows 33 driving, back to the default the same
way; the API refuses the relay pin (2) and an input-only pad (34) without
rebooting; the API round trip (POST 33 → back on 33 → POST default → back on 18, slot unchanged, `data_pin_next` null after each boot) matches, and input-only (34) and non-existent (99) pins are refused the same way.

Docs: docs/lang.md "Device & environment", docs/api.md (`/api/datapin`,
`/api/config`, the `/api/pins` note), docs/boards.md "Runtime pins" (the
allow tables), docs/webui.md (pin panel note, data-pin picker),
builtins.ts autocomplete text, docs/UNTESTED.md (#238's two legs).
Tickets: #237 (touchRead driver), #238 (Jeremy's bench legs).

Soak after the merge, on the deployed v0.1.40 (`tools/hw-bench.mjs`, 2 h, 60 px ws2812): **299/299 clean, 0 errors**, median 118 fps at 60 px, lowest heap 83,448 B, curve 122/99/52/27/16/8 fps at 60→2048 px (the committed report was an SK9822 run at 8 MHz SPI, so its curve is not comparable; ws2812 runs sit where they did). The seven patterns the previous report listed under 30 fps are at the same fps to within 1; the five new entries (fractal flower, both spiders, Butterfly 2D, Glittering Jewels) are the 2026-09-01 review-pass rewrites, not a regression — the per-frame pin sync is one mask test when no pattern names a pin. docs/bench-report.md regenerated.
## 2026-09-01 — curl noise: analytic simplex derivatives, `curl2`/`curl3`

The half of docs/ideas.md's "Simplex noise + curl noise" that had been
deferred since the simplex landed. The deferral was specific and correct —
"a finite-difference curl on 16.16 noise is too quantization-noisy to be
pretty; do it when the noise gets analytic derivatives" — so this change
gives the noise analytic derivatives first and the curl falls out of them.

**Derivatives.** `noise::simplex2_grad` / `simplex3_grad` return
`(n, ∂n/∂x, ∂n/∂y[, ∂n/∂z])`, differentiating the corner sum in closed
form: with per-corner offset d, t = cap − |d|² and n = S·Σ t⁴(g·d),
the exact partial is `S·Σ [ t⁴·gᵢ − 8·t³·dᵢ·(g·d) ]` over the corners
with t > 0. `grad2`/`grad` only ever returned the dot product, so
`grad2_vec`/`grad3_vec` read the same eight and twelve basis directions out
as components. The value path is untouched *structurally*, not just by
convention: `simplex2`/`simplex3` and their `_grad` twins are one function
with a `const GRAD: bool`, so with GRAD off every derivative line is dead
code and the noise value is bit-identical by construction (pinned anyway by
a sweep test over negatives, lattice points and four seeds). The derivative
terms carry 8 extra fraction bits (16.24) — t³ and t⁴ are small enough
that truncating their products at 16.16 costs about a percent, which the
divergence check sees immediately.

**Builtins (appended, batch 9).** `curl2(x, y, out, seed = 0)` writes
`(∂n/∂y, -∂n/∂x)` into `out[0..2]`; `curl3(x, y, z, out, seed = 0)` builds
three potentials from seeds `seed`, `seed+1`, `seed+2` and writes
`(∂P3/∂y - ∂P2/∂z, ∂P1/∂z - ∂P3/∂x, ∂P2/∂x - ∂P1/∂y)` into `out[0..3]`.
Both write in place and return `out`, the arrayAdd/mixColors convention, so
a render loop reuses one array; an `out` too short (or not an array) is a
clean runtime error naming the builtin. Missing `seed` reads as 0 the same
way `simplex2`'s does.

**Numbers.** Gradient vs a central finite difference at h = 1/32: max
|error| **0.085** (2D) and **0.168** (3D) against gradients peaking at
**6.39** — and it falls as h² over h = 1/8 … 1/64 (0.94 → 0.25 →
0.085 in 2D), which is what a *correct* analytic gradient looks like
against an FD that is itself the approximation. `curl2`'s finite-difference
divergence is **0.095** at h = 1/64, same h² fall-off, flooring at 0.077
by 1/128 where 16.16 quantization takes over; analytically it is
identically zero. Output magnitude (these are noise *derivatives*, not unit
vectors): `curl2` components peak at **6.2**, mean vector length **2.7**;
`curl3` peaks at **8.3**, mean length **4.0**.

**Pattern.** `library/curl-flow-2d.js` — specks advected along the field,
trails on a canvas, three real-unit sliders (display widths/second, trail
half-life in seconds, vortices across the display), and a 1D fallback that
flies a horizon line across the same canvas so a bare strip still shows the
flow.

**Verified:** `cargo test --workspace` green; `tools/check-library.sh`
299/299 on all five rigs; the pattern driven in real chromium (60 fps, no
page errors). Firmware +8,512 B on `board-c6-devkit` (991,984 B, 5.40 %
of the OTA slot left) and +8,272 B on `board-pixelblaze-v3` — two extra
monomorphizations of the simplex kernels; `.stack` unchanged at 27,484 B
with no new frames. Details in docs/boards.md.

## 2026-09-01 — `analogRead`/`touchRead` get an injection path

Gitea #206, the analog half of the pin-injection work #177 started. Both
builtins returned a flat 0 no matter what, so any pattern riding a
potentiometer or a capacitive pad rendered exactly one state — the same
unjudgeable position digital buttons were in before #177.

**Engine.** `Vm` grows a `[u16; 64]` table of injected 0..1 values plus an
`analog_used` mask — 136 bytes, no allocation. Values are stored as 16.15
codes (`Fx` raw >> 1) so the whole range *including exactly 1.0* fits a
`u16` and round-trips bit-exactly for anything a pattern literal can
express. `Vm`/`Engine::set_analog_pin` clamps to 0..1, `analog_read` reads
back, `analog_pins_used` reports which pins the pattern sampled. Unlike the
digital surface there is **no driven bit and no release**: an undriven ADC
or touch pad reading 0 is a defensible resting state (an undriven pulled-up
button reading LOW was not), so releasing a pin is just writing 0. Both
builtins share one value per pin — different peripherals on real silicon,
but no board wires an ADC and a touch pad to the same pad, and one "what
does this pin read" value is all a host has to offer either way.
`BUILTINS` is untouched: `analogRead`/`touchRead` were always in it.

**Surfaces.** wasm ABI gains `lx_set_analog_pin` / `lx_analog_read` /
`lx_analog_pins_used`. `POST /api/pins` keeps its digital grammar and adds
a leading kind word for the analog form — `a 33 0.42` (`analog`/`touch`
also accepted), with `x`/`release` meaning 0; `parse_pin_lines` now returns
a `PinWrite` enum rather than `(pin, level)` pairs. The verify harness
gains `--analog-orig`/`--analog-port` on snap.mjs, an `analogPins` fixup
block (seventh kind), `analogPinsApplied` in `meta.json`, and the
pass-throughs in report.mjs / review.mjs / the review UI.

**Playground.** The Pins panel (#205) now also lists pins the pattern reads
with `analogRead`/`touchRead` and gives each a 0..1 slider — no press/latch,
because a pot has a position, not a pressed state. The panel appears for an
analog-only pattern that never calls `pinMode`.

**Verified:** `cargo test --workspace` green (238 tests) with two new engine
tests (`set_analog_pin_drives_analog_read`,
`analog_pins_used_reports_what_the_pattern_read`) and
`netin::analog_pin_lines_parse`; `tools/wasm-smoke.mjs` drives the raw FFI
undriven → full scale → half → released and checks the clamp;
`tools/serve-e2e.mjs` gains seven checks pushing `a 33 0.5` into a live
pattern's pixels through the mirror (127,127,127 at half scale);
`web/tools/e2e.mjs` drags the slider with a real pointer in chromium and
reads the strip back (full scale → red 255, quarter → dimmer, rows appear
and disappear with the pattern). No corpus original reads analog live —
the one hit is a commented-out line in `light-organ-2-0` — so the harness
plumbing was proven on a scratch pair instead: fixup-driven 0.7 renders
mean 60 on both sides, `--undriven` renders mean 0 with the warning, and
`--analog-orig 33=0.25 --analog-port 33=1` renders 21 vs 87. Firmware
builds; measured with `tools/stack-check.sh` on pixelblaze-v3, `.stack`
(leftover-DRAM main-task stack) goes 27,756 → 27,484 B and the
`budgeted_engine` frame 1,264 → 1,536 B — 272 B either way, two `Engine`
values' worth of the 136-byte table — still far under the 12 KB per-frame
budget and the 24 KB stack floor.

Real firmware GPIO (#177 item 4) is still open — the playground says so,
and nothing is forwarded to a bound device.

## 2026-09-01 — Builtins batch 8: the per-pixel state buffer (`pixelState` / `setPixelState`)

The last engine idea in docs/ideas.md with no partial credit: a sanctioned
scratch buffer the engine double-buffers, so feedback effects stop
hand-rolling `array(pixelCount)` pairs and swap loops. Jeremy's one
condition — it must cost nothing unless a pattern actually uses it — is
the design's spine.

**API.** `pixelState(index[, ch])` reads the value committed for that
pixel LAST frame; `setPixelState(index[, ch], v)` writes the value for
NEXT frame and returns `v`. Up to four channels (a colour plus a scalar).
Every read in a frame sees the same snapshot no matter which pixels have
already rendered — the property a single user array can't give a
neighbour tap inside `render()` — and a pixel nobody writes keeps its
value (the commit swaps the buffers, then copies the new front over the
new back). Out-of-range indices and channels read 0 and write nothing,
so `pixelState(index - 1)` at the strip's end needs no clamp; a channel
outside 0..3 is the one runtime error. The handoff happens once per
completed frame (`beforeRender` writes included); a frame held by
`setFrameRate` runs no pattern code and doesn't hand off; state seeded at
top-level init is committed once so frame 1 reads it.

**Memory only when used.** `Vm.pixel_state` is `Option<Box<…>>`, `None`
until the first `setPixelState`. Reads never allocate — a read-only
pattern gets 0 and the buffer stays absent (`Engine::pixel_state_bytes()`
= 0, asserted). The first write allocates `pixelCount × channels × 4 B ×
2` with fallible reservations and charges it to the same array byte
budget the arena uses, so on the device an oversized buffer is the
familiar "memory budget exceeded" vmerr, never an allocator panic
(asserted: 2048 px against a 4 KB budget rejects and allocates nothing;
against 16 KB it takes exactly 16,384 B). A later, higher channel grows
the buffer in place (channel-major layout, so growth is an append that
keeps existing values). Versus two swapped `array(pixelCount)` buffers
this is half the RAM (4-byte Fx, not 8-byte Value) and stays off the
PB-compatible 10,240-element ledger.

**Where it lives.** `Builtin::{PixelState, SetPixelState}` appended to
the table (ids stay stable); `PixelState` struct + `pixel_state_ensure`
/ `pixel_state_commit` in vm.rs; the engine commits through a new
`finish_frame()` at the three normal end-of-frame exits (last pixel, no
render entry, zero pixels) and once after init. Eight engine tests cover
laziness, read-last/write-next, neighbour consistency, carry-over,
init seeding, channel growth, out-of-range behaviour, and the budget.

**Docs + showcase.** docs/lang.md gets a "Per-pixel state" section;
builtins.ts carries both entries for autocomplete; docs/ideas.md marks
the item DONE. `library/ember-diffusion.js` is the neighbour-reading
showcase (3-tap heat diffusion + random sparks, no arrays at all) —
check-library 298/298 on the default grid and the 60/512 px strips;
driven in real chromium (gallery search → tile → editor: the preview
lights and varies frame to frame, no compile banner, autocomplete
offers `setPixelState`). Firmware builds for `board-pixelblaze-v3`; the
change is heap-only (no new statics), image delta recorded in the PR.
## 2026-09-01 — arrayReplace-as-fill: the last 15 sites fixed, and a lint so it can't come back

Review pass 2 established that `arrayReplace(a, v1, v2, …)` **splats** its
value list from index 0 — one value writes one slot — and fixed 13 ports
that read it as a fill. Fifteen two-argument call sites across eight
patterns were deliberately left behind because Jeremy had scored those
patterns *good* and a blind rewrite risked changing an approved render
(Gitea #225). This pass audits all fifteen individually, renders each
pattern before and after, and adds the lint.

All fifteen were misuses; none was a legitimate single-slot splat. The
replacements are the pass-2 idiom, `feedback(a, 0)` to zero a buffer, plus
`arrayMutate(a, (v) => 1)` where the intent was a fill with a non-zero
constant.

Before/after was rendered headlessly through the engine wasm at a pinned
seed and delta (`tools/verify/snap.mjs` needs the corpus original for its
side-by-side, so a scratch port-only harness over `tools/verify/enginehost.mjs`
did the comparison — same engine, no corpus).

**Genuinely degenerate, now animating**

- `stargen-polar-2d` (mode 11) — `arrayReplace(energy, 0)` sat under the
  comment "buffer cleared every frame" and cleared exactly one pixel. The
  per-pixel accumulation buffer only ever grew, so `min(1, energy*6)`
  pinned the whole strip to white within ~2 s and stayed there: avg
  brightness 207/255 with 418 of 600 frames showing zero motion. Now 7.2
  avg, zero frozen frames, and the drifting icy snow-sparkle the header
  describes.
- `cellular-automata-1d` — `reseed()` cleared `cur[0]` and `hueAge[0]`,
  so the periodic reseed (default 20 s) did not reset anything; it dropped
  one live cell into the running generation and the Sierpinski triangle
  became a garbled superposition of two overlapping runs. Now each reseed
  restarts cleanly from the centre cell.
- `falling-sand-2d` — the post-fade clear left sub-visible dust in every
  cell, which reads as sand: `grid[spout] != 0` and `grid[spout+gw] > 0`
  both latch immediately, so the panel re-entered the fade at about a
  third full, every ~8 s, forever. The pile now fills the panel over ~24 s
  and fades as the header describes ("when the pile reaches the spout the
  panel fades out and pours again").
- `reaction-diffusion-2d` — `arrayReplace(A, 1)` left chemical A at zero
  everywhere, so the seeded culture always starved, went black, and only
  recovered via the 3-second reseed check. With A saturated the seeds bloom
  directly. Both versions converge on the same worm attractor by ~8 s and
  are visually indistinguishable from there on; the change is confined to
  the startup transient, which no longer passes through a dead-black gap.
  Per-frame motion over 30 s: 0.71 → 18.2, zero-motion frames 26 → 0.
- `music-sequencer-for-v3-only` — `fpOff()`'s `arrayReplace(valA, 0)` meant
  the "off" mini-pattern never turned the strip off; the previous look
  stayed frozen on screen for the whole entry (nothing else decays `valA`
  globally). `resetShared()` had the same problem three ways over, including
  `arrayReplace(satA, 1)` which left the scratch desaturated instead of
  saturated. Off entries are now actually dark.

**Correct now, identical as rendered**

- `lissajous-curve-tracer` — `clearTrail()` runs only from the A/B/delta
  control handlers, so an undriven render is byte-identical before and
  after (verified: same PNG md5 over 24 s). Driving `sliderA` mid-run
  diverges at exactly that frame, which is the fix: changing the curve
  shape now clears the persistence buffer instead of leaving the old figure
  smeared under the new one.
- `flash-posterize-music-sequencer-framework` and `music-sequencer-for-v2`
  — the per-entry "fresh canvas" clears (and v2's one-shot keyboard clear).
  60 s under the deterministic beat120 sensor synth is byte-identical
  before and after, because these entries repaint every pixel each frame;
  the fix is latent correctness for entries that do not.

**Lint** — `tools/check-library.sh` now fails the sweep on any
two-argument `arrayReplace(` in `library/*.js`, before any pattern is
compiled, naming file:line and the three replacements (`feedback(a, 0)`,
`arrayMutate(a, (v) => c)`, `arrayReplaceAt(a, i, v)`). It counts
top-level commas inside the call, so three-argument splats,
`arrayReplaceAt`, longer identifiers and mentions inside `//` comments are
not flagged — the fixed sites all cite the old idiom in their comments. A
deliberate single-slot splat opts out with a trailing
`// arrayReplace-2arg-ok`; no site in `library/` needs one today.

Verification: `tools/check-library.sh` 297/297 on all five rigs (both
lints clean), `cargo test --workspace` green, `gen-gallery.mjs` 297
patterns, `web/tools/lxp.mjs compile` on all eight touched patterns. The
sweep's stale 323/323 baseline is corrected to 297/297 in the script header
and docs/tools.md. Closes #225.

## 2026-09-01 — soak.mjs actually soaks again (LXP1 envelopes, device mode, loud failure); e2e mkdirs its shot dir

`tools/soak.mjs` was POSTing raw pattern source to `/api/code`, which has taken
an **LXP1 envelope** (source + LXBC bytecode) since devices stopped compiling.
Every upload bounced with `bad envelope magic (expected LXP1 — old client?)`,
landed in the "rejected" bucket, and the script still exited 0 — so the
"host-side twin" of the hardware soak had been measuring the mirror's envelope
validator, not the engine (Gitea #218, found in the #211 API audit; every other
consumer had been moved to `lxpBody()` when the envelope landed).

soak.mjs now compiles each pattern once up front via `web/tools/lxp.mjs` and
uploads the envelope, distinguishing a *local* compile failure from a *device*
rejection. It **exits 1** when under half the uploads are accepted, with the
rejection reasons printed — an all-rejected run can no longer masquerade as a
green soak. Two additions fell out of verifying it: `--device <ip|url>` runs the
same churn against real hardware instead of spawning the mirror (with
hw-bench's `connection: close` + retry manners), `--limit <n>` caps the pattern
count for a short run, and the pattern source falls back to
`web/public/gallery.json` when the gitignored `corpus/` is absent, so a fresh
worktree can soak at all. `tools/event-soak.mjs`'s `st0.version !== "0.1.39"`
hard-throw became a `>= 0.1.39` minimum check; it was unrunnable against any
later build.

Verified on the Athom rig (192.168.0.183, v0.1.39, 60 px ws2812, slot ota_0):
24/24 uploads accepted over two rounds of 12 gallery patterns, `/api/pattern`
confirming the last-pushed pattern ("2D Wandering Fireball") actually live,
95–123 fps, `vmerr:null` throughout; 6/6 accepted again through the corpus
loader path. Host mode against `luxel serve`: 25/25 accepted, RSS 5.5 → 6.1 MB.
The failure path was exercised by deliberately posting non-envelope bodies —
3/3 rejected, exit 1, which also reproduces the pre-fix #218 behaviour exactly.
Rig left running rainbow at 123 fps, heap 104,944, `vmerr:null`.

Separately, `web/tools/e2e.mjs` now `mkdirSync`s its screenshot directory
(Gitea #224). Pointed at a non-existent dir it used to die mid-suite with a bare
ENOENT on `e2e-1-library.png`, reading like a puppeteer fault; it cost a debug
cycle during the #205 pin-panel work. Re-ran the full suite into a fresh nested
path: all checks pass, nine screenshots written.

## 2026-09-01 — Review pass 2: 99 open decisions closed out; the arrayReplace fill myth falls

Jeremy's second sitting with the review UI produced 166 new/updated
decisions (tools/verify/decisions.json): 121 good, 31 delete (25 of them
pass-1 deletions that had never been addressedAt-stamped), 61 needs-work,
7 fork. Every actionable one is acted on in this pass, fanned out across
20 subagents in one worktree, plus Gitea #197 folded in.

**Deleted (6)** — christmas-lights-2, dimbypixel, icicleblaze,
iran-solidarity, lightning-zap (its pass-1 fork lightning-strike stays),
pride-progress: port + spec + verdict each; pairs.json regenerated.

**The systemic find: `arrayReplace(a, 0)` is not a fill.** It splats its
value list from index 0 — one value writes one slot — but
web/src/lib/builtins.ts documented it as "Set every element to value",
and 13 ports used it as a per-frame buffer clear. Buffers accumulated
forever: sparks-center froze solid white, wanderers went solid red,
portal/scrolls/slowflies/heatshivers/aurorashivers/bustle railed into
static washes, matrix-rain kept one live column, amoeba decayed to
black, nano-orbital smeared into a frozen rainbow, wanderedges died to
one pixel, bouncing-balls-hsv never blanked. All 13 fixed
(`feedback(buf, 0)` is the idiom); builtins.ts + docs/lang.md corrected.
The ~8 patterns using the same idiom that Jeremy scored "good" are
deliberately untouched — audit ticketed (#225).

**Controls pass (12+)** — the "perfect, needs controls" bucket:
blink-fade, color-bands(+buffered — whose dimness was a stray /2 before
a 4th power; mean 12→77 vs orig 75), crossfading, fireblobs, fireflies,
halloween-wavy-bands, nyan-lights, scary-pumpkin, shimmer-crossfade-2d,
unstable-orbits-2d, xorcery-2d-3d, both music sequencers (renamed
Opening Act / Main Stage). House style throughout; untouched renders
proven byte-identical except where the feedback demanded a look change.

**Defect fixes (highlights)** — 80s-kid-show's one-frame flicker was a
stale draw cache surviving object swaps; doom-fire-2d had runaway wind
advection plus cooling that never ended the flame; zoom-kaleidoscope had
no time term in its scale (now an 8x exponential breathe);
2d-sinc-theta-theta carried a spurious x3 ring frequency that aliased
into confetti; all-lasers-fire cycled 10x too fast and 8x too dim;
raindrops-2d injected drops into the wrong sim buffer and had a dead
border ring; eye-of-sauron's ridge offset of 0 collapsed the eye to
specks; rgbclock-2d froze against the harness's pinned wall clock;
sound-music-spectrum-visualizer had a 16.16 floor() band-boundary bug
and amplified its own squelch floor; upward-waves-3d had gravity mapped
onto the wrong lattice axis; a-peak-integrator rebuilt to the original's
fixed 144-px meter with per-pixel drain (beat cycle now byte-for-byte);
3d-rotation-spotlights' cone was a hairline (SCALE 1/PI² vs 1/PI);
oasis's speed dial's entire range sat under the visible-motion floor —
now real px/s — and its #197 16.16 overflow (index+off*w leaving ±32767)
is fixed and clean at 64–2048 px. matrix-rain, mandelbrot-2d,
metaballs-of-fire-2d, perlin-simplex-noise-1d, heart, bustle,
bouncing-balls-hsv, distance-function-kaleidoscope-2, amoeba,
aurorashivers, accelerometer-level-example, three-red-pixels-array (now
byte-identical to its original), sparks-center, wanderedges, wanderers,
portal, scrolls, slowflies, heatshivers, nano-orbital all diagnosed from
pixels + judge verdicts and brought back to their originals.

**Redesigns & forks (feedback-driven, fidelity waived)** —
2d-fireworks-fade is a real shell sim (six modes: peony/willow/crackle/
ring/comet/finale, loop toggle walking them); kaleidoscope-2d is a
stained-glass rosette; butterfly-2d has actual anatomy and a flap;
frogger-2d is a full self-playing game with a validated Smartness dial;
both spiders got exaggerated silhouettes with jointed alternating-tetrad
gaits (dire's glow behind a dial); millipede is a segmented creature
with peristaltic gait; both perlin fires share a proper thresholded
flame model (wind variant gusts and leans); 4th rebuilt (crackle
deposits outran fade → white-out; Spacing now StripeWidth in px); the
coronal pair share a rayed-eruption engine; bouncy-boxes lost its tear;
fractal-flower/animated-asterisks/crosstown-traffic/geometry-morphing
got the requested defaults; orv-christmas-tree's giant stars were decay-
buffer smears, now point sprites.

**Verification** — every pattern re-rendered against its original via
snap.mjs (motion strips, statsSummary, per-pixel dumps where it
mattered); every new dial probe-responsive across its declared bounds;
tools/check-library.sh 297/297 on all five rigs, directive lint clean.
addressedAt stamped on all 99 acted decisions (incl. the 25 stale
pass-1 deletes); pairs.json regenerated.

Residue on Gitea: arrayReplace audit for the untouched "good" patterns
(#225), pre-existing judge nits deliberately out of scope are listed in
the PR description.

## 2026-08-31 — The playground asks for local network access, and says so when refused

Gitea #162. The firmware's fallback page hands a device with no on-flash UI
to the hosted console as
`https://googlebot42.github.io/luxel/?device=http://<host>`. Every request
that console then makes is an https page reaching a plain-http LAN address:
mixed content, and a local-network request. The installer page has known how
to ask for that since 2026-08-15 — `targetAddressSpace: "local"` — but the
playground's own fetch gate rode on Chromium's auto-detection with no hint,
and when the request failed it said "cannot reach device", which is exactly
the wrong diagnosis: the device is fine and the browser never dialled.

**One classifier, not two.** The rule is unforgiving in a way that makes
duplication dangerous: a hint that DISAGREES with the target's real address
space hard-fails the request (measured on the live site, PR #27/#28), so
"send it everywhere" is not a safe default — it breaks working setups. The
installer's copy is now `web/src/lib/lna.ts` and both callers share it:
`gatedFetch` (every API call and startup asset the app fetches) and
`src/flash/lib/device.ts`. It hints only from an https page, only to a
genuinely local-space host — RFC1918, 169.254/16, `.local`, IPv6 ULA and
link-local — and never to loopback or public. Same module answers "did the
BROWSER refuse this?", which `fetch()` itself will not tell you: an https
page asking for an http target is the only shape that gets refused that way,
so it's decided by shape, not by error text. 29 unit cases pin the
classification and the hint selection (`web/tests/lna.test.mjs`, `npm test`),
including the near-misses either side of the 172.16/12 block.

**The UI now names the problem.** A refused connect raises a full-width bar
above every tab: this copy is https, the device speaks http, Chromium has to
grant Local Network Access — then the two ways around it, with the first one
a live link at the device itself, which serves this same console over plain
http with no permission involved. Same code path, same failure, from an http
origin: the ordinary unreachable error, unchanged.

**Verified, and the part that isn't.** New harness `web/tools/lna-e2e.mjs`
serves `web/dist` twice — over TLS with a throwaway self-signed cert
(`openssl` added to the dev shell) and over plain http — because the whole
behaviour keys on `location.protocol`, which is `[Unforgeable]`: you cannot
fake https from inside the page. 8/8 checks, ~40 s, no hardware: the blocked
bar and its device link on https, the plain error on http, and the un-mocked
https→LAN request landing in the blocked state. Plus flash-e2e 18/18,
device-e2e and e2e green, `svelte-check` 0/0. The emitted `<script>`/`<link>`
set of `dist/index.html` is byte-for-byte the same shape — the new module
landed in the shared `app` chunk that `index.js` already imported — so the
cold-load burst (#92) is untouched.

What could not be verified stays #162's open half. The harness also A/Bs the
hint from the page (none / local / public) against Chromium's own errorText,
and all three came back `net::ERR_CONNECTION_REFUSED`: this browser is not
running the policy at all. Forcing it with
`--enable-features=LocalNetworkAccessChecks` and
`--ip-address-space-overrides=127.0.0.1:<port>=public` changed nothing
(recorded in the harness header so nobody retries it hoping). So the granted
path — permission prompt, allow, console connects — remains a headful task
against a real device, now written down in docs/UNTESTED.md. The blocked
path is the one a machine can prove, and it is proven.

## 2026-08-31 — the playground can press a button now (pin panel)

Gitea #205. Pin injection shipped on 2026-08-30 (`Vm::set_pin`, `lx_set_pin`,
`setPin` in `web/src/lib/luxel.ts`), and every consumer of it was a script:
the port-review harness, `snap.mjs`, `POST /api/pins` on the CLI mirror. A
person sitting in front of the playground with a pattern that reads
`digitalRead(4)` still had no way to change what it read. The ABI was
reachable; the surface was not.

The blocker was knowing *which* pins to offer. Pin numbers are ordinary
runtime values — `digitalRead(buttonPin)` where `buttonPin` is a variable —
so nothing static in the bytecode says "this pattern uses GPIO 26". The
engine now just records it: a `pin_used` bitmask on the VM, set on every
`pinMode` and `digitalRead` of an in-window pin, read back through a new
`lx_pins_used(h, half)` wasm export (two i32 halves; the C ABI has no u64
return) plus `lx_pins_idle_high` for the pull-up mask. No new builtin —
`BUILTINS` is append-only and this is host-side introspection, not language.
The mask is **sticky** for the life of the VM on purpose: a pin read inside a
branch that stops being taken keeps its control instead of flickering away.
It is also polled, not computed once, because a top-level `pinMode` is
visible at compile time but a pin only ever named inside `beforeRender`
isn't known until the pattern has run a frame.

The panel (`web/src/components/PinPanel.svelte`) appears under Controls only
when that mask is non-empty, so the great majority of patterns never see it.
Per pin: a momentary `press` (pointer down drives, pointer up releases — plus
Space/Enter, since a `<button>` would otherwise be keyboard-dead for a
press-and-hold), a `hold` latch that keeps the pin driven with nothing held
down, a live HIGH/LOW readout, and the idle level spelled out. Press and
latch are independent holds on the same pin: either one drives it, neither
releases it. Pressing drives the pin to the **opposite of its idle level**,
which is the whole point of the driven-vs-idle model the ABI already had — a
pulled-up pin goes LOW (button to ground), an `INPUT`/`INPUT_PULLDOWN` pin
goes HIGH. A recompile builds a fresh VM that drives nothing, so the latch
state is owned by `App.svelte` and cleared with it.

Deliberately **not** forwarded to a device: `firmware/src/server.rs` has no
`/api/pins` — that endpoint exists only on the CLI mirror, and real firmware
GPIO is #177 item 4, still open. Rather than send presses into a void, device
mode says in the panel hint that this is preview-only.

Verified in real chromium against the library: `Lightbulb - Crank Hue to
Complete` (GPIO 25, `INPUT_PULLDOWN`) shows one pin, idles LOW, and a press
turns the strip red as the crank sensor registers a turn; `Example - Button
w/ debounce` (GPIO 26, `INPUT_PULLUP`) idles HIGH and presses LOW. A pattern
with no GPIO shows no panel at all. Ten checks in `web/tools/e2e.mjs` now
cover press/latch/release and panel appear/disappear, plus a `pins_used`
semantics test in `crates/luxel-core/tests/semantics.rs`.

That button-debounce pattern's header comment claimed there was "no way to
drive it from outside the pattern" — true when it was written, false as of
this change; rewritten.

## 2026-08-31 — snap.mjs `--vars-sweep`: reading a dial's map through the vars API

Gitea #207. Fidelity work keeps asking one question the pixels answer only
vaguely: *does the port's dial map the way the original's does?* The exact
answer has always been available — an original's EXPORTED vars are its public
`/api/vars` surface, so reading them costs nothing at the clean-room firewall
— but getting it needed a throwaway script every time.

`node tools/verify/snap.mjs <slug> --vars-sweep <control>` now does it in one
command. It sets one named control across N settings (`--sweep-steps`,
default 5) on both sides and reads back every exported var's value at each,
plus a **baseline row** with the control untouched. Output is a
`--probe-controls`-style table on stdout, `varsweep.json` in the pair's output
dir, and a short `varsSweep` block in `meta.json`. Variable names and numbers
only — never source, never an excerpt.

Two decisions carry most of the value:

- **Each side is swept across its own range**, not across the same raw
  numbers. A corpus original's slider sends 0..1 (Pixel Blaze ignores the
  `//#` comment); a port that declares `min=1 max=10` sends 1..10. The two
  dials are comparable at the same *fraction* of travel, which is what the
  table's `frac` column is. `--vars-sweep "NAME=0,0.5,1"` sweeps literal
  values when you want them.
- **The baseline row**, because a default mismatch is invisible in any
  dialed-in comparison — the finding that first motivated the ticket
  (line-dancer-2d's original defaults Speed to 4.6).

It pays immediately. `all-lasers-fire`: the original's Speed is *inverted and
strongly non-linear* (`speed` 1.0 → 0.42 → 0.13 → 0.016 → 0.0001 across the
dial) where the port's is a linear identity (0 → 1), with defaults 0.3 vs 0.5
and `blastScale` 5 vs 1. `midpointdisplacement1d`: the original's Speed runs
0.1 → 0 (inverted, 10× narrower) against the port's 0.05 → 1.05, on top of
different var *names* (orig `mapLifetime`/`maxLevel`, port `lifetime`/`detail`).
Neither is legible from a render.

Edge cases are explicit rather than silent: a control on neither side is a
hard exit naming what each side does have; a control on only one side warns
and sweeps the side that has it (that asymmetry is a finding); sides with
different var sets get the union as columns with `-` for the absentee; a pair
exporting nothing at all says so instead of printing an empty grid.

## 2026-08-31 — docs/api.md: the HTTP surface, written down once

Gitea #211. Until now the only way to learn what a Luxel device or the
mirror answers was to grep `firmware/src/server.rs` and
`crates/luxel-cli/src/serve.rs` side by side, and the split between them —
which routes exist on one target and not the other — was written down
nowhere at all. `docs/api.md` is that reference: 52 route entries grouped by
area (status, live coding, pattern library, playlist, settings, network,
injection, firmware maintenance, pages), each with method, request body,
response shape, and a column saying which target serves it.

Everything in it is read out of the two servers, not remembered. The parts
that were genuinely undiscoverable:

- **Three route entries are not on both sides.** `POST /api/pins` is
  mirror-only (real device GPIO is #177 item 4); `POST /api/ota` and
  `POST /api/assets` are firmware-only. Everything else — 49 entries —
  matches.
- **`ok:false` arrives with HTTP 200.** Only unrouted paths 404, so clients
  must read the body, not the status.
- **Raw 16.16 is not uniform.** `/api/vars`, `/api/readouts`,
  `/api/control`, `/api/var`, `/api/map` and playlist `C` lines are raw
  i32; but `GET /api/playlist`'s `items[].controls` come back as *decimal*.
  You POST raw and GET decimal on the same field.
- **Two response-shape divergences between the targets**, both documented
  rather than silently papered over: firmware `/api/status` carries
  `src`/`bc`/`web` that the mirror does not, and `POST /api/output/palette`
  answers a bare `{"ok":true}` on firmware but echoes `palette` +
  `paletteAmount` on the mirror.
- **Exactly three routes reboot a device**: `POST /api/wifi`,
  `POST /api/apmode`, and a successful `POST /api/ota`. Nothing else does.

README.md and docs/tools.md now point at it instead of describing routes in
passing, and `firmware/src/server.rs`'s 37-line inline route table — the
thing everyone was grepping — is replaced by a pointer plus the invariants
you actually need before touching the dispatcher. That table had gone stale
twice: it still claimed `POST /api/code` took bare source long after uploads
became LXP1 envelopes, and it never grew entries for `/api/sensors`,
`/api/mqtt`, `/api/sync`, `/api/clock`, `/api/apmode`, `/api/ota` or
`/api/assets`. A doc that drifts silently is worse than no doc; one file
now owns this.

The audit also found a broken harness: `tools/soak.mjs` still POSTs raw
pattern source to `/api/code`, so every upload is rejected by the envelope
decoder and the "soak" measures nothing. Filed as #218 (with two smaller
finds: `event-soak.mjs` pinned to firmware v0.1.39, and `POST /api/var` +
`GET /api/readouts` having no consumer in the repo at all).

## 2026-08-31 — hosted-ui on metal: the stale bundle stayed invisible

Gitea #198, the bring-up checklist that #204 could not run: `hosted-ui` had
been built and measured on every board and exercised through the native
mirror, but no hosted-ui image had ever *booted*. It has now, on the Athom
rig (`board-athom-music`, 60 px ws2812, brightness 4), with serial captured
end to end.

The deliberate part of the setup is what the previous image left behind. The
hosted-ui build went out over plain OTA onto a device that had been running a
normal v0.1.39 out of `ota_0`, so the **assets partition still physically
held that image's LUXA archive for the whole test** — nothing erases it, by
construction. That is the only interesting question the mode asks: with the
reader compiled out, is the bundle actually unreachable, or does some path
still find it? It is unreachable. `/` and `/min` served the 1204-byte
embedded page (uncompressed, compiled in) where the normal image had served a
368-byte gzip body out of flash; `/assets/index-<hash>.js` 404'd with a
complete 9-byte body; `POST /api/assets` answered
`{"ok":false,"error":"hosted-ui build: this image has no on-device web app"}`
and `tools/deploy.sh --assets-only` turned that into its explanatory message
and exit 1. Five more 200 KB pushes at the same endpoint changed neither
`heap_free` nor the `web` slot-stage array — refusing costs nothing and wedges
nothing.

The proof that the archive really was there the whole time came from the
recovery leg. A firmware-only OTA back to a normal image — no asset push
at all — booted straight to `assets: 8 files installed` and served the
playground again at the **same ETag** it had before the hosted-ui push.

At parity otherwise: `tools/hw-bench.mjs` ran the full 303-pattern gallery
with **303 clean, 0 errors**, and a pixel-count curve identical at all six
points to the normal build's (123/120/64/33/20/10 fps at 60/150/300/600/
1024/2048 px). Idle heap is **+15,988 B** on the hosted image (105,976 vs
89,988) — the DRAM the asset reader was holding, now measured rather than
modelled. Exactly one `rst:0x3` in the serial capture across ~2.5 hours (the
OTA's own reboot), no panic, no `preboot guard` line, `slot` never moved.

One leg stays open and it is not the firmware's. Driven from a plain-http
origin, the playground reads the device's config, edits its running pattern
and round-trips a brightness write over CORS with no same-origin app at all.
Driven from the real **https** Pages copy, every request dies on Chromium
150's Local Network Access permission — which headless denies outright, and
which neither a CDP `localNetworkAccess` grant nor an explicit
`targetAddressSpace` hint got past. That is Gitea #162, now confirmed against
hardware instead of predicted, and it needs a headful browser to close.

## 2026-08-30 — Pin injection: buttons you can press without a button

Gitea #177 items 2 and 3. Item 1 (PR #187) made `digitalRead` report a pin's
*idle* level, so an `INPUT_PULLUP` button-to-ground finally reads "not
pressed" instead of "held forever". That fixed the wrong answer but left no
way to give the right one: nothing outside a pattern could drive a pin, so
every button pattern in the corpus rendered exactly one state and its two
sides were incomparable by construction. Jeremy's review note on
`example-button-w-debounce` says it plainly — *"I don't know how to test this
one. I don't have a button to test with."*

**The ABI.** A driven pin is a **level, not an event**, so it is modelled on
`set_sensors` (latest-wins state) rather than `push_event` (a FIFO):

```rust
Vm::set_pin(pin: i32, level: Option<bool>) -> bool   // None = release to idle
Vm::pin_read(pin: i32) -> bool                       // what digitalRead reports
```

mirrored by `Engine::set_pin`/`pin_read` and exported as
`lx_set_pin(h, pin, level) -> i32` / `lx_pin_read(h, pin) -> i32`, where
`level` is `0` LOW, `>0` HIGH, `<0` release. Two more `u64` bitmasks beside
the existing `pin_pullup` — `pin_driven` and `pin_level` — so the whole
0..63 window costs 16 bytes and `digitalRead` stays a mask test. Three
deliberate choices, each locked by a test:

- an injected level **beats the pin's own bias**, because the host is
  standing in for a wire;
- a later `pinMode` does **not** knock the injection loose, for the same
  reason — a wire does not come off because the pattern reconfigured the pad;
- an out-of-window pin is **rejected** (returns false / 0) rather than
  silently aliasing pin 0, because a typo'd pin number and an input stuck at
  idle look identical in a render.

The level is held for the life of the engine — a pattern polling the pin
every frame sees a button held down, not a one-frame blip.

**Reach.** Bindings in `web/src/lib/luxel.ts` (`setPin`/`pinRead`),
`tools/verify/enginehost.mjs` and its browser twin
`tools/verify/review/engine.js`; the CLI mirror grows `POST /api/pins` (text
body, one `<pin> <0|1|x>` per line — a hand- and script-driven surface, not a
per-frame stream, so no binary frame) drained between frames exactly like
`/api/events`. `review.mjs`'s stale-wasm guard now also refuses a binary
without `lx_set_pin`, and `enginehost.mjs` says "rebuild the wasm" instead of
throwing a bare TypeError.

**Item 3: per-side fixups.** `tools/verify/fixups.json` grows `controls` and
`pins` blocks alongside `vars`, all three per side, because the interesting
pairs are exactly the ones whose two sides expose the *same* input through
*different* surfaces. `snap.mjs` gains `--pins-orig/--pins-port` and renames
`--no-vars` to `--undriven` (old spelling still works) now that it drops all
three kinds of pin. Everything threads through `report.mjs`, `review.mjs` and
the review UI, and lands in `meta.json` as `pinsApplied` +
`provenance.fixups`.

The payoff, on the pattern that prompted the issue:

```json
"example-button-w-debounce": {
  "pins":     { "orig": { "4": 0 } },
  "controls": { "port": { "toggleButton": 1 } }
}
```

The original polls a real switch on pin 4; the clean-room port offers a UI
toggle. Pressing both at t=0 makes the two sides **byte-identical GIFs** out
of `report.mjs` — both debounce one press and settle on the same solid mode-1
green — where undriven they sat at mode 0 forever and the debouncer, the
entire point of the pattern, was never exercised.

**Verified:** `cargo test --workspace` green; three new engine tests
(`set_pin_drives_digital_read`, `pin_injection_and_pin_mode_compose`,
`netin::pin_lines_parse`); `tools/wasm-smoke.mjs` drives the raw FFI through
idle → driven → held → released; `tools/serve-e2e.mjs` gains six checks
driving `/api/pins` into a live pattern's pixels; `snap.mjs` runs show
driven (motion 3, red→green) vs `--undriven` (motion 0) and CLI-beats-manifest
per pin; the review UI driven in real chromium shows both canvases at
`rgb(0,255,0)`.

Still open on #177: item 4, real firmware GPIO. Deferred and filed: a
playground UI surface for driving pins (#205) and injection for
`analogRead`/`touchRead` (#206).

## 2026-08-30 — `hosted-ui`: a build that ships no web app at all

Gitea #11 asked for devices with less space not to store the web UI. The
first slice (#164) made a device with an empty assets partition point at the
CI-published playground instead of dead-ending. This is the other half: a
build mode that never carries the UI in the first place.

`hosted-ui` is a cargo feature, combined with any board:

```sh
EXTRA_FEATURES=hosted-ui BOARD=board-c6-devkit firmware/build-esp32.sh
nix build .#luxel-fw-c6-devkit-hosted          # the one shipped variant
```

What leaves the image: the LUXA archive reader and its TOC, the streaming
`FlashAsset` body, the ETag/`If-None-Match` 304 path, `HVal::Owned`, the
`/assets/` cache policy and the `POST /api/assets` installer. What stays is
`assets::read_chunk` — that function is the tree's stack-safe flash reader
and ota.rs/takeover.rs depend on it; it was only ever *filed* under assets.
`/` then always serves the embedded fallback page, which links to
`https://googlebot42.github.io/luxel/?device=http://<this host>`.

**Measured, devshell A/B at one revision: −13,936 to −14,544 B of app image
on every board** — `board-c6-devkit` 981,696 → 967,728 B, slot margin 66,880
→ 80,848 B. Strikingly flat across chips (1.42–1.67 %), because what leaves
is plain logic with no chip-specific codegen. It also hands back DRAM:
`.stack` on `board-pixelblaze-v3` 27,828 → **32,524 B**, since the response
future the web-task arena is sized for loses its asset arm (largest frame
9,552 → 7,504 B). Whole-fleet table in docs/boards.md. Measured the way CI
measures — credless flake build of the shipped `luxel-fw-c6-devkit-hosted`
variant — it is 966,832 B, **81,744 B / 7.79 % of the slot free** against
980,784 B / 6.46 % for the normal C6 image: the mode is what takes the
fleet's tightest board clear of `image-check.sh`'s warn line.

The bigger number is the one that isn't in the image at all: the ~641 KB
bundle is never packed, never written, never shipped. `build-esp32.sh`
skips the pack-and-flash step by construction, its `image` command composes
a full-flash binary with the assets region left erased, and the release
workflow does the same for the new `luxel-c6-devkit-hosted` artifacts. The
`assets` partition's 983,040 B stays allocated-to-nothing — reclaiming it
needs a partition-table fork, which is Gitea #199 and deliberately not this.

**Two guards, because this is a *subtractive* mode and those fail silently.**
`tools/image-check.sh` grew an absent-marker half: with `hosted-ui` in
`EXPECT_FEATURES` it asserts the `assets: hosted-ui build` boot line is
present *and* that the LUXA reader's strings are gone, so an image that kept
the asset code — quietly giving back the entire saving — fails the build.
(Verified in both directions against real ELFs.) And `firmware/build.rs`
now resolves build-mode blocks in `src/index.html`, so the fallback page
cannot advertise `tools/deploy.sh --assets-only` on an image with no
`/api/assets` route; it asserts the markers still exist rather than silently
emitting the wrong page. The native mirror (`luxel-cli` serve.rs) resolves
the same blocks at startup, and `tools/serve-e2e.mjs` checks it.

Verified: all eight board combos build in both modes and pass image-check;
`nix build .#luxel-fw-c6-devkit-hosted`; `tools/stack-check.sh` clean on
pixelblaze-v3 in both modes; `cargo test --workspace`; `tools/serve-e2e.mjs`;
the hosted fallback page rendered in real chromium with the `?device=`
anchor resolving. **Not verified: any of it on hardware** — no hosted-ui
image has ever booted. The bring-up checklist (including the one assertion
that really needs metal: `/` must ignore a leftover LUXA archive that is
still physically in flash) is Gitea #198.

Drive-by: `tools/deploy.sh` passed `$BOARD` as build-esp32.sh's *command*
argument, which that script reads as `flash|image|log` — so `BOARD=…
tools/deploy.sh` had been silently building the script's default board.

## 2026-08-30 — Fidelity residue swept: six library patterns, five fixed, one measured-and-left

Gitea #181 parked six measured-but-unchanged fidelity findings from the PR #176
review pass. Worked through all six with `tools/verify/snap.mjs` (grid/strip
rigs, seed 1, pinned wall clock, `beat120` synthetic sensors) as the measuring
instrument; five turned into fixes and one is deliberately left, with numbers.

- **crossfading** — scene schedule was 6 s/scene (18 s cycle) against the
  original's 5 s/15 s, and the two inner scene clocks ran 3.93 s vs 3.0 s.
  Retimed. Autocorrelating the 60 s mean-brightness envelope: port scene cycle
  **17.95 s → 15.45 s** (original 15.00 s), and the KITT sweep's fine period is
  1.50 s on both sides. Scene A's breath is now a 0.51–1.0 arc at ~3.25 s
  instead of a shallow 0.6–1.0 at 5.9 s.
- **glittering-jewels** — the Sparkle dial measured **non-responsive**
  (`--probe-controls` maxDelta 0.49 / maxDeltaLit 0.88, threshold 1 / 4). Cause:
  the dial only re-phased the glint carousel, and the glint itself was a
  `sin^24` spike clipped to `core^3`, so every setting looked equally glittery.
  Now a fixed carousel with a wider spike that reaches past the core, and the
  glint *whitens* as well as brightens. **maxDelta 0.49 → 4.90, maxDeltaLit
  0.88 → 8.71** — the original measures 5.08 / 7.90.
- **blinky-eyes-2d** — iris hue 0.66 → **0.60** (the measured original), and the
  blink no longer passes through full blackout: a fully squashed ellipse is
  thinner than one pixel row, so its rim fell *between* rows. Added a soft
  eyelid bar in display units, outside the ellipse test. Port per-frame minimum
  mean brightness **0 → 4** (original 3); the filmstrip shows a held lit lid
  line through the shut phase.
- **line-dancer-2d** — the issue's premise was **wrong**, and measuring said so:
  reading the original's exported `speed` var over the dial gives exactly
  `1 + 9v`, identical to the port, and both scale motion smoothly off a nonzero
  baseline (orig 10.2 → 28.7, port 9.3 → 20.9 across the sweep). No authority
  difference. The real defects were the *defaults* and the Twist range: original
  Speed default 4.6 (port 3.25) and Twist `1.25 + 0.75v` default 1.75 (port
  `1.2 + 1.2v` default 1.62). Matched both. Port motion at defaults **12 → 16**
  against the original's 18.
- **bouncer3d** — `sliderSpeed` reshuffles every ball, so a host that applies
  control defaults at load (the playground) started on a different layout than
  one that doesn't (snap.mjs). The reshuffle is now skipped on the first call
  only. `BallSize` default 0.4 mapped to r2 = 0.086 against the top-level 0.08;
  gain retuned to `0.01 + 0.175v` so the default lands on 0.08 exactly. Measured
  untouched-vs-defaults-applied pixel divergence: **meanAbsDiff 8.89 / max 252
  → 0.00 / 0**.
- **sound-spectrokalidamandala** — the orig/port stats gap is **left**, but a
  real bug behind it is fixed. The port's auto-gain was a purely *additive*
  integrator, so from silence it crawled: sensitivity was still climbing
  linearly at 11 s and would have taken ~a minute to reach a watchable picture
  (`brightnessTrend` "rising" over the whole 8 s window, first frame mean 1).
  Made the correction *relative* — the gain steps by a fraction of itself —
  which settles in ~3 s and then holds (sensitivity 2.7–3.9 stable over 60 s).
  Port mean **35 → 57**, min **1 → 9**, trend rising → **steady**. The residual
  gap to the original's 183 is NOT a gain bug: the original *opens at 255* and
  decays (first 255, last 131, trend "decaying") — it starts saturated and its
  AGC pulls down. Sweeping the port's Target Fill to its slider maximum (90)
  only reaches ~120, so closing the gap would mean driving the panel into
  near-total whiteout, which contradicts the "perfect" visual verdict this
  pattern already earned. Left deliberately; filed as Gitea #202.

Verification: `luxel check` ok on all six, `cargo test --workspace` green,
`web/tools/e2e.mjs` all checks pass, and all six driven in real chromium in the
playground (tiles render, zero page errors).

## 2026-08-30 — Five patterns' `//# default=` now reproduces the shipped constant

Fallout from #179: attaching the previously-inert `//#` directives made
`default=` the *effective* default (the playground seeds every control with it
on load), and in five patterns the declared default had never had to agree with
the top-level initializer. It didn't — worst case `b-lightning-flashes`, whose
`sliderLightningLength default=0.3` produced a 3.7 px half-width against a
shipped `var halfWidth = 8`, halving mean brightness (Gitea #188).

Fixed by making the handler produce the shipped constant *at* the declared
default, never by moving the constant. Two shapes, chosen per control:

- Where the constant is a **real quantity**, the dial now carries that unit and
  `default=` is literally the constant — agreement is structural, not arithmetic:
  `sliderLightningLength` px (`default=8`), `sliderOffDurationRandomness` seconds
  (`default=1` → the shipped 1000 ms), `bouncy-boxes` `sliderTearRate` Hz
  (`default=3`), `twinkling-…` `sliderCycleTime` seconds (`default=15`),
  `rock-sparks` `sliderDrive` (overdrive ceiling, `default=4`) and `sliderSpeed`
  (rate multiplier, `default=1`). Each handler guards with `max()` so a stock
  Pixel Blaze, which always sends 0..1, still degrades to the dial's floor.
- Where it's an opaque coefficient, the 0..1 dial stays but the mapping is
  **centered on the constant**: `rainbow-comet` `headInterval = 0.09 - (v-0.5)*0.12`
  and `decay = 0.9 - (v-0.5)*0.14`, `rock-sparks` `beamFocus = 0.03 + (v-0.25)*0.1`
  (its `default=` moved 0.3 → 0.25). Ranges are preserved; the constant now
  appears as a literal in the handler.

That centering is not cosmetic. In 16.16 fixed point `0.97 - 0.5*0.14` is one
LSB short of the literal `0.9`, which was enough to shift pixels: driving
`rainbow-comet`/`rock-sparks` at their defaults still differed from undriven
under the first (algebraically correct) rewrite. Writing the constant as the
literal and zeroing the offset term at the default makes it bit-exact.

`b-lightning-flashes` was the one design call. Its initializer is absolute
pixels while the handler was fraction-of-strip, so no `default=` reproduces it
on every rig — one model had to give, and absolute won: the pattern's own code
is absolute throughout (`abs(index - center)`, `min(halfWidth, pixelCount/2-1)`),
so a px dial keeps the approved look identical on *any* strip length instead of
only on the 60 px reference, `newStrike()`'s existing cap already handles the
short-strip case the fraction model was reaching for, and the spec describes the
control as "a single pixel up to a few dozen pixels".

Verification: for all five, `snap.mjs` `port.png` md5 is unchanged before vs
after, **and** a run driven at every declared default now matches that same md5
— the check the issue actually asks for. Dial ends probed with directed
`--controls-port` runs (all responsive, no warnings/compile/runtime errors);
`tools/check-library.sh` clean 303/303 on both grids; all five opened in real
chromium at 60 fps with their controls seeded at the new bounds.

## 2026-08-30 — The library gate never ran a mapless strip (Gitea #193)

The #167 soak turned up one error in 303 patterns: `sound - spectromatrix
render2D` → `array index out of bounds`. The issue guessed a sensor-less
device hands the pattern an empty `frequencyData`; that was wrong on both
counts. `Engine::from_program_budgeted_at` declares `("frequencyData", 32)`
unconditionally, so a sensor-less engine gets 32 zeroed bins like any other,
and the array that actually overflowed was the pattern's own 256-slot
`persist[]` buffer.

The pattern's **1D fallback** was the bug:

```js
var fbWidth = max(1, floor(sqrt(pixelCount)))
render2D(index, col / fbWidth, row / fbWidth)   // row count is NOT fbWidth
```

`floor(sqrt(n))` gives the width; the row count is `ceil(n / fbWidth)`, which
is larger on every count that isn't a perfect square. At 60 px the matrix is
7 wide and **9** rows, so the last rows normalize to `y = 8/7 = 1.14`, and
`floor(y * 15.99) * 16` walks `persist[]` past 255. Fixed by deriving the
height separately (`fbHeight = max(1, ceil(pixelCount / fbWidth))`), which
also keeps `y < 1` strictly. Swept clean now on every count 1..400 plus 512 /
600 / 1000 / 1024 / 1500 / 2048.

The interesting part is why 322/322 sweeps kept missing it. `luxel check`
**always installed a 2D grid map**, and with a map the engine picks
`render2D` — so a 2D pattern's own `render` fallback is code the acceptance
gate never executed, on any grid. Only a real mapless strip runs it. So
`luxel check` gained `--strip N` (N pixels, no map) and
`tools/check-library.sh` now sweeps five rigs: grids `default`/`16x16` plus
**mapless strips at 60 / 300 / 512 px**. Baseline **303/303 on all five**.
The strip counts stay under the PB-compat 10,240-element array budget, which
a couple of patterns legitimately exceed at 2048 px; `STRIPS=` drops the
strip half.

Two secondary notes from the sweep. The soak ran at **60 px, not 300** — the
issue's "at 300 px" came from the report's hardcoded *"fps at 300 px"* line,
and 300 px is one of the counts where this bug happens to be benign
(`y = 17/17 = 1.0` exactly, which still floors to slot 255). And `oasis`
goes out of bounds above ~1450 px from 16.16 overflow in
`(index + off) * w`, filed separately.

Verified on the Athom rig at its real 60 px: the pre-fix source pushed via
`/api/code` reports `vmerr: "array index out of bounds"`, the fixed source
reports `vmerr: null` at the same fps and heap.

`tools/hw-bench.mjs` got three fixes for the misdirection it caused. Its
summary line hardcoded *"fps at 300 px"* and its header hardcoded *"SK9822"*,
so every report described a rig the sweep hadn't run on — both now come from
`/api/status` and `/api/config`. And the restore hardcoded 300 px instead of
the count it found, quietly reconfiguring the rig on every run (documented as
a gotcha to work around; now just fixed).

## 2026-08-30 — One `Reply` type for the whole device API: −24 KB of firmware

`firmware/src/server.rs` returned **thirteen different response shapes** —
`(CORS, JSON, String)` for the ~40 JSON routes, `(gzip, etag, cc, FlashAsset)`
for a compressed asset, `(StatusCode, [(&str,&str); 4], &str)` for the CORS
preflight, and so on. picoserve monomorphizes `IntoResponse::write_to` once
per tuple *shape*, `ForEachHeader::call` once per header-*value* type, and
`Response`/`HeadersChain`/`ContentBody` once per combination, so those
thirteen shapes cost 22 `write_to` instantiations (20,478 B), 37
header-machinery symbols (8,492 B), and a `core::fmt` `Display` shim per value
type. Gitea #167 collapses all of it into one concrete type:

```rust
struct Reply { status: StatusCode, headers: heapless::Vec<(&'static str, HVal), 4>, body: ApiBody }
enum HVal  { Static(&'static str), Owned(String) }          // the ONE header value type
enum ApiBody { Json(String), Text{…}, Bytes(Vec<u8>), Asset(FlashAsset),
               Source(CurrentSource), Envelope(CurrentEnvelope), Empty{…} }  // the ONE body type
```

Exactly one `Response<HeadersChain<ContentHeaders, &[(&str, HVal)]>,
ContentBody<ApiBody>>` now exists in the image, and therefore exactly one
`write_to`.

**Measured, `board-c6-devkit` credless flake build: 1,005,168 → 980,784 B
(−24,384), slot margin 43,408 B / 4.14 % → 67,792 B / 6.46 %.** Every other
board came in between −23.0 and −24.4 KB (whole-fleet table in
docs/boards.md; both columns are an A/B with only `server.rs` differing —
which turned out to matter, because a side-finding of this work is that
editing *only markdown* moves the C6 image ~600 B via `.Lanon` symbol
renaming. That ~±0.7 KB noise floor is now written down in
docs/size-report.md). This is the largest single reduction the firmware has
taken
since the opt-level "s" switch, and — unusually — it *beat* its ≥10 KB
estimate rather than missing it. The C6 is out of CI's 6 % warn band for the
first time since the board was added.

Symbol-level: `write_to` 22 syms/20,478 B → 3 syms/6,882 B (one real closure
at 6,556 B plus drop glue); header machinery 37 syms/8,492 B → 4 syms/966 B;
the `picoserve` bucket 32,948 → 7,278 B; `rust core` 58,815 → 50,843 B (the
per-value-type formatting shims genuinely became dead — unlike #168, where
`core::fmt` could never leave). Our own `luxel-fw` bucket *grew* 10 KB and the
flat dispatcher's closure went 22,050 → 26,204 B: with one response type the
LTO boundary moves and the write path inlines into the dispatcher instead of
standing alone 22 times. Bucket deltas lie; the image total is the number.

**What had to be preserved, and was.** The pad-don't-truncate discipline in
`stream_flash_readback`/`stream_store_readback` (a short body wedges a pool
slot for 45 s), the exact-from-snapshot `content_length()` of
`FlashAsset`/`CurrentSource`/`CurrentEnvelope`, the 1 ms `Timer` between 4 KiB
flash reads, the flat-match dispatcher with a single `write_to` exit per arm,
the reply-then-reboot ordering on `POST /api/wifi` and `/api/apmode`, and all
four headers on the OPTIONS preflight (`MAX_HEADERS` is sized for exactly that
arm, with a compile-time assert, because a silently dropped CORS header breaks
cross-origin DELETE with no trace). The routing shape is now load-bearing for
size too: picoserve's `MethodRouter` would wrap the writer in a private
`IgnoreBody<W>` for HEAD — a second writer type that duplicates every GET
instantiation — so the hand-written `PathRouterService` stays, and says so in
a comment next to the existing stack warning.

**Two wire changes, both fixes.** JSON/octet-stream/HTML responses used to go
out with **two** `Content-Type` headers (picoserve's `Content` impl for
`String`/`&str` emits `text/plain; charset=utf-8`, then our explicit header
was appended); now there is exactly one, correct, and matching the native
mirror in `crates/luxel-cli/src/serve.rs`. And a 304 asset revalidation now
carries the `Content-Type`/`Content-Length` its 200 would have carried (RFC
9110 §15.4.5) where before it carried neither — `Response::new` always emits
both and cannot suppress them, and RFC 9112 §6.3 keeps a 304 body-less
regardless of what Content-Length says.

**RAM cost.** The single `write_to` future is the union of every body type, so
the per-slot static task arena grew: `tools/stack-check.sh` on
`board-pixelblaze-v3` reads `.stack` 29,124 → 27,828 B (3,828 B above the
24 KB floor) and the dispatcher's poll frame 2,128 → 4,928 B against a
12,288 B budget. Both within limits; the poll frame is now the one to watch
when adding routes.

**Gates.** All 8 board images build and pass `tools/image-check.sh` (markers
+ margin); `tools/stack-check.sh` clean; `cargo clippy` on the default board
shows no new lints; `cargo test --workspace` 223/223; the full QEMU harness
(`tools/qemu/run-all.py` — takeover app1/app0/fault, heap-regions
selfheal/rollback) passes on the athom image built from this change;
`tools/serve-e2e.mjs` green. The firmware's HTTP surface has no hardware-free
e2e, so the wire contract was verified on the Athom rig after an OTA of this
build (slot ota_1 → ota_0, same 0.1.39 version string — slot is the
discriminator): every response family checked with raw curl — single
`Content-Type` on JSON/octet/HTML, `Content-Length` == body bytes everywhere
including the flash-streamed `/api/pattern`/`/api/pattern.lxp`, asset 200 with
ETag+Cache-Control+gzip, body-less 304 revalidation carrying the would-be-200
`Content-Type`/`Content-Length` and the same ETag, all four OPTIONS preflight
headers, text/plain 404, captive-portal shape untouched. Then a full
`tools/hw-bench.mjs` soak: 302/303 patterns clean through the new reply path
(the one error, an `array index out of bounds` in sound-spectromatrix-render2d
at 300 px, does not reproduce natively under snap.mjs and is a VM/sensor-env
issue unrelated to this change — filed as #193), pixel-count curve unchanged,
device healthy after (vmerr null, heap ~86 KB free at 60 px).

## 2026-08-30 — Lint detached `//#` directives; sweep 13 library patterns

A `//#` control directive parses in exactly two places — trailing on the
`export function` line, or on the line directly above it (web/src/lib/
hints.ts). Written on the first line *inside* the handler body, which reads
perfectly naturally, it is silently ignored: the control degrades to an
unbounded 0..1 slider with no default. That was the whole ryb-colors defect
in the review pass, and Gitea #179 found the same inert placement across the
library.

**The lint (the durable half).** `tools/check-library.sh` now runs a directive
lint before it compiles anything: any line carrying a `//#` with a
`min`/`max`/`step`/`default` key that is not in one of the two parsed
positions fails the sweep, naming file:line. Prose that merely mentions `//#`
is not flagged. Implemented as inline awk mirroring the two hints.ts regexes,
so the gate has no new dependency.

**The sweep.** 13 files carried 37 detached directives: b-lightning-flashes,
bouncy-boxes, dire-spider-2d, m5stack-hex-panels, multisegment-demo,
music-sequencer-for-v3-only, rainbow-comet, rock-sparks,
sound-music-spectrum-visualizer, sunrise-2d, thunderstorm,
twinkling-classic-xmas-strands, voronoi-2d. All 13 untouched renders stayed
byte-identical (snap.mjs port.png md5, before vs after) — moving a comment is
inert to the engine. The four "partial misses" the issue also listed
(perlin-simplex-noise-1d, rgbclock-2d, rocket-by-tony-hampton,
spinning-plasma-2d) turned out to be clean; the issue's scan had counted prose
mentions of `//#`.

**Two real-unit handler bugs, exposed by attaching.** A directive that
declares non-0..1 bounds makes the UI send real units, so a handler that
rescales as if it got 0..1 breaks. dire-spider-2d's `sliderLegWidth`
(min=0.1 max=0.2) would have collapsed to a 0.11..0.12 dead slider; both its
handlers now clamp instead of rescale. voronoi-2d had the same bug in three
*already-attached* directives, i.e. live on master: `sliderPoints`
(min=1 max=8) ran `1 + floor(v * 7)`, so the UI's own declared default of 5
set `count = 36` against an 8-element array — the pattern went black with
"array index out of bounds" the moment the playground seeded its defaults.
Both fixed and verified in real chromium.

**The playground seeds `default=` on load** (App.svelte), so attaching a
directive makes it the effective default. Seven patterns' UI-default render
now differs from their top-level initializer state, because the declared
defaults were written while the directives were inert and nobody could see
them: b-lightning-flashes (halfWidth 8 px → 3.7 on a 60 px strip) and
rainbow-comet (headInterval 0.09 → 0.075) are the visible ones; bouncy-boxes,
rock-sparks and twinkling-classic-xmas-strands drift by a few percent;
dire-spider-2d differs by one LSB of 16.16 control quantization; voronoi-2d
re-rolls its seed positions. Retuning the control math so each `default=`
reproduces its shipped constant is Gitea #188, deliberately kept out of a
placement fix.

## 2026-08-30 — snap.mjs probes dials across their real range, not 0/0.5/1

`--probe-controls` swept every control at raw 0, 0.5 and 1 and ignored the
`//#` directive entirely. That was fine when ports were 0..1 sliders; with
the library now largely on real-unit bounds (the controls pass below), it
under-drove nearly everything — a `min=1 max=60` dial was poked across 1% of
its range, and `clamp(floor(v), 1, 12)` collapsed all three probe points onto
the same value — so dials reported borderline or inert while working fine.
Five separate fix-pass agents hit this independently and fell back to hand-run
`--controls-port` sweeps (Gitea #180).

The probe now sweeps each control at the **min, midpoint and max of its own
`//#` range**. A one-sided directive takes the raw default for the other end;
the midpoint snaps to `step` when that lands strictly inside the range, so an
integer dial is probed at an integer (`min=1 max=8 step=1` → 1/5/8, not
1/4.5/8). Raw 0/0.5/1 remains the fallback for a control with no usable
bounds — which is every corpus ORIGINAL, since Pixel Blaze has no directives —
and for pickers, whose three components are colour axes rather than a range.
`probe.json` gained `bounds`, `probedAt` and `boundsProbed` per control, and
the stdout table a `probedAt` column that stars the bounds-driven sweeps.
Measured on real ports: chill-confetti's HueJitter went from a whole-rig delta
of 0.76 (under the bar, rescued only by the lit threshold) to 8.55;
2d-wandering-fireball's BallSizePercent from 9.87 to 31.82.

`meta.json`'s per-side `controls` entries now carry the parsed `bounds`
alongside `{name, kind, label}`, so a snap alone confirms a directive was
seen — and a port control with no `bounds` key has one that did NOT bind
(detached by a blank line, misspelled, on the wrong export), which used to be
invisible without re-reading the source.

Plumbing: the `//#` parser was duplicated in `web/src/lib/hints.ts` and
`tools/verify/review/engine.js`; rather than adding a third copy, the harness
copy moved to `tools/verify/hints.mjs` (served to the review UI as
`/hints.mjs`, the same way `sensormodel.mjs` already is) and now also owns
`directiveRange()`, the min/mid/max derivation. `web/tests/hints.test.mjs`
runs every parser case against BOTH implementations so the twins can't drift,
and covers `directiveRange` edge cases (one-sided, step coarser than the
range, inverted/degenerate bounds). `hints.mjs` folds into `harnessSha256`,
so cached runs from before this invalidate correctly. Also fixed the two
header-doc drifts #180 named: per-side blocks live under a top-level `sides`
key, and `--controls-orig/-port` match a control by export name **or** display
label.

## 2026-08-30 — digitalRead honours pinMode: a pulled-up pin idles HIGH

`digitalRead()` returned 0 unconditionally, which for the standard
button-to-ground wiring (`pinMode(pin, INPUT_PULLUP)`) reads as "button
held forever" — the exact opposite of idle, and the reason every button
corpus pattern renders its pressed state on both sides of a review pair.
The VM now remembers the pull-up bit of the last `pinMode` per pin (a
64-bit mask, pins 0..63; ESP32 tops out at 39) and `digitalRead` reports
the resulting idle level: 1 under `INPUT_PULLUP`, 0 for plain `INPUT`,
`INPUT_PULLDOWN`, outputs, and unconfigured pins — so the default stays
what it was. Masking on the pull-up bit (0x04) rather than comparing to
`INPUT_PULLUP` (5) also covers a hand-built `INPUT | 4`.

No shipped pattern changes behavior: sunrise uses plain `INPUT`,
lightbulb-crank uses `INPUT_PULLDOWN`, and example-button-w-debounce
drives its debouncer from a UI control (its header comment about the
stub is updated). This is Gitea #177 item 1 only — there is still no way
to *drive* a pin from outside the pattern, so the pin-injection ABI
(item 2), fixups control pins (3), and real firmware GPIO (4) stay open.

## 2026-08-30 — Fix pages/release CI: crates.io API 403 on esp-hub75 fetch

GitHub pages (and release) builds were dying in `nix develop`: the
esp-hub75 source fetch hit `crates.io/api/v1/.../download`, which now
returns 403 to non-browser clients. Swapped the fetchurl to the
`static.crates.io` CDN URL (same bytes, hash unchanged — verified by
sha256 before the swap). Also added `flake.nix`/`flake.lock` to
pages.yml's push path filter, since devshell changes can break the site
build.

## 2026-08-31 — The review verdict lands: 110 pattern decisions acted on

Jeremy sat down with the review UI and decided all ~139 judged pairs
(tools/verify/decisions.json): 29 good, 25 delete, 82 needs-work, 3 fork.
This pass implements every one of the 110 actionable decisions, fanned out
across ~20 subagents in one worktree.

**Deleted (25)** — ports plus specs, verdicts, and their fixups entries
(automap's vars pin, skypirate's grid rig); pairs.json regenerated (268 →
267 pairs after the flag redesign below).

**Controls pass (53 patterns)** — the big "perfect, but give me controls
the original doesn't have" bucket. House style established: real-unit
`//#` bounds (seconds, degrees, pixels, percent, integer counts with
step=1) on the line above the export, handler math shaped so the declared
default reproduces the shipped constant, untouched renders byte-identical
(verified port.png-vs-port.png per slug), never a pattern-level brightness
control. Two look changes were explicitly requested and made:
color-twinkle-bounce (full-value coherent crests, four palettes) and
rainbow-fonts (the "muted" was a 0.35 value cap).

**Defect fixes (17)** — highlights: crossfading measured everything as
the wrong canvas fraction (5 bars vs 8, braid pinned at pixel 0);
glittering-jewels indexed 20-element arrays at 381 (real-units slider,
0..1 handler); kitt-w-color-picker's darkness was decay-before-draw plus
a fade shorter than one frame; line-dancer-2d's melt phase and
millipede's backwards-walking gait were both derivative bugs (a `+1`
floor that never collapsed; a triangle wobble whose derivative is a
square wave); blinky-eyes-2d mixed folded and display units (aspect 0.71
vs the original's 1.28); animated-asterisks' AnimateWidth was a ~59 s
cycle that silently overrode the width slider (removed); ryb-colors had
all its `//#` directives inside function bodies where the parser
silently ignores them.

**Rewrites and forks (10 new/replaced patterns)** — us-flag + us-flag-2d
(cloth-wave redesigns replacing 1-usflag-blink, 13 crisp stripes at every
size); fireworks-finale (full shell lifecycle, forked from 4th);
lightning-strike (near-white restrike lightning, forked from
lightning-zap); matrix-green-waterfall-1d; bouncing-balls-rgb-2d (true
ballistic analog — the existing 2d one is Lissajous drift); Grinch's
Heist (bulb-stealing story pattern) and Infinite Snake (self-playing
snake whose Smartness dial measures 413 deaths → 9 across its range)
replacing the-grinch and snake-2d; continuous-cellular-automata reworked
into a self-evolving ring-topology waterfall with a convergence watchdog.

**Engine: 21 easing builtins appended** (batch 7 — the full Sine/Quart/
Quint/Expo/Circ trios plus the missing Back/Elastic/Bounce variants),
tested against f64 references; easing-library-v1-0 now showcases the
builtins as first-class values instead of 30 hand-rolled lambdas.
`board-c6-devkit` re-measured post-rebase: 1,005,488 B (+4,016 B over
the #168 fmt-diet baseline), margin 43,088 B / 4.11 % —
above the 3 % floor, but the next VM growth should measure C6 first.

**Harness** — review UI no longer fabricates untouched slider positions
(the holiday-diagonal-stripes "sync bug": both panes drew 0.5 while the
engines held 0.4 vs 0.5; unhinted untouched controls now show a `?`
placeholder), and it grew an "addressed" filter chip: the fix pass stamps
`addressedAt` on every decision entry it acts on, re-deciding clears the
stamp. example-button-w-debounce is drivable from the UI (Button hold /
too-short Tap / DebounceMs / Modes) pending real GPIO.

Deferred items are on Gitea (GPIO input stubs, the playground's copy of
the fabricated-slider bug, a `//#`-placement lint, probe-controls
ignoring bounds, minor fidelity residue).

## 2026-08-30 — size-report reads real ELF sizes; the ~1 GB RISC-V row is gone (#174)

`tools/size-report.py` bucketed `nm -C --size-sort`, which *estimates* a
size for symbols that carry none by subtracting from the next symbol's
address. On `board-c6-devkit` that turned linker-script NOTYPE symbols
(`_rwtext_len`, whose value *is* a length), `$d` mapping symbols and `.L*`
locals into a 1,082,830,941-byte "other C/asm" row. Now it buckets
`nm -C -S --defined-only` — real `st_size`, size-less symbols dropped and
counted — so the C6 row reads **133,402 B** and the accounted total
915,322 B against a 1,005,024-byte OTA image.

Xtensa never showed the blow-up only because its `_rwtext_len` estimate
landed above the old `>= 0x80000000` drop guard — but its numbers were
guesses too. On a credless `board-athom-music` build the fix moves "other
C/asm" 99,097 → 74,949 B and the blob bucket 215,207 → 212,003 B: 87
size-less symbols, all hand-written asm or ESP32 ROM stubs
(`_WindowOverflow*`, `save_context`, `mktime`, `atoi`, `idle_hook_fn`,
`g_wifi_osi_funcs`) plus pure markers like `_rwtext_end`, which the old
script credited with 12,028 B of bytes it does not own. **Every
Rust-crate bucket is byte-identical before and after on both
architectures** — the deltas are exclusively size-less symbols. Those asm
bytes are real but unattributable, so the accounted total is now an
honest lower bound and the script says how many symbols it skipped.
docs/size-report.md's "ignore the other C/asm row" caveat is retired.

## 2026-08-30 — The playground stops fabricating untouched control values (#178)

The review UI's fabricated-slider fix, ported to `web/src/components/Controls.svelte`.
An untouched control is running whatever the pattern's own top-level code
put in the variable, and the engine cannot hand that back — `lx_set_control`
with no args *invokes* the handler. Only a `//# default=` declares it. The
playground was nonetheless drawing `?? 0.5` (or `?? 0` / `?? 1` per kind)
and presenting it as the running value, which is exactly what hid the
orig-0.4 / port-0.5 Slope gap in holiday-diagonal-stripes on the review side.

Untouched controls with no `//#` default now render as placeholders: the
slider/number widgets dim to 0.4, an amber `?` badge carries a tooltip
explaining the position is a guess, and toggles go **indeterminate** (the
native tri-state says "unknown" better than any badge). Triggers, gauges
and showNumbers are exempt — nothing to guess. First user interaction
writes `values[name]` and the row settles to a normal control. Controls
*with* a `//#` default are seeded into `controlValues` at compile, so they
were never guesses and are untouched by this change.

Verified in real chromium (puppeteer-core): Blink Fade's undeclared
`sliderSpeed` renders dimmed + badged, a real mouse drag settles it to
0.831 at full opacity; Audio Volume Meter's 7 `//#`-defaulted sliders and
its gauge render exactly as before while both undeclared toggles show
indeterminate, and clicking one settles only that one. `svelte-check` 0
errors, 11/11 `npm test`, full `web/tools/e2e.mjs` suite green (69 checks,
including the pre-existing `//# default applied` and slider/number
round-trip assertions).

## 2026-08-30 — fmt diet: JSON builders off core::fmt; C6 margin 4.24 → 4.49 % (#168, #169)

Converted every `format!`-built JSON/response body — `server.rs` (41
sites), `playlist.rs`, `patterns.rs`, `resume.rs`, `devicemap.rs`,
`mqtt.rs`, `main.rs`'s vmerr strings, and luxel-core's shared
`jsonview.rs` — to `push_str`-style building with new non-fmt printers:
`Fx::dec_str` (the exact 16.16 decimal printer, now shared with `Display`
so they can't diverge) and `jsonview::{push_u32, push_i32, push_u64,
push_i64, push_hex}`, all pinned against `format!` output by unit tests.

**The headline is the lesson, not the number.** `core::fmt` never leaves
the image — `println!` and `Debug` keep it linked, so the measured
23.5 KB fmt bucket was never reclaimable and only dropped ~0.6 KB
(23,550 → 22,922 B by #160's measure). Worse, the naive conversion GREW the C6 image by
8,592 B: `String::push_str` inlines a reserve-and-copy at every call
site, and the builders have hundreds. The fix that turned it around is
`#[inline(never)] jsonview::push_piece` — one shared append function
every literal goes through. Net: `board-c6-devkit` 1,004,112 →
**1,001,472 B** (−2,640 B, margin 44,464 → 47,104 B / 4.49 %),
`board-athom-music` 955,376 → 954,016 B (−1,360 B). #168's 5–10 KB
estimate was wrong for the structural reason above; the remaining diet
lever is the picoserve monomorphization collapse (#167).

Verified: byte-identical bodies (mechanical literal-sequence comparison
over all 236 append sites, plus a live before/after capture of 18 GET
endpoints on the Athom — only volatile fields differ), 219 luxel-core
tests green, both toolchains build clean, all 5 QEMU harness tests pass,
stack-check clean, OTA'd to the Athom (ota_1) and 300-poll status soak
with stable heap. Also refreshed `docs/size-report.md` per #169 (the
f64-parsing bucket it listed as deliberate is no longer linked at all)
and recorded the new margins in docs/boards.md.

## 2026-08-30 — WLED takeover validated on metal; installer sheds its beta banner (#53)

Second full bench conversion on the Athom, this time with the
preboot_guard build (the fix that was QEMU-only since 2026-08-16): stock
WLED 0.13.2 restored over serial (button-held download mode — the one
Jeremy step), provisioned onto the LAN via Improv-serial, then a real
takeover from a **credless master image** (the faithful user path —
release images never carry creds) uploaded to WLED's `/update`. Result:
foreign table detected, WiFi **and settings** inherited (30 px, ws2812,
brightness, power cap, gamma — all carried over from WLED), 936 KiB
self-copy verified clean on the FIRST attempt (the 08-16 verify flake
did not recur), table rewritten, boot from ota_0, LAN rejoin on
inherited creds at the same address, and `boot guard: healthy` after a
deliberate cold power cycle. No heap-regions panic; the armed
preboot_guard never had to fire (its 3-panic rollback path stays
QEMU-verified — it guards a flake we can't summon). One transient
RTC-WDT reset before the first takeover boot self-recovered instantly.

Shipped on the back of it: the installer's beta banner is gone
(`Flash.svelte`), docs/wled-migration.md's "why beta" section is now a
history section, and the Improv packet-builder the 08-16 session left
in a scratchpad is a real tool — `tools/improv-provision.mjs`
(docs/tools.md) — with the athom-rig skill updated to point at it.

## 2026-08-30 — CI now fails on a thin OTA slot, not just a full one (#160)

The release workflow only ever asked "does the app image fit the 1 MiB OTA
slot?", which is the wrong question one release too late. `board-c6-devkit`
lost ~7 KB of headroom in two days (map-aware blur/glow, then the device
output palette) and sits at 43,872 B / 4.18 % free — and it is the one
board nobody here can serial-recover (#56), so the failure mode is a device
that `/api/ota` refuses to update.

`tools/image-check.sh` grew a second gate beside the //SIZETEST marker
check: for app images it computes the slot margin, **fails below 3 %**
(31,458 B) and **warns below 6 %** (62,915 B). All eight board variants go
through it in `.github/workflows/release.yml`, which drops its own
duplicate ceiling check — one place owns the size rule now. The math is
integer-only shell (no bc/python on a minimal runner) and the thresholds
are env-overridable (`MIN_MARGIN_PCT` / `WARN_MARGIN_PCT` / `OTA_MAX`).
ELF inputs skip the size half, since build-esp32.sh's local call hands it
an ELF and an ELF is not the artifact that has to fit.

Why 3 %: it is ~12 KB under today's tightest board, so master stays green
today, but roughly two more medium features trip it — which is the point at
which the diet in docs/size-report.md stops being optional. Verified
against stubbed images at all eight recorded board sizes plus the
threshold edges (31,458 B passes with a warning, 31,457 B fails, the exact
ceiling fails, one byte over fails with the "EXCEEDS" message), and against
the real credless C6 flake build: 1,003,824 B → warns at 4.26 %, and
`MIN_MARGIN_PCT=5` on the same image exits 1.

**Where the C6's bytes actually go.** Profiling both RISC-V boards with
`tools/size-report.py` killed the standing guess. The 91,616-byte gap
between the C3 (912,208 B) and the C6 (1,003,824 B) is almost entirely
Espressif's radio blob — ~51 KB of blob symbols plus ~11 KB of
`.rodata.wifi`, with `.rwtext.wifi` going 33,768 → 55,060 B. Our Rust is
chip-independent to within a rounding error: `luxel-core` is byte-identical
at 76,628 B on both chips and `picoserve` at 32,948 B. So there is no
C6-specific diet to write; every win is fleet-wide, and the only C6-only
lever is dropping a feature from that board's profile. The candidate list
(picoserve's 22 `IntoResponse::write_to` monomorphizations at 20,478 B,
the 24,434 B routing table, core::fmt at 23,550 B, MQTT at 29,383 B) is on
Gitea #160; nothing was optimized in this pass on purpose.

## 2026-08-30 — v0.1.29 protocol-re-init checklist closed out on the Athom (#155)

The two never-verified items of the v0.1.29 hardware checklist
(docs/webui.md) ran on the Athom rig (v0.1.39, ota_0, serial captured
throughout — zero panic/reset lines):

- **Item 2, encode-buffer realloc at 2048 px under heap pressure**: with
  the heaviest allowed pattern loaded (~24 KB of arrays, heap_free
  ~34 KB), 3× sk9822↔ws2812 round-trips — every switch clean, the ~10 KB
  encode-buffer delta visible in heap_free each way, no reboot, no slot
  rollback. An over-budget 6-array variant (vmerr "array memory budget
  exceeded") switched just as cleanly. Notably the "output paused"
  fallback path is unreachable via patterns alone — the VM array budget
  keeps enough headroom that the realloc always succeeds — so that branch
  stays QEMU/review-verified only.
- **Item 4, switch under live traffic**: 6 protocol switches under a
  ~60 fps DDP stream (`live:"ddp"` held throughout) and 6 more
  mid-crossfade (4 s blend, playlist auto-advancing) — all clean, no
  vmerr, no reboot. Visual no-tearing confirmation remains the one
  eyes-on residue (tracked in #155).

Items 1/3/5 turned out to have been hardware-verified back on 2026-07-19
(v0.1.30 session) with the checklist never updated — docs/webui.md now
records per-item status. Device restored to as-found (60 px ws2812,
brightness 4). Also fixed two stale README claims: "195-pattern gallery"
(now 322) and "ESP32/ESP32-C3" (release images now cover ESP32, C3, C6,
S3, and the HUB75-panel boards).

## 2026-08-30 — A device with no web UI now links to one (#11, first slice)

A device whose assets partition is empty served a dead end: a dark page
saying "the web UI isn't installed" and a `tools/deploy.sh` line, which
assumes you have the repo, a checkout, and `nix develop`. Everything
needed to do better was already in place and simply never wired up — the
playground has honoured `?device=<base>` since device mode existed, the
firmware answers every route with `Access-Control-Allow-Origin: *`
precisely so a foreign origin can drive it, and CI has published the web
dist to `https://googlebot42.github.io/luxel/` since 2026-08-15. The
embedded page now closes the loop: one anchor, its `?device=` filled in
client-side from `location.host`, so the device tells the hosted UI its
own address. `tools/deploy.sh` stays as the second paragraph.

**Cost, because this lives in the app image.** `firmware/src/index.html`
is `include_str!`'d into the binary (`server.rs`, and the native mirror
in `luxel-cli/src/serve.rs` embeds the same file), so every byte is
1 MiB-OTA-slot budget — see #160. Source +275 B; measured `.text` on
riscv32imc +272 B, `.data`/`.bss` unchanged. The whole feature is one
`<a>` and a 44-character `<script>`; no framework, no fetch, nothing to
go stale.

**The part that isn't done, and why it's a separate ticket (#162).** The
Pages copy is https and devices are http, so the playground's calls to
the device are mixed-content / Local-Network-Access requests.
`web/src/flash/lib/device.ts` already handles exactly this for the WLED
installer — it passes `targetAddressSpace: "local"` when the page is
https and the target is genuinely local-space — but the playground's
`gatedFetch` has no equivalent, so it rides on Chromium's auto-detection
alone and has no browser-blocked message when that fails. Filed as #162
with the measured constraints from docs/wled-migration.md; the link is
unambiguously right on a plain-http host today, and #11 stays open for
the device-side build mode that omits the assets entirely.

Verified: `cargo build --release` (riscv32imc) before/after for the size
delta, `tools/serve-e2e.mjs` green (`/` fallback + `/min` routing), and
the page rendered in real chromium against a stand-in host — the anchor
resolves to `https://googlebot42.github.io/luxel/?device=http://<host>`.

## 2026-08-30 — The installer stops claiming only two chips are built (#57)

The installer page told anyone with an unsupported chip that "only classic
ESP32 and ESP32-C3 are built today". That stopped being true on 2026-08-22:
the release workflow builds eight board variants across ESP32, C3, S3 and
C6. The images exist; what does not exist is any hardware to test the new
ones on (#56), so they ship as artifact downloads and the takeover flow
deliberately doesn't offer them. The copy now says that instead — the
takeover covers the chips it covers, other ESP32 chips have release images
that are untested on real hardware and flashed by hand. Same for the
footer, which made the same "works on ESP32 and ESP32-C3" claim about
Luxel as a whole rather than about the takeover.

The durable part is where the chip list lives. It was written out three
times — the `archUnsupported` predicate, the error text, the footer — so
adding a chip meant finding all three. `releases.ts` now owns one
`FLASHABLE_CHIPS` record; `isFlashableChip()` and the user-facing
`FLASHABLE_CHIP_NAMES` string both come from it, and the `BOARDS` comment
says out loud that it is a *subset* of what the pipeline builds rather than
a mirror of it. Neither the error text nor the footer enumerates the
release variants, so the release workflow can grow boards without dating
this page again.

`flash-e2e.mjs` grew scenario 3b for the case the old copy got wrong: a
WLED device reporting `esp32-s3` hits the stop with the release-download
wording and gets no flash button. The pre-existing esp8266 stop (a
genuinely never-supported chip, not a not-yet one) is untouched. Verified
in real chromium: 15/15 flash-e2e checks green, `svelte-check` clean.

No decision was made about whether S3/C6 belong in the board list — that is
still #57, still gated on #56.

## 2026-08-30 — The installation's palette is a device setting now (#139)

`setOutputPalette(pal, amount)` shipped with the global post-process chain,
but only as a *pattern-side* builtin: the device-settings half landed for
brightness curve, blur and glow and stopped there. The reason was storage,
not plumbing — those three are one byte each in the fixed-size `LXDV`
record, and a palette is a variable-length stop list. It now has its own
storage, its own routes, and an editor in the Output card.

**Where a variable-length setting actually goes.** The obvious home was a
fifth nvs record, and that is what the first cut did — until the Athom rig
answered a power cycle with an empty palette. The nvs partition is four
4 KiB sectors and all four are taken: WiFi, device settings, MQTT, and —
this was the collision — `ota::GUARD_OFFSET` at 0xC000, the boot-loop
guard, which rewrites (and therefore erases) its sector on *every boot*.
A record parked there is gone before anything can read it. The fix is the
mechanism the device map, playlist and resume record already use: a
reserved-key blob in the pattern store (`patterns::store_blob`,
`PALETTE_KEY`), which is variable-length by construction and CRC-checked by
sequential-storage. `config.rs` now carries a header comment saying the nvs
partition is full and where the next setting goes, so nobody rediscovers
0xC000 the same way.

The blob is `u8 version=1  u8 amount_pct  u8 count` then `count` stops of
`(pos, r, g, b)`. `outpal::deserialize` validates the count against the
cap before reserving — the rule a torn pattern-store TOC taught this
codebase the hard way — and rejects an unsorted stop list as corrupt rather
than sampling nonsense from it. Clearing writes a zero-count record, the
same way the device map clears itself.

Adding a fourth persisted setting was still the moment to stop
copy-pasting the nvs writer: `config::write_record(offset, rec)` now does
the pad-to-word, word-aligned staging, erase and write for all three nvs
records. That deleted three duplicates of the `unsafe` staging block and
paid back about 600 B of image on the tightest board.

**Composition, not override.** A device palette does not replace a
pattern's. `apply_outpipe` runs the device stage on the frame the engine
already finished — which may already have been recolored by the pattern's
own `setOutputPalette` — exactly the way device blur stacks on top of
pattern blur. The device setting is the installation's look; the pattern
keeps its own voice. Stage order inside `apply_outpipe` mirrors the
engine's chain: recolor, then spread light, then the output transfer.

The 256-entry lookup is cooked off the hot path. `shared::set_post_palette`
bumps an epoch inside the same critical section that swaps the stop list,
and the render task caches `(cooked-for epoch, Box<[[u8;3];256]>)` beside
its gamma LUT — an unchanged palette costs one atomic compare per frame,
and the cache updates its epoch even when the result is "no palette" so an
empty list can't re-cook forever. (The epoch bump is a load/store under the
lock, not `fetch_add`: the C3's riscv32imc has no atomic read-modify-write,
which the build found before hardware did.)

`outpipe::fill_palette_lut` and `outpipe::parse_palette_stops` are the
shared halves — the engine's `ensure_remap_lut` and the firmware's cache
cook through the same function, and the firmware, the native mirror and the
tests all parse the wire form through the same parser, with
`MAX_OUTPUT_PALETTE_STOPS` defined once.

API: `POST /api/output/palette` takes `"<amount_pct> <pos> <r> <g> <b> …"`,
all 0..=255, positions ascending, at most 32 stops; `DELETE` clears it;
`GET /api/output` echoes `palette` (the flat array) and `paletteAmount`
alongside the existing fields. Boot reads the blob before WiFi — a few
hundred bytes, unlike the multi-KB pattern/playlist resume that has to wait
for `wait_config_up()`.

Verified on the Athom rig (60 px WS2812): set a three-stop palette, power
cycle, palette still there and applied at boot; `DELETE`, power cycle,
still gone. That the stage really reaches the wire — `/api/pixels` shows
the pre-outpipe frame, so it can't answer this — was measured at the wall
plug at brightness 31: 11.4 W with no palette, **4.5 W** with an all-black
palette, 11.4 W with an all-white one, 11.4 W again after `DELETE`.

UI: the Output card grows a Palette editor — a gradient preview that models
the engine's asymmetric ends (below the first stop clamps, past the last is
black), one row per stop with a color swatch and a 0–255 position, and
add/remove/clear plus the blend amount. Covered end-to-end in
`device-e2e.mjs` against the native mirror.

Size: +5,120 B on `board-c6-devkit` (1,004,704 B, 43,872 B of slot left)
and +4,256 B on `board-pixelblaze-v3`; `.stack` 29,228 → 29,196 B.

## 2026-08-30 — The post-process blur follows the panel, not the wire (#140)

`setBlur`/`setGlow` — and the `/api/output` blur/glow device settings —
worked along the pixel index. On a strip that is physical order and the
result is right; on a matrix it followed the wiring, so a serpentine panel
got a horizontal smear that folded back at every row end and never spread
vertically at all. The chain now recognizes a grid and sweeps it in two
dimensions.

**The grid is six bytes, not a neighbour table.** `outpipe::detect_grid`
runs once per `Engine::set_map` and answers one question: is this map a
regular matrix walked row by row? It reads the coordinates as contiguous
runs — equal length, same fast-axis values forwards or backwards, both axes
strictly monotonic — and returns `GridMap { w, h, serpentine }`. Cell →
pixel index is then arithmetic (`row * w + col`, mirrored on odd rows when
serpentine), so the per-frame cost of map awareness is *zero* allocation,
zero cached indices, and one branch per lookup. That mattered more than
generality: a 4096-pixel neighbour table would have been 8–16 KB of ESP32
heap held forever for a stage that is off by default.

Two wirings cover the space because mirroring is free: the kernels are
symmetric and clamp at the edges, so a panel wired entirely backwards, or
one whose rows run right-to-left first, describes the same neighbourhoods
as its mirror. Column-wired panels fall out as the transpose. Anything
else — a ring, a scatter, a ragged last row, a 3D map — returns `None` and
keeps the exact index-space behaviour it had, which the tests pin.

`blur_frame_grid`/`glow_frame_grid` are separable: one sweep along the
rows, one down the columns, sharing a single inner loop between the two
axes (an `along_rows` flag rather than a closure per axis — worth ~370 B of
image on the tightest board). One `passes` is one row sweep plus one column
sweep, so the 2D kernel is the 1D one squared, and glow's corner cells pick
up `g²/256` — a naturally round falloff.

Firmware: `apply_outpipe` takes the engine's `GridMap` (`Engine::grid()`,
`Copy`, read before the frame borrow) and uses the grid kernels when it
matches the frame length. The live DDP/E1.31 path gets it too whenever a
pattern engine is loaded.

Measured, not estimated: app image +2,272 B on `board-c6-devkit` (997,328 →
999,600 B, the fleet's tightest margin, still 48,976 B free) and +2,208 B on
`board-pixelblaze-v3`; `.stack` 29,244 → 29,228 B, no function frame near
the 12 KB budget. 206 core tests green including eight new ones — detection
across four wirings and seven rejections, no row-end seam, symmetric spread
on serpentine, and the index-space fallback byte-for-byte unchanged.

Docs: docs/lang.md's "Index space, not map space" note is now "map space
when the map is a grid", with the fallbacks spelled out; ideas.md, webui.md
and the editor's builtin help follow. **The visual check needs the 64×64
HUB75 panel, which hasn't arrived** — tracked on #75 and docs/UNTESTED.md.

## 2026-08-30 — Language gap-fill: `switch`, `**=`, and the ternary /
## compound-member audit (docs/ideas.md "Language" batch)

Three queued language items closed in one pass. Two of them turned out to
be documentation-and-tests work, not fixes.

**Ternary chains — already correct, now pinned and documented.** `?:`
parses right-associatively (`a ? 1 : b ? 2 : c ? 3 : 4` is a run of
else-ifs), a ternary in the *then* slot is closed by its own `:`, each
branch is a full assignment expression, and only the taken branch is
evaluated. All of that already worked; nothing was broken. The gap was
that docs/lang.md mentioned `?:` in a single operator-list clause. It now
has a **The conditional operator** section covering associativity, the
then-slot nesting rule, the parentheses requirement in the condition
slot, and single-branch evaluation — with tests for each claim
(`ternary_chains_are_right_associative` in tests/semantics.rs, plus an
AST-shape test in parse.rs).

**Compound member ops — one real gap, `**=`.** Audited the whole family
against array elements: `+= -= *= /= %= <<= >>= &= |= ^=` and
prefix/postfix `++`/`--` all worked, with correct JS result values
(`a[i]++` yields the old value), correct copy-on-write on const-pooled
literal arrays, and — the thing worth checking — the array and index
sub-expressions evaluated exactly **once** (`a[idx()] += 3` calls `idx`
once; the codegen already used `Dup2`/`LoadIdx`). The one missing member
of the family was `**=`, which the `**` extension never got: it lexed as
`**` followed by a stray `=` and died with "expected an expression". Now
a lexer token + one match arm, working on variables and elements alike.

**`switch` — implemented, no new opcodes.** JS semantics: the
discriminant is evaluated once, labels are compared with the language's
`==` in source order (only up to the match), bodies fall through until a
`break`, and `default` may sit anywhere — including in the middle, where
it is still the no-match target *and* still falls through into the arm
below it. `break` binds to the innermost switch-or-loop; `continue`
skips past a switch to the enclosing loop. It lowers onto instructions
that already existed:

```
<disc>
Dup; <label_i>; Ne; JmpIfFalse T_i     one per `case`, in order
Pop; Jmp default|end                   nothing matched
T_i: Pop; Jmp body_i                   trampoline drops the discriminant
body_0 … body_n                        source order ⇒ fall-through is free
```

Every path pops the discriminant *before* entering a body, so arms run on
an ordinary statement-context stack and `break`/`return` out of them need
no unwinding. `BUILTINS` untouched, LXBC format version untouched;
docs/spec/vm.md gains a note on the lowering. Compiler-side, the old
`LoopFrame` became `BreakFrame` with an `is_loop` flag — that flag is
what routes `continue` past a switch to the loop that owns it.

`switch`/`case`/`default` become reserved words. Nothing in `library/`
used them as identifiers, and they are reserved on PB too (its compiler
is JS-based), so PB-source compatibility is unaffected.

**PB compatibility, probed compile-only** (`tools/oracle/compiler.mjs`
runs PB's own compiler locally — no websocket, so no oracle-wedge risk):
PB **rejects** `switch` ("Unsupported type SwitchStatement") and `**`, so
both are documented as Luxel extensions. PB's ternary and `arr[i] +=`
accept the same shapes we do.

**Verification.** 7 new test functions and ~90 new assertions across `parse.rs`,
`lex.rs` and `tests/semantics.rs` (fall-through, default-in-the-middle,
nested switch, switch-in-a-loop with `break` vs `continue`, `return` out
of an arm, `var` hoisting out of an arm, per-pixel dispatch in `render`,
single evaluation of the discriminant and of the labels past the match).
`cargo test --workspace` green; `tools/check-library.sh` 323/323 on both
grids; `tools/wasm-smoke.mjs` native↔wasm identical; firmware builds
(no_std clean); `web/tools/e2e.mjs` green; and a real-chromium check that
a `switch` pattern compiles and renders its 1-bright/2-dim dispatch in the
playground, with `switch` in the editor's autocomplete.
## 2026-08-30 — `orig-unrenderable` reaches zero: the last one was never
## broken, it was non-visual (#123), plus a tracker sweep

**`performance-test-framework` is excluded, not scored (Gitea #123).** The
original is a CPU benchmark whose render body is the comment
`//sorry, no blinkenlights!` — it reports through PB's Vars Watch, and
all-black is its correct output on real hardware too. The 2026-08 sweep
filed it `orig-unrenderable` after ten runs (every rig, fps, seed, wall
clock and skip) proving the original is black: right observation, wrong
bucket. The original does not *fail* on our engine; it succeeds at drawing
nothing.

The mechanism is a fourth fixup kind, `nonVisual`, a one-string reason in
`tools/verify/fixups.json`:

- `fixups.mjs` gains `nonVisualReason(slug)` — a malformed marker throws
  rather than silently putting the pair back in the scored population.
- `snap.mjs` warns on **every** run of such a slug and records the reason in
  `meta.json`'s `provenance.fixups`, so the next judge stops instead of
  spending a batch re-proving the original is black.
- `report.mjs` and the review UI file it under its own `non-visual`
  heading/badge with **no score** — "0/10" on an excluded pair reads as a
  failing port.
- `JUDGE.md` documents `non-visual` as a manifest decision, not a judgement
  a judge makes on its own, and distinguishes it from the
  degenerate-constant-output subtype ("we cannot get output out of it" vs
  "it correctly has no output, on real PB too").

The annotation lives in `fixups.json` and **not** `pairs.json` because
`gen-pairs.mjs` regenerates `pairs.json` from `library/` + `corpus/` — it
would silently drop either an annotation or a deletion. The port's
phase/progress readout stays: the playground has no Vars Watch and a
permanently black gallery tile reads as broken, so it is now a documented
deliberate deviation rather than an unscored defect.

**Sweep headline numbers refreshed** in `tools/verify/FINDINGS.md`, which
still carried the original 2026-08 table long after the re-judges moved
things. Recounted from `results/*.json` (293 files): match 24, close
**122** (was 119), divergent **128** (was 123), broken **18** (was 17),
orig-unrenderable **0** (was 10), non-visual 1; mean 5.38/10 over the 292
scored pairs. `orig-unrenderable` is empty for the first time — the sweep's
ten were all diagnosed and re-judged (#99 sentinel-strip, #106 wrap freeze,
#108 silent-nulls, #109 array budget, #122 var-driven `automap`) and this
was the last. The table now carries the one-liner that recounts it, so the
next reader doesn't have to trust a stale number.

Verified in real chromium against the live review UI: the `non-visual`
filter chip appears, the pair badges as `non-visual` with no score, its
card shows the exclusion summary, original renders black and port renders
its bar. `snap.mjs` re-run confirms the warning and the provenance stamp.

**Tracker hygiene** — five stale issues closed against evidence, two new
ones filed for the pieces that were actually still open:

- **#4 "Improve UI"** — the docs/webui.md redesign backlog is done; the
  live backlog moved to numbered issues long ago. The one genuinely open
  thing in that document was its "v0.1.29 hardware-verification checklist",
  untracked anywhere → filed as **#155**.
- **#3 "Board presets"** — shipped: seven board features, the
  "Adding a board (a five-minute diff)" recipe, the installer board picker,
  and WLED cfg.json import on takeover (PR #76). The remaining piece, a
  runtime LED data-pin picker (pins are esp-hal *types*, so today the pin
  is fixed by the board build and takeover only *logs* WLED's), → **#154**.
  Mic enablement was already #119, the untested S3/C6 boards #56/#57.
- **#5 "less space"** — docs/boards.md already records the decision: A/B
  OTA alone is 2 MB and `storage` became load-bearing in v0.1.34, so 2 MB
  variants are out. Dropping the webui wouldn't help anyway: assets live in
  their own partition, not in the app image.
- **#143 "16 MB partitions"** — none of its own three revisit triggers has
  fired (shipped asset bundle ~615-641 KB of 983,040 B; `storage` manages
  512 KiB of its 1 MB and caps at 24 patterns; no bulk-storage consumer
  exists). Closed won't-do, as its text says is legitimate.
- **#147 "6-arg arrayReplace hangs the oracle"** — oracle-only research
  with no Luxel correctness gap, and probing further costs a hang.
  Closed answered/wontfix; `tools/oracle/oob-probes.mjs`'s Q8 preamble
  refreshed, since it still described the pre-#107 engine behaviour that
  783978f settled the other way.

## 2026-08-30 — `//#` control hints now bind from the line above the export
## too (#146), which is how 112 of 130 library patterns write them

`parseControlHints` (`web/src/lib/hints.ts`) only ever matched a **trailing**
`//#` directive on the export line — its `[^\n]*?` can't cross a newline. Most
of `library/` puts the directive on its own line above the export, so those
controls silently fell back to the default 0..1 / step 0.001 / value 0.5
slider instead of the bounds they declare. Nothing errored; the patterns just
came up wrong.

**Decision: widen the parser, don't normalize the library.** The own-line form
is the one pattern authors actually reach for (it reads better above a long
function body), `docs/lang.md` already told them to use it, and the review UI's
own copy of the parser (`tools/verify/review/engine.js`) had accepted both
placements since it shipped. So the fix is two regexes — trailing, plus
`^[ \t]*//#…\n[ \t]*export function …` — merged per control name, own-line
winning on shared keys. No `library/*.js` was touched.

- **Docs**: `docs/lang.md` gains a *Control bounds* section under Luxel
  extensions spelling out both placements, the four keys, that a blank line
  between directive and export breaks the association, and that `default` is a
  UI starting position, not a variable initializer. The frame-model bullet and
  the PB-divergence bullet now link to it instead of describing one placement.
- **Tests**: `web/` had no test runner; it does now — `npm test` runs node's
  built-in runner with type stripping (`--experimental-strip-types`), so
  `web/tests/hints.test.mjs` imports `hints.ts` directly with zero new
  dependencies. 11 cases: both placements, indentation, negatives/fractions,
  multi-line bodies not stealing the next control's directive, merge
  precedence, unknown keys, blank-line separation, non-export functions.
- **Verified in real chromium** (not just `svelte-check`): `library/eye-of-sauron.js`,
  `1d-aurora-borealis.js`, and `2d-fireworks-fade.js` opened in the playground.
  Before, all twelve sliders read min 0 / max 1 / step 0.001 / value 0.5; after,
  each shows its declared bounds (e.g. Eye of Sauron's AngularDensity 2..18
  step 1 at 8, Dilation 0.15..0.6 step 0.01 at 0.35). `tools/e2e.mjs` all-green.

The three consumers (`App.svelte` ×3, `PlaylistRow.svelte`) call the same
function and needed no change.

## 2026-08-30 — automap port fix: exported var renamed to `pixel`, invented idle scan removed (#136)

Two non-visual defects from the #122 re-judge of `library/automap.js`
(clean-room port of the community mapping helper), both fixed with minimal
edits to our own file:

1. **The port renamed the interface.** It exported `pixelIndex` where the
   original exports `pixel`. For a pattern whose entire purpose is to be
   driven over the vars API by a mapper/companion client, the NAME *is* the
   interface — a client that writes `pixel` drove the original and silently
   failed to drive the port, leaving it in its scan state. The export (and
   its only reference, in `render`) is now `pixel`.
2. **The port invented an idle self-scan.** At a negative index it swept one
   red pixel along the strip every ~3.3 s; the original is completely inert
   when undriven (measured with `--no-vars`: 1200/1200 zero-motion frames,
   still black at t=320 s). The sweep and its `target` indirection are gone —
   `render` is now the single line `hsv(0, 1, index == pixel)`, so an
   unset/negative/out-of-range index renders black, as the original does.

Acceptance checks from the issue, run in-worktree with `tools/verify/snap.mjs`:

| check | result |
|---|---|
| (a) `sides.*.varsExported` reads the same name on both sides | both `["pixel"]`; `varsApplied` `{pixel: 30}` on both, no warnings |
| (b) `--no-vars` posts meanBrightness 0 and zero lit pixels on BOTH sides | 60 s @ 5 fps: mean/R/G/B/motion all max 0, `zeroMotionFrames` 300/300 both sides; `--dump` at t = 0.6/20/40/59 s → 0 lit of 60 px on both |

No visual cost: a driven `--dump` (pin `pixel=30`) is still byte-identical
between the sides at t = 0.5/2/3.9 s — one `[255,0,0]` pixel at index 30,
everything else black.

Also updated: `tools/verify/fixups.json` pins the port side to `pixel` (the
per-side form stays, since pairs may still name a var differently), and the
matching sentence in `docs/tools.md`. The judge verdict in
`tools/verify/results/automap.json` is left as the historical record.

## 2026-08-29 — Out-of-range writes: the oracle says PB does NOT tolerate
## them either (#107 closed), and the splat builtins were the real gap

The verify sweep's engine gap 3 held that "real Pixel Blaze tolerates
out-of-range array/pixel writes where Luxel hard-errors", blocking a
re-judge of five pairs. **The premise is false.** `tools/oracle/oob-probes.mjs`
grew a second battery (Q1–Q8, fw 3.67) that asks the question shape by
shape instead of inferring it from patterns that "visibly work":

| probe | source (`a = array(3)`) | PB |
|---|---|---|
| Q1 / Q2 | `a[5] = 1` / `a[-1] = 1` | **aborts** |
| Q3a / Q3b | `v = a[5]` / `v = a[-1]` | **aborts** |
| Q4a | `v = a[1.5]` | tolerated, truncates |
| Q4b | `v = a[3.5]` | **aborts** — truncate first, then bounds-check |
| Q4c/d/e/f | fractional write: variable index, literal index, array literal, init scope | tolerated, truncates — **all four** |
| Q5 | `array(4)`, `a[6] = 1`, read every slot back | untouched: no clamp, no wrap, no partial write |
| Q6 | out-of-range write every 3rd invocation | frames 21 → 138 in 1.5 s — the pattern survives |
| Q7 | `t = array(4); t[0](1, 2)` | **aborts** — calling an unassigned slot is not a no-op |

So PB errors on exactly what Luxel errors on. What made `nano-orbital` and
friends look tolerant on a device is the error's narrow blast radius — one
handler invocation, not the pattern — which #84/PR #88 already matched.
`rainbow-comet`'s one-shot frame-982 error and `tixy` walking off its
formula table after ~46 modes happen on real hardware too; both are
original-side faults, which is how the judges had already scored them, and
the three `fixups.json` rig pins are correct rig data, not workarounds.
**No fixup removed, no verdict changed** (rainbow-comet 4, tixy 4,
nano-orbital 2, orv-christmas-tree 5, rainbow-smiley 6 — all re-rendered,
stats unchanged).

Two things did come out of it:

- **A real divergence in the sibling splat path.**
  `arrayReplace`/`arrayReplaceAt` were silently dropping every element that
  fell outside the array and clamping a negative offset to slot 0 — the
  opposite of what `a[i] = v` does. The oracle validates the span up front:
  `offset + count > length` is a runtime error leaving the array
  **completely** untouched (not even the in-bounds prefix lands, Q8f),
  `offset + count == length` is the accepted boundary, and a negative
  offset shifts rather than clamps. `vm.rs` now does exactly that, pinned
  by `array_replace_span_is_bounds_checked` and
  `rejected_replace_span_leaves_the_array_untouched`. No library or corpus
  pattern uses either builtin, so nothing rendered changed.
- **A VM panic found on the way in.** `arrayReplaceAt(b)` — the offset form
  with nothing to splat — indexed `args[2..1]`, an inverted slice range, so
  ordinary pattern source could panic the VM (on device: a reboot). It is
  now the PB-shaped no-op, with the three too-few-args spellings pinned.
- **A stale "deliberate divergence" retired.** A 2026-07 note claimed PB
  aborts on a literal-index fractional write (`a[1.5] = 9`) and that
  Luxel's uniform truncation diverged on purpose. Q4d/Q4e/Q4f say
  otherwise in every form — it is an exact match. `vm.rs`, `docs/spec/vm.md`
  and `docs/research/04-oracle-findings.md` corrected.

Also filed: **#147**, two 6-argument `arrayReplace*` shapes that
reproducibly HANG the oracle (websocket stops acking, device off WiFi for
~a minute, recovers on its own — and explains two mystery dropouts during
this session). Four-arg versions of both error cleanly, so the bounds rule
was settled without them; `oob-probes.mjs`'s header names both and says not
to add them back.

Gates: `cargo test --workspace` green, `tools/check-library.sh` 322/322 on
both grids, `tools/wasm-smoke.mjs` native ↔ wasm bit-identical.

## 2026-08-29 — The global post-process chain is finished: `setOutputPalette`
## → `setBlur` → `setGlow` → `setGamma`, plus a device brightness curve

`setGamma` shipped as a lone stage months ago and the docs/ideas.md entry
has read "STARTED" ever since. The rest of it is in, as an actual *chain*:
whole-frame stages the engine runs **once per frame** after the last
`render()`, in one fixed order, rather than a bag of per-pixel tweaks.

Order is the point. Recolor first (the remap works on the pattern's own
luma), then spread light spatially, then apply the output transfer curve
last — the order a display pipeline uses. That meant moving `setGamma`
out of the per-pixel quantize step, where it was, and onto the end of the
chain; otherwise a remapped pixel would have skipped gamma entirely and a
blur would have averaged already-curved values.

- **`setBlur(amount, passes = 1)`** — 3-tap blur along the pixel index.
  `amount` is each neighbour's share (0.5 = the classic 1-2-1 kernel, 1 =
  a pure neighbour average), `passes` 1–8 widens the radius. Ends clamp,
  so light that reaches the last pixel stays on the strip. Allocation-free
  — an in-place pass only has to remember one pixel (the previous
  pre-blur value), which is what keeps it honest on an ESP32.
- **`setGlow(amount)`** — light-bleed bloom: each pixel takes the brighter
  of itself and `amount` of its brightest neighbour. Unlike blur the
  source keeps its full value, so highlights spread without the frame
  going dim. Also allocation-free, one pass.
- **`setOutputPalette(pal, amount = 1)`** — recolor the finished frame by
  luma through a `setPalette`-format stop list: the pattern's structure
  survives, its hues are replaced. This is the "put the installation's
  palette over any pattern" knob. The 256-entry table is cooked on
  install (tracked by a VM epoch counter) so the per-pixel cost is a
  3-multiply luma plus a lookup, and the stops are a snapshot — unlike
  `setPalette`, which PB keeps live against the source array.
- Every stage is **off by default and costs one comparison per frame when
  unset**. An untouched pattern renders byte-identically to before.

Settings-page half (the ideas entry asked for one), device config record
**v6 → v7**, three new bytes in the reserved pad area of the `LXDV`
record, all on `/api/output` and in Settings → Output:

- **brightness curve** (`brightCurve`, gamma×10) — deliberately *not* the
  same knob as gamma. Gamma shapes every pixel's channels (content); the
  brightness curve shapes only how the master dimmer responds (control),
  which is the actual fix for "everything above 20% looks the same". A
  non-zero brightness never curves to 0 — a lit strip stays lit.
- **blur %** and **glow %** — the same two spatial stages as an
  installation setting, applied by the firmware after the engine's chain
  so you can dial a soft look in without editing patterns.
- The POST body's three new tokens are **optional** — an older client
  still sending `<order> <gamma> <cap>` keeps the stored values, so no
  flag day. v6 records still read (the version table gained an explicit
  `ver == 6` arm; `ver == DEV_VER` alone would silently have stopped
  accepting them).

Index space, not map space: the spatial stages follow the pixel index, so
on a serpentine matrix they follow the wiring path. That's documented,
and the two follow-ups are ticketed rather than left as prose — palette
remap as a *device* setting needs real palette storage in flash (Gitea
#139) and a map-aware 2D pass needs a neighbour index or a grid fast path
(Gitea #140).

`library/post-process-chain.js` is a demo whose pattern deliberately draws
nothing but hard single-pixel sparks on black — everything soft in it is
the chain, and each slider switches one stage off so you can see what it
was doing. Driving it in real chromium turned up a separate bug worth
knowing about: `//#` control hints only bind as a **trailing** comment on
the export line, so the several library patterns that put them on the
line above have been silently running with default 0..1 sliders all along
(Gitea #146).

Sizes were re-measured on all eight board combos (docs/boards.md): the
chain costs an even **+2.8–3.6 KB everywhere**, and the C6 still owns the
tightest margin at 51,232 B (4.9 %). Worth noting how that number was
nearly wrong — measuring against the table as it stood *before* the same
day's HUB75 board work made the chain look like it cost 10 KB on RISC-V
and 5 KB on Xtensa. It doesn't; the baseline had simply moved under us.
Re-measure after the rebase, not before. `tools/stack-check.sh`
is unmoved (no frame over budget, `.stack` 29,244 B), and
`web/tools/device-e2e.mjs` grew eleven output-pipeline checks covering the
six-key GET, the optional-token back-compat, out-of-range rejection, and
the three new Settings fields.

## 2026-08-29 — HUB75 panel boards: `board-seengreat-hub75` + per-board
## MAX_PIXELS (whole 64x64 panel finally addressable) (#73, #74)

Code-side prep so the Seengreat RGB Matrix HUB75 S3 and its 64x64 panel
can be brought up the day they arrive (#75). **No hardware was touched —
every number below is a build/link measurement, not a panel.**

- **`board-seengreat-hub75`** joins the board features. Real pin map,
  transcribed by signal name from the [vendor wiki](https://seengreat.com/wiki/214/):
  R1 IO5 · G1 IO4 · B1 IO6 · R2 IO15 · G2 IO7 · B2 IO17 · A IO8 · B IO18 ·
  C IO10 · D IO9 · **E IO16** (64 rows need it) · CLK IO12 · LAT IO11 ·
  OE IO13. Both panel outputs (ribbon connector and plug-in header) share
  those pins. The vendor's table is *not* in R1,G1,B1,R2,G2,B2 pin order —
  transcribe by name or you get a colour-swapped panel.
- **The HUB75 pin map moved out of main.rs into `board::hub75_pins!`**
  (#73's scope note). Pins are esp-hal types, so it's a macro rather than
  a const table, but it lives beside the `def` blocks: a second panel
  board is now a def-block + macro-arm diff, and main.rs keeps exactly one
  wiring line for all panel boards. Being a panel board, the feature
  enables `hub75` itself (`["esp32s3", "hub75"]`) instead of needing it
  passed at build time.
- **`MAX_PIXELS` is per board** (#74), moved from `shared` to `board.rs`
  and re-exported: **4096** with `hub75`, **2048** everywhere else. Before
  this, a 64x64 panel was clamped to half its area and the bottom 32 rows
  rendered black. It is deliberately not a global raise: at 4096 px the
  classic ESP32's WS2812 encode buffer alone would be ~36 KB against an
  80 KB heap. The panel path never builds that buffer at all — the driver
  owns two ~28 KB bitplane framebuffers allocated once at boot — so the
  extra pixels only cost the ~12 KB-each per-frame buffers. A `const`
  assertion now fails the build if a panel's area ever exceeds its board's
  cap, so the half-dark panel cannot come back silently.
- **The cap reaches the editor.** `/api/status` (firmware *and* the native
  mirror) now carries `max_pixels`, and the playground takes its pixel
  clamp from there on every poll, falling back to `/api/config`'s `max`
  for older firmware. Status wins over config when both answer — the
  precedence matters, and getting it backwards is exactly what the first
  cut did (caught in chromium, not review). New regression check:
  `web/tools/maxpixels-e2e.mjs`, which drives the 4096 path by
  intercepting status to impersonate a panel board — the only way to
  exercise it without the panel.
- **Partition decision (#73): the standard 4 MB table stays**, even though
  the module is 16 MB. OTA slots are 1 MiB-capped either way, storage is
  nowhere near full, and the assets partition holds a 641 KB bundle in
  983,040 B. A second table would have to be threaded through
  build-esp32.sh, flake.nix, the release workflow, build.rs and
  `takeover.rs` — real complexity for space nothing needs yet. PSRAM stays
  uninitialised for the same reason (DMA buffers must be internal SRAM
  regardless); the array-arena idea is unchanged and still future work.
- **Verification (all eight board/feature combos):** every one builds,
  links its load-bearing features (`tools/image-check.sh`) and fits the
  1 MiB OTA slot — tightest is still the C6 at 994,352 B / 54,224 B
  margin; the new board is 882,976 B / 165,600 B. Existing boards moved
  by 80–550 B, all of it the new status field and codegen noise.
  `tools/stack-check.sh` on all eight: no frame over the 12 KB budget,
  `.stack` from 29,324 B (classic ESP32) to 141,256 B (C6), the new board
  at 50,500 B — identical to `board-s3-devkit` + `hub75`, as expected.
  `cargo test --workspace` green; device-mode e2e green. Sizes and stack
  figures refreshed in docs/boards.md.

Still open, and deliberately so: **FPS at 4096 px, real `heap_free` under
load, and whether the panel's driver IC is even a shift-register type**
all need the hardware and ride on #75.

## 2026-08-29 — Verify harness learns to DRIVE a pattern: `--vars-*` pins
## exported vars, and automap goes orig-unrenderable 0 → close 7 (#122)

The last silent-null holdout from #108 was never an engine gap. `automap`
is a mapping HELPER: an external client writes a pixel index into an
exported var and the pattern lights exactly that pixel. At its default
index nothing lit is the CORRECT render — the harness simply had no way
to write to a pattern, so the judge saw black and scored the pair
`orig-unrenderable` 0.

- **`enginehost.mjs` gained `setVar()`** — wrapping the `lx_set_var` ABI
  entry that already existed (values scale into raw 16.16; only EXPORTED
  globals are settable, exactly as on hardware, so the wrapper returns
  whether the name existed instead of no-oping silently).
- **`snap.mjs` gained `--vars-orig` / `--vars-port`** (per side, because
  the two sides of a pair may NAME the same variable differently — which
  is exactly what automap does) and `--no-vars` (render a pinned pair
  UNDRIVEN; an empty `--vars-*` cannot express "no value"). Values land
  once, after init and after any `--controls-*`, before the first frame —
  the same single write a companion app makes. meta.json now carries
  `varsExported` + `varsApplied` per side, so a judge can see the var
  interface and what actually landed.
- **`fixups.json` gained a per-side `vars` key**, and automap pins both
  sides to mid-strip index 30 declaratively. All three consumers apply it
  (snap.mjs, report.mjs, and the live review UI — verified in real
  chromium: both canvases light pixel 30 red).
- **Re-judged: `orig-unrenderable` 0 → `close` 7** (high confidence, ~50
  render experiments). Driven, the two sides are byte-identical at every
  index tried, on strip / 32×8 grid / 5×5×5 cloud, 12–300 px, across
  seeds, clocks and a 60 s window. Two real port defects fell out that no
  visual diff could ever have shown: the port renames the exported var
  (`pixelIndex` vs `pixel`), so a mapper client written for the original
  silently fails to drive the port, and at a negative index the port runs
  a self-scanning demo the original does not have (original: black).
- JUDGE.md now teaches the surface (a black side that exports vars may be
  DRIVEN, not unrenderable), plus a trap the re-judge hit: whole-rig
  `meanBrightness` rounds to 0 on single-pixel patterns, so `mean 0` is
  not evidence of a black render.

Verified: renders on three untouched pairs are byte-identical (PNG
sha256) to the same runs on the pre-change harness, so the no-vars path
is provably unchanged.

## 2026-08-29 — #132: const→owned COW promotion is budget-checked

`Vm::arr_mut`'s copy-on-write materialization added
`array_cost(len) - CONST_ENTRY_COST` to `array_bytes` with no check, so on
a device-budgeted VM the first write to a `[…]` literal could push the
byte ledger past `array_byte_budget` (bounded by the element budget, so an
overshoot rather than a leak). The delta now goes through a new
`charge_array_bytes` — the byte half of `charge_array`, split out because
re-checking the element budget at the promotion site would demand a
spurious extra header's worth of headroom for an entry that allocates no
new arena slot — and it is checked *before* the copy is reserved. Error
semantics are unchanged from the OOM path already at that site: an
ordinary pattern-level runtime error, not a resource guard, so the PB
blast radius from #84 holds (the handler invocation aborts, the pixel pass
still runs). Two regression tests (`cow_promotion_*` in
`crates/luxel-core/tests/engine.rs`) pin the budget edge and the
within-budget delta; the edge one fails against the old code. docs/spec/vm.md
§1.2 documents the promotion charge. Workspace tests green.

## 2026-08-29 — Small-items batch: MSv3 300 px re-test clean on v0.1.39,
## playlist pre-flight dedup (#125), truthful corpus report, #124 pinned

Four picked from a fetch-work sweep, three landed by parallel worktree
agents (PRs #129/#130/#131), one run live on the Athom:

- **"Music Sequencer - for V3 ONLY" re-test at 300 px** (Athom, v0.1.39):
  the 2026-07-19 soak's one capacity holdout, fixed by v0.1.34's
  flash-resident pattern — reconfirmed on current firmware. Push accepted,
  120 s run, no vmerr, no reboot, min heap_free 71,260 B (matches the
  v0.1.34-era ~70 KB figure); fps swings 20–63 with the pattern's phases
  (some phases sit below the 30 fps SLOW line — capacity fine, some phases
  are just heavy). Rig restored exactly as found (60 px, brightness 4,
  prior pattern; post-restore /api/status byte-identical to the snapshot).
- **#125 → PR #129**: the playlist pre-flight in firmware main.rs now
  calls `budget::array_budget` instead of open-coding the same
  floor/headroom arithmetic. The inline constants matched the helper
  exactly, so this is pure dedup — the two paths can no longer drift.
- **Stale `TODO_BUILTINS` → PR #130**: all 39 hardcoded "not yet
  implemented" names in `tools/corpus/report.mjs` were long since
  implemented (BUILTINS has 138 impls, zero Todo entries), so the corpus
  report's headline gap column was pure fiction. The set is now derived
  from vm.rs at run time (loud failure if the table can't be parsed),
  `tools/corpus/last-report.json` regenerated (326 stale `uses.todo`
  lines dropped), derivation documented in docs/tools.md.
- **#124 → PR #131**: the `array(0)`-in-a-loop unbounded arena growth was
  already fixed as a side effect of #109's ledger alignment (zero-length
  arrays charge `ARRAY_HEADER_UNITS`, capping the arena at 2,559 slots) —
  verified by measurement plus a negative control, then pinned: 4
  regression tests, a compile-time `ARRAY_HEADER_UNITS > 0` assert,
  `Engine::arena_stats()`, and a docs/spec/vm.md §1.2 note. Adjacent
  finding filed as **#132**: `arr_mut`'s const→owned COW promotion adds
  to `array_bytes` without a `charge_array` check (budget overshoot,
  not a leak).

## 2026-08-29 — Re-judge queue cleared: 15 pairs, three judge batches
## (closes the re-judge halves of #99 and the #126 follow-ups)

All pairs unblocked by the engine-gap settlements got fresh output-only
verdicts, run per tools/verify/ORCHESTRATION.md (5 parallel Opus judges
per batch, one commit per batch):

- **Freeze family** — the oracle-confirmed 32.768 s freeze now scores
  AGAINST the ports (none reproduces it): fire-blue divergent/4,
  fire-red divergent/5, spring-colors close/6 (its active phase is
  near-exact). New reference detail: the freeze onset is fps-dependent
  and vanishes entirely at 40 fps on spring-colors.
- **Music-sequencers (#99)** — both sentinel strips work; the originals
  compile and render for the FIRST time, superseding orig-unrenderable:
  both divergent/4. Shared headline defect: the ports are
  x-coordinate-driven where the originals are index-only (vertical
  stripes vs index runs on the default grid); v2's sequencer-grid
  lattice is otherwise bit-exact, v3's macro schedule misses the
  original's 181 s dark phases.
- **Fixed silent-nulls** — both engine fixes confirmed working in
  anger: fast-palette-blending close/6 (setPalette live-alias; port's
  sweep is triangle-wave at 0.48 amplitude vs the original's full-strip
  sine) and slime-mold-palette close/6 (late-bound render2D; port grows
  4x too fast and lacks the original's remap startup animation).
  coral-plasma divergent/2 (port field ~40x too fine spatially, ~30x
  too fast — one shared scale constant suspected).
  skypirate-s-centered-spectrum divergent/4 on its 3x600 fixup rig
  (index-only original with hard-coded 300-px meter centres vs a
  normalized-coordinate port).
- **Fixup-rig pairs** — nano-orbital broken/2 (original: 12 dots at
  exactly 12 px/s; port ~60x slow and accumulates arcs into a frozen
  wash), orv-christmas-tree divergent/5 (byte-identical tree silhouette;
  port's snow is 2x2-snapped, smears instead of drifting, dwells 3x
  long; ornaments confetti vs red/blue garlands).
- **De-orphaned perlin ports** — perlin-fire divergent/4 (up from
  broken/2: crash gone, noise field now correlates 0.69-0.96 with the
  original; stays cold, one Mode band pure black),
  coronal-mass-ejection divergent/4 (improved but rings-vs-rays
  topology stands), eye-of-sauron broken/1 and
  distance-function-kaleidoscope-2 broken/0 (both ports still
  near-black — full fix-pass targets with precise numeric targets in
  their verdicts).

Cross-cutting finding, recorded in ORCHESTRATION.md: sweep-era verdicts
predating the perlin refit can describe a STALE original render (the
refit changed how perlin-using ORIGINALS draw). The 2026-08-29 verdicts
supersede those references; other perlin-heavy pairs deserve a
re-render before their old verdicts are trusted in the #101 fix pass.
JUDGE.md gained nine trap notes from judge friction (beat-aliasing
onsets, sensors-off at ≥20 fps, gradient-dominated cross-correlation,
whole-second dump lists, large-grid noise-vs-scale lens, fps-400
decorrelation, slow-cycle and marker-only probe false-inerts, rj-label
convention). Verdict distribution over the 15: 0 match, 3 close, 8
divergent, 4 broken — mean score 3.7, and every pair now has a current,
actionable verdict for the #101 fix pass and for review in the
tools/verify/review.mjs UI.

## 2026-08-29 — Engine gaps 4/5/7 settled against the oracle: wrap is
## authentic, setPalette aliases live, render late-binds, the array
## ledger is PB-exact (Gitea #106, #108, #109)

Three oracle probe batteries (fw 3.67; `tools/oracle/overflow-probes.mjs`,
`budget-bisect.mjs`, `alias-probes.mjs`) settled the remaining verify-sweep
engine gaps, and two of the four answers flipped the issue's premise:

- **#106 closed as authentic**: plain add/subtract/`+=` WRAP on real PB
  (`32000+1000` reads −32536 via exported vars; a post-wrap `>= delay`
  gate reads false), so the fire-blue/fire-red/spring-colors 32.768 s
  freeze family happens on real hardware too. No engine change — docs
  now say so (spec/vm.md, oracle findings), re-judges queued.
- **#109 reframed**: PB never frees arrays either — per-frame
  `array(100)` kills the pattern on the oracle at exactly frame 98. The
  actionable half was PRECISION: bisecting the real ledger gave
  **10,236 units with every array costing len+4** (single max 10,232;
  5113+5113 ok / 5116+5116 abort; all boundary points check). Engine now
  charges that exact model (was flat 10,240, no per-array cost), with
  boundary + exhaustion tests pinning the device numbers.
- **#108, the six silent-null originals**: two were real engine gaps,
  both oracle-confirmed and fixed — `setPalette(arr)` holds a LIVE
  reference (in-place writes re-cook the palette; snapshot dropped) and
  a render function assigned to `export var render`/`render2D` at
  runtime now dispatches (entry re-resolved each frame through the
  global; slime-mold-palette renders, and live re-assignment swaps
  entries like the oracle does). The other four: coral-plasma was a port
  arity bug (6-arg `perlinRidge`, fixed), skypirate needs its author's
  1800-px 3-column rig (fixups.json pins grid 3×600),
  performance-test-framework is non-visual BY DESIGN, automap needs a
  harness `--vars` flag (ticketed).
- **Bonus family**: the perlin octave refit (b37df0a) silently orphaned
  four ports written against the old min-1-octave clamp — their calls
  now ran 0 octaves and froze into constants (perlin-fire's "fire" was a
  static smear on current master, distinct from its frame-512 crash).
  De-orphaned with explicit single-octave calls (perlin-fire,
  eye-of-sauron, coronal-mass-ejection, distance-function-kaleidoscope-2)
  and killed the frame-512 crashes (perlin-fire + eye-of-sauron
  installed their setPalette literal per frame; hoisted to init).

Verified: workspace tests green (5 new engine tests pin the oracle
boundary numbers, palette aliasing, and late-bound dispatch);
snap.mjs re-runs show all four fixable silent-null sides rendering with
motion; FINDINGS.md carries the full addendum. The PB oracle was left on
its original pattern (restore-in-finally per probe).

## 2026-08-29 — Cold loads back to 10/10: the installer page's second
## vite entry was overflowing the device's 3-socket pool (Gitea #92)

Root cause of the 0/10 cold-load regression: commit `0b651c4` (the
WLED→Luxel installer page, 2026-08-15) added `flash.html` as a second
rollup input — 97 minutes *after* the 10/10 baseline was recorded on the
single-entry bundle. Vite then hoists the modules shared by the two
pages into an `app-*.js` chunk (injected as `<link rel="modulepreload">`)
and, with the default `cssCodeSplit: true`, emits a separate `app-*.css`
for it — so the browser-native burst right after `index.html` parses
went from 2 requests to 4. Native loads can't go through fetchgate, the
default pool is 3 sockets (`server.rs` `WEB_TASK_POOL_SIZE`), and a
`web_task` only listens while parked in `accept()` — smoltcp answers the
4th SYN with a RST, which Chromium reports as `ERR_CONNECTION_REFUSED`.
The refused victim (`/assets/app-*.css`, 758 bytes) didn't even exist at
baseline. The pool-churn from that burst also knocked over one *gated*
fetch (`/luxel.wasm`, refused then retried clean by fetchgate ~200 ms
later) — the second per-load failure the issue recorded at 60 px.

Fix is two lines of vite config, no firmware change and no new
dependency: `build.cssCodeSplit: false` (one shared stylesheet instead
of per-entry files) and `build.modulePreload: false` (the shared chunk
is fetched via its static import after the entry chunk arrives, instead
of preloaded in parallel). Native burst is now html → entry js +
stylesheet, 2 concurrent sockets worst case, and the second-wave
`app-*.js` lands after the first sockets close.

Verified on the Athom (v0.1.39, default build, assets pushed with
`deploy.sh --assets-only`): before-fix repro 0/3 with the exact issue
signature, after-fix **10/10 clean cold loads, 0 failed requests
total**, load times unchanged (3.6–4.3 s vs 3.9–4.4 s dirty). Playground
e2e and flash-e2e both pass (the installer page shares the merged
stylesheet), and the settled device UI screenshots clean. Docs touched:
ideas.md's stale "10/10 clean" claim now records the regression window;
tools.md's coldload row no longer calls the default pool 2-socket.

## 2026-08-24 — Interactive port review UI: both sides of all 293 pairs
## live in a browser, with per-pattern decisions that persist

`tools/verify/review.mjs` turns the finished verification sweep into
something a human can actually work through. It's a zero-dependency
local server (`node tools/verify/review.mjs`, default port 4183) that
serves the engine wasm and a plain-ES-module UI which compiles and runs
**both** sides of every pair live in the page — the corpus original and
its clean-room port, on the same engine, same rig, seed 1, the same
pinned wall clock and the same beat120 synthetic sensor feed the judges
saw. So what you watch is what was judged, except you can drive it.

List view is one card per pair: two live canvases (strip bar, pixelated
grid, or cloud z-slices), rig/verdict/decision badges, the judge's
summary, and a compact decision bar. The sticky top bar carries a global
fps slider (1–60), pause/reset-all, verdict and decision-status filters,
and slug/name search. Clicking a card opens a detail modal with bigger
canvases, per-side control panels (sliders honouring `//#` bounds hints —
including the line-above placement `library/` files use — plus hsv/rgb
channel sliders, toggles, triggers and polled showNumber/gauge
readouts), a per-side reset, and the full verdict: summary,
observations, a per-dial match table, feedback. 293 × 2 engines would
melt the tab, so an IntersectionObserver keeps only visible cards live,
capped at ~40 engines, with a single rAF loop round-robining a fixed
step budget — the same shape as the playground's Gallery.

The point of the tool is the **decision**: delete / good / fork (with an
optional new name) / needs-work, each with an optional feedback note
that goes verbatim to the agent doing the fix pass. A decision POSTs
immediately and lands in the tracked `tools/verify/decisions.json` via an
atomic tmp+rename write, so a review survives restarts and spans as many
sittings as it takes. Everything else is assembled at request time —
edit a `library/*.js` and reload.

Alongside it, `tools/verify/fixups.json` + `fixups.mjs`: a declared
per-slug fixup manifest shared by snap.mjs, report.mjs and review.mjs.
It strips author-planted tripwire lines from **originals** (the
deliberately-invalid sentinels a pattern's README tells the user to
delete — both `music-sequencer-*` originals were scored
`orig-unrenderable` purely because of these, and now compile and render
on both sides), and overrides the rig for **both** sides where an
original only renders on a specific geometry: `nano-orbital` ≥144 px,
`nyan-lights` a 300-px strip, `orv-christmas-tree` grid 20×20 — the
three manifest fixes SWEEP-NOTES.md had been carrying as a to-do. Fixups
in force are stamped into snap.mjs's `provenance.fixups`, and the
manifest is folded into `harnessSha256` so editing it correctly
invalidates cached runs. It is deliberately not a place to patch
patterns into working.

Verified in real chromium: 293 cards, canvases animating, both
music-sequencer originals rendering with no compile error, modal
controls rendering and a slider drag causing no runtime error, and a
needs-work decision plus feedback surviving a server restart and a fresh
page load. `docs/tools.md` gains rows for review.mjs and the fixup
manifest; the report.mjs row now says it's superseded for triage.

## 2026-08-23 — pow/exp2 overflow now saturates, PB-exact (Gitea #112)

Follow-on from the re-judge batch: PB's `pow` saturates on overflow —
positive to raw `0x7FFFFFFF`, negative-odd to raw `0x80000000`, both
oracle-pinned exactly (fw 3.67) — while our `exp2` wrapped
(`pow(2,16)` = 0, `pow(2,15)` = −32768). This is the one non-wrapping
corner of PB arithmetic found so far; documented in
docs/research/04-oracle-findings.md. Fix in `fmath::exp2` (saturating
integer shift) + `pow`'s negative-odd path (MIN, not −MAX). Unit tests
pin all seven probed values. With it, `synchronized-random-numbers`'s
original BSD-rand LCG (`% pow(2,16)`) comes alive — prng_state now
walks [0, 32768) like the oracle's. Workspace tests + wasm smoke green.

## 2026-08-23 — Engine gaps #104/#105 fixed: wall clock reaches init,
## random(negative) is PB-exact; sweep's wall clock was never applied

The verify sweep's two front engine gaps are root-caused and fixed
(Gitea #104, #105), plus a harness bug the sweep itself hid behind:

- **#104 time-of-day builtins.** Two stacked bugs. (1) snap.mjs recorded
  `--wall-clock` in meta.json but never passed it to `renderSide` — every
  render of the entire 293-pair sweep ran at epoch 0 via
  `setWallClock(undefined)` → NaN → 0. (2) The engine ran top-level init
  inside construction, before any host could hand it the clock, so
  init-time `clockHour()` reads were always 0 — on device and CLI too, not
  just the harness. New: `Engine::new_at`/`from_program_budgeted_at`
  (clock at construction), `lx_set_default_wall_clock` in the wasm ABI,
  and every host (firmware, serve, CLI, playground, enginehost) now
  supplies the clock at build time; the verify hosts throw on a
  non-finite clock. `pixelclock` renders now differ across wall clocks on
  both sides.
- **#105 init-time randomness.** Not init-specific: `random(max)` clamped
  negative `max` to 0, and `random(0xffff)` is `random(-1.0)` (16.16
  literal wrap, PB-identical). Oracle probe (fw 3.67): PB draws the whole
  signed range for negative max — `scale_random` now multiplies by the
  raw word unsigned, PB-exact; positive max unchanged.
  `static-random-colors` goes solid-red → 59/60 distinct colors;
  `synchronized-random-numbers` regains motion.

Filed the remaining untracked sweep gaps as Gitea #106 (2^15 ms freeze),
#107 (OOB-write tolerance residual), #108 (silent-null originals), #109
(array element budget). Tests: init-clock + negative-random semantics
tests added; workspace suite, wasm smoke, firmware build, web
typecheck+build all green. FINDINGS.md carries a dated addendum — sweep
verdict observations involving wall clocks describe epoch-0 renders.

**Re-judge batch (same day, post-fix):** all six unblocked pairs
re-judged by fresh Opus judges — pixelclock close/6 (port's real defect:
an i/60·60 16.16 round-trip that floors to i−1 except at 0/15/30/45 →
+1-shifted markers, 4/5/6-px hour bar, second-dot dropouts at
14/29/44/59), naturallightsync close/5 (port too white at noon, pale
night, sunset an hour early, ramp law off), sunrise-alarm-clock close/6
(neither side reads the wall clock — it's a 1 h/s time-lapse; port's
Cloudiness dial inverted+weak, pixel 0 dead, clock origin +5.7 h),
utility-scheduled-percent-on-demo close/7 (prior "total collapse" was
purely the harness bug; real defects: hour quantizer one LSB low at
exact k/24 sliders, invented 08:00–20:00 default schedule),
static-random-colors close/6 (fully-saturated pixel mass 16% vs 58%),
synchronized-random-numbers divergent/4. That last judge caught a NEW
engine gap: the original's BSD-rand LCG needs pow(2,16), and our
pow/exp2 WRAPS on overflow where PB SATURATES — oracle-pinned to raw
0x7FFFFFFF (pos) / 0x80000000 (neg) exactly. Filed as Gitea #112.
JUDGE.md gained a clock-driven-static-slug section (dense --wall-clock
sweeps; --probe-controls at one fixed clock can fake dead/mirror-image
dials); ORCHESTRATION.md re-judge queue updated.

## 2026-08-24 — Clean-room port verification sweep COMPLETE: all 293
## pairs judged (tools/verify/results/ + FINDINGS.md)

Every corpus/library pair now has an output-only verdict from an
independent Opus judge (5 parallel judges per batch, ~59 batches):
**24 match · 119 close · 123 divergent · 17 broken · 10
orig-unrenderable**, mean score 5.42/10. Each
`tools/verify/results/<slug>.json` carries measured observations,
per-dial comparisons and concrete acceptance numbers for a fix pass;
`tools/verify/FINDINGS.md` synthesizes the systemic defect families
(PB time-base constants, frame-vs-time coupling, missing lifecycle
management, control-surface drift, coordinate/units errors, colour
constant families incl. the Christmas template, direction flips) plus
the engine gaps the sweep surfaced (time-of-day builtins pinned —
confirmed; init-random constant; out-of-range write intolerance; the
32.768 s freeze family; silent-null originals) — tracked in Gitea #84
and #99. JUDGE.md grew ~30 measurement-trap notes contributed by the
judges as they hit them; snap.mjs gained `--wall-clock` and a
false-clamp-warning fix along the way.

## 2026-08-22 — The DEFAULT build takes the mild WiFi RX trim too
## (+6.4 KB idle, soaked with serial attached — Gitea #60)

Follow-up to the same day's small-chip WiFi tuning, and the first soak on
the Athom with `/dev/ttyUSB0` actually present — which is the whole reason
#60 was left open. `static_rx_buf_num` 10→6 on the **default** build; that
is the entire diff. AMPDU RX stays on and `dynamic_rx_buf_num` stays at
32, deliberately: `static_rx_buf_num` is the only knob that reclaims
anything at idle (those buffers are allocated in `esp_wifi_init` and never
freed), while the dynamic pool and the block-ack buffers are on-demand, so
trimming them would bound the worst case and cost RX throughput on a busy
network for no idle gain. `rx_ba_win` stays 6 and still validates
(6 < 32 dynamic, 6 < 2 × 6 static). `small-chip` keeps its harder 4/16/off.

**A/B on the Athom rig (v0.1.39, 60 px WS2812, idle `heap_free`, 20
samples each, both immediately post-OTA):**

| build | idle heap_free | Δ |
|---|---:|---:|
| default, stock pools (master) | 98,352 | — |
| default, `static_rx_buf_num` 6 | **104,832** | **+6,480 B** |

Exactly 4 × ~1,620 B, i.e. the arithmetic the small-chip session
predicted, with none of the estimate error that entry warned about. App
image is byte-identical in size (946,288 B both builds), so the 1 MiB slot
margin is untouched. `.stack` 29,396 B (athom) / 29,372 B (pixelblaze-v3),
clear of the 24 KB floor, no frame over 12 KB.

**Soak, all on the trimmed build, with a live serial capture the whole
time** (the thing the small-chip session could not do):

- `tools/hw-bench.mjs`: **321/322**, ~45 min. The one failure is the
  long-standing pattern-side array OOB in "sound - spectromatrix
  render2D", present on every prior soak. Lowest `heap_free` across the
  churn 78,408 B; fps-vs-pixels curve unremarkable (123 @ 60 px … 5 @
  2048 px). Serial shows exactly one boot in the whole run — the OTA's own
  reboot — and no panic, no `rst:` other than that, no boot-guard trip.
- **RX-pool stress**, A/B'd rather than just run (new `tools/rx-stress.mjs`,
  docs/tools.md): 180 s of DDP at ~244 pkt/s × 300 px (~217 KB/s inbound
  UDP) concurrent with a 6-worker HTTP API hammer.

  | build | DDP frames | HTTP served / refused | min heap_free |
  |---|---:|---|---:|
  | default, stock pools | 44,096 | 617 / 2,346 | 78,352 |
  | default, static RX 6 | 44,104 | 641 / 2,238 | 84,844 |

  The trimmed build served *more* requests under identical load, so the
  smaller pool costs nothing measurable here, and its heap floor under
  load is ~6.5 KB higher. Every watchdog sample during the run reported
  `live: "ddp"` — the frames were being received, not silently dropped
  (the harness fails if that count is zero, precisely so a dropping RX
  path can't read as a clean pass). Slot held `ota_0`, no vmerr, nothing
  on serial. Then a **640,026 B streaming asset upload** succeeded.
- `web/tools/coldload.mjs`: **at parity, and the parity is the finding.**
  Trimmed and stock-pool builds both score 0/10 clean at 300 px (1 refused
  sub-resource per load) and 0/5 clean at 60 px (2 per load), with
  identical timings; every load still boots fully (`boot ok`, editor
  populated, 0 page errors). So today's master does not hit the 10/10
  docs/ideas.md records for the 3-slot pool — that is a pre-existing
  regression on master, unrelated to the RX pools (an undersized esp-radio
  pool presents as a crash or dropped frames, never as a clean
  `ERR_CONNECTION_REFUSED` before any body). Filed as **Gitea #92**.

New tool: **`tools/rx-stress.mjs`** — the DDP+HTTP RX gate, written
because this stress has now been hand-rolled twice; it also watches
`slot` for the silent boot-loop rollback. Builds verified for
athom-music, pixelblaze-v3, c3-devkit, c6-devkit and athom+small-chip.
The post-rebase merged build was re-OTA'd and reproduces the same
104,832 B idle figure plus a clean 60 s stress, so the number survives
the day's other merges. Device left as found: 60 px WS2812, brightness 4,
playlist empty and stopped, running the merged build.

## 2026-08-22 — Perlin family fitted to the oracle and matched bit-for-bit
## (Gitea #65)

Offline fit of the 3,320 raw samples the 2026-08-22 oracle session
captured into `tools/oracle/sweeps/`. **PB's `perlin`/`perlinFbm`/
`perlinRidge`/`perlinTurbulence`/`setPerlinWrap` are a float32 port of
Sean Barrett's `stb_perlin.h`, using its non-power-of-two wrap variant**
(`stb_perlin_noise3_wrap_nonpow2`). `crates/luxel-core/src/noise.rs` now
reproduces it; the previous implementation was an invented stand-in.

How it was pinned down, from captured input/output only (no firmware
reversed) — full derivation in docs/research/04-oracle-findings.md:
per-cell polynomial fits of the fine sweep collapse at *exactly* degree 6
(⇒ gradient noise × quintic fade — value noise would be 5, simplex 8);
the recovered per-corner gradients all fall in the ±1/±1/0 basis; and the
lattice byte at every sampled cell is `randtab[randtab[floor(x) mod wrap]
+ seed]` against stb's own tables — the *double* lookup that distinguishes
the nonpow2 variant from plain `noise3_internal`, which fits no seed at
all. Arithmetic is f32 because that is what PB does: a careful 16.16
re-derivation sits ±5 raw units off, while f32 + truncate-toward-zero into
16.16 is bit-exact on 99.5% of the samples and within 1 LSB on 100%.

`compare-sweeps.mjs` before → after, all ten noise sweeps: 0% exact (max
error ~1.0 in value) → perlin1d_fine, perlin_seed, perlin_wrap4,
fbm_arg4/5/6 and ridge1d **100% bit-exact**; perlin1d, fbm1d, turb1d 99.0
–99.5% exact and **100% within one raw LSB** (1/65536). That harness had
been silently dropping each sweep's `setup` line, so `perlin_wrap4` was
being compared without its `setPerlinWrap(4,4,4)` — fixed here too.

**Existing patterns' noise visuals change** — that is the point: they now
look like they do on a Pixel Blaze. Behaviours worth knowing: the fractal
variants are *not* normalized (fbm at gain 0.5 spans ~±1.75, ridge is
non-negative and can exceed 1); each octave uses the octave index as its
seed, so layers never share lattice lines; `seed` wraps mod 256; ridge
starts at amplitude 0.5 and weights each octave by the previous octave's
value. `octaves` truncates toward zero (≤ 0 → 0) and is capped at 32 so a
runaway argument can't stall a frame.

New host tests lock the behaviour against subsampled device fixtures
(`matches_pixelblaze_perlin`, `matches_pixelblaze_fractals`, ±1 raw
tolerance) plus octave-count and seamless-wrap tests. Verified:
luxel-core 69/69, `cargo test --workspace` green, clippy clean in
noise.rs, stack-check ok (no frame over 12,288 B, `.stack` 29,372 B).
Flash cost on the tightest board (c6-devkit) +1,472 B → 993,648 B app
image, 54,928 B of slot margin. `web/public/luxel.wasm` in the main
checkout lags master as always — rebuild in your own worktree.

## 2026-08-22 — Runtime-error blast radius now matches PB: an error kills
## the handler call, not the frame (Gitea #84)

The two corpus originals that "hit array-OOB at frame 0 and render
all-black" (Nano Orbital, Orv - Christmas Tree) weren't hitting an
array-semantics gap at all — `array(3.2)` truncates to 3 slots on PB
exactly like ours (probed), so both patterns OOB on the real device too.
The gap was what happens NEXT. Oracle probes (fw 3.67, new
`tools/oracle/oob-probes.mjs`, self-judging): a runtime error aborts only
the current handler invocation — writes made before the abort stick, a
`beforeRender` abort does NOT skip the per-pixel pass, and a `render(i)`
abort keeps that pixel's pre-error hsv while later pixels render
normally. Our engine ended the whole frame on any error, so a
pattern erroring in `beforeRender` every frame stayed black forever.

`engine.rs drive()` now coerces a non-fatal error to the handler's normal
completion (the existing continuation logic then does the right thing —
`vm.pixel` already holds the pre-abort color); first error of a frame
wins `last_error` (a per-pixel error would otherwise re-alloc its message
per pixel). Deliberately still frame-fatal: `assert()` (init-only by
construction, belt-and-braces) and the VM resource guards — step limit,
value-stack bounds (named consts + `VmError::is_resource_guard`) — since
re-running a stuck handler per pixel would multiply the step limit by
pixel_count per frame and starve the firmware watchdog. Blast radius
recorded in docs/research/04-oracle-findings.md §10 and docs/lang.md.

Verified: luxel-core suite + full workspace green (5 new blast-radius
tests; 1 old test updated — it pinned the pre-oracle blanking behavior);
both originals render their real designs at 64 px via `luxel-cli pixels`
(orbit dots / full tree scene) with the OOB still reported as vmerr;
native↔wasm goldens bit-identical (wasm-smoke); athom-music firmware
builds, image-check ok, app 946,432 B (90.26% of slot), stack-check ok.
Follow-up: the output-verifier sweep (#84 filing) should re-judge the two
pairs as renderable once it lands/reruns.

## 2026-08-22 — Hygiene sweep: stale wasm goldens, the clippy deny that
## couldn't hold, and an indexed library sweep (#85, #79, #67)

Three small things that had each been quietly blocking a gate.

**#85 — `tools/wasm-smoke.mjs` goldens were stale.** The PB-exact
floor-quantization change (`floor(v·255)`, `quantize()` in
`crates/luxel-core/src/engine.rs`) moved the golden bytes, but only the
native tests were updated; the JS mirror was red against a healthy build.
Both goldens now match their native counterparts, cross-checked rather than
copied from what the wasm happened to emit: the rainbow frame's 128s → 127
(`tests/engine.rs::rainbow_golden_frame`), and — the one the issue missed —
the 2D map/transform frame's 255s → 254, because grid world coords max out
at ≈0.99998 and the quantization floors (`tests/semantics.rs::map_and_introspection`
asserts exactly that). Comments now name the native test each golden mirrors.

**#79 — `cargo clippy` failed on the firmware with 7
`large_stack_arrays` errors** (clippy 1.96), so the deliberate
`#![deny(clippy::large_stack_arrays)]` in `main.rs` gated nothing. Fixed
without weakening the deny:

- `netin.rs` (`bufs!`, 3 instantiations) and `provision.rs` (dhcp + dns
  tasks) now use `static_cell::ConstStaticCell` instead of
  `StaticCell` + `.init([0; N])`. This is a real fix, not an annotation:
  the zeroed array becomes a *const* initializer stored in the static
  itself (`.bss`), and `take()` just hands out the reference — no multi-KB
  value built at the call site that the optimizer is merely *likely* to
  elide. The 1024-byte DNS buffers were converted too; they sit exactly on
  the lint threshold. Metadata arrays stay `StaticCell` (not
  const-constructible here, and far under the threshold).
- `ota.rs:315` (`preboot_guard`) keeps its 3 KiB stack array under a scoped
  `#[allow]` with the real constraint spelled out: it runs *before* the
  `heap_allocator!` calls, so the `alloc::vec!` every other `OtaUpdater`
  site in that file uses would allocate from a heap that does not exist yet.
- `hub75.rs:83` likewise — the array is inside esp-hub75's
  `hub75_dma_descriptors!` macro (third-party, carried as a patch file),
  and DMA descriptors must live in a fixed static anyway.

Verification: `cargo clippy --release` clean (0 errors, warning count
unchanged at 32) on **board-c3-devkit** (default), **board-c6-devkit**,
**board-athom-music** (Xtensa/esp32) and **board-s3-devkit,hub75** — the
last is where the 7th error lives, invisible to a default-feature run.
`cargo build --release` still builds; `.bss` and `.data` are byte-identical
to master (296392 / 9564), `.text` +20 B — the buffers did not move out of
`.bss`. `tools/stack-check.sh` identical before and after: c3
`.stack` 39616 B over 1179 functions, largest frame 8944 B; the default
pixelblaze-v3 board `.stack` 29372 B over 1260 functions — no function over
the 12288 B budget on either.

Note for future Xtensa clippy runs: `cargo clippy` picks `clippy-driver`
off `PATH`, so the esp toolchain's `bin/` must be *prepended* to `PATH` —
setting `RUSTC` alone (what `stack-check.sh` does, which is enough for
plain builds) leaves mainline clippy compiling the fork's `core` and it
dies on intrinsic mismatches.

**#67 — `tools/check-library.sh`**, the library sweep every session was
re-deriving from prose. Builds `luxel-cli`, runs `luxel check` (compile +
LXBC round-trip + 3-frame smoke) over every pattern in `library/` on both
rigs the established acceptance uses — `check`'s default 10×10 and an
explicit 16×16 — prints a per-grid pass count plus the file and engine
stage/error for each failure, and exits non-zero if any failed. `GRIDS=`
overrides the rig list, an optional positional arg points it at another
directory. Indexed in `docs/tools.md`, alongside a row for
`tools/wasm-smoke.mjs`, which turned out to be the one script in `tools/`
the index had never listed.

Verification: sweep run end to end — **322/322 on the default grid,
322/322 on 16×16**. `cargo test -p luxel-core --release` green (163 tests,
0 failed). `node tools/wasm-smoke.mjs` passes against a freshly built
`luxel_wasm.wasm`.

## 2026-08-22 — Output-only port verifier: render-and-judge harness for
## the clean-room library (tools/verify/)

Many clean-room ports work poorly or not at all, and nothing measured
that. New harness verifies a port against its Pixelblaze original purely
from rendered output — no code inspection, which also keeps it cleanly on
the right side of the corpus firewall. `tools/verify/`:

- `gen-pairs.mjs` — pairs all **293/293** corpus `.epe` with their
  `library/` ports (provenance-comment key + slug key + 5 name-drift
  fixups; 4 ambiguous duplicate-name cases carry candidate id lists).
  29 library files are Luxel originals with no corpus counterpart.
- `snap.mjs` — renders BOTH sides headlessly (node + luxel wasm, zero
  deps) on an identical rig (strip 60 px / grid 16×16 / cloud 5×5×5),
  same seed + pinned wall clock + fixed delta ⇒ byte-deterministic.
  Artifacts per side: waterfalls (1D/3D), timestamped contact sheet +
  consecutive-frame filmstrip + full-window rhythm waterfall (2D),
  `meta.json` (controls, errors, stats summary + trend flags,
  provenance hashes), `stats.json` (full per-frame series),
  `--probe-controls` → per-dial responsive/inert fingerprint. Judge-
  safe: artifacts never contain pattern source.
- `JUDGE.md` — the judge-agent procedure: firewall rules, animations-
  not-frames doctrine (mandatory 60 s survey run, steady-state check,
  filmstrip/rhythm-based motion evidence), dial probing, verdict schema
  with output-level improvement feedback for a later fixing pass.
- `enginehost.mjs` / `png.mjs` — wasm C-ABI host and dependency-free
  PNG encoder (+3×5 digit font for cell timestamps).

Calibration verdicts (in `tools/verify/results/`): **amoeba** = broken
2/10 (port freezes ~2 s in and decays to black; original churns at
steady brightness indefinitely); **2d-fireworks-fade** = divergent 4/10
(mode cycle + palette match, but bulbs→solid bars, one sweeping beam →
always-on comb, missing blue phase, and a fully mismatched dial
surface). Both verdicts carry fixer-ready feedback. The corpus-wide
sweep runs next; findings so far: originals `nano-orbital` and
`orv-christmas-tree` hit array-OOB on our engine while their ports run
(engine gaps, ticketed separately).
## 2026-08-22 — HUB75 output driver: ESP32-S3 LCD_CAM via esp-hub75
## (Gitea #72, feature `hub75`)

The first non-strip `OutputDriver`: `firmware/src/hub75.rs` composes each
post-outpipe RGB888 frame into a bitplane BCM framebuffer that a circular
DMA chain rescans autonomously (esp-hub75 0.14 `circular-dma` +
`skip-black-pixels`) — refresh is decoupled from engine rate and costs no
ISR work. Two heap-leaked framebuffers (~28 KB each, 64x64 x 7 planes,
allocated at wiring time while the heap is fresh — alloc failure disables
output, never panics) double-buffer via esp-hub75's atomic descriptor
swap; the swap is waited at the *next* frame's start, so it's free at
engine rates. Compile-time geometry (const-generic DMA statics): 64x64,
7 bitplanes (~77 Hz at the 20 MHz example clock; 8 planes would halve
that — depth/clock tuning is on-metal work, #75). Brightness scales in
software like WS2812; `set_protocol` returns `Err` per the output.rs
fixed-wire-format contract (render task keeps the old protocol).
`PowerModel` branch in `luxel_core::outpipe`: HUB75 divides the strip
estimate by the scan ratio (1/32 for 64-row panels) — host-tested,
deliberately ~2x conservative vs typical rated panel draw.

esp-hub75 targets release esp-hal 1.1.0 and our stack pins esp-hal git
main; the drift (two renamed DMA APIs) is carried as a **patch file**
(`firmware/patches/`, per Jeremy's preference over vendoring) — the flake
materializes the patched source into gitignored `firmware/vendor/` (devshell
symlink + hermetic copy-in), `[patch.crates-io]` points at it. New nix
variant `luxel-fw-s3-hub75` is in the release board loop; image-check
gained feature-gated markers (`EXPECT_FEATURES`) with a `hub75:` marker.
Default pixel count on hub75 builds = 2048 (cap-clamped half panel) until
#74 lifts `MAX_PIXELS` per-board.

Verified: builds clean for s3+hub75, c3-devkit, athom-music, c6-devkit;
luxel-core host tests 65/65; hermetic `nix build .#luxel-fw-s3-hub75`
works (app image 884,320 B creds-baked → 164,256 B slot margin — smaller
than plain s3-devkit, the strip encoders drop out); stack-check ok (no
frame over the 12,288 B budget, `.stack` 50,548 B ≥ 24 KB floor — the
framebuffers are heap, only the ~600 B descriptor static lands in .bss).
UNTESTED ON METAL — no S3 on the bench; hardware bring-up is #75 (QEMU
can't model LCD_CAM, and the harness-isolation rule forbids faking it).

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
