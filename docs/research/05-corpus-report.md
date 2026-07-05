# Corpus compatibility report

> Corpus: 293 of 297 community patterns from patterns.electromage.com
> (fetched 2026-07-05 via `tools/corpus/fetch.mjs` into the gitignored
> `corpus/` dir — community-owned content, kept local for testing only).
> Checker: `luxel check` (compile + init + 3 frames at 100 px), aggregated by
> `tools/corpus/report.mjs` → `tools/corpus/last-report.json`.

## Result (2026-07-05, after fixes + transforms/perlin/palettes/clock/map)

| metric | count | share |
|---|---|---|
| compiles | 291/293 | **99.3%** |
| …of which the 2 failures are *deliberate* interlock lines ("REMOVE_THIS_INVALID_LINE_IF_YOU_UNDERSTAND") that fail on PB too | — | effectively **100%** |
| compiles + smoke-runs clean (3 frames, 10×10 map + 16×16 retry, wall clock) | **291** | **99.3%** |
| runtime stop on a not-yet-implemented builtin | 0 | 0% |
| real runtime errors | 0 | 0% |

Three patterns (Snake 2D, Perlin/Simplex Noise 2D, Nano Orbital) hardcode
16×16 rigs and OOB at other sizes (on real PB too); the checker retries
runtime failures on a 16×16 grid, where all three pass. **Every valid
community pattern compiles and runs.** The only two failures are the
deliberate consent-interlock lines in the Music Sequencer patterns, which
fail on PB by design. M0's ≥90% exit criterion is exceeded at 100%.

## What the corpus taught us (all fixed)

1. **Scientific-notation literals** (`1e4`, `2.5e-3`) — 5 patterns. Decimal
   literals now go through f64 parsing (exactly what PB's JS compiler does)
   before 16.15 quantization.
2. **Function expressions** (`f = function (a) {…}`) — 8 patterns.
3. **All function declarations are global, regardless of nesting.** Corpus
   patterns call helpers declared *inside other functions* from elsewhere
   (Sunrise Alarm Clock's `updateMS`). PB's compiler flattens declarations;
   ours now does too. Duplicate declarations: last wins.
4. **Assigning to a function name demotes it to a variable** (Music
   Sequencer family) — the name becomes a global initialized with the
   function value, JS-style.
5. **GPIO constants** oracle-probed and predefined: LOW=0, HIGH=1, INPUT=1,
   OUTPUT=2, INPUT_PULLUP=5, INPUT_PULLDOWN=9, OUTPUT_OPEN_DRAIN=18,
   ANALOG=192.
6. **`null` / `undefined` / `===` / `!==`** — JS-isms the PB compiler
   accepts; strict equality folds to plain equality, null/undefined are 0.
7. **Fractional array writes truncate** (variable index) — the stock `sparks`
   pattern depends on it. Confirmed on hardware; the earlier "fractional
   writes abort" oracle result only applies to *literal* indices (a PB
   compiler-path quirk we deliberately don't copy).
8. **Sensor stubs**: `export var frequencyData/accelerometer/analogInputs`
   without initializers now bind zero-filled arrays (32/3/5), so ~50
   sound/motion patterns run dark instead of erroring (per plan §2.5).

## Environment-dependent "failures" (not bugs)

- FireFlies-class patterns do `array(1 + pixelCount/10)` then index the full
  loop range — genuinely out of bounds when pixelCount isn't a multiple of
  10 (PB aborts identically; verified via array(7.4) probes). The checker
  runs at 100 px to avoid penalizing these.
- Remaining 3 real runtime errors (Nano Orbital, Perlin/Simplex Noise 2D,
  Snake 2D) index by 2D-map-shaped math at a 1D pixel count — expected until
  maps land (M3); revisit then.

## What to build next (usage-ranked, from static scan)

| feature | patterns referencing |
|---|---|
| **transforms** (translate 34, resetTransform 29, scale 16, rotate 14, 3D variants ~13) | ~45 unique |
| **perlin family** (perlin 13, perlinRidge 9, perlinTurbulence 7, perlinFbm 6, setPerlinWrap 6) | ~25 |
| **clock** (clockHour 8, clockMinute 6, clockSecond 6, clockWeekday 2) | ~10 |
| **palettes** (setPalette 6, paint 6) | 6 |
| GPIO (pinMode 6, digitalRead 4, …) | ~7 |
| mapPixels / nodeId / sequencer | ≤3 each |

Feature usage: render2D 112, render3D 29, sensor vars 52 — 2D mapping is
used by over a third of the corpus, reinforcing the M3 mapper priority.
Priority order for builtins: **transforms → perlin → palettes → clock**.
