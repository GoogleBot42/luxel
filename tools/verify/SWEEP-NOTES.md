# Sweep running notes (orchestrator scratch — committed so nothing is lost)

Queued items discovered mid-sweep, to be posted/filed at the next milestone.
Cleared entries move to Gitea or the aggregate report.

## Queued for the next Gitea #84 (engine gaps) comment
- **Out-of-range pixel writes hard-error** — real PB tolerates them. The
  `nano-orbital` original writes a fixed 144-px canvas and throws
  `array index out of bounds` at frame 0 on any rig under 144 px (bisected:
  fails ≤128, fails at frame 13 @140, clean ≥144). Either clamp/ignore
  out-of-range writes engine-side or bump that slug's manifest rig ≥144.
- **Wall-clock builtins may not be wired** — `--wall-clock` sweeps across a
  full simulated day (4 instants) changed NOTHING on either side of
  `naturallightsync` (a pattern whose premise is time-of-day sync). Either
  both sides ignore the clock or engine clock builtins return constants
  regardless of `lx_set_wall_clock`. Check engine wiring during the fix pass.
- (previously queued, from batches ≤30): fast-palette-blending silent
  all-black original; fire-blue/fire-red exact-32.768s freezes with
  fps-dependent onset; chill-confetti delta clamp ~100 ms; single-set
  control interaction anomaly; seed 2-5 collisions.

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
