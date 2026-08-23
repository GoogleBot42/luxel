# Output-only port verification — judge procedure

This is the prompt template for a judge agent verifying ONE clean-room port
against its Pixelblaze original, purely from rendered output. The judge never
reads pattern code; it renders both sides, twiddles dials, looks at the
images, and writes a verdict with improvement feedback.

## Hard rules (firewall)

- NEVER read files under `corpus/`, `library/`, or `docs/pattern-specs/`.
  Not "just to check a name" — never. Your entire view of both patterns is
  what `snap.mjs` renders plus the control names/kinds it reports.
- Do not read the harness source either; this document tells you everything
  the harness does. If the harness seems broken, report that rather than
  debugging it yourself.
- Your verdict must be based on rendered output only.

## Patterns are ANIMATIONS

A single frame proves nothing. Two patterns can share a frame and be entirely
different animations; two frames can look unrelated and be the same animation
one step apart. Every claim you make — and the verdict itself — must be
grounded in behaviour ACROSS frames: how the image evolves, how fast, in what
direction, with what rhythm, and whether it repeats or drifts.

Some patterns only reveal divergence over long windows: a slow palette walk, a
beat that lands every 8 seconds, a drift that accumulates. The default 6-second
window can hide all of these. If you cannot describe the port's motion in
words, you have not looked at enough time yet.

Worse, many patterns have MODES — they rotate through several distinct looks on
a timer. A short window lands inside exactly one mode, and two patterns that
agree in that mode can be unrelated in the others. That is why the survey run
in step 1 is mandatory, not optional: you cannot know your window is
representative until you have seen the whole cycle.

## Tools you may use

All commands run from the worktree root inside nix:

    nix develop -c node tools/verify/snap.mjs <slug> [options]

Options: `--seconds N` (default 6), `--fps N` (default 20), `--skip N`
(warmup seconds discarded before capture, default 0), `--seed N` (default 1),
`--controls-orig "name=v;name2=v1,v2,v3"`, `--controls-port "..."`,
`--label NAME` (names the output subdir — use a fresh label per experiment).
For 2D rigs also: `--strip-frames N` (default 12) and `--strip-at S` (seconds
into the captured window where the filmstrip starts; default midpoint). Note
`--strip-at` is the odd one out: it is WINDOW-relative (add `--skip` yourself),
while `sheetTimesSeconds`/`stripTimesSeconds` and `--dump`'s reported `times`
are absolute on the run timeline.
Also `--sensors auto|synth|off` (default `auto`), `--probe-controls`
(+ `--probe-seconds N`, default 4) and `--dump "t1,t2,..."` — see below.
And `--wall-clock N` (epoch seconds, default 1756000000): sets the pinned
wall clock BOTH sides see, for probing time-of-day/clock-driven patterns at
different instants (e.g. sweep several times of day and compare each). The
clock is fixed for the whole run — it never advances with simulated time, so
`--skip 86400` does NOT move it; only this flag does.

**`--skip` preserves the timeline.** Both sides run one deterministic clock
(same seed, same pinned wall clock, same fixed frame delta), and `--skip N`
renders the first N seconds and throws them away rather than restarting time.
So `--skip 14` shows you t=14 s of the *same* run a `--skip 0` capture began,
and `--skip 0 --seconds 20` and `--skip 14 --seconds 6` describe the same
timeline. That makes regime-by-regime comparison via `--skip` sound: you can
park both sides at whatever moment the survey flagged and compare there.
One trap: that only holds WITHIN one fps. Different fps = different timeline
(see below), so a regime timestamp read off a 5 fps survey is not valid as a
`--skip` target for a 20 fps run on a frame-coupled pattern — take `--skip`
targets from a survey at the same fps you will replay at.

**`--fps` changes the SIMULATION, not just the sampling.** The frame delta
handed to both engines is 1/fps, so a 5 fps run and a 20 fps run are different
timelines — not one timeline sampled coarsely — and `--seconds` × `--fps` is
how much simulated time you actually bought. Two consequences:

- A pattern that integrates real elapsed time and one that steps a fixed amount
  per frame agree at exactly ONE fps. Rendering the SAME side at 10, 20 and
  40 fps is the diagnostic: unchanged speed means time-based, speed scaling
  with fps means frame-stepped. Original fps-invariant and port not (or the
  reverse) is a real port bug — frame-rate coupling — for `observations` and
  `feedback`.
- The low-fps survey can make a frame-stepped pattern look degenerate: when its
  per-frame step lands on an exact cycle at 5 fps, the rhythm image collapses
  to flat bands and it reads as frozen when it is not. Worse, it can INVENT
  whole regime structure on a healthy pattern — a frame-coupled scroll
  accumulator at 5 fps produced two 33 s "freezes" and a 38 s "blackout"
  that simply do not exist at 20 fps. Before calling either side broken OR
  chasing a regime from a survey-rate run, re-check that stretch at
  `--fps 20`.
- TICK-QUANTISED patterns (a cursor/step that advances on a fixed sub-second
  tick rather than moving continuously) collapse onto the survey's frame grid:
  at 5 fps a 0.30 s tick and a 0.35 s tick BOTH round to 0.4 s, so the survey
  shows the two sides agreeing on rate when they differ by 17% — it fakes both
  timing agreement and fps-invariance. Never measure a stepping pattern's tick
  or cycle period from a run whose frame interval isn't several times finer
  than the tick; use ≥20 fps dumps and read the actual transition times.

**The 10/20/40 fps diagnostic is unreliable on large flat regions.** It works by
comparing the `motion` stat across rates, and `motion` is a mean absolute
frame-to-frame diff — which SATURATES on coarse imagery. Once a feature moves
further than its own width in one frame, every larger displacement produces the
same diff, so a pattern made of big flat blocks, a two-colour split, or a wide
solid bar posts nearly identical `motion` at 10, 20 and 40 fps whether it is
frame-coupled or not. A flat `motion` triple is therefore NOT evidence of
fps-invariance on such a pattern. Confirm coupling by tracking POSITION instead:
`--dump` the same feature at two times on each rate (spaced well under half its
period, per the Nyquist note below) and read how far it actually travelled per
second. If the per-second displacement scales with fps, the pattern is
frame-stepped; if it holds, it is time-based. Only report frame-rate coupling
once the `--dump` positions say so.

STATIC FIELDS: if both sides post motion 0 with min==max brightness across a
long window (and dumps confirm byte-identical frames over time), the pattern
is a static test field — the temporal machinery (surveys, regimes,
autocorrelation, fps coupling) is a no-op. Pivot to RIG-GEOMETRY sweeps
instead: --pixels lattice sweeps (odd vs even cube sides sample the exact
0.5 midplane differently — decisive for threshold strictness), grid sizes,
and strip vs grid. Note the harness pins z = 0.5 EXACTLY on flat rigs
(grid/strip), which turns a 3D pattern's midplane handling into a
whole-channel difference — a useful lens, but recognize it as the same root
cause, not a separate "port adds a channel" finding. Per-setting probes on a
static slug are extremely cheap: `--seconds 1 --fps 20 --dump "0"` per dial
value makes a 12-point fine sweep or a 3x3 saturation/value grid practical.

