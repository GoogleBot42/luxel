# Sweep running notes (orchestrator scratch — committed so nothing is lost)

Queued items discovered mid-sweep, to be posted/filed at the next milestone.
Cleared entries move to Gitea or the aggregate report.

## Queued for the next Gitea #84 (engine gaps) comment
- **Out-of-range pixel writes hard-error** — real PB tolerates them. The
  `nano-orbital` original writes a fixed 144-px canvas and throws
  `array index out of bounds` at frame 0 on any rig under 144 px (bisected:
  fails ≤128, fails at frame 13 @140, clean ≥144). Either clamp/ignore
  out-of-range writes engine-side or bump that slug's manifest rig ≥144.
  More instances: `rainbow-comet` ORIGINAL throws the same error exactly
  once at frame 982 (line 32 col 7) on runs >~49 s and keeps rendering
  normally after; `orv-christmas-tree` original errors at frame 0 unless
  pixelCount % 20 == 0. All three would presumably run clean on real PB.
- **Wall-clock builtins NOT wired — CONFIRMED** (upgraded from suspected):
  `pixelclock` (a pattern that exists to display time) renders byte-identical
  output at ELEVEN wall clocks spanning epoch 0..2000000000, on BOTH sides;
  `naturallightsync` corroborates across a simulated day. Engine clock
  builtins return constants regardless of `lx_set_wall_clock`. Every clock
  pattern's core behaviour is unjudgeable until fixed; re-judge pixelclock
  and naturallightsync after the engine fix.
- (previously queued, from batches ≤30): fast-palette-blending silent
  all-black original; fire-blue/fire-red exact-32.768s freezes with
  fps-dependent onset; chill-confetti delta clamp ~100 ms; single-set
  control interaction anomaly; seed 2-5 collisions.

## Manifest rig fixes needed (pairs.json), tied to the out-of-range-writes gap
- nano-orbital: original writes a fixed 144-px canvas → rig must be ≥144 px
  (default 60-px strip errors at frame 0).
- nyan-lights: original is an index-only 60x5 = 300-px sprite LUT → rig
  should be 300 px (16x16 renders it as mosaic; errors ≥324 px).
- orv-christmas-tree: original renders only at pixelCount multiples of 20 →
  rig should be grid 20x20 (default 16x16 errors at frame 0).

## Harness nits (unfixed, low priority)
- `--dump` duplicate-time entries waste runs (no dedupe/warning).
- `--dump` landing on the LAST frame of a window may report a slightly wrong
  `times` entry: nano-orbital judge saw t=5.95 report the t≈0 dot positions
  in a 6 s/20 fps run while every other time was exact, `warnings` empty.
  Suspect final-frame snap. Un-reproduced; check before trusting last-frame
  dumps.
- Rhythm image is useless on axis-aligned gradients (known, documented).
- oasis judge: promote spatial-DFT phase-drift from caveat to first-line tool
  for 1D travelling-wave patterns (motion stat saturates in BOTH directions
  there); probe.json could carry a per-side "identical to untouched" flag.

## Sweep-process facts
- `orig-unrenderable` score semantics pinned 2026-08-23 (always 0 + summary
  note); `automap.json` normalized retroactively.
- Sentinel-tripwire .epes (author-planted invalid lines): tracked in Gitea
  #99 (music-sequencer-for-v2, music-sequencer-for-v3-only). Corpus-prep
  fix + re-judge; NOT engine gaps.
- `--wall-clock N` flag added to snap.mjs 2026-08-23 (fixed for the run;
  never advances with simulated time).
