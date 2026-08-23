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
into the captured window where the filmstrip starts; default midpoint).
Also `--probe-controls` (+ `--probe-seconds N`, default 4) — see below.

**`--skip` preserves the timeline.** Both sides run one deterministic clock
(same seed, same pinned wall clock, same fixed frame delta), and `--skip N`
renders the first N seconds and throws them away rather than restarting time.
So `--skip 14` shows you t=14 s of the *same* run a `--skip 0` capture began,
and `--skip 0 --seconds 20` and `--skip 14 --seconds 6` describe the same
timeline. That makes regime-by-regime comparison via `--skip` sound: you can
park both sides at whatever moment the survey flagged and compare there.

If `--strip-at`/`--strip-frames` don't fit the window they are silently
clamped — except the harness now says so, on stderr and in meta.json's
top-level `warnings` array. If that array is non-empty, the filmstrip is not
where you asked for it; re-run with a longer `--seconds` before reading it.

Rig overrides — the pattern's default rig comes from the manifest, and you may
replace it:

- `--rig strip|grid|cloud` — render both sides on a different rig entirely.
- `--grid WxH` — grid dimensions for the grid rig (implies the pixel count).
- `--pixels N` — pixel count for the rig.

Re-rendering a 1D pattern on a small grid (`--rig grid --grid 8x8`) is often
worth doing: the contact sheet and filmstrip make spatial structure legible in
a way the waterfall does not, and a port that indexes pixels differently from
the original will show it immediately. Both sides always get the SAME rig,
whatever you pass, so an override never makes the comparison unfair — it just
changes the lens. A rig override is a legitimate experiment, not a workaround;
label it (`--label grid8-recheck`) like any other.

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
- `meta.json` — settings, run-level `warnings`, each side's control list
  (name + kind), any compile/runtime error, and each side's `statsSummary`.
  It no longer carries the per-frame series, so it is short: **Read it whole.**
- `stats.json` — the FULL per-frame series, per side: `meanBrightness`,
  `meanR/G/B`, `motion` (mean abs frame-to-frame diff). Each series is one
  line. Use these to detect black, static, or strobing output numerically
  before trusting your eyes — but read `meta.json`'s summaries first and only
  come here when one of them flags something.
- `probe.json` — **only when you passed `--probe-controls`.** See below.

### Reading meta.json

Read each side's `statsSummary` FIRST — it is the whole window in a dozen
numbers:

- per series (`meanBrightness`, `meanR`, `meanG`, `meanB`, `motion`):
  `{avg, min, max, first, last}`. `first` vs `last` catches drift the average
  hides; `min`/`max` catch a pattern that blacks out or saturates.
- `zeroMotionFrames` — captured frames with no change at all. A high count on
  one side and not the other is a frozen port (note: with `--skip 0` the very
  first captured frame always scores motion 0, so expect a count of 1).
- `brightnessTrend` — `steady` | `decaying` | `rising` | `volatile`, from the
  first quarter of the window against the last (±20%), with `volatile` when the
  series swings more than 60% of its own mean. `decaying` on the port and
  `steady` on the original is the classic dying-port signature.

Only drop into the full series in `stats.json` when the summary flags something
(an odd `min`, a trend mismatch, motion you want to check for periodicity).
Each series prints on ONE line, so reading one is cheap — but a summary
comparison usually settles the question.

Top-level `warnings` collects run-level complaints (currently: a `--strip-at`
or `--strip-frames` you asked for that had to be clamped to fit the window).
Non-empty means one of your arguments did not take effect as written.

`sheetTimesSeconds` and `stripTimesSeconds` are TOP-LEVEL, not per side: both
sides sample identical frame indices by construction.

`provenance` (top-level) stamps the run: `gitSha` of the worktree plus
`portSha256`, `epeSha256` and `harnessSha256` (12-hex prefixes). If you reuse
an existing run directory rather than rendering it yourself, check its
provenance against a fresh run of the same slug — if `portSha256` or `gitSha`
differs, the port or the engine has changed since and the old run describes
code that no longer exists. Discard it and re-render; never mix runs with
different provenance in one verdict.

