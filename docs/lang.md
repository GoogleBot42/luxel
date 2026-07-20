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

Supported: `if`/`else`, `while`, `for(;;)`, `break`, `continue`,
`return`, blocks, and expression statements. Semicolons are optional
(JS automatic semicolon insertion rules). Comments: `//` and `/* */`.

Expressions: arithmetic `+ - * / %`, comparisons `< <= > >= == !=`
(`===`/`!==` are accepted and identical to `==`/`!=`), logical `&& || !`
(short-circuiting, returning the deciding value like JS), bitwise
`& | ^ ~ << >>` (operating on the raw 16.16 bits — `1 >> 16` is the
smallest positive value; `~` additionally clears the result's fraction
bits), ternary `?:`, and compound assignment `+= -= *= /= %=` etc.

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
that stops the current frame's entry point — the engine reports it with
the source location and keeps rendering. Array storage is budgeted
(10 240 elements total by default) to protect small devices.

## The frame model

1. `beforeRender(delta)` — once per frame. Do your animation math here.
2. `render(index)` / `render2D(index, x, y)` / `render3D(…)` — once per
   pixel. Keep it cheap; it runs `pixelCount` times per frame.
3. Coordinate transforms (`translate`, `rotate`, `scale`, …) apply to the
   `x, y, z` values your mapped renderer receives. They compose in call
   order and **persist across frames** — call `resetTransform()` first if
   you rebuild the transform every frame (hardware-confirmed PB behavior).
4. UI controls: `export function sliderSpeed(v) { … }` creates a slider
   named "Speed" that calls your function with 0..1 when moved. Prefixes:
   `slider`, `toggle`, `trigger`, `inputNumber`, `hsvPicker`, `rgbPicker`
   (inputs) and `showNumber`, `gauge` (readouts — return the value to
   display). Luxel extension: a `//# min=0 max=5 step=0.5 default=2`
   comment on the line above bounds the control in the UI (PB ignores it).

## Builtin reference

Arguments and results are 16.16 numbers; angles are radians; `t` values
are typically 0..1. Missing arguments default to 0 (Luxel is lenient
where PB compile-errors on arity).

### Math

| builtin | notes |
|---|---|
| `abs(x)` `floor(x)` `ceil(x)` `round(x)` `trunc(x)` `frac(x)` | `round(x)` = `floor(x + 0.5)` |
| `clamp(x, lo, hi)` `min(a, b)` `max(a, b)` | |
| `mod(x, y)` | floored modulo (sign of `y`); `%` is truncated (sign of `x`) |
| `sqrt(x)` | sign-preserving |
| `sin cos tan asin acos atan` `atan2(y, x)` | radians |
| `pow(b, e)` `exp(x)` `log(x)` `log2(x)` | negative bases: integer exponents follow the sign rule, fractional yield 0 |
| `hypot(a, b)` `hypot3(a, b, c)` | wraps like all arithmetic |

### Waveforms, time, randomness

| builtin | notes |
|---|---|
| `time(interval)` | 0..1 sawtooth with period `interval` × 65.536 s |
| `wave(v)` | `(1 + sin(v·2π)) / 2` |
| `square(v, duty)` `triangle(v)` | unit-period waveforms |
| `random(max)` | true random [0, max) |
| `prng(max)` `prngSeed(seed)` | seedable PRNG; Luxel's sequence differs from PB's (see divergences) |

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
`setPalette([pos, r, g, b, …])` then `paint(t, brightness)`.

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

**Luxel extension — output gamma**: `setGamma(g)` applies an output gamma
curve after `render` (a cached 256-entry table — no per-pixel cost). LEDs
are linear but eyes aren't; `setGamma(2.2)` makes fades and dim colors
perceptually even. `setGamma(0)` or `setGamma(1)` turns it off. Full
brightness always stays full.

### Arrays

`array(n)`, `arrayLength`, `arraySum`, `arrayForEach(a, fn)`,
`arrayMutate(a, fn)`, `arrayMapTo(src, dst, fn)`,
`arrayReduce(a, fn, init)`, `arrayReplace(a, v)`,
`arrayReplaceAt(a, i, v)`, `arraySort(a)`, `arraySortBy(a, cmp)`.

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

### Noise

`perlin(x, y, z, seed)` and the fractal variants `perlinFbm`,
`perlinRidge`, `perlinTurbulence`; `setPerlinWrap(x, y, z)` makes the
lattice tile.

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
`touchRead`) — stubs until a board wires them. Clock (`clockYear` …
`clockWeekday`) — needs wall time from the host. Sequencer/playlist
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
  each platform, not across them.
- **Array writes with fractional literal indices**: PB aborts the frame;
  Luxel truncates (consistent with PB's own variable-index behavior).
- **Builtin arity is not compile-checked** — missing arguments are 0
  (PB rejects e.g. one-arg `square(x)` at compile time; we default duty).
- **Builtins are first-class values** in Luxel (`f = floor` works); PB
  rejects referencing a builtin without calling it ("Undefined symbol").
- `//#` control-bound comments are a Luxel extension; PB ignores them.

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
  `(-x) ** 2`; JS makes it a syntax error).

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

  This is the one extension that breaks PB-source compatibility when
  used: a real Pixel Blaze has no `assert`, so patterns using it are
  Luxel-only by choice.

More conservative JS conveniences may follow (see the roadmap).
