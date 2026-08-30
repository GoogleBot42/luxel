# Oracle findings: differential testing against real Pixel Blaze hardware

> Method: black-box only. Test patterns are compiled with the browser compiler
> extracted from the device's own web UI (the MIT pixelblaze-client recipe,
> ported to Node in `tools/oracle/`), live-coded over the public websocket
> (nothing saved to flash), and exported vars read back via `getVars` — exact
> 16.16 raws. Device: "dodecahedron", Pixelblaze v3, **fw 3.67**, 420 px.
> Battery: `tools/oracle/vectors.mjs`; driver `tools/oracle/run.mjs`;
> latest results `tools/oracle/last-results.json`.
>
> Status 2026-07-05: **109/135 vectors bit-exact** after fixes below.

## Confirmed & fixed in luxel-core (bit-exact now)

1. **Literals are 16.15**: raw = trunc(v·65536) with the LSB cleared.
   Probes: `lit_epsilon` (raw-1 literal → 0), `multrunc` (0.7 → 45874,
   0.00005 → 2), `tri6` (0.6 → 39320). Our earlier "full-precision literals"
   deviation was removed — patterns tuned on PB must render identically.
2. **Predefined constants are literal-quantized too**: PI = 205886 (not the
   correctly-rounded 205887), E = 178144, SQRT2 = 92680, PI3_4 = 154414, etc.
3. **round() is floor(x + 0.5)** (round half toward +∞): round(-2.5) = -2,
   round(-0.5) = 0.
4. **sqrt is sign-preserving**: sqrt(-4) = -2 (the famous forum quirk).
5. **hypot/hypot3 wrap the sum of squares** into 16.16 before the sqrt:
   hypot(200,200) = 120.266 (80000 wraps to 14464), not 282.84 and not a
   per-square wrap.
6. **pow**: pow(x, 0) = 1 including 0⁰; negative bases follow the sign rule
   for integer exponents (pow(-2,3) = -8). Fractional exponent on a negative
   base → 0 on our side (PB value unprobed — likely NaN-ish; TODO).
7. **Division/modulo by zero → 0** (x/0, 0/0, x%0, mod(x,0) all 0).
8. **log(0) and log(negative) → MIN** (-32768.0 raw 0x80000000).
9. **Shift counts mask to 0..31** (1<<32 = 1, 1<<33 = 2); fractional counts
   truncate; negative counts mask (1<<-1 = 1<<31 → wraps to 0 for 1.0).
