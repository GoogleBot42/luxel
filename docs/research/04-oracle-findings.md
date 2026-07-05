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

## Known remaining differences (recorded, not yet matched)

- **Transcendental internals** (sin/cos/tan, exp/log, atan/atan2, asin/acos,
  sqrt rounding): PB uses its own approximations with characteristic error
  curves — e.g. sin(PI) = 0.00616 (suggests a ~1024-step quantization),
  sin(π/2) overshoots to 1.0000153, atan(100) is off by +0.002, asin(0.5) by
  +0.0025, sqrt shows a sporadic +1..+3 ulp positive bias, exp is
  near-float-exact. Ours are deterministic and generally *more* accurate;
  differences are ≤ ~0.006 — invisible in LED output but not bit-exact.
  Plan: dedicated sweep probes (export a 32-element array of f(x) per upload,
  fit the algorithm) if/when bit-exactness matters.
- **prng sequence**: algorithm unknown; prngSeed returns the previous raw
  state (initial state observed: raw -402413388 after fresh boot pattern).
  Needs a seed→sequence sweep to reverse. `random()` is true-random (not
  comparable).
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