Checkerboard/alternating-sublattice fields (a value living on only one
parity of pixels per frame, as in pond/wave sims) MOIRÉ badly under the
PNG's nearest-neighbour upscale — contact sheets can show convincing arcs
or ring structure that is pure display artefact. Verify any structural
claim on such a pattern from `--dump` numbers, never from the images.

Cross-correlation shift scans SATURATE at their search bound: a scan over
±6 px that returns exactly -6 every frame is pinned, not measuring — the
true shift may be larger. A flat result exactly at the bound means widen
the radius, never report it as a constant step. And glyph/text patterns
must be measured from `--dump`s, never from contact sheets — sheet cells
are narrow enough that one glyph spanning cells reads as a whole word, and
hump/cycle COUNTS read off a sheet cell are equally untrustworthy (the
nearest-neighbour upscale invents extra humps; a wave pinned at 3.0 cycles
by DFT "breathed" between 3 and 6 on the sheet). HUE/PALETTE periods read
off a sheet alias the same way motion does: a 5.4 s sheet spacing against a
6.0 s hue cycle beats into a fake ~55 s super-cycle and can make matching
sides look like they cycle at different rates — read hue period from an
unwrapped hue series over many cycles instead. And any rate metric whose
CEILING is the frame rate (e.g. head-moves counted per rendered frame maxes
at fps moves/s) will fake a frame-coupling finding at low fps — cross-check
with an fps-unbounded proxy like newly-lit pixels per second.

Centroid tracking is INVALID near canvas edges: a feature that slides
in/out of frame gets its visible centroid dragged toward the canvas
interior, which reads as slow-fast-slow "easing" when the true motion is
linear. For anything that enters or leaves the rig, fit the feature's known
kernel shape (taken from a fully-on-canvas frame) over a search range
extending PAST the edges instead of using the centroid.

To tell whether a side is INDEX-ONLY (drives pixels off the flat pixel
index) vs (x,y)-DRIVEN: re-render at a second grid size and diff the FLAT
pixel sequences — an index-only pattern's flat sequence is byte-identical
across grid shapes (and often across grid-vs-strip), while a coordinate-
driven one changes completely. Corroborate with the i-vs-i+1 versus
i-vs-i+gridWidth diff ratio (index-only patterns are much smoother along
the flat index than along columns). Index-only originals may also have a
NATIVE geometry (motifs recurring at a fixed index stride; errors above a
fixed pixel count) — reshape to that geometry before describing structure.

Period autocorrelation has a HARMONIC AMBIGUITY trap: the top-lag list can
rank the second harmonic above the fundamental, and two runs at different
fps agreeing on the same FRAME lag then reads as proof of frame-stepping
when it isn't. The cheap decisive test is a cross-fps rescale fit — compute
corr(series_fps40[i], series_fps20[k·i]) over a scan of k: best k = 0.5
means time-based (the same seconds), best k = 1.0 means frame-stepped (the
same frames). Run it before reporting frame coupling from autocorrelation
lags alone.

For measuring a SLOWDOWN/SPEEDUP FACTOR between the sides (or against fps),
whole-frame spatial autocorrelation vs lag beats centroid tracking when
features split, merge, or wrap: dump frames at fixed spacing, compute each
side's correlation of frame(t) with frame(t+lag) as a function of lag, and
read off how long each side takes to decorrelate to the same level (e.g.
corr 0.34 at 0.5 s on one side vs 8 s on the other = 16x). A time-rescale
scan — corr(orig[t], port[k·t]) maximized over k — pins an exact factor.
Two follow-ons once a speed factor exists: (1) `--skip` applies to BOTH
sides, so a single run compares the sides at DIFFERENT phases of their own
cycles — a fast port can look "frozen late" purely because its quiet point
arrives earlier, and ANY single-moment image comparison (not just late
windows) can invent structural differences that are pure phase (a "wedges
vs streaks" or "two-colour vs rainbow" read can evaporate at matched
phase). Pair runs with per-side-appropriate skips (orig at its event time,
port at event×k — fractional --skip works) or use phase-independent
distribution statistics before reporting any structural divergence between
sides whose periods differ.
(2) A slow hue-walk/super-cycle pattern may not repeat within 240 s — extend
the survey (500 s at 2 fps is cheap) until you have seen the cycle land at
least twice per side, or the period ratio is a guess.

If `--strip-at`/`--strip-frames` don't fit the window they are silently
clamped — except the harness now says so, on stderr and in meta.json's
top-level `warnings` array. If that array is non-empty, the filmstrip is not
where you asked for it; re-run with a longer `--seconds` before reading it.

### Sound and motion patterns: `--sensors`

Some patterns bind the Pixel Blaze sensor-expansion board — `frequencyData`,
`energyAverage`, `maxFrequency`, `maxFrequencyMagnitude`, `accelerometer`,
`light`. With no input those globals sit at zero and the pattern renders black
or frozen on BOTH sides, which is unjudgeable: two patterns agreeing on "all
zeros" tells you nothing.

So the harness feeds a SYNTHETIC sensor signal. It is fully deterministic —
computed from the frame index and fps alone, never from wall time or randomness
— and both sides receive byte-identical input on every frame, warmup included.
Model `beat120`:

- **A 2 Hz beat (120 BPM).** Each beat is a near-instant attack followed by an
  exponential decay across the 0.5 s interval. `energyAverage` rides it from
  0.15 at rest to 0.70 on the hit.
- **A 32-bin spectrum**, all of it scaled by that beat envelope over a small
  floor: a bass peak near band 1–2 (~48 Hz) thumping on the beat, a mid melody
  peak that STEPS THROUGH A REPEATING 8-STEP SCALE, one step per beat (so the
  melody bin walks up and resets every 4 seconds), and a small high-frequency
  shimmer around band 28 (~6.5 kHz) with its own slow tremolo.
- **`maxFrequency` / `maxFrequencyMagnitude` track the melody peak** — so
  `maxFrequency` is a staircase in Hz repeating every 4 s, not a constant.
- **An 8-second accelerometer tilt circle**: x and y trace a circle of radius
  0.2 once every 8 s, z holds a constant resting value.
- **`light` is a constant 0.5**; `analogInputs` are zero.

Modes:

- `auto` (default) feeds a side ONLY when its engine reports that the pattern
  binds sensor globals. A pattern that ignores sensors renders exactly as it
  would have with no feed at all, so `auto` is safe everywhere and needs no
  special-casing.
- `synth` always feeds, even a pattern that never reads it (a no-op).
- `off` never feeds — this is the old idle-state view, and it is how you
  reproduce a pre-feed run or see what the pattern does with dead sensors.