10. **Array indexing** (the big behavioral find):
    - In-bounds integer read/write: normal.
    - In-bounds **fractional read truncates** (a[1.5] → a[1]).
    - **Everything else is a runtime error that aborts execution**: OOB reads
      (a[5]), negative reads (a[-1], a[-0.5]), OOB fractional reads, and OOB
      or negative writes.
    - **CORRECTION 2026-08-29 (Gitea #107)**: this entry used to also list
      "in-bounds fractional writes (a[1.5] = 9 aborts)". It does not — the
      re-probe found `a[1.5] = 9` **tolerated and truncating** in every form
      (literal index, `array(n)` target, array-literal target, and top-level
      init scope). Luxel's uniform truncation is an exact match, not the
      deliberate divergence the old note described.
    - **Blast radius of that abort** (probed 2026-08-22 for Gitea #84,
      `tools/oracle/oob-probes.mjs`): only the current handler invocation
      dies. Writes made before the abort stick; after a `beforeRender`
      abort the per-pixel render pass STILL RUNS; a `render(i)` abort keeps
      that pixel's pre-error hsv and later pixels render normally. If the
      abort lands BEFORE any hsv/rgb that invocation, the pixel shows
      BLACK — not a stale value held from the previous frame (probed with
      a two-phase red→green pattern; the erroring pixel went [0,0,0], so
      PB zeroes per invocation exactly like Luxel's per-pixel reset). Also:
      `array(3.2)` truncates to 3 slots (same as Luxel) — so corpus
      patterns that OOB every frame (Nano Orbital, Orv - Christmas Tree)
      error on a real PB too, and visibly work anyway.
    - Beware when probing: an aborted init leaves later exported vars at 0 —
      always pair probes with sentinels (we got fooled once).
11. Everything already matching on first contact: two's-complement wrap
    (182², ±32768 boundaries, MIN/−1), full-word bitwise (`x|0` keeps
    fraction, `~` zeros low 16), truncated `%` vs floored `mod()`,
    floor/ceil/trunc/frac sign behaviors, `||`/`&&` returning operands,
    true/false = 1/0, comparison results, ternary, inc/dec value semantics,
    array methods (sum/length/sort/sortBy semantics incl. comparator sign),
    mix/smoothstep/bezier (exact), triangle/square/wave at representable
    points, missing-args-are-0 / extra-args-dropped.

## Transform semantics (hardware-confirmed 2026-07-05, bit-exact)

- **Ops apply to points in call order** (pre-multiply composition):
  `translate(0.25,0); scale(2,2)` → x' = 2(x+0.25); reversed order →
  x' = 2x+0.25. Probe `transform-order` matched our implementation exactly.
- **Transforms accumulate across frames** — no per-cycle auto-reset; only
  `resetTransform()` clears (probe `transform-accumulation`, bit-exact
  including the 16.15-literal step sizes).
- **rotate(+θ) is counterclockwise** (device map baseline (0, 0.5) →
  rotate(π/2) → (−0.5, 0), matching our matrix).
- `translate(t)` adds to the point (the rotate-about-center corpus idiom
  works as written).

## Known remaining differences (characterized via sweeps, deliberately not matched)

Sweep probes (`tools/oracle/sweep.mjs`, data in `tools/oracle/sweeps/*.json`,
captured 2026-07-05 from fw 3.67) sampled each builtin over dense input grids
with exact-raw inputs (`x = h + l1*(256*eps) + l0*eps` with `eps = 1 >> 16`
keeps every literal a small int and the value bit-exact). Findings, all
errors in raw 16.16 units unless noted:

- **sin/cos (and wave/tan, which share it)**: table-based, accurate to ±1 raw
  over the whole domain *except* a seam around π. Extracting the implied
  phase error δ per sample shows δ ramping in quarter-slot plateaus from 0 at
  x ≈ 2.5 to exactly **−1.000 table slots (2π/1024) at π**, snapping to
  +1.000 just past π and decaying back to 0 by x ≈ 3.7 — the signature of an
  off-by-one in the LUT mirror-fold branch of a 1024-entry (or 256×4) sine
  table. Max value error 407 raw (0.0062) at the seam; sin(π/2) plateaus 1
  raw high (65537). Bug-for-bug replication would mean reproducing their
  off-by-one exactly; our polynomial sin is within ±2 raw of true sin
  everywhere, so we differ from PB only inside their seam. **Keeping ours.**
- **sqrt**: 86% of samples match exact `floor(sqrt(x))` (which is what ours
  computes); the rest sit +1..+4 above, consistent with a Newton refinement
  from a float seed that sometimes lands high. Max diff 4 raw.
- **log/log2/exp/pow**: log and log2 are within ±4 raw of true everywhere
  (small systematic bias, never matching floor/round exactly — table+interp
  internals). exp/pow are relative-precision accurate (~4e-5 relative, so
  large raw diffs at large outputs are noise in the last few significand
  bits). Not exactly fittable without reproducing their table.
- **atan/asin/acos**: distinct low-order polynomial approximations with
  smooth error curves up to 666 raw (atan, 0.01) / ~0.0025 (asin). Ours are
  within ±1 raw of true atan across the sweep. **PB endpoint bug**: at
  exactly |x| = 1, PB asin returns 0 (not ±π/2) and acos returns π/2 for
  *both* ±1 (not π / 0) — the classic `atan(x/sqrt(1−x²))` construction
  hitting PB's divide-by-zero→0 rule. Ours returns the true endpoints;
  deliberate divergence.
- **luxel vs PB, same inputs** (`tools/oracle/compare-sweeps.mjs` reruns
  every sweep pattern through luxel-cli and diffs raws): sqrt 86% exact /
  100% ±3; log2 100% ±4; log 100% ±7; sin/wave exact-to-±2 outside PB's
  table seam; atan/asin/acos differ by exactly PB's polynomial error curve
  (ours ±1 raw of true); exp/pow relative ~7e-5 vs PB's ~4e-5 (both
  invisible); tan diverges only at the pole where both approximations blow
  up. Every remaining difference is attributable to PB-side approximation
  error or the endpoint/seam bugs above — our implementations are equal or
  closer to true math on every function measured.
- **prng (timeboxed, unresolved)**: `prngSeed(s)` returns the *previous* raw
  32-bit state and seeding is a raw passthrough, so the state chain is
  observable. Sequences are deterministic per seed and stable across reloads.
  `prng(max)` output = `mod(state, max)` for small maxima (large maxima are
  polluted by PB's division quirks); CRT over coprime small moduli
  (97/89/83/79/73) recovers full hidden states exactly (seed 42 → state
  0x2c7056a4 after one step). The state *transition*, however, is not
  xorshift (not affine over GF(2)), not an LCG mod 2^32 or mod 2^32−k; the
  best affine fit leaves structured residuals suggesting float-internal
  arithmetic or a multi-step generator. **Decision: keep xorshift32** —
  structurally equivalent (deterministic, seedable, uniform), sequences
  differ. Documented divergence; patterns that need *reproducible* sequences
  across PB and Luxel are the only casualty. `random()` is true-random on
  both (not comparable).
- **Builtin arity is compile-checked on PB** (`square(x)` with one arg is a
  compile error: "found 1 expected 2"). We default missing args to 0 —
  deliberate leniency (everything that compiles on PB compiles here);
  revisit if strict-compat mode is wanted.

## Sweep 2026-07-07 (fw 3.67 — 130/165 exact, + 6 new probes)

- **Transforms fully verified** (the raw sweep "diffs" decoded as map
  artifacts: the oracle's installed dodecahedron map puts pixel 0 at world
  x≈1.0 while our CLI side ran mapless). On equal maps Luxel matches PB on
  all three questions: first-called transform is *outermost*
  (`translate(.25)` then `scale(2)` → `(x+.25)·2`), transforms
  **accumulate across frames** (no implicit per-frame reset), and
  `rotate(PI/2)` maps `(x,y) → (−y,x)`. Pinned in
  `semantics.rs::transform_semantics_match_pixelblaze` and
  `transforms_accumulate_across_frames`. 1D-x transform behavior remains
  unverifiable while a map is installed on the oracle.
- **Refs in arithmetic**: an array value used in math acts as 0
  (`a+1 == 1`) — matches Luxel. Ref equality is identity (`a==a` true,
  `a==b` false for equal contents) — matches.
- **Assigning over a builtin's name** (`floor = 5`) is allowed and reads
  back 5; *calling* the name afterwards aborts pattern init — both sides
  identical (sentinel never lands).
- **`arr.replace(find, val)`** method form: exact wrap-encoded match.
- **PB rejects a builtin as a value** (`f = floor` → "Undefined symbol
  floor") — Luxel's first-class builtin references are a documented
  superset, same class as 1-arg `square`.
- Division/modulo-by-zero, overflow wrap, all shift edge cases (incl.
  negative `>>` = arithmetic shift), rounding family, `%` vs `mod`, logic
  values, literal precision, array OOB/fractional-index sentinels: **all
  exact matches** — those TODO(oracle) markers are settled.
- **`pow(negative, fractional)` fixed**: PB propagates log2(neg) = raw
  `0x80000000` through (its vars JSON shows it *unsigned* as +32768.0); we
  returned 0. Luxel now returns `Fx::MIN` — the identical bit pattern, the
  closest an i32 can get (+2³¹ is unrepresentable). `log2(0)`/`log2(neg)`
  = raw i32::MIN verified exact on both.
- **`random(max)` with a NEGATIVE max draws the whole signed range on PB**
  (probed 2026-08-23, fw 3.67; matched in Luxel same day, Gitea #105):
  `random(0xffff)` — the literal wraps to −1.0 on both engines — and
  `random(-5)` both measured min/max ≈ ±32760 over a few thousand draws.
  Consistent with an unsigned multiply-and-keep-high-word with no clamp;
  Luxel's `scale_random` now mirrors it (max's raw word taken unsigned).
  Corpus patterns use `random(0xffff)` for full-width PRNG seeds.
- **`pow` overflow SATURATES on PB** (probed 2026-08-23, fw 3.67; fixed in
  Luxel same day, Gitea #112): pow(2,16), pow(2,20), pow(2,15),
  pow(2,15.5), pow(10,10) all return raw `0x7FFFFFFF` exactly (pinned via
  raw-wrap subtraction: `pow(2,16) - 32768` reads −1 raw), and
  pow(−2,17) returns raw `0x80000000` exactly (equality against a
  runtime-computed MIN). This is the one place PB arithmetic does NOT
  two's-complement wrap — general arithmetic wraps, pow/exp2 saturate.
  Load-bearing for corpus PRNGs that compute `% pow(2,16)`
  (synchronized-random-numbers): the old wrap made the modulus 0 and the
  LCG stuck at 0.
- Transcendentals: ±1–5 raw as before. Largest remaining numeric gaps:
  `asin/acos` ~167 raw (~0.0025) and `atan(100)` 128 raw — fine for LEDs,
  but the place to look if we ever chase exactness.

## Pixel-level sweep 2026-07-08 (previewFrame harness — fw 3.67)

`tools/oracle/pixels.mjs` + `luxel pixels`: whole per-pixel test batteries
ship as ONE live-coded pattern (a case per pixel) and the device's
previewFrame stream (binary ws type 5, enabled by `{"sendUpdates":true}`)
is diffed byte-for-byte against the local engine. previewFrames are NOT
scaled by device brightness.

- **Quantization: PB is `floor(v·255)`** (0.5 → 127, 1−ε → 254). Luxel
  rounded to nearest; switched to floor — all 21 rgb/hsv rounding /
  clamping / hue-wrap cases now **bit-exact** (hsv internals were already
  identical). Every golden pixel test updated.
- **`paint(v)` position = floored-frac(v) exactly** (1.25 → 0.25, −0.5 →
  0.5) — measured with an identity (black→white ramp) palette so the
  output byte reveals the internal position directly. Edge artifacts,
  pinned to match: `v == 1` stays at the palette end; whole `v ≥ 2` lands
  at ≈1−ε (byte 254). Luxel previously frac'd everything (paint(1) wrapped
  to the start — a visible red-vs-blue divergence, now fixed).
- Result: **38/41 exact, 3 within ±1, none diverging.** The ±1s
  (paint(1.5), paint(2.5), paint(−2)) are PB float32 ULP loss on
  out-of-range inputs — not representable in 16.16 and ≤1/255 visually.
- Untouched pixels default to black on both. hsv24 behaves as hsv in the
  preview path.

## Maps and render2D (probed 2026-07-08, fw 3.67)

Motivated by the sqrt-grid soak failures (Breakout 2D et al. index out of
bounds on a mapless Luxel):

- **A PB that has ever saved a map cannot be made mapless through its
  public interface.** Saving a blank map source and saving `[]` are both
  no-ops: `pixelmap.txt` updates but the compiled map (`pixelmap.dat`)
  and the LIVE map survive untouched. Combined with new devices shipping
  with a default matrix map, `render2D` on a real PB always receives
  genuine map coordinates — the "no map" state effectively doesn't exist
  for users. (True factory-mapless behavior remains unverifiable without
  a factory-fresh device.)
- With BOTH `render` and `render2D` exported and a 2D map installed, PB
  calls **only `render2D`** (10,920 calls vs 0) — matches our dims-2
  render selection priority.
- **Luxel consequence**: a pattern that renders only in 2D/3D now gets a
  default ceil(√pixelCount) row-major grid map at engine construction
  (`Engine::set_default_grid_map`) instead of 1D-fallback coordinates —
  the PB-as-experienced behavior. A host/user map replaces it, exactly
  like saving a map on a PB. Note this does NOT paper over square-rig
  patterns' own assumption: `floor(1.0·√300) = 17` indexes past a
  17-slot array on a real PB just like on Luxel (engine test pins both
  halves of that).
- **Resolution (2026-07-19)**: the four square-rig soak holdouts
  (Breakout/Crosstown Traffic/Frogger/Swirlpool 2D) are gone — not via an
  engine change but because the clean-room library reimplementations that
  now source the gallery deliberately dropped the original patterns'
  `sqrt(pixelCount)`-square assumption (fixed 16×16 virtual canvases /
  map-driven normalized-coordinate math), so they run at any pixel count.
  Verified three ways at 300 px: native CLI (10 simulated minutes each,
  plus seed and fps sweeps), the shipped playground wasm (6000 frames
  each), and on-device (no vmerr). The default-grid semantics above are
  unchanged and remain the PB-as-experienced behavior; a corpus-faithful
  square-rig pattern would still OOB at non-square counts on Luxel *and*
  on a real PB alike (verified live at 10×30 earlier).
- Map plumbing learned along the way: maps save via `POST /edit`
  (form-file `/pixelmap.txt`) + ws `{savePixelMap:true}`; the compiled
  map is `GET /pixelmap.dat` (u16 header: version, dims, byte length;
  u16-quantized coords /65536); `GET /list?dir=/` enumerates the FS. An
  installed map can be reconstructed losslessly from INSIDE the pattern
  language (export arrays, fill from render2D/3D args) — how the probe
  snapshotted and bit-exactly restored the oracle's map
  (`tools/oracle/mapdump.mjs`).

## Operational notes

- The device's websocket (port 81) wedges permanently if clients vanish
  without a close handshake — power cycle required. The harness now always
  completes the close handshake; keep it that way.
- Preview/vars are snapshotted after a rendered frame; getVars needs a
  retry loop after live-coding.
- Restore the user's active pattern (`activeProgramId`) when done; the
  harness does this automatically.

## TODO(oracle) sweep 2026-08-22 (fw 3.67 — `tools/oracle/todo-probes.mjs`)

One session settled every remaining probe-able `TODO(oracle)` marker. New
probe battery: `tools/oracle/todo-probes.mjs` (self-judging — prints
verdicts rather than diffing raws, since map-dependent probes can't be
compared against a mapless CLI run).

Confirmed matches (markers removed, no code change):

- **Method-form `a.replace(2, 9)` writes from index 0** ([2,9,0,0] on a
  4-slot array) — it is `arrayReplace`, not the offset form. **Global
  `arrayReplaceAt(b, 1, 7, 8)` exists on PB** and matches Luxel exactly
  ([0,7,8,0]); PB's compiler accepts it variadically (arity 2+).
- **rotateX/rotateY/rotateZ are all CCW for +angle, right-handed** —
  probed at π/2 against pixel 0's 3D map coords, all three axes match our
  matrices bit-for-bit (rotateX: (x,y,z)→(x,−z,y), rotateY: →(z,y,−x),
  rotateZ: →(−y,x,z)).
- **`paint()` with no palette installed is the grayscale ramp** [v,v,v]
  (floor-quantized), and **palette state does NOT persist across
  live-code reloads** (A/B/A probe: grayscale → all-red palette →
  grayscale again, bit-identical).
- **`null` compiles and acts as 0 at runtime** (null+1 = 1, null*5+3 = 3).
- **Clock builtins**: device in America/Denver matched host wall clock
  exactly (date, hour, minute, weekday Sun=1). The **no-time-source case
  is untestable** — a configured PB's clock can't be unset via the public
  API; our "return 0" stays a documented choice.
- **Perlin-family arities match** (checked against PB's own compiler,
  no device needed): perlin(4), perlinFbm(6), perlinRidge(7),
  perlinTurbulence(6), setPerlinWrap(3) — exactly Luxel's signatures.

Divergences found and FIXED in luxel-core:

- **Transform stack cap is a SILENT cap at 31, not an error.** 40 stacked
  translates: dx advances for 31 steps then stalls; the pattern keeps
  running (no abort, sentinel lands). Luxel previously errored past 31 —
  `push_op` now silently ignores ops past the cap
  (`transform_stack_caps_at_31_silently`).
- **Palette lookup past the LAST stop is BLACK, not a clamp.** With stops
  {0.25→blue, 0.75→green}: paint(0.1..0.25) = blue (below-first CLAMPS),
  paint(0.75) = green exactly, paint(0.7501..0.999) = (0,0,0). Hard edge
  exactly at the stop (a "+1 raw" literal quantizes back to the stop, so
  the first representable value above it is already black). Single-stop
  palette [0.5→red]: ≤0.5 red, >0.5 black. Luxel clamped both ends —
  `palette_lookup` now returns black above the last stop
  (`palette_edges_match_pixelblaze`). The asymmetry is bug-for-bug PB.
- **`undefined` is NOT a PB symbol** ("Undefined symbol undefined" at
  compile). `null` is. Luxel keeps `undefined` = 0 as a documented
  leniency (same class as 1-arg `square`).

Noise sweeps captured for offline fitting: `sweeps/perlin1d*.json`,
`perlin_seed`, `perlin_wrap4` (setPerlinWrap(4,4,4) periodicity),
`fbm1d` + per-arg sweeps `fbm_arg4/5/6`, `ridge1d`, `turb1d` — 3,320
samples total. Fitted and matched the same day; see the next section.

## Perlin family fitted 2026-08-22 (fw 3.67 — Gitea #65, bit-exact)

The 3,320 captured samples pin PB's noise down completely. **PB's perlin
family is a float32 port of Sean Barrett's `stb_perlin.h`**, using its
*non-power-of-two* wrap variant:

| pattern language | stb_perlin.h |
|---|---|
| `perlin(x, y, z, seed)` | `stb_perlin_noise3_wrap_nonpow2` |
| `perlinFbm(x,y,z,lac,gain,oct)` | `stb_perlin_fbm_noise3` |
| `perlinRidge(x,y,z,lac,gain,off,oct)` | `stb_perlin_ridge_noise3` |
| `perlinTurbulence(x,y,z,lac,gain,oct)` | `stb_perlin_turbulence_noise3` |
| `setPerlinWrap(x,y,z)` | the wrap arguments (default 256) |

How each piece was established, purely from the captured input/output
pairs (no firmware was reversed):

1. **Noise class + interpolation curve.** Least-squares polynomial fits
   of `perlin1d_fine` (400 samples inside the single lattice cell
   x ∈ [0,1]) collapse to *exactly* degree 6: residual RMS 2005 raw at
   degree 2, 51 at degree 5, **0.30 at degree 6**, and no further
   improvement at 7–9. Degree 6 = a quintic fade (`6t⁵−15t⁴+10t³`)
   multiplying a gradient term that is itself linear in t. Value noise
   with the same fade would be degree 5; simplex's `(r²−d²)⁴` falloff
   would be degree 8. So: classic gradient noise, quintic fade, lattice
   on the integers.
2. **Gradient set.** Writing each cell's fit as
   `n(t) = (1−u)(Pt+Q) + u(R(t−1)+S)` recovers per-cell constants; the
   t² coefficient is 0 to within noise (as the form demands), and
   `R,S` of cell *i* equal `P,Q` of cell *i+1* across all 12 sampled
   cells. Every recovered `P` decomposes into the fixed y/z fade weights
   times gradient x-components drawn from {−1, 0, +1} — i.e. the
   ±1/±1/0 basis, not normalized random directions.
3. **The hash.** With y,z fixed, every lattice x depends on a single
   permutation byte. Solving for that byte at each x reproduces
   `randtab[randtab[floor(x) mod wrap] + seed]` using **stb's own
   randtab and grad-index tables**, at all 12 cells. That double lookup
   is the signature of `noise3_internal`'s *nonpow2* sibling — the plain
   `noise3_internal` chain (`randtab[x0+seed]`) does not fit any seed.
4. **Arithmetic.** A careful 16.16 re-derivation lands ±5 raw units off.
   Evaluating in **float32** and truncating the result toward zero into
   16.16 reproduces PB's raws **exactly on 99.5%** of all 3,320 samples
   and within one LSB (1/65536) on 100% of them. Rounding to nearest
   instead of truncating halves the exact-match rate, so the truncation
   is real.
5. **Fractal loops.** `fbm`/`ridge`/`turbulence` are stb's verbatim,
   including the details that matter visually: the **octave index is
   used as the noise seed** (every octave is a different field), ridge
   starts at amplitude 0.5 and weights each octave by the *previous*
   octave's value, and none of the three normalizes its sum. `octaves`
   truncates toward zero, and PB's docs confirm `setPerlinWrap` applies
   to all four functions.

`tools/oracle/compare-sweeps.mjs` after the port: perlin1d 99.0% exact /
100% ±1, perlin1d_fine · perlin_seed · perlin_wrap4 · fbm_arg4/5/6 ·
ridge1d all **100% exact**, fbm1d and turb1d 99.5% exact / 100% ±1.
Before the port, all ten sweeps were 0% exact with max errors near a
full unit of output.

(That harness had been silently dropping each sweep's `setup` line, so
`perlin_wrap4` was compared without its `setPerlinWrap(4,4,4)` — fixed
in the same change.)

## Overflow, allocation, and late binding (probed 2026-08-29, fw 3.67)

Batteries: `tools/oracle/overflow-probes.mjs`, `budget-bisect.mjs`,
`alias-probes.mjs`. Four findings, all now matched by the engine:

1. **Plain add/subtract (and `+=`) WRAP** at the 16.16 range ends:
   var-operand `32000 + 1000` reads **−32536**, `−32000 − 1000` reads
   `+32536`, and a post-wrap `x >= 20` comparison is false. So the
   `timer += delta` idiom (fire-blue / fire-red / spring-colors) really
   does freeze for exactly 2^15 ms on a real PB once the accumulator
   crosses +32767.9998 — the 32.768 s freeze family is AUTHENTIC device
   behavior, not an engine artifact (closes the engine half of Gitea
   #106; pow/exp2 saturating remains the documented exception).
2. **Array ledger, exact model**: every array costs its length **+ 4
   units** against a **10,236-unit** budget (equivalently a 40 KiB pool
   of 4-byte elements with 16-byte headers, 16 bytes pre-consumed).
   Boundary probes: `array(10232)` ok / `array(10233)` aborts init;
   `array(5113)+array(5113)` ok / `array(5116)+array(5116)` aborts;
   3×`array(3403)` ok. Engine's `DEFAULT_ARRAY_BUDGET` now mirrors this
   (was a flat 10,240 with no per-array cost).
3. **Arrays are never freed on PB either**: a pattern allocating
   `array(100)` per frame updates its frame counter exactly **98**
   times, then its `beforeRender` aborts every frame (98·104 = 10,192 ≤
   10,236 < 99·104). Per-frame allocation is a pattern bug on both
   engines; free-on-rebind would be a deliberate divergence (Gitea #109).
4. **Late binding is live on PB**:
   - `setPalette(arr)` holds a LIVE reference — in-place writes through
     `arr` changed `paint()`'s rendered output (red → blue) with no
     second `setPalette` call.
   - A pattern with NO exported render function renders through
     `export var render` holding a function, and re-assigning the var
     swaps the entry between frames (red ↔ green observed live).
   Both semantics are now implemented (palette re-cooks on backing-array
   mutation; the render entry re-resolves each frame through the global).

Still open, and why: **1D transform coords** (this oracle can never be
mapless again — see the maps section above); **sensor-board accel/light
scaling** (needs the physical PB sensor board); **no-time clock behavior**
(untestable on a configured device).

## Out-of-range index tolerance settled (probed 2026-08-29, fw 3.67 — Gitea #107)

`tools/oracle/oob-probes.mjs` grew a second battery (Q1–Q7) to answer the
question #84 left open: is an out-of-range array access an *error* on PB at
all, or does PB tolerate it (clamp / wrap / silent no-op)? The verify sweep
had inferred tolerance from corpus patterns that go out of range every frame
and visibly work on a real device.

**The premise is false.** PB errors on exactly the shapes Luxel errors on.
What made those patterns "work" is the narrow blast radius already fixed in
#84 (the abort kills one handler invocation, not the pattern), not tolerance.

Measured, `a = array(3)` unless noted, `before`/`after` sentinels bracketing
the statement inside `beforeRender`:

| probe | source | result |
|---|---|---|
| Q1 | `a[5] = 1` | **aborts** at the statement |
| Q2 | `a[-1] = 1` | **aborts** |
| Q3a | `v = a[5]` | **aborts** |
| Q3b | `v = a[-1]` | **aborts** |
| Q4a | `a[1] = 11; v = a[1.5]` | tolerated, truncates → 11 |
| Q4b | `v = a[3.5]` | **aborts** (truncate first, then bounds-check) |
| Q4c | `i = 1.5; a[i] = 9` | tolerated, truncates → `a[1] == 9` |
| Q4d | `a[1.5] = 9` (literal index) | tolerated, truncates → `a[1] == 9` |
| Q4e | `b = [10,20,30]; b[1.5] = 9` | tolerated, truncates |
| Q4f | same, in top-level init scope | tolerated, truncates |
| Q5 | `a = array(4); a[6] = 1` then read every slot next frame | array **untouched** — no clamp (slot 3), no wrap (slot 2), no partial write |
| Q6 | out-of-range write every 3rd invocation | frame counter kept advancing 21 → 138 in 1.5 s: the pattern **survives** erroring frames |
| Q7 | `t = array(4); t[0](1, 2)` | **aborts** — calling a never-assigned slot is an error, not a no-op |

The **sibling splat path** was the one real divergence (`b = array(4)` unless
noted; Luxel used to silently drop every element that fell outside the array
and to clamp a negative offset to slot 0):

| probe | source | result |
|---|---|---|
| Q8a | `arrayReplaceAt(b, 3, 7, 8, 9)` | **aborts** (would write 3,4,5) |
| Q8b | `arrayReplaceAt(b, 9, 7)` | **aborts** |
| Q8e | `arrayReplace(array(2), 1, 2, 3)` | **aborts** — the offset-0 form checks too |
| Q8g | `arrayReplaceAt(b, 2, 7, 8)` | tolerated — `off + count == length` is the accepted boundary, `b[3] == 8` |
| Q8f | `arrayReplaceAt(b, 2, 7, 8, 9)` then read back | **nothing written** — all-or-nothing, not even the in-bounds prefix |
| Q8c | `arrayReplaceAt(b, -1, 7)` | tolerated, `b[0]` untouched |
| Q8d | `arrayReplaceAt(b, -1, 7, 8)` | tolerated, `b[0] == 8` — a negative offset **shifts**, it does not clamp |

So the rule is: the span is validated up front (`offset + count > length` →
runtime error, array untouched), and a negative offset is not validated at
all. PB gets the Q8c/Q8d behavior by storing through a negative index —
memory-unsafe, and it is how the hazard below arises. Luxel matches the
observable in-range half and skips the rest.

**DEVICE HAZARD.** Two shapes reproducibly hang the oracle's engine (the
websocket stops acking `setCode`, and it drops off WiFi for a minute or so
before recovering on its own — four identical reproductions, plus two
unexplained dropouts before the pattern was spotted):

    arrayReplace(array(4), 1, 2, 3, 4, 5)       // 6-arg call, overruns
    arrayReplaceAt(array(4), -3, 1, 2, 3, 4)    // 6-arg call, 3 negative

Both are 6-argument calls, and 4-argument versions of both shapes error
cleanly (Q8e, Q8c) — so the bounds rule above is probe-able without the
hang, and `oob-probes.mjs` deliberately contains neither. Whether the
trigger is the argument count or the number of unchecked negative stores is
unresolved and deliberately not probed further.

Consequences:

- **The tolerance premise needed no engine change at all.** Luxel already
  matched every Q1–Q7 row; they are now pinned by
  `crates/luxel-core/tests/semantics.rs` (`array_bounds_are_runtime_errors`)
  and three tests in `crates/luxel-core/tests/engine.rs` (untouched-array,
  survives-an-erroring-frame, unset-array-slot-call).
- **The Q8 splat divergence was fixed** in `vm.rs`'s
  `ArrayReplace | ArrayReplaceAt` arm, pinned by
  `array_replace_span_is_bounds_checked` and
  `rejected_replace_span_leaves_the_array_untouched`. No library or corpus
  pattern uses either builtin, so nothing rendered changed.
- **Q4d/Q4e/Q4f retire a documented divergence.** The 2026-07 note that PB
  aborts on a *literal*-index fractional write does not reproduce; every
  fractional-write form truncates. See the correction in item 10 above.
- **The `tools/verify/fixups.json` rig overrides stay.** nano-orbital,
  nyan-lights and orv-christmas-tree go out of range on the sweep's default
  rig because their authors wrote them for a specific fixture — a real PB
  errors there too. The overrides are correct rig data, not a workaround for
  an engine gap.
- **rainbow-comet and tixy are faithful.** rainbow-comet's original glitches
  one frame (Q6 shape) and recovers; tixy's original calls an unset slot of
  its 100-entry formula table once its mode counter walks past the last
  assigned formula and goes black (Q7 shape). A real PB does both.
