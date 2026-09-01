# Luxel pattern language

The Luxel pattern language is source-compatible with the Pixel Blaze
pattern language: patterns written for a Pixel Blaze compile and run on
Luxel unchanged (`.epe` imports supported), with a small set of documented
divergences and — over time — optional Luxel extensions that PB does not
accept. If you know JavaScript you already mostly know this language; it is
a JS subset with LED-specific builtins and fixed-point arithmetic.

## A first pattern

```js
export function beforeRender(delta) {
  t1 = time(0.05)                 // 0..1 sawtooth, ~3.3 s period
}

export function render(index) {
  hsv(t1 + index / pixelCount, 1, 1)
}
```

The engine calls `beforeRender(delta)` once per frame (`delta` =
milliseconds since the previous frame, fractional), then `render(index)`
once per pixel. The last `hsv()`/`rgb()`/`paint()` call in `render` sets
that pixel's color.

For mapped fixtures, export `render2D(index, x, y)` or
`render3D(index, x, y, z)` instead — coordinates arrive normalized to
0..1 from the installed pixel map. The most specific exported renderer
wins for the installed map; plain `render` is the 1D fallback.

## Numbers

Every value is a **16.16 fixed-point** number: 32 bits, two's complement,
16 integer bits and 16 fraction bits.

- Range ±32768 (exclusive); resolution 1/65536 ≈ 0.0000153.
- **Arithmetic wraps** on overflow (like the hardware it emulates):
  `32767 + 10` goes negative. Patterns rely on this for cheap cyclic math.
- Literals are quantized slightly coarser (see the spec: parse as float,
  truncate, clear the lowest bit) — `PI` is the closest *literal*, not the
  closest 16.16 value, to π. Runtime results use full 16.16 resolution.
- Division or modulo by zero yields 0. `log`/`log2` of ≤ 0 yields the most
  negative value. `sqrt` of a negative is sign-preserving: `sqrt(-4) = -2`.
- There is no separate integer or boolean type: comparisons yield 0/1,
  and any nonzero value is truthy.

## Variables and scoping

```js
var speed = 1        // top level: a global
leds = array(30)     // assignment without var: also a global

export function beforeRender(delta) {
  var i = 0          // var inside a function: local to the function
  accum = 5          // no var: global, even from inside a function
}
```

The rules are JavaScript's (PB's compiler is JS-based):

- `var` at the top level, or any assignment to an undeclared name —
  including from inside a function — creates a **global**.
- `var` inside a function creates a **function-scoped local**. Locals
  shadow globals of the same name.
- Function *declarations* always become globals, no matter how deeply
  nested they are written (PB flattens them; so do we). Duplicate names:
  the last definition wins.
- `export var x` makes a global visible to the environment (vars watcher,
  device API). `export function` makes a function a host entry point
  (`render*`, `beforeRender`, UI controls) — exported names are how the
  engine finds your code.

## Statements and expressions