**Survey sensor patterns at 10 fps or better.** The standard low-fps survey
(5 fps, or 2 fps extended) samples a 2 Hz beat at or below its own rate: every
sample can land between flashes, and a healthy beat-locked pattern reads as
pure black or as a slow unrelated flicker. A judge was burned by exactly this
— a 240 s / 2 fps survey reported an original as mean-brightness 0. For any
side with `wantsSensors: true`, run the survey at `--fps 10` minimum and treat
beat-rate structure (multiples of 0.5 s) as the thing to look for. The same
aliasing bites `--dump`: times spaced at whole or half seconds land at the SAME
beat phase every time — use offset spacings (0.13, 0.27, 0.41, …) to see beat
dynamics.

`meta.json` records, per side, `wantsSensors` (does this pattern bind sensor
globals at all?) and `sensors` — `"synth"` or `"off"`, meaning what was
ACTUALLY fed, not the flag typed — plus `sensorModel: "beat120"` when synth.
Compare those two fields between runs before comparing anything else: a run fed
`beat120` and a run fed nothing are not comparable, and a future change to the
signal will change the model name.

What this means for judging:

- **Expect beat-locked behaviour at 2 Hz.** At the default 20 fps a beat is
  every 10 frames; in a 6 s window there are 12 of them. In the waterfall /
  rhythm image they read as evenly spaced horizontal bands, and in the `motion`
  series as a peak every 10 frames. A sound pattern that does NOT show that
  cadence — on either side — is failing to react, and that is a finding.
- **Timing and structure comparisons are valid**, because both sides hear the
  identical signal on identical frames. "Original flashes on every beat, port
  flashes on every other beat", "original's segment decays over ~0.3 s, port's
  sustains for 2 s", "original's hue steps with the melody, port's holds" are
  all real, reportable divergences — not sensor noise.
- **The melody's 4-second scale cycle and the 8-second tilt circle are the
  long periods in the signal.** A window shorter than 8 s cannot show the
  accelerometer behaviour completely; survey motion patterns over at least a
  couple of tilt circles (`--seconds 32`).
- `--sensors off` recovers the old idle-state view. Use it to establish what
  each side does with dead sensors (a well-written pattern degrades gracefully;
  one that errors or goes black is worth noting) — but judge the pattern on
  the fed run.
- **`--sensors off` is also a DIAGNOSTIC, not just a legacy view.** Rendering
  the same window fed and unfed localizes WHICH input path drives a divergence:
  whatever survives `off` unchanged is not sensor-driven, and whatever appears
  only when fed is. A real case separated two very different findings that
  looked identical on the fed run — "the port ignores audio entirely" versus
  "the port reacts to audio but its accelerometer term swamps it" — because the
  `off` render showed the port still moving on its own while the original went
  still. Run the pair (`--label sens-on` / `--label sens-off`, same seed, same
  window) before writing any "port doesn't react to X" claim.
- Dial probing uses the same feed, so `--probe-controls` now works on sound
  patterns instead of comparing silence against silence.

Rig overrides — the pattern's default rig comes from the manifest, and you may
replace it:

- `--rig strip|grid|cloud` — render both sides on a different rig entirely.
- `--grid WxH` — grid dimensions for the grid rig (implies the pixel count).
- `--pixels N` — pixel count for the rig. Some rigs need a count of a
  particular shape and SNAP yours to the nearest one: the cloud rig is an
  n×n×n lattice, so `--pixels` picks the side (`round(cbrt(N))`) and the count
  becomes side³ — `--pixels 1000` gives a 10×10×10 cloud, the default is
  5×5×5 = 125 — and the grid rig fills whole rows. When a snap changes your
  number the harness says so on stderr and in `meta.json`'s `warnings`, and
  `settings.pixels` records the count actually rendered.

Re-rendering a 1D pattern on a small grid (`--rig grid --grid 8x8`) is often
worth doing: the contact sheet and filmstrip make spatial structure legible in
a way the waterfall does not, and a port that indexes pixels differently from
the original will show it immediately. Both sides always get the SAME rig,
whatever you pass, so an override never makes the comparison unfair — it just
changes the lens. A rig override is a legitimate experiment, not a workaround;
label it (`--label grid8-recheck`) like any other.

**Resolution independence (2D patterns).** The same trick applied to a grid
pattern — re-render at `--grid 8x8` and again at `--grid 48x48` — says how it
maps onto the rig. One drawing into a fixed logical canvas shows the same
picture at both sizes, just coarser or finer; one rasterizing at native
resolution changes feature counts, sizes and detail with the grid. Whichever
the original does, the port should do too: original resolution-independent and
port not (or the reverse) is a structural finding, not a lens artefact — report
it with both grid sizes named.

**Rig-content caveat.** A rig override changes the lens, but for
pixel-count-dependent patterns it also changes the CONTENT: spark/particle
counts scale with pixel count, feature sizes are fractions of the strip,
density thresholds move. A pattern that shows 3 comets on 60 px may show 12 on
a 16x16 grid — on both sides. That is the pattern behaving correctly, not a
finding. Any headline claim (feature counts, densities, sizes, speeds in
px/frame) must be confirmed on the pattern's DEFAULT rig before it goes in the
verdict; override-rig runs are for spotting structural divergence, and
numbers read off them belong in observations only if you say which rig.

Output lands in `tools/verify/out/<slug>/<label>/`:

- `orig.png` / `port.png` — the renders. For 1D/3D rigs these are
  waterfalls: x = pixel index, y = time (top row = first frame). For 2D
  rigs they are contact sheets: frames sampled left-to-right, top-to-bottom
  across the capture window, each cell stamped in its bottom-left corner with
  its absolute time on the run's timeline (`3.0s`) — the same values
  `meta.json` `sheetTimesSeconds` lists. With `--skip` those stamps are
  absolute, not window-relative, so a `--skip 14` sheet starts at `14.0s`.
  The sheet's samples are spread far apart in time, so it shows WHAT the
  pattern draws, not how it moves — fast motion aliases into apparent chaos
  and slow motion into apparent stillness. Never read motion off the sheet.
- `orig-motion.png` / `port-motion.png` — **2D rigs only.** A filmstrip of
  `--strip-frames` CONSECUTIVE captured frames (same cells, same layout as
  the sheet), starting at `--strip-at`. Adjacent cells are one frame apart,
  so this is true frame-to-frame motion, unaliased: use it to judge
  direction, per-frame speed, and smoothness (does a feature glide, jump, or
  jitter?). Cells are stamped with their absolute time (two decimals here,
  since consecutive frames are closer than 0.1 s at the default fps); the same
  values are in `meta.json` `stripTimesSeconds`.
