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
      (a[5]), negative reads (a[-1], a[-0.5]), OOB fractional reads, and ALL
      irregular writes — OOB, negative, and even in-bounds fractional
      (a[1.5] = 9 aborts).
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
  smooth error curves up to 666 raw (atan, 0.01) / ~0.0025 (asin). Ours use
  different (more accurate) polynomials. At |x| = 1 exactly, PB asin/acos
  return the true ±π/2 / 0 endpoints; ours agree.
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

## Operational notes

- The device's websocket (port 81) wedges permanently if clients vanish
  without a close handshake — power cycle required. The harness now always
  completes the close handshake; keep it that way.
- Preview/vars are snapshotted after a rendered frame; getVars needs a
  retry loop after live-coding.
- Restore the user's active pattern (`activeProgramId`) when done; the
  harness does this automatically.