Supported: `if`/`else`, `while`, `for(;;)`, `switch` (a [Luxel
extension](#switch-luxel-extension)), `break`, `continue`, `return`,
blocks, and expression statements. Semicolons are optional (JS automatic
semicolon insertion rules). Comments: `//` and `/* */`.

Expressions: arithmetic `+ - * / %`, comparisons `< <= > >= == !=`
(`===`/`!==` are accepted and identical to `==`/`!=`), logical `&& || !`
(short-circuiting, returning the deciding value like JS), bitwise
`& | ^ ~ << >>` (operating on the raw 16.16 bits — `1 >> 16` is the
smallest positive value; `~` additionally clears the result's fraction
bits), the [conditional operator](#the-conditional-operator) `?:`,
and compound assignment.

### The conditional operator

`cond ? a : b` evaluates `cond`, then **only** the selected branch — the
other one never runs, so it is safe to guard a division or an array read
with it:

```js
v = n != 0 ? total / n : 0
c = i < a.length ? a[i] : 0
```

It is **right-associative**, exactly like JS, which is what makes a
chain read as a run of else-ifs:

```js
// a ? 1 : (b ? 2 : (c ? 3 : 4))
level = a ? 1 : b ? 2 : c ? 3 : 4
```

A ternary in the *then* slot is closed by its own `:`, so
`a ? b ? 1 : 2 : 3` means `a ? (b ? 1 : 2) : 3`. Nesting in the
*condition* needs parentheses (`(a ? b : c) ? d : e`), since `?` binds
looser than every binary operator. Each branch is a full assignment
expression, so `a ? x = 1 : y = 2` works and the whole thing is itself an
expression — usable as a call argument, an array index, or the body of a
lambda (`c => c ? 1 : 0`).

### Compound assignment

Every binary operator has an `op=` form: `+= -= *= /= %= <<= >>= &= |=
^=`, plus `**=` (matching the [`**` extension](#luxel-extensions)). All
of them work on plain variables **and on array elements**:

```js
buf[i] += delta          // read-modify-write one element
buf[i] *= 0.9            // decay in place
buf[i]++                 // and ++ / -- , prefix or postfix
```

For `arr[i] op= x` the array and index sub-expressions are evaluated
**once** (so `buf[nextSlot()] += 1` advances the slot once, not twice),
the element is read, then the right-hand side runs — JS's order. The
result of the whole expression is the *new* value; `arr[i]++` yields the
old one and `++arr[i]` the new one, as in JS. Properties are not
assignable, so `a.length += 1` is a compile error.

Functions are first-class: lambdas (`(a, b) => a + b`) and function
expressions can be stored in variables, passed to the array HOFs
(`arrayForEach`, `arrayReduce`, …), and called through variables.
Builtins are values too: `f = sin; f(PI)` works.

`true`/`false` are 1/0; `null`/`undefined` exist for source compatibility
and behave as 0.

## Arrays

```js
a = array(10)        // 10 zeros
a[0] = 1
a[4.7] = 2           // fractional indices truncate (index 4)
n = arrayLength(a)
```

Arrays are reference values: assigning or passing an array aliases it.
Out-of-bounds or negative access (read or write) is a **runtime error**
that aborts only the current handler invocation — the engine reports it
with the source location and the frame keeps going: after a
`beforeRender` abort the per-pixel pass still runs, and a pixel whose
`render` aborts keeps whatever `hsv`/`rgb` it set before the error while
later pixels render normally (PB-matched blast radius, oracle fw 3.67).
Array storage is budgeted (10 240 elements total by default) to protect
small devices.

## The frame model

1. `beforeRender(delta)` — once per frame. Do your animation math here.
2. `render(index)` / `render2D(index, x, y)` / `render3D(…)` — once per
   pixel. Keep it cheap; it runs `pixelCount` times per frame.
3. Coordinate transforms (`translate`, `rotate`, `scale`, …) apply to the
   `x, y, z` values your mapped renderer receives. They compose in call
   order and **persist across frames** — call `resetTransform()` first if
   you rebuild the transform every frame (hardware-confirmed PB behavior).
   At most 31 ops stack; further calls are silently ignored until the next
   `resetTransform()` (hardware-confirmed PB behavior).
4. UI controls: `export function sliderSpeed(v) { … }` creates a slider
   named "Speed" that calls your function with 0..1 when moved. Prefixes:
   `slider`, `toggle`, `trigger`, `inputNumber`, `hsvPicker`, `rgbPicker`
   (inputs) and `showNumber`, `gauge` (readouts — return the value to
   display). Luxel extension: a `//# min=0 max=5 step=0.5 default=2`
   comment bounds the control in the UI (PB ignores it) — see [Control
   bounds](#control-bounds).

### Timing controls (Luxel extension)

Two builtins let a pattern shape its own timing. Both default to off, so
an untouched pattern behaves exactly as it always has. Call them from
top-level init (they take effect from the first frame) or from
`beforeRender` (from the next one).

- **`timeScale(s)`** — run the pattern-visible clock at `s` × real time.
  The engine scales each frame's delta before advancing the clock, so
  everything time-based follows together: `time()`, `beat()`/`beatSin()`,
  the clock `time_ms` used for Luxel-to-Luxel sync, and the `delta`
  handed to `beforeRender`. `timeScale(0.25)` is slow-motion for reading
  a fast effect, `timeScale(0)` freezes the animation while still
  rendering every frame (interaction and controls keep working),
  `timeScale(2)` doubles the speed. Negative values clamp to 0 — the
  clock never runs backwards. Returns the previous scale.
- **`setFrameRate(fps)`** — cap how often the pattern is *evaluated*. The
  engine holds the previous frame — same pixels, no pattern code run —
  until `1000/fps` ms of **real** time have passed, then runs
  `beforeRender` with the whole accumulated interval as its delta, so
  delta-driven motion lands in exactly the same place as it would
  uncapped. Deliberately chunky, hand-animated looks (`setFrameRate(8)`)
  and CPU headroom on a heavy pattern. `setFrameRate(0)` removes the cap;
  returns the previous cap (0 = uncapped). The period is clamped to 60 s,
  so a pathological `setFrameRate(0.0001)` stalls for a minute, not an
  hour.

The cap is enforced **inside the engine**, so it behaves identically on
every host — firmware, playground, CLI — rather than depending on each
render loop. Two consequences worth knowing:

- The host's output stage is untouched. The firmware still pushes pixels
  to the LEDs (and the playground still paints the preview) at its own
  cadence, re-sending the held frame; `setFrameRate` throttles pattern
  work, not the wire. The `fps` figure in the status bar and the HA
  diagnostic sensor is that host loop rate, so it does **not** drop to
  the cap you set.
- The clock keeps running while frames are held, so a sync follower's
  `time_ms` stays continuous; a clock jump from sync convergence is not
  reported to the pattern as elapsed delta. `timeScale` is a local
  authoring/debug control, though — a device that scales its clock will
  drift from its sync group and keep getting yanked back.

## Builtin reference

Arguments and results are 16.16 numbers; angles are radians; `t` values
are typically 0..1. Missing arguments default to 0 (Luxel is lenient
where PB compile-errors on arity).

### Math

| builtin | notes |
|---|---|
| `abs(x)` `floor(x)` `ceil(x)` `round(x)` `trunc(x)` `frac(x)` | `round(x)` = `floor(x + 0.5)`; `frac` truncates toward zero (`frac(-0.25)` = `-0.25`) — for an always-positive wrap use Euclidean `mod(x, 1)` |
| `clamp(x, lo, hi)` `min(a, b)` `max(a, b)` | |
| `mod(x, y)` | floored modulo (sign of `y`); `%` is truncated (sign of `x`) |
| `sqrt(x)` | sign-preserving |
| `sin cos tan asin acos atan` `atan2(y, x)` | radians |
| `pow(b, e)` `exp(x)` `log(x)` `log2(x)` | negative bases: integer exponents follow the sign rule, fractional yield −32768 (raw MIN, PB-exact); overflow saturates to ±32768, never wraps (PB-exact — the one non-wrapping corner of the arithmetic) |
| `hypot(a, b)` `hypot3(a, b, c)` | wraps like all arithmetic |

### Waveforms, time, randomness

| builtin | notes |
|---|---|
| `time(interval)` | 0..1 sawtooth with period `interval` × 65.536 s |
| `wave(v)` | `(1 + sin(v·2π)) / 2` |
| `square(v, duty)` `triangle(v)` | unit-period waveforms |
| `timeScale(s)` `setFrameRate(fps)` | Luxel extensions — see [Timing controls](#timing-controls-luxel-extension) |
| `random(max)` | true random [0, max) |
| `prng(max)` `prngSeed(seed)` | seedable PRNG; Luxel's sequence differs from PB's (see divergences) |
| `randomSeed(seed)` | Luxel extension — pins `random()`'s stream (see below) |

#### Determinism and seeding (Luxel extension)

Both generators are **pinned**: the algorithms below are part of Luxel's
contract, asserted by `crates/luxel-core/tests/semantics.rs`, so a seeded
pattern produces the identical sequence on every Luxel build — ESP32
firmware, playground WASM, CLI. That is what synced installations need:
several devices running the same pattern from the same seed pick the same
"random" sparkles.

| | `random()` | `prng()` |
|---|---|---|
| algorithm | splitmix64 (γ = `0x9E3779B97F4A7C15`, the two standard mix constants), output = the low 32 bits | xorshift32, Marsaglia 13/17/5, output = the whole state |
| seeded by | `randomSeed(s)` | `prngSeed(s)` |
| state ← seed | the seed's raw 16.16 word, zero-extended (splitmix is counter-based, so a low-entropy seed is fine) | the seed's raw 16.16 word; `0` is remapped to `1` (xorshift32's fixed point) |
| returns | the **previous seed** (0 if never seeded) | the **previous state** — 32 bits, so it round-trips: save it, draw, restore, and the same draws come back |
| unseeded | host seed at pattern start — differs per device | host seed at pattern start |

Both are scaled to the requested range the same way: `(r · max) >> 32`
with `max`'s raw 16.16 word taken **unsigned**, so a positive `max` gives
plain `[0, max)` while a *negative* `max` — most often via a wrapped
literal like `random(0xffff)`, since `0xffff` is `-1.0` in 16.16 — draws
over the **whole signed range** (≈ ±32768). That is PB-exact
(oracle-verified, fw 3.67), and community patterns rely on it to seed
hand-rolled PRNGs with full-width values. Fractional seeds are distinct
states (`randomSeed(1)` and
`randomSeed(1.5)` start different streams). The two streams are
independent — seeding one never disturbs the other.

```js
randomSeed(1234)                 // every device in the installation agrees
randomSeed(1234 + clockDay())    // …and re-rolls once a day, together
```

### Interpolation

`mix(a, b, t)`, `smoothstep(lo, hi, x)`, `bezierQuadratic(t, p0, p1, p2)`,
`bezierCubic(t, p0, p1, p2, p3)`.

**Luxel extensions** (not in PB): `map(x, inLo, inHi, outLo, outHi)`,
`sign(x)`, `step(edge, x)`, `saturate(x)` (= clamp to 0..1),
`dist(x1,y1,x2,y2)`, `dist3(x1,y1,z1,x2,y2,z2)`, and easing curves on 0..1:
`easeInQuad`/`easeOutQuad`/`easeInOutQuad`, the `…Cubic` trio, and the
springy set — `easeOutBack(t)` (overshoots ~10% and settles),
`easeOutElastic(t)` (decaying spring wobble), `easeOutBounce(t)` (dropped
ball). The springy ones exceed 0..1 mid-curve by design; saturate() the
result if you're feeding a color channel directly.

**Easings, in full**: the standard thirty are all builtins — ten families,
each with an `easeIn…`, `easeOut…`, and `easeInOut…` form, matching the
public easings.net reference curves: `…Sine`, `…Quad`, `…Cubic`, `…Quart`,
`…Quint`, `…Expo`, `…Circ`, `…Back`, `…Elastic`, `…Bounce` (so
`easeInOutQuint(t)`, `easeInBounce(t)`, and so on). Steeper family = more
of the motion crammed into the tail. `…Expo`, `…Elastic`, and `…InOutExpo`
pin their endpoints exactly to 0 and 1; everything else is the plain
polynomial/analytic form with no clamping, so feeding t outside 0..1
extrapolates (the same contract as `smoothstep`). `…Back` anticipates below
0 before it starts and `…Elastic` winds up the same way — that's the point
of them, but saturate() before a color channel.

**Luxel extensions — tempo & hashing**: `beat(bpm)` is a 0..1 sawtooth
beat phase at `bpm` on the engine clock; `beatSin(bpm, lo = 0, hi = 1)`
oscillates sinusoidally between `lo` and `hi` at `bpm` (FastLED-style —
music-synced-feeling motion without audio hardware). `hash(x)` and
`hash2(x, y)` return a deterministic value in [0, 1) from their inputs —
stable per-pixel randomness (sparkle that doesn't reshuffle each frame:
`hash(index)` is constant per pixel, `hash2(index, floor(t))` re-rolls
once per tick). Same input → same output on every device, pinned by test.

**Luxel extensions — vectors**: `dot(x1,y1, x2,y2)`, `dot3(x1,y1,z1,
x2,y2,z2)`, and `angleBetween(x1,y1, x2,y2)` — the signed angle from
vector 1 to vector 2 in radians (counter-clockwise positive, like `atan2`).

### Color

`hsv(h, s, v)` (hue wraps), `hsv24`, `rgb(r, g, b)`; palette:
`setPalette([pos, r, g, b, …])` then `paint(t, brightness)`. With no
palette installed `paint(t)` is a grayscale ramp. `setPalette(arr)`
holds a LIVE reference (hardware-confirmed): later writes through `arr`
change what `paint()` renders with no second call — but install the
palette ONCE, not per frame, or the array literal allocates every frame
and exhausts the element budget (arrays are never freed). Outside the stops the
ends are asymmetric (hardware-confirmed PB behavior): below the first stop
clamps to the first color, past the last stop paints **black** — end a
palette at position 1.0 unless you want the cutoff.

**Luxel extension** — perceptual color: `oklch(l, c, h)` sets the pixel from
OKLCH (lightness 0..1, chroma ~0..0.4, hue in turns like `hsv`), and
`oklab(l, a, b)` from OKLab. Gradients and fades through these look far
smoother than HSV — even brightness across hues, no dark band through blue.

**Luxel extensions — value-returning color**: because functions return one
number, the conversion forms write into a caller-provided array (first
three slots) and return it — reuse one array so render loops don't grow
the arena. `hsv2rgb(h, s, v, out)` → `[r, g, b]`; `rgb2hsv(r, g, b, out)`
→ `[h, s, v]` with hue in turns; `mixColors(r1,g1,b1, r2,g2,b2, t, out)`
blends two RGB colors **in OKLab** — perceptually even, no muddy midpoints.

```js
var c = array(3)
export function render(index) {
  mixColors(1, 0, 0, 0, 0, 1, index / pixelCount, c)  // red → blue, evenly
  rgb(c[0], c[1], c[2])
}
```

**Luxel extension — the post-process chain**: four whole-frame stages the
engine runs *once per frame*, after the last `render` call, in a fixed
order:

```
setOutputPalette  →  setBlur  →  setGlow  →  setGamma
```

Recolor first (the remap works on the pattern's own luma), then spread
light spatially, then apply the output transfer curve last — the order a
display pipeline uses. Every stage is off by default and costs one
comparison per frame when unset, so a pattern that never calls them
renders exactly as it always did. Call them anywhere (top level,
`beforeRender`, or `render`); the value in effect when the frame finishes
is the one that applies.

- `setGamma(g)` — output gamma curve (a cached 256-entry table, so no
  per-pixel `pow`). LEDs are linear but eyes aren't; `setGamma(2.2)`
  makes fades and dim colors perceptually even. `setGamma(0)` or
  `setGamma(1)` turns it off. Full brightness always stays full.
- `setBlur(amount, passes = 1)` — 3-tap blur **along the pixel index**.
  `amount` 0..1 is each neighbour's share: `0.5` is the classic 1-2-1
  kernel, `1` is a pure neighbour average. `passes` (1..8) re-runs the
  kernel to widen the radius. Ends clamp, so light that reaches the last
  pixel stays on the strip rather than falling off it. Returns the
  previous amount. Allocation-free — it remembers exactly one pixel.
- `setGlow(amount)` — light-bleed bloom: every pixel takes the brighter
  of itself and `amount` of its brightest neighbour. Unlike blur, the
  source keeps its full value, so highlights spread without the frame
  going dim. Returns the previous amount.
- `setOutputPalette(pal, amount = 1)` — recolor the finished frame by
  luma through `pal`, a stop list in `setPalette`'s flat
  `[pos, r, g, b, …]` form. The pattern's *structure* survives and its
  hues are replaced, which is how you put one installation's palette
  over any pattern. `amount` blends against the original; passing a
  non-array (e.g. `setOutputPalette(0)`) turns the stage off. Unlike
  `setPalette`, the stops are snapshotted at the call — the engine cooks
  a 256-entry table and rebuilds it only when you install a new one.

  There is a *device-level* palette too — Settings → Output, persisted in
  flash, `POST /api/output/palette` (Gitea #139). The two compose: the
  pattern's stage recolors the frame, then the device's stage recolors the
  result, the same way device blur stacks on pattern blur. A pattern that
  wants the device's look untouched simply doesn't call
  `setOutputPalette`.

Map space when the map is a grid, index space otherwise. With a 2D map
installed that reads as a regular W×H matrix — row-major or serpentine,
which is what the playground's grid map and the usual device maps produce
— `setBlur`/`setGlow` spread in *two dimensions*: one sweep along the
rows, one down the columns, so a spike becomes a soft disc and nothing
folds back at a row end. The device-level blur/glow settings
(`/api/output`) follow the same rule.

Everywhere else — a strip, a 3D map, a ring or any other layout that isn't
a grid, or no map at all — they follow the pixel index, which on a strip is
physical order and on an unrecognized matrix is the wiring path. (A pattern
that renders only in 2D falls back to the engine's default √n grid map, so
when the pixel count is a perfect square its output blurs in the same
geometry it was drawn in.) For softening a layout the chain can't
recognize, use `blur2D` over a canvas array inside the pattern.

```js
// any pattern, recolored and softened at the output
var pal = array(12)
pal[0] = 0   pal[1] = 0    pal[2] = 0    pal[3] = 0.2   // deep blue
pal[4] = 0.5 pal[5] = 0.9  pal[6] = 0.1  pal[7] = 0.4   // magenta
pal[8] = 1   pal[9] = 1    pal[10] = 0.9 pal[11] = 0.6  // warm white

export function beforeRender(delta) {
  setOutputPalette(pal)
  setBlur(0.35)
  setGlow(0.4)
  setGamma(2.2)
}
```

The same three stages exist as **device settings** (Settings → Output:
blur %, glow %, and a brightness curve), applied by the firmware after
the engine's chain so an installation can dial them in without editing
patterns. The device's brightness curve is a separate knob from
`setGamma`: gamma shapes every pixel's channels (content), the brightness
curve shapes only how the master dimmer responds (control).

### Arrays

`array(n)`, `arrayLength`, `arraySum`, `arrayForEach(a, fn)`,
`arrayMutate(a, fn)`, `arrayMapTo(src, dst, fn)`,
`arrayReduce(a, fn, init)`, `arrayReplace(a, v1, v2, …)`,
`arrayReplaceAt(a, i, v)`, `arraySort(a)`, `arraySortBy(a, cmp)`.

Both `replace` forms take any number of values and store them from the
offset onwards (`arrayReplace` starts at 0; `a.replace(…)` is that form).
Neither is a fill: `arrayReplace(a, 0)` writes only `a[0]`. Zero a whole
buffer with `feedback(a, 0)`.
The span is checked as a whole: if `offset + count` runs past the end it is
a runtime error and **nothing** is written, not even the elements that
would have fit. A negative offset is not an error — the splat shifts down
and only the values landing at a valid index are stored. Both PB-matched
(oracle fw 3.67).

**Luxel extensions**: `blur1D(arr, radius)` box-blurs the array in place
(window `2·radius + 1`, edges clamped) and returns it; `feedback(arr,
decay)` multiplies every element by `decay` in place — the trails/glow
decay loop as one call. Both are the ubiquitous hand-rolled patterns
(KITT's decay, fire's cooling blur) as builtins.

**Luxel extensions — bulk array math**: element-wise in-place loops as
single VM calls (the big FPS lever for grid simulations, which otherwise
spend ~15 interpreted ops per cell per frame). `arrayAdd(dst, src)` /
`arraySub(dst, src)` do `dst[i] ±= src[i]` over the shorter of the two
lengths (extra `dst` elements are untouched); `arrayScale(arr, k)`
multiplies every element by `k` (alias of `feedback`); `arrayMix(dst,
src, t)` does `dst[i] += (src[i] − dst[i])·t` — an unclamped lerp like
`mix`, so `t = 1` copies `src` into `dst`. All return `dst` for
chaining, and `src` is never written.

**Luxel extensions — 2D canvases**: helpers for the row-major
virtual-canvas idiom (draw into a small `array(w * h)` buffer in
`beforeRender`, sample it in `render2D`).

- `blur2D(arr, w, h, radius)` — in-place separable box blur over the
  first `w × h` elements (window `2·radius + 1` per axis, edges clamped
  exactly like `blur1D`); returns the array. Diffusion/soften loops —
  the hottest loop in buffer-based 2D patterns — as one call. An array
  shorter than `w × h` is a runtime error; `w`, `h`, or `radius` < 1 is
  a no-op.
- `canvasSet(buf, w, x, y, v)` — write `v` at the cell under normalized
  `(x, y)` (cell = `floor(x·w)`, clamped to the edges: `x = 1` lands in
  the last column, out-of-range coordinates clamp instead of aborting
  the frame). Returns `v`. Replaces the manual
  `buf[floor(y * 15.99) * w + floor(x * 15.99)]` footgun.
- `canvasGet(buf, w, x, y)` — **bilinear** sample at normalized
  `(x, y)`. Texel centers sit at `(i + 0.5)/w`, so a read at a cell's
  center returns exactly what `canvasSet` stored there; between centers
  it blends the four neighbors (edges clamp). Canvas patterns get
  smooth upscaling on larger maps for free; the canvas height is
  `arrayLength(buf) / w`.
- `canvasAdd(buf, w, x, y, v)` — the accumulate variant: `cell += v` at
  exactly the cell `canvasSet` would write (same edge-clamped
  `floor(x·w)` addressing, same runtime error on a non-array). Particle
  deposits, heatmaps and splat accumulation without a manual
  read-modify-write. Returns the cell's **new** value, like `+=` in JS,
  so you can react to a cell saturating; negative `v` subtracts.

```js
// deposit N particles into a 16×16 canvas, then diffuse
for (i = 0; i < n; i++) canvasAdd(buf, 16, px[i], py[i], 0.2)
blur2D(buf, 16, 16, 1)
feedback(buf, 0.92)
```

### Noise

`perlin(x, y, z, seed)` and the fractal variants `perlinFbm(x, y, z,
lacunarity, gain, octaves)`, `perlinRidge(x, y, z, lacunarity, gain,
offset, octaves)`, `perlinTurbulence(x, y, z, lacunarity, gain,
octaves)`; `setPerlinWrap(x, y, z)` makes the lattice tile (2..256 per
axis, 256 by default) and applies to all four.

These are **bit-compatible with Pixel Blaze** — fitted from captured
oracle sweeps and reproduced exactly (see
docs/research/04-oracle-findings.md). Practical consequences:

- `perlin` returns roughly [-1, 1]; the fractal variants are **not**
  normalized, so `perlinFbm` with `gain` 0.5 and 3 octaves spans about
  ±1.75 and `perlinRidge` is non-negative and can exceed 1. Scale to
  taste rather than assuming [0, 1].
- `seed` picks one of **256** fields; it truncates to an integer and
  wraps mod 256, so `perlin(x, y, z, 5)` and `perlin(x, y, z, 261)` are
  the same field.
- `octaves` truncates toward zero; ≤ 0 yields 0, and the loop is capped
  at 32 so a runaway argument can't stall a frame.
- Each octave of the fractal variants uses a *different* field (the
  octave index is its seed), so layers never share lattice lines.

**Luxel extensions**: `simplex2(x, y, seed = 0)` and `simplex3(x, y, z,
seed = 0)` — simplex noise in roughly [-1, 1]: smoother than perlin with
no axis-aligned artifacts (the classic choice for organic motion; feed
`time(...)` into one axis). The simplex lattice does not wrap —
`setPerlinWrap` doesn't apply.

### Mapped coordinates

`resetTransform`, `translate(x, y)`, `scale(x, y)`, `rotate(θ)`,
`translate3D`, `scale3D`, `rotateX/Y/Z`, full `transform(…)`;
`pixelMapDimensions()`, `has2DMap()`, `has3DMap()`;
`mapPixels(fn(index, x, y, z))` iterates every pixel with coordinates.

### Device & environment

GPIO (`pinMode`, `digitalWrite`, `digitalRead`, `analogRead`,
`touchRead`) — stubs until a board wires them, with one exception:
`digitalRead` reports the pin's *idle* level per the last `pinMode`, so
an `INPUT_PULLUP` pin reads `HIGH` and everything else reads `LOW`. A
button-to-ground pattern therefore idles "not pressed" instead of "held
forever". A host can also DRIVE a digital input from outside the pattern
(`lx_set_pin` in the wasm ABI, `Engine::set_pin` natively, `POST
/api/pins` on the CLI mirror, `pins` in `tools/verify/fixups.json`):
levels are held until released, so a button pattern can be pressed
deterministically without hardware. In the playground that surface is
the **Pins** panel, which appears only for patterns that actually name a
pin (`lx_pins_used` reports the mask, since pin numbers are runtime
values) and offers a momentary press plus a latch per pin.
`analogRead`/`touchRead` still read 0 unconditionally, and nothing
drives a real pad yet.
Clock (`clockYear` …
`clockWeekday`) — needs wall time from the host; every host supplies it
at engine construction too, so top-level reads see real time-of-day
(PB patterns do this — the device RTC is set by pattern-load time). No
time source reads as 0. Sequencer/playlist
(`sequencerNext`, `playlistGetPosition`, …) and `nodeId()`.

### Sensor bindings

`export var` any of `frequencyData` (32 bins, ~37 Hz–10 kHz, 0..1),
`energyAverage`, `maxFrequencyMagnitude` (0..1), `maxFrequency` (Hz),
`light`, `accelerometer` (3), `analogInputs` (5) — the PB sensor-board
surface. Bound vars are zero-filled when no source is attached (sound
patterns run dark, not error). Sources: the playground's **sound** toggle
(browser microphone → WebAudio FFT), and on hardware the PB sensor
expansion board's serial stream. Updates land between frames (~40 Hz on
real hardware).

### External events (Luxel extension)

Injected events carry `[type, x, y, value]` — the generic surface for
keypress-reactive patterns (QMK splash/nexus/heatmap style), MQTT/HA
bridges, and anything else that pokes a pattern from outside:

- `eventCount()` — events waiting to be read.
- `readEvent(out)` — pop the oldest event into `out[0..4]` and return 1;
  returns 0 (out untouched) when none are queued. `out` must be an array
  of length ≥ 4. Drain per frame with `while (readEvent(ev)) { … }`.

Sources: **click or drag the preview** (type 1, `x`/`y` normalized 0..1,
value 1 — 1D previews send `y = 0`); `POST /api/events` with an
`EV1\0` frame (magic + u8 count + count × 4×i32-LE raw 16.16; see
`luxel_core::netin`); or **MQTT** — publish to `luxel/<id>/event`, one
event per line as whitespace-separated decimals `type [x [y [value]]]`
(missing x/y default to 0, value to 1, so an HA automation can send just
`"1"` — topic reference + an HA automation example in
[mqtt.md](mqtt.md)). The queue holds 32 events, dropping the OLDEST
when full, and events land between frames. `type`/`value` meanings
beyond the pointer convention are yours to define.

### Predefined globals

`pixelCount`; math constants `PI PI2 PI3_4 PISQ E SQRT2 SQRT1_2 LN2 LN10
LOG2E LOG10E`; GPIO constants `LOW HIGH INPUT OUTPUT INPUT_PULLUP
INPUT_PULLDOWN OUTPUT_OPEN_DRAIN ANALOG`; `null`/`undefined` (= 0).

## Known divergences from Pixel Blaze

Bit-exactness with real PB hardware is verified by a differential test
battery (docs/research/04-oracle-findings.md has the full story). Where we
differ, it is deliberate:

- **Transcendentals** (`sin`, `atan`, `asin`, `acos`, `exp`, `log`,
  `sqrt`): Luxel's implementations are equal or closer to true math at
  every point measured. PB's sin table has an off-by-one seam near π
  (0.6% error); PB's `asin(±1)`/`acos(±1)` return the wrong endpoint.
  Differences are invisible in LED output.
- **`prng` sequences** differ (PB's generator is unidentified float-based
  state machine; Luxel uses xorshift32). Seeded determinism holds within
  each platform, not across them — Luxel's algorithms are pinned by test
  and documented under [Determinism and
  seeding](#determinism-and-seeding-luxel-extension), which PB does not
  promise. `randomSeed` is a Luxel-only builtin.
- **Array writes with fractional literal indices**: PB aborts the frame;
  Luxel truncates (consistent with PB's own variable-index behavior).
- **Builtin arity is not compile-checked** — missing arguments are 0
  (PB rejects e.g. one-arg `square(x)` at compile time; we default duty).
- **Builtins are first-class values** in Luxel (`f = floor` works); PB
  rejects referencing a builtin without calling it ("Undefined symbol").
- `//#` control-bound comments are a Luxel extension; PB ignores them
  (see [Control bounds](#control-bounds)).

## Luxel extensions

These are accepted by Luxel but not by Pixel Blaze. They never break
PB-source compatibility — every valid PB pattern stays valid.

- **`let`** — declares a variable, currently identical to `var`
  (function-scoped; true block scoping is a future refinement). Use it
  wherever you'd write `var`.
- **`const`** — like `let`, but must have an initializer and cannot be
  reassigned; a later assignment is a compile error
  (`cannot assign to \`x\` — it is declared const`). Const-ness is scoped:
  a local `const` doesn't lock a same-named global.

```js
const TAU = PI2          // reassigning TAU later is an error
let speed = 1            // fine to reassign
speed = 2
```

- **`**` (exponent)** — `x ** 2` = `pow(x, 2)`. Right-associative
  (`2 ** 3 ** 2` = 512), binds tighter than `*`. One divergence from JS:
  a unary minus on the left is allowed and binds first (`-x ** 2` means
  `(-x) ** 2`; JS makes it a syntax error). The compound form `x **= 2`
  comes with it, on variables and array elements alike.
- <a id="switch-luxel-extension"></a>**`switch`** — JS semantics, no
  surprises:

```js
switch (mode) {
  case 0:
    h = 0
    break
  case 1:
  case 2:            // empty label ⇒ falls into the next body
    h = 0.5
    break
  default:
    h = time(.1)
}
```

  The discriminant is evaluated **once**, then labels are compared
  against it with the language's `==` in **source order** until one
  matches (`===` is the same operator here — there is one value domain).
  A label can be any expression, and only the labels up to the match are
  evaluated. Control then falls through from body to body until a
  `break` or the end of the switch — including *into and out of*
  `default`, which may sit anywhere among the cases and is simply the
  target when nothing matched. At most one `default` per switch.

  `break` inside a switch leaves the **switch**, not an enclosing loop;
  `continue` skips past the switch to the enclosing loop, as in JS.
  `var` in an arm hoists to the function scope like anywhere else (there
  is no block scoping yet), and `return` out of an arm works normally.

  `switch` needs no new bytecode: it compiles to the same
  compare-and-jump instructions `if` uses, so the cost is one comparison
  per label tested rather than a jump table. For a hot per-pixel
  dispatch over many modes, an array of lambdas indexed by the mode is
  still faster.

- **`assert(cond[, "message"])` — configuration invariants.** A pattern
  can declare what it needs from the rig; if the condition is falsy the
  pattern refuses to run (black output + a clear error carrying the
  message) instead of misbehaving or crashing mid-render:

```js
assert(pixelCount % 2 == 0)

var w = sqrt(pixelCount)
assert(floor(w) == w, "needs a square number of pixels")
assert(pixelCount >= 100, "needs at least 100 pixels")
```

  `assert` is real init code, executed **inline where it appears** in
  top-level initialization — so the condition can use anything set up
  above it: derived vars, function calls, array contents. A failed
  assert aborts init on the spot (statements above it ran; statements
  below it don't) and blocks rendering with
  `pattern requires: <message> (pixelCount = N)`. Without a custom
  message, the condition's source text is used. Changing the device's
  pixel count (or any config that rebuilds the engine) re-runs init and
  therefore re-checks every assert automatically.

  Restrictions: `assert` is only legal as a **top-level statement** —
  not inside functions (it would fire per frame), not nested in blocks
  or branches (a conditional invariant isn't an invariant). The quoted
  message is the language's only string literal and exists only there.
  A runtime error *inside* the condition (e.g. indexing out of bounds)
  is an ordinary vmerr, not a violation.

  Like `switch` and `**`, using it makes the pattern Luxel-only by
  choice: a real Pixel Blaze has no `assert` (its compiler rejects
  `switch` outright — "Unsupported type SwitchStatement" — and `**` as
  well; oracle-probed 2026-08-30). The compatibility guarantee runs the
  other way: every valid PB pattern still compiles here.

More conservative JS conveniences may follow (see the roadmap).

### Control bounds

A `//#` comment attached to an exported control function bounds that
control in the UI. Pixel Blaze parses it as an ordinary comment and
ignores it — its sliders always send 0..1 — so a pattern carrying these
directives stays valid PB source.

Keys: `min`, `max`, `step`, `default` (all optional, all plain numbers;
negative and fractional values are fine). Anything the parser doesn't
recognize is ignored. Without a directive a slider is 0..1.

**Both placements work and mean the same thing** — trailing on the
export line, or on a line of its own directly above the export:

```js
// trailing
export function sliderSpeed(v) { speed = v }      //# min=0 max=5 step=0.5 default=2
export function toggleMirror(on) { mirror = on }  //# default=1

// own line
//# min=1 max=100 step=1 default=10
export function inputNumberCount(v) { count = v }
```

The own-line form is what most of `library/` uses; it reads better when
the function body is long. If a control carries both, the directives are
merged and the own-line one wins on any key they share. The own-line
directive must sit immediately above the `export` — a blank line or an
intervening comment between them breaks the association.

The bounds are UI-only: the engine receives whatever value the control
sends, and `default` is where the UI starts the control, not an
initializer for your pattern's variables. Give the variable a sensible
top-level value too, so the pattern looks right before anyone touches a
control.

Without a `default=`, the playground has no way to know that top-level
value (a control's live value can't be read back from the engine —
calling the handler would overwrite it), so it draws the untouched
control dimmed with a `?` badge to say the position shown is a
placeholder, not the running value. Toggles render indeterminate. The
control becomes normal on first interaction. Declaring a `default=`
avoids the placeholder state entirely.