- `orig-rhythm.png` / `port-rhythm.png` — **2D rigs only.** EVERY captured
  frame collapsed to a single row of per-column mean RGB (mean down each grid
  column), stacked top-to-bottom over time — a 1D-style waterfall of the whole
  window. Use it to judge temporal structure: beats, cycles, sweeps, bounces
  (zigzag traces), drift, and whether the pattern repeats or keeps changing.
  Slopes read as horizontal motion; bands read as flashes. When the window is
  long, adjacent frames are averaged into one row rather than dropped —
  `meta.json` `rhythmRowsPerPixel` says how many frames each row covers.
- `meta.json` — settings, run-level `warnings`, and — under the same `sides`
  wrapper (`sides.orig` / `sides.port`) — each side's control list
  (name + kind), any compile/runtime error, its `wantsSensors` /`sensors`
  (/`sensorModel`) record, and its `statsSummary`.
  It no longer carries the per-frame series, so it is short: **Read it whole.**
- `stats.json` — the FULL per-frame series, per side. Exact shape (the series
  live under a `sides` wrapper, NOT at the top level):

      stats.json = {slug, label, capturedFrames,
                    sides: {orig: {meanBrightness: [...], meanR: [...],
                                   meanG: [...], meanB: [...], motion: [...],
                                   motionLit: [...]},
                            port: {…same keys…}}}

  so the port's motion series is `sides.port.motion`. `motion` is the mean abs
  frame-to-frame diff over the WHOLE rig; `motionLit` is the same diff averaged
  only over pixels lit (any channel non-zero) in either frame. Each series is
  one line. Use these to detect black, static, or strobing output numerically
  before trusting your eyes — but read `meta.json`'s summaries first and only
  come here when one of them flags something.
- `probe.json` — **only when you passed `--probe-controls`.** See below.
- `frames.json` — **only when you passed `--dump`.** Exact per-pixel values at
  the moments you asked for. See below.

### Reading meta.json

Read each side's `statsSummary` FIRST — it is the whole window in a dozen
numbers:

- per series (`meanBrightness`, `meanR`, `meanG`, `meanB`, `motion`,
  `motionLit`): `{avg, min, max, first, last}`. `first` vs `last` catches drift
  the average hides; `min`/`max` catch a pattern that blacks out or saturates.
- **`motion` vs `motionLit` — use `motionLit` on sparse patterns.** `motion`
  averages the frame-to-frame diff over EVERY pixel, so a pattern that lights
  2-3% of the rig has its real motion divided by ~40 and rounds to 0: the
  summary then reads `motion {avg 0, max 0}` and `zeroMotionFrames` at 100%
  while the thing is plainly animating. `motionLit` is the same diff averaged
  over the lit set only (any channel non-zero in either frame), so it measures
  how hard the lit pixels are working and survives sparseness. Real case: a
  star pattern's port posted `motion` 0 on all 300 captured frames while
  `motionLit` was non-zero on 246 of them — not frozen at all. Rule of thumb:
  if `motion.max` is 0 or 1 and `motionLit.max` is several times larger, the
  pattern is sparse and `motionLit` is the honest number. `motion` remains the
  like-for-like historical stat — quote it when comparing against older runs or
  verdicts, and quote both when they disagree.
- `zeroMotionFrames` — captured frames with no change at all, counted on
  `motion` (not `motionLit`). A high count on one side and not the other is a
  frozen port (note: with `--skip 0` the very first captured frame always
  scores motion 0, so expect a count of 1).
  **Dim, sparse, SLOW, and LOW-AMPLITUDE patterns false-read as frozen.**
  `motion` is computed on the QUANTIZED 8-bit output and then rounded to an
  integer, so a pattern animating at low brightness, over few pixels, very
  slowly (sub-quantum change per frame — several buggy ports crawl 15-20x
  under the original's rate), or densely but by only a few counts per pixel
  (a calm ember flicker) can post `zeroMotionFrames` near 100% while
  genuinely moving. Check
  `meanBrightness` and `motionLit` before concluding "frozen": if brightness
  is low (say under ~10) or `motionLit` is non-zero while `motion` is not,
  confirm with a `--dump` at widening lags (0.05 s up to seconds apart) and
  compare values directly — a slow port shows real change only at long lags.
  Related trap: a SLOW port read over a short window can also false-read as
  "too dim" simply because it hasn't visited a bright phase yet — check
  integrated brightness over a long window before reporting a brightness gap.
- `brightnessTrend` — `steady` | `decaying` | `rising` | `volatile`, from the
  first quarter of the window against the last (±20%), with `volatile` when the
  series swings more than 60% of its own mean. `decaying` on the port and
  `steady` on the original is the classic dying-port signature.

Only drop into the full series in `stats.json` when the summary flags something
(an odd `min`, a trend mismatch, motion you want to check for periodicity).
Each series prints on ONE line, so reading one is cheap — but a summary
comparison usually settles the question.

Top-level `warnings` collects run-level complaints (currently: a `--strip-at`,
`--strip-frames` or `--dump` time you asked for that had to be clamped to fit
the window). Non-empty means one of your arguments did not take effect as
written.

`sheetTimesSeconds` and `stripTimesSeconds` are TOP-LEVEL, not per side: both
sides sample identical frame indices by construction.

`provenance` (top-level) stamps the run: `gitSha` of the worktree plus
`portSha256`, `epeSha256` and `harnessSha256` (12-hex prefixes). If you reuse
an existing run directory rather than rendering it yourself, check its
provenance against a fresh run of the same slug.

**The authoritative reuse keys are `portSha256`, `epeSha256` and
`harnessSha256`** — the port source, the original `.epe`, and the harness
itself. If ANY of those three differ, the old run describes inputs that no
longer exist: discard it and re-render, and never mix runs with different
values in one verdict.

`gitSha` is informational context only — it is NOT a discard trigger on its
own. Sweeps commit verdict files and doc edits mid-run, so `gitSha` moves
constantly without any pattern or harness byte changing. A run whose three
content hashes match a fresh one is reusable no matter what `gitSha` says.

View the PNGs with the Read tool. Both sides always render on the same rig,
same seed, same clock — differences you see are real differences.

### Exact numbers: `--dump`

The PNGs are nearest-neighbour upscaled, so every pixel is a block several
output pixels wide, and a waterfall row is one or two pixels tall. Counting
stripe widths, locating a feature's pixel index, or naming an exact colour off
those images is guesswork — and a judge who guesses "the port's stripes look a
bit wider" writes weaker feedback than one who says "orig: 3-px stripes with
3-px gaps; port: 4-px stripes, no gaps".

    snap.mjs <slug> --label dump-mid --dump "0,3,5.5"

Times are seconds INTO THE CAPTURED WINDOW (so with `--skip 14`, `--dump 0` is
t=14 s on the timeline). For each one the nearest captured frame is taken and
written to `frames.json`:

```json
{ "times": [0, 3, 5.5], "frameIndices": [0, 60, 110],
  "orig": [ [[255,0,0], [255,0,0], [0,0,0], ...] ],
  "port": [ ... ] }
```

(plus `slug`, `label`, `rig`, `pixels`, `grid` and `requestedTimes`, so a dump
carries its own settings). `times` are the ABSOLUTE times actually dumped (`skip + index/fps`, the same
convention as `sheetTimesSeconds`), so they tell you which moment you really
got; a time past the end of the window is clamped and warned about in
`meta.json`'s `warnings`. Each side is one entry per time. A strip/cloud frame
is a flat array of `[r,g,b]` triples, one per pixel, in pixel-index order; a
GRID frame is nested as `gridH` rows of `gridW` triples, one row per line, so
the spatial layout reads straight off the page. A side that failed to compile
is omitted.

**`orig` and `port` are indexed by the SHARED `times` list.** They are not
per-side time lists: entry `i` of `orig` and entry `i` of `port` are both the
frame at `times[i]`, because both sides are dumped from the same frame indices.
So a dump list that mixes "the time the original peaks" with "the time the port
peaks" does not give you one entry per side — it gives EVERY side an entry at
EVERY time, and reading `orig[0]` against `port[1]` because that is the order
you wrote the times in compares two different moments. Ask for every time you
care about, then index both sides by the same `i`.

**The time list is parsed strictly, and a wrong list is worse than a rejected
one.** Entries are trimmed (so a list piped in with newlines after the commas
is accepted) and one trailing comma is allowed, but an empty entry or anything
that is not a plain non-negative decimal (`1 2`, `abc`, `0x10`, `1e2`, `-1`,
`NaN`) exits 2 naming the entry — nothing is silently skipped and there is no
default list to fall back to. What the parser CANNOT catch is a list that is
well-formed but not yours: if you stage a generated list in a file or shell
variable, give it a name unique to your run — other judges run concurrently in
the same worktree and share the scratchpad directory, and a `times.txt` written
by one judge has been overwritten by another between the write and the `--dump`
that read it. The run was silent because the substituted list was perfectly
valid and fitted inside the window. Prefer passing the list inline; when you do
stage it, check `frames.json`'s `requestedTimes` against what you meant to ask
for before reading a single pixel.

Use it whenever exact layout matters: stripe/band widths and duty cycle, the
pixel index of a feature's leading edge (compare the same dump time on both
sides to get a per-frame speed in pixels), exact palette RGB, whether "black"
is really 0 or a dim 6, or whether an off-by-one in indexing mirrors or shifts
the pattern. It costs one extra file and no extra render.