View the PNGs with the Read tool. Both sides always render on the same rig,
same seed, same clock — differences you see are real differences.

### Dial triage: `--probe-controls`

`snap.mjs <slug> --probe-controls --label probe` does the normal run AND, for
each side, sweeps every settable control ONE AT A TIME (others left untouched)
at 0, 0.5 and 1, comparing each setting's short render against the untouched
one. It prints a table and writes `probe.json`: per side, per control,
`{kind, deltas: {"0":d,"0.5":d,"1":d}, responsive}`, where `d` is the mean
absolute pixel difference and `responsive` means some setting cleared the
threshold. One command replaces a dozen manual low/mid/high runs.

Run it ONCE, early (right after the survey), and use it to decide where to
spend effort:

- It tells you WHICH dials are live and roughly how strongly. It does NOT tell
  you HOW a dial changes the output — for every dial the probe calls
  responsive, still do the manual low/mid/high sweep on BOTH sides and
  describe the change (faster? denser? bluer?) in `dials`.
- A dial the probe calls inert may just be slower than the probe window. The
  window is `--probe-seconds` (default 4 s). A mode/regime dial on a longer
  timer will read as inert; re-probe with a bigger `--probe-seconds`, or at a
  later `--skip`, before you believe it. Conversely, a dial with a large
  delta at only ONE of 0/0.5/1 is usually a threshold or mode switch, not a
  continuous knob — worth a manual sweep at intermediate values.
- **An inert dial on the ORIGINAL is a caveat, not a licence.** Record it as
  "dial effect unverifiable from output" and move on; it is not evidence that
  the port may drop the control. Control-SURFACE mismatches stay reportable
  regardless of responsiveness: a control the original has and the port lacks,
  one the port has and the original lacks, a name that differs, or a `kind`
  that differs is ALWAYS a finding in `observations`.
- Display-only kinds (`showNumber`, `gauge`) and `trigger` are not probed.

## Procedure

1. Baseline AND survey — **two runs, both required, before anything else.**

       snap.mjs <slug> --label baseline
       snap.mjs <slug> --label survey --seconds 60 --fps 5

   `baseline` (6 s, 20 fps) is the fine-detail view. `survey` is 300 frames of
   coarse time — cheap, and the only thing that shows you the pattern's FULL
   cycle.

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
   Do this before any manual dial work so you sweep only the live dials.
3. If a side is black or static, don't conclude yet: try `--skip 2`
   (some patterns warm up), a longer window (`--seconds 12`), and a lower
   or higher fps. Patterns that react only to dials may need a control set.
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
   dies and you must look at the late window before scoring it.
6. Always consult the `motion` and `meanBrightness` series in stats.json.
   The eye misses periodicity and trends the numbers make obvious: a repeating
   motion peak every N frames, a brightness ramp that never resets, motion
   pinned at 0 (frozen) or saturated (strobing). Compare the two sides' series
   shape, not just their averages.
7. Compare on these axes, in order of importance:
   - **Alive vs dead**: does the port render at all, without runtime errors?
   - **Structure**: spatial layout — number/shape/size of features (bands,
     blobs, waves, sparkles), 1D vs 2D character.
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
     the manual sweep gives you the character of the change. A dial that
     visibly does something on the original and nothing on the port is a
     finding. A dial that does nothing on the ORIGINAL is a caveat — record
     "dial effect unverifiable from output" and do not treat it as permission
     for the port to drop the control.

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
   avoid judging a one-seed fluke.

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

`experiments` must include `baseline`, `survey`, `probe`, and one label per
regime the survey revealed. If it doesn't, you skipped a mandatory step and
the verdict is not yet defensible.

Verdict meanings: `match` = a viewer would accept them as the same pattern;
`close` = same pattern, minor visible differences (list them); `divergent` =
recognizably related but wrong in a major axis (structure/motion/color);
`broken` = port errors, renders black/garbage, or bears no resemblance;
`orig-unrenderable` = the ORIGINAL fails on our engine so no comparison is
possible (report the error — this is an engine-gap finding, not a port bug).

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