**Thin diagonal features alias into fake kinks on contact sheets.** A 1-2 px
bar near vertical renders in a 16-px cell as two half-segments at opposite
corners — it reads convincingly as a bent or forked shape when the pattern
actually draws a straight line. Before reporting "the port bends/breaks the
feature", confirm with a `--dump` (e.g. brightest-row-per-column monotonicity)
that the geometry really differs.

**`--dump` times snap to the nearest CAPTURED frame.** Dump spacing finer than
1/fps silently collapses onto the frame grid (only out-of-window clamps warn) —
a 0.05 s-spaced list at `--fps 10` really samples every 0.1 s and aliases.
Match dump spacing to the run's fps, or raise `--fps` for fine sampling.

**Mind Nyquist when tracking phase.** Your dump times ARE the sampling rate for
the feature you are following: dumps 1 s apart on something rotating about once
a second show it standing still, and a hair off that, running backwards. Read
the feature's period off the rhythm/waterfall image first, then space dumps
well under half of it — 0.1–0.2 s apart when measuring rotation or oscillation
direction. A direction claim from sparse dumps is an aliasing artefact until
you have re-dumped it densely. The SPATIAL twin of this trap: a side whose
texture period is only ~2 pixels on the default rig can read as moving the
WRONG DIRECTION from profile cross-correlation or DFT phase — de-alias on a
larger grid before claiming a direction disagreement between the sides.
Rotational symmetry tightens this further: an
N-fold-symmetric figure repeats every 360/N degrees, so the unambiguous
tracking range shrinks by the symmetry order — a 12-arm asterisk spinning past
~300 deg/s already aliases at 0.05 s spacing. Rotation that is fps-invariant
(verify that first) can be dumped at `--fps 100` purely to buy 0.01 s spacing.

Pair it with a SMALL rig. Re-rendering at `--pixels 12` (or `--rig grid --grid
8x8`) makes period and duty structure unambiguous — a whole frame fits on one
line and you can read the repeat directly instead of inferring it. Watch for
the rig-content caveat above: on some patterns feature sizes are fractions of
the strip and change with the pixel count, so confirm any headline number on
the default rig, with a `--dump` there too. The OPPOSITE move exists too: a
pattern whose spatial frequency runs to tens of cycles per strip aliases into
unrelated noise on small rigs — de-alias those with a LARGER rig
(`--pixels 600`) instead.

For any pattern with a size-dependent feature (tail length, block stride,
spot width), run at least one off-default `--pixels` size on BOTH sides and
compare the feature's absolute pixel size vs strip-fraction law. Two
different laws can INTERSECT at the default rig — a fixed-10-px tail and a
15%-of-strip tail agree at 60 px and diverge wildly at 12 or 300 px — so a
default-rig-only judgment can miss a real structural bug entirely.

(Environment note: `python3` and `bc` are not on PATH inside `nix develop`
— do frames.json/stats.json post-processing with `node -e`. A bash loop
using `$(... | bc)` fails half-silently: printf still emits 0.00 per entry,
producing a well-formed all-zeros `--dump` list the parser accepts — check
the run's echoed time range ("240 times, 0s .. 0s" is the giveaway). Use
`fs.readFileSync` with ABSOLUTE paths in `node -e` snippets — `require()`
with relative paths fails confusingly across the shell's cwd resets. And
`node` is not on PATH OUTSIDE `nix develop`: every half of a compound shell line that runs
node needs its own `nix develop -c`, including `$(...)` substitutions that
generate `--dump` time lists. Shell loops must be bash syntax — the login
shell is fish, but the Bash tool runs bash.)

### Dial triage: `--probe-controls`

`snap.mjs <slug> --probe-controls --label probe` does the normal run AND, for
each side, sweeps every settable control ONE AT A TIME (others left untouched)
at 0, 0.5 and 1, comparing each setting's short render against the untouched
one. It prints a table and writes `probe.json`: per side, per control,
`{kind, deltas: {"0":d,"0.5":d,"1":d}, deltasLit: {…}, responsive}`, where `d`
is the mean absolute pixel difference and `responsive` means some setting
cleared a threshold. One command replaces a dozen manual low/mid/high runs.

`deltas` averages over the whole rig; `deltasLit` averages over the pixels
either render lights — the same sparse-pattern correction as `motionLit`, and
for the same reason: a dial that visibly redraws a pattern lighting a few
percent of the rig posts a whole-rig delta under 1 and used to read INERT.
`responsive` is now true when EITHER clears its bar (`settings.threshold` for
`deltas`, `settings.thresholdLit` for `deltasLit`), and the table prints both
maxima. Quote `maxDeltaLit` on a sparse pattern; on a dense one the two agree.

Run it ONCE, early (right after the survey), and use it to decide where to
spend effort:

- It tells you WHICH dials are live and roughly how strongly. It does NOT tell
  you HOW a dial changes the output — for every dial the probe calls
  responsive, still do the manual low/mid/high sweep on BOTH sides and
  describe the change (faster? denser? bluer?) in `dials`.
- **Inert-in-probe is a HINT, never a conclusion — the probe reports false
  inerts.** The window is `--probe-seconds` (default 4 s), and if it lands
  inside a single long event, mode or dark gap, a perfectly live dial shows
  nothing: a real case was a palette dial that only takes effect on the NEXT
  event, several seconds after the probe ended. Before you record "inert" for
  a dial on EITHER side, confirm it with a manual low/mid/high sweep over a
  window long enough to contain several events/cycles (as the survey measured
  them) — e.g. `--seconds 60 --fps 5` per setting, or re-probe with a much
  bigger `--probe-seconds` and a `--skip` that lands elsewhere in the cycle.
  Only after that manual check may "inert" appear in your verdict, and say
  which window you checked over. Conversely, a dial with a large delta at only
  ONE of 0/0.5/1 is usually a threshold or mode switch, not a continuous knob
  — worth a manual sweep at intermediate values.
- **A single-dial sweep can be flattened by setter interaction.** Setting ONE
  control and varying it is the obvious experiment, and on some patterns it
  produces nothing: an original was swept across four values of the only dial
  set and all four renders came out BYTE-IDENTICAL — the dial started working
  the moment a second control was also set, because the pattern only consumes
  its dial state inside the other control's handler. So when a sweep looks
  impossibly flat (identical stats, identical images, delta exactly 0.00),
  re-run it with one other control explicitly pinned to a neutral value
  (`--controls-orig "sliderX=0.5;sliderOther=0.5"`) before recording "inert".
- **An inert dial on the ORIGINAL is a caveat, not a licence.** Record it as
  "dial effect unverifiable from output" and move on; it is not evidence that
  the port may drop the control. Control-SURFACE mismatches stay reportable
  regardless of responsiveness: a control the original has and the port lacks,
  one the port has and the original lacks, a name that differs, or a `kind`
  that differs is ALWAYS a finding in `observations`.
- Display-only kinds (`showNumber`, `gauge`) and `trigger` are not probed.

## Procedure

### Untouched defaults are real, but dialed comparisons decide (patterns WITH controls)

The harness applies NO control values unless you pass them, and neither side's
control functions are called at init — both patterns run on whatever their code
sets up internally. **This matches the real Pixel Blaze** (oracle-verified,
fw 3.67): on load it calls no control function of any kind, and control-backed
globals keep whatever top-level code assigned. So untouched-default state is
genuine hardware behaviour, not a harness artefact, and an untouched divergence
between original and port is REAL evidence of different behaviour — report it in
`observations` at normal confidence, no caveat needed.

It still should not decide the verdict, for a different reason. Some originals
render degenerate untouched — a real case was a spiral drawing zero arms
untouched and spiralling perfectly the moment any Arms value was set — which
tells you the author expected dial interaction; and whether PB replays saved
values when a user runs a SAVED pattern (the normal way patterns are run) is
UNVERIFIED. So the "intended look" is the dialed one.

Policy: for a pattern WITH controls, base the verdict primarily on
explicitly-dialed comparisons — set the SAME values on both sides
(`--controls-orig` / `--controls-port`) and judge there. Compare the untouched
defaults too and weigh any divergence, but cap how far it drags the score: a
port that matches at every explicitly-dialed setting while differing untouched
still belongs in `close`-or-better territory, with the difference spelled out in
`observations` and `feedback`. For a pattern with no controls, nothing changes.

Related artefact: on some patterns merely SETTING a control perturbs the output
even when the value is irrelevant (the setter call itself reseeds something).
This too mirrors PB, where setting a control always invokes the control function.
So a probe `responsive` verdict needs value-dependence confirmed — 0 and 1 must
differ from EACH OTHER, not just from untouched — before you call a dial live.
And the interaction cuts BOTH ways: a dial that is live when set alone can go
completely inert once a second control is pinned (a port gating one feature on
another's value). When you find a dial-gating structure, re-check at least one
RESPONSIVE dial inside each gating state too — not only the inert ones.
A sharp tool for suspected control-mapping bugs: the CROSS-DIALED pairing —
set DIFFERENT values on the two sides (`--controls-orig "dial=1"
--controls-port "dial=0"`) and test for byte-identity. Proving "orig at 1 ==
port at 0 exactly" turns a vague colour gap into a precise "the selector is
reversed/shifted; the looks themselves are perfect" finding.

Also: an UNTOUCHED default can sit OUTSIDE the range any dial value reaches
(e.g. an untouched slope of +0.98 when the slider spans 0..−0.98) — if no
sweep value reproduces the untouched render, that's a real property of the
pattern, not an analysis bug; report the default and the dial range separately.

Picker footgun: an `hsvPicker`/`rgbPicker` needs ALL THREE components
(`name=h,s,v`). Passing a single value leaves the other two at 0 — for an
hsvPicker that means value=0, i.e. pure black on that side, which reads as a
pattern failure rather than the bad argument it is. And when the two sides
name a control differently, `--controls-orig`/`--controls-port` must carry the
two different names — the same string on both sides silently no-ops on one.

Two more probe blind spots: MODE-SELECTOR controls — `inputNumber`s stepped
over integers, but also plain `slider`s whose range is chopped into k
discrete modes (e.g. six modes with boundaries at k/6) — are undersampled by
the probe's 0/0.5/1 sweep, which reaches at most two or three of the modes.
When a control snaps the output between distinct looks rather than varying
it continuously, sweep it finely (a dozen or more points across its range)
to find every mode boundary, and judge each mode on both sides. The cheap
decisive recipe: render ONE short deterministic window at many dial values,
hash each frame dump (or the PNG), and cluster — hash changes mark the exact
boundaries (two decimal places for ~20 two-second runs), and byte-identical
hashes prove two "modes" are actually the same look. Much stronger than
reading stats: it distinguishes round(v*3) from floor(v*4) directly.
One caution: hash-clustering runs are tempting to do at very low fps for
speed, but on a sensor-fed side that aliases the 2 Hz beat away and fakes
"this dial only works near v=X" — keep them at ≥10 fps (20 for beat-locked
dials) just like surveys. And
mode-gated sliders (live only inside one mode) probe inert from the untouched
mode — when a pattern has a mode selector, re-sweep the secondary dials INSIDE
each mode before recording any of them as inert.

Asymmetric control surfaces: when one side exposes controls the other lacks
(usually a port that invented or dropped dials), the "dialed comparison"
requirement is necessarily one-sided — dial the side that has them, compare
each setting against the other side's only render, and check whether the
dialed side's DEFAULT reproduces the fixed side. The asymmetry itself is
always a control-surface finding for `dials`/`feedback`, even when the
defaults happen to agree.

For a translating 1D pattern, the honest speed measurement is circular
cross-correlation of two `--dump`ed frames (find the shift that best aligns
them, divide by dt) — the `motion` stat exaggerates or compresses speed ratios
on smooth gradients and sparse dots alike. For SUPERIMPOSED counter-moving
waves (standing-wave/interference patterns) neither cross-correlation nor
single-k phase tracking can separate the trains — use a 2D space-time DFT over
~100+ consecutive dumped frames, and validate the sign convention against a
synthetic wave of known velocity before trusting any direction. Caveat: on a
pattern with a strong
REPEATING spatial period, plain cross-correlation degenerates (any multiple of
the period aligns equally well) — track the PHASE of the dominant spatial-DFT
component per frame instead; it also separates two superimposed waves moving
in different directions, which cross-correlation blurs into one bogus answer.

1. Baseline AND survey — **two runs, both required, before anything else.**

       snap.mjs <slug> --label baseline
       snap.mjs <slug> --label survey --seconds 60 --fps 5

   `baseline` (6 s, 20 fps) is the fine-detail view. `survey` is 300 frames of
   coarse time — cheap, and the only thing that shows you the pattern's FULL
   cycle.

   **60 s is a MINIMUM, not the survey length.** It is where you start; the
   survey is finished only when you have seen several full events or cycles.
   If the 60 s survey shows sparse events — long dark or static gaps, only one
   or two events in the whole window, a cycle that is still unfinished at the
   end — extend it (`--label survey-240 --seconds 240 --fps 2`) and read that
   instead. One judge needed 240 s before an aurora pattern's true event
   cadence was visible; at 60 s the cadence it would have reported was an
   artefact of the window. Sparse-event patterns are exactly the ones where a
   short survey produces a confident wrong answer, so spend the extra run.

   **Read the survey first**, and read its rhythm/waterfall images before its
   sheets. Regime structure shows up there as horizontal banding: a stretch of
   one texture, then a boundary, then another. That is what multi-mode
   patterns, slow palette walks, and long-period beats look like from far
   away. Note every distinct regime and the time it starts.

   Then inspect EVERY regime the survey revealed, on both sides, with a
   `--skip <t>` run parked inside it (`--label mode-a`, `--label mode-b`, …).
   Because `--skip` preserves the timeline, both sides are at the same moment
   of the same clock and the comparison is exact.

   **A verdict from a window shorter than the pattern's full cycle is
   invalid.** A 6-second look at a pattern with a 20-second mode rotation
   samples one mode and tells you nothing about the other three; two patterns
   that agree perfectly in mode A can be unrelated in mode B. If the survey
   shows the pattern still changing at 60 s, survey longer (`--seconds 180
   --fps 2`) until you have seen it repeat, and say in `confidence` if you
   never did.

2. Dial triage: `snap.mjs <slug> --probe-controls --label probe` (see above).
   Do this before any manual dial work so you sweep only the live dials — but
   treat an inert result as a hint to check by hand, not as a finding, and a
   `responsive` result as live only once 0 and 1 differ from each other.
3. If a side is black or static, don't conclude yet: try `--skip 2`
   (some patterns warm up), a longer window (`--seconds 12`), and a lower
   or higher fps — remembering that changing fps changes the simulation, so a
   pattern that looks degenerate at survey rate may be fine at `--fps 20`.
   Check `meanBrightness` before believing a frozen-looking `motion` series
   (dim patterns false-read as frozen), and re-render with controls dialed:
   patterns that react only to dials, and originals that render degenerate
   untouched, both need a control set before they can be judged.
   Check `sides.*.wantsSensors` in `meta.json` too: `true` on a black or frozen
   side means it is a sensor pattern, and it should already be getting the
   `beat120` feed (`sensors: "synth"`). If it says `"off"` you passed
   `--sensors off` — re-run without it. If it says `"synth"` and the side is
   still dead while the other side is alive, the port genuinely fails to react
   to the same signal, which IS the finding. For sensor patterns, judge the fed
   run and read the beat cadence (2 Hz, every 10 frames at 20 fps) off the
   rhythm/waterfall image.
4. Long window: the survey is coarse (5 fps); when it flags a stretch whose
   rhythm you cannot read, re-render just that stretch at a middling rate —
   `--skip <t> --seconds 20 --fps 10` — with a fresh label, and read the
   rhythm images before concluding. A "match" called from a 6-second window on
   a slow pattern is a guess, not a verdict.
5. Steady state: a port can be alive at t=0 and dead by t=5 — freezing on one
   frame, decaying to black, or saturating to a flat wash — while its opening
   seconds look exactly right. Before ANY verdict, confirm the port's steady
   state matches the original's. Two decisive experiments:
   `--seconds 30 --fps 4` (a long, cheap window that shows where the pattern
   settles) and `--skip 20` (capture only the late window, so nothing early
   flatters it). `brightnessTrend` and `zeroMotionFrames` in `statsSummary`
   flag this for free: port `decaying` against original `steady`, or a
   `zeroMotionFrames` count that climbs with the window length, means the port
   dies and you must look at the late window before scoring it — unless
   `meanBrightness` is low there, in which case rule out the dim-pattern false
   freeze with a `--dump` first.
6. Always consult the `motion` and `meanBrightness` series in stats.json.
   The eye misses periodicity and trends the numbers make obvious: a repeating
   motion peak every N frames, a brightness ramp that never resets, motion
   pinned at 0 (frozen) or saturated (strobing). Compare the two sides' series
   shape, not just their averages.

   **Get periodicity from AUTOCORRELATION of the raw series, never from
   eyeballing or binning.** Take the series straight out of stats.json,
   subtract its mean, and score every lag: the first strong peak is the period,
   in frames — divide by `--fps` for seconds. Coarse bins invent structure that
   is not there: one judge binned a series into 1 s buckets, a 1.1 s cycle beat
   against the bin width, and the resulting envelope was reported as an 11 s
   super-cycle that does not exist. If you must bin, bin at a width you already
   know divides the period. State periods as "N frames at F fps", and confirm a
   claimed period on BOTH sides with the same lag scan before calling a rhythm
   divergence.
7. Compare on these axes, in order of importance:
   - **Alive vs dead**: does the port render at all, without runtime errors?
   - **Structure**: spatial layout — number/shape/size of features (bands,
     blobs, waves, sparkles), 1D vs 2D character. When the claim is a WIDTH, a
     COUNT, a POSITION or an exact COLOUR, get it from `--dump` (and/or a
     `--pixels 12` re-render), not from the upscaled image.
   - **Motion**: direction, speed, rhythm, smoothness. For 1D/3D rigs read
     the waterfall slope. For 2D rigs, motion claims MUST come from
     `*-motion.png` (frame-to-frame) and `*-rhythm.png` (whole-window
     structure) — the sampled sheet alone is not evidence about motion.
     Back both up with the `motion` stat.
   - **Color**: palette, saturation, brightness envelope. Exact hue phase
     may differ; the palette family and distribution should not.
   - **Dials**: start from `probe.json` (step 2) — it already says which dials
     are live on each side. For each dial the probe calls responsive, and for
     each control name the sides share (or that obviously correspond), set it
     low/mid/high on BOTH sides (`--label ctl-<name>-low` etc.) and check the
     response direction and magnitude match: the probe gives you magnitude,
     the manual sweep gives you the character of the change. Sweep the dials
     the probe called INERT too, at least once each over a multi-event window
     — that is where the probe's false inerts turn up. A dial that visibly
     does something on the original and nothing on the port is a finding —
     but only after the port has been swept over a window long enough to
     contain several events, since "nothing" from a short window is exactly
     the false inert described above. A dial that does nothing on the ORIGINAL
     is a caveat — record "dial effect unverifiable from output" and do not
     treat it as permission for the port to drop the control.

     Controls semantics: the harness enumerates each side's controls FROM THE
     ENGINE, so the lists in meta.json are what the patterns actually expose —
     the original's list is authoritative for what the port ought to have.
     Both sides empty means the pattern genuinely has no dials; that is
     normal, not a harness failure — report `dials: []` and move on. But if
     the original has a control the port lacks, or the port has one the
     original lacks, or the same name has a different `kind`, that is ALWAYS a
     reportable finding in `observations` — record it before you have tested
     any effect, because a missing dial is a fidelity gap regardless of how
     the default render looks.
8. Randomness caveat: ports don't share the original's RNG sequence. Judge
   statistical character (density, rate, distribution), not per-pixel
   alignment, for stochastic patterns.
9. Run at least one non-default seed if the pattern looks random-driven, to
   avoid judging a one-seed fluke. **Pick well-separated seeds.** Adjacent
   small seeds can collide: on one pair, seeds 2, 3, 4 and 5 rendered
   identically on BOTH sides, so a "seed sweep" across them proved nothing and
   looked like seed-independence. Use something like 1 vs 7 vs 1234, and if two
   seeds render identically treat that as a fact about the seeds, not about the
   pattern — try a much larger one before concluding the pattern ignores its
   seed.

## Verdict

Write `tools/verify/results/<slug>.json`:

```json
{
  "slug": "...",
  "verdict": "match | close | divergent | broken | orig-unrenderable",
  "score": 0-10,
  "confidence": "high | medium | low",
  "summary": "one sentence",
  "observations": [
    "output-level facts, e.g. 'original: ~3 cyan comets moving right at ~1 px/frame; port: static rainbow gradient'"
  ],
  "dials": [
    {"name": "speed", "origEffect": "...", "portEffect": "...", "matches": true}
  ],
  "feedback": [
    "actionable, output-level improvement guidance for a future fixer, e.g. 'port's motion is ~4x too fast at default dials' — never code suggestions"
  ],
  "experiments": ["labels of the snap runs you performed"]
}
```

`experiments` must include `baseline`, `survey`, `probe`, one label per regime
the survey revealed, and — for any pattern with controls — at least one
explicitly-dialed comparison run. If it doesn't, you skipped a mandatory step
and the verdict is not yet defensible.

Verdict meanings: `match` = a viewer would accept them as the same pattern;
`close` = same pattern, minor visible differences (list them); `divergent` =
recognizably related but wrong in a major axis (structure/motion/color);
`broken` = port errors, renders black/garbage, or bears no resemblance;
`orig-unrenderable` = the ORIGINAL fails on our engine so no comparison is
possible (report the error — usually an engine-gap finding, not a port bug).
Three cause subtypes, and the observations must say which: (a) a real engine
gap — the original would run on a Pixel Blaze but not on us; (b) an artefact
unrunnable BY DESIGN — e.g. an author-planted sentinel line whose identifier
tells the user to delete it ("REMOVE_THIS_INVALID_LINE_…"); that fails on
real PB firmware too and needs a corpus-prep fix, not an engine fix;
(c) SILENT NULL OUTPUT — compiles and runs with no error but emits exactly
0,0,0 on every pixel under every configuration (rule out dim/sparse and
warm-up first: dump pixels, long windows, rig/seed/wall-clock sweeps).
Benchmark/instrumentation-flavoured slugs are prone to (c) — their display
may depend on measured real elapsed time, which the deterministic harness
pins — so suspect it early on such names rather than burning ten runs.
For a TRIGGER-heavy original that looks entry-gated (`--controls-*` accepts
`trigger` controls and records them in `controlsApplied`), give it ONE
combined fire-everything-in-dependency-order run, then conclude (c) — don't
burn runs on trigger orderings. And before any orig-unrenderable call, run
one unrelated slug as a harness-sanity control (use a unique label so you
don't collide with a concurrent judge in that slug's out/ directory).
Score for this verdict is always 0 with an explicit note in `summary` that
the score records "no comparison obtainable", not port quality — never score
the port's solo render.

The mirror image also exists: some ORIGINALS have a long warm-up transient
(one ran railed-at-full-brightness for ~1000 rendered frames — frame-counted,
so it outlasts any low-fps survey) before settling. A frame-counted
transient's DURATION moves with fps (~450 frames = 45 s at 10 fps but 11 s
at 40 fps), so a settle time read off one rate is invalid at another —
re-measure the settle point at whatever fps you compare at, and remember
the whole default 6 s baseline can sit inside the transient. When the two sides' mean
brightness differs wildly from t=0, run a settle check (`--skip 60`+ at 20 fps,
or `--seconds 120`) and judge the SETTLED regimes against each other; the
transient difference is then its own separate finding.

Tie-break for time-degenerate ports — right at first, then frozen, black, or
saturated: judge the STEADY STATE. A port whose steady state is dead is
`broken` even when its first seconds are a convincing match, because a viewer
living with the pattern sees the steady state, not the first two seconds.
Record the early-window resemblance in `observations` ("port matches for
~3 s, then decays to black and stays there") so the fixer knows how much is
already right. The same principle settles the softer cases: verdict and score
describe what someone watching the pattern all evening would see.

Score anchor: 10 = indistinguishable, 8 = match with nitpicks, 6 = close,
4 = divergent but salvageable with the feedback given, 2 = wrong pattern,
0 = dead.

Be concrete in `feedback` — it is the sole input a later fixing pass gets.
"Colors are wrong" is useless; "original's palette cycles blue→purple over
~4 s, port holds fixed green" is actionable.
