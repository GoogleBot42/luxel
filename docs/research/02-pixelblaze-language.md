# Research: Pixel Blaze pattern language & engine internals

> Raw research (July 2026). Primary sources: official language reference
> (github.com/simap/pixelblaze README.expressions.md — same text as
> electromage.com/docs/language-reference), Ben Hencke's forum posts (cited by topic/post),
> zranger1/pixelblaze-client source, simap peripheral repos, jvyduna/pb-examples.

## 1. Language semantics

JS-like ("a JavaScript-like subset of ES6"), compiled to bytecode for a stack VM.
One scalar value domain: **32-bit signed 16.16 fixed point** (plus arrays and function
references as distinct object kinds).

### Number format
- Range ±32,768, resolution 1/65,536; two's complement across all 32 bits (value = int32/65536).
- **Overflow wraps** (two's complement), not saturates. Classic footgun: `x*x` for x ≥ ~181 wraps.
- Products of small fractions quantize to 0 (0.001 × 0.001 = 0, since 1e-6 < 1/65536).
- Bytecode **literals are 31-bit (16.15)** — the LSB of the word is an instruction flag, so
  an immediate constant loses the least-significant fraction bit; full 32-bit values must be
  composed at runtime (Hencke, forum topic 1597 post 14).
- Truthiness: 0 false, nonzero true; `true`/`false` are 1/0.

### Operators
`= + - ! * / % >> << | & ~ ^ > < >= <= == != || && ?:`
- **Bitwise ops act on the full 32-bit fixed-point word** (16 int + 16 fraction bits) —
  unlike JS. Exception: `~` zeros the lower 16 bits. So `x | 0` does NOT truncate;
  `x << 1` is exactly `x * 2` including fraction.
- `||`/`&&` return operand values like JS (`0 || 42` → 42).
- `%` is truncated remainder (sign of dividend): `-3.5 % 3 == -0.5`. The `mod(x,y)` builtin
  is floored (sign of divisor): `mod(-3.5, 3) == 2.5`.
- `++`, `--`, compound assignment work in practice (used in official example patterns).

### Variables & scope
- `var x` in function → local (can shadow); `var` at top level → global.
- **Implicit assignment always creates a global**, even inside a function.
- `export var x` → visible in Var Watcher and via websocket getVars/setVars (arrays element-wise).
  Snapshots taken after the last pixel of a frame.
- Predefined global: `pixelCount` (available in top-level init code).
- No `let`/`const`. Constants: `E PI PI2 PI3_4 PISQ LN2 LN10 LOG2E LOG10E SQRT1_2 SQRT2`.

### Functions
- `function f(a) {…}` or lambdas `f = (a) => a * 2` (expression or block body).
- First-class: storable in vars/arrays, passable, returnable. **No closures** (inner
  functions see only globals + own params). Dispatch-table idiom replaces `switch`:
  `modes[currentMode]()`.

### Control flow
`if/else`, `while`, `for`, `break`, `continue`. No `switch`, no objects/classes, no runtime
strings, no GC (arrays are never freed; re-assigning an array literal orphans the old one).

### Arrays
- `array(n)` (zero-filled) or literals `[a,b,c]` (v3.20+, self-allocating). Reference
  semantics, nestable. `.length` is a property, not a call.
- API (global-fn and method forms): `arrayForEach/forEach(fn(v,i,a))`, `arrayLength/length`,
  `arrayMapTo/mapTo(dest,fn)`, `arrayMutate/mutate(fn)` (in-place map),
  `arrayReduce/reduce(fn,init)`, `arrayReplace/replace(…)`, `arrayReplaceAt/replace(off,…)`,
  `arraySort/sort()`, `arraySortBy/sortBy(cmp)` (not stable), `arraySum/sum()`.

## 2. Builtin API

### Entry points (engine calls these exports)
- `beforeRender(delta)` — once per frame; delta = fractional ms (claimed 6.25 ns resolution).
- `render(index)` — per pixel, 1D. Current firmware also passes `render(index, x)` with
  normalized 1D coord (defaults to index/pixelCount without a map).
- `render2D(index, x, y)`, `render3D(index, x, y, z)` — world units 0..1 (exclusive).
- Selection priority by installed map: no/1D map → render, render3D, render2D;
  2D map → render2D, render3D, render; 3D map → render3D, render2D, render.
  Missing dims filled with mid-space values.

### Color
- `hsv(h, s, v)` — h wraps (negative wraps backward); uses HDR extra brightness bits on
  APA102-class LEDs. `hsv24(h,s,v)` forces 24-bit. `rgb(r, g, b)` 0..1. No rgbw() builtin —
  RGBW handled by hardware color-order config.
- Palettes (v3.30+): `setPalette(flatArray of [pos,r,g,b,…])`, `paint(value, brightness=1)`.

### Waveforms / interpolation (phase args wrap 0..1)
- `time(interval)` — sawtooth 0→1 every `65.536 × interval` seconds (`.015` ≈ 1 s);
  network-syncable. `wave(v)` = (1+sin(v·2π))/2. `square(v, duty)`, `triangle(v)`.
- `mix(a,b,w)`, `smoothstep(lo,hi,v)`, `bezierQuadratic(t,p0,p1,p2)`, `bezierCubic(t,p0..p3)`.
- Perlin (v3.30+): `perlin(x,y,z,seed)`, `perlinFbm(x,y,z,lacunarity,gain,octaves)`,
  `perlinRidge(…,offset,octaves)`, `perlinTurbulence(…)`, `setPerlinWrap(x,y,z)`.

### Math
`abs acos asin atan atan2 ceil clamp cos exp floor frac hypot hypot3 log log2 max min mod
pow round sin sqrt tan trunc` — note `floor(-5.1) == -6` but `frac(-5.5) == -0.5` and
`trunc` rounds toward zero. `random(max)` true random; `prng(max)` + `prngSeed(seed)`
deterministic.

### Transforms (v3.17+; apply to next render cycle; up to 31 stacked)
`resetTransform() transform(m11..m44) translate(x,y) scale(x,y) rotate(rad)
translate3D scale3D rotateX rotateY rotateZ`. Note `scale(2,2)` halves apparent size
(scales pixel density).

### Map introspection
`pixelMapDimensions()` (0 = none), `has2DMap()`, `has3DMap()`,
`mapPixels(fn(index,x,y,z))` (walks all pixels outside a render pass, transforms applied).

### GPIO / misc
`pinMode(pin, INPUT|INPUT_PULLUP|INPUT_PULLDOWN|OUTPUT|OUTPUT_OPEN_DRAIN|ANALOG)`,
`digitalRead/Write`, `analogRead(pin)`→0..1, `touchRead(pin)`→0..1.
Clock: `clockYear/Month/Day/Hour/Minute/Second/Weekday()` (Sun=1; needs internet time).
Sync: `nodeId()`. Sequencer: `sequencerNext()`, `sequencerGetMode()` (0..3),
`playlistGetPosition/SetPosition/GetLength()`.

### UI controls (see product doc for full table)
slider / hsvPicker / rgbPicker / toggle / trigger / inputNumber (inputs);
showNumber / gauge (outputs, return-value displayed). Called with saved values on pattern
activation (except trigger). Canonical idiom:
```js
var mySetting = 0.5
export function sliderMySetting(v) { mySetting = v }
```

### Sensor board bindings
`export var frequencyData` (32 bins, 37.5 Hz–9.96 kHz), `energyAverage` (silence ≈ 0.0002,
loud ≈ 0.06), `maxFrequency` (Hz), `maxFrequencyMagnitude`, `accelerometer[3]`, `light`,
`analogInputs[5]`. Detection idiom: set `frequencyData[0] = -1` at init and test later.

### Canonical examples
```js
export function render(index) {
  hsv(time(.1) + index/pixelCount, 1, 1)
}
```
Speed-optimized (225K px/s on v3, Hencke forum 4574/6):
```js
export function beforeRender(delta) { t1 = time(.1) }
export function render(index, x) { hsv(t1 + x, 1, 1) }
```
Frame-buffered shape (blink fade): allocate `array(pixelCount)` at top level, decay in
`beforeRender` with `values[i] -= fade * delta * .1`, read in `render`.

## 3. Engine architecture

- **Compilation happens entirely in the browser** (compiler JS ships inside the device-served
  web app). Hencke (forum 1092/6): `program = compile(src, {predefinedGlobals:
  ['pixelCount'], extendedOperators, constants})`. Device runs **only a bytecode VM** —
  no parser on device.
- Compiler output: `program.compiled` = array of s32 opcode words; `program.exports` =
  `{address, name}` list; plus source-map info. Bytecode is platform-specific (v2 ≠ v3).
- Bytecode upload container: u32 LE opcode-bytes-length, u32 LE exports-bytes-length,
  s32 LE opcodes, then per export u32 LE address + ASCII name + NUL. Live-code announce:
  `{"pause":true,"setCode":{"size":N,"crc":crc32,"id":…}}` before binary frame.
- Source stored LZString-compressed (compressToUint8Array; UTF-16 code units big-endian);
  device never decompresses, just serves it back.
- pixelblaze-client's trick: download `/index.html.gz`, regex-extract the compiler JS +
  constants (version-dependent), execute `window.compile(…)` in MiniRacer. Only public
  recipe for producing device bytecode outside the official UI.
- Frame pipeline: beforeRender(delta) once → render fn per pixel 0..pixelCount-1 → push to
  LEDs → previewFrame packet. Runtime VM errors don't halt — flagged via `vmerr`/`vmerrpc`
  in stats and highlighted in editor.
- Performance (Hencke, Nov 2025, v3): avg ≈ 48K px/s over stock patterns (Blinkfade 52K,
  xorcery 25K, fast pulse 1D 63K); trivial-pattern ceiling 225K px/s; WS2812 bus caps
  ~33K px/s per output at 800 kbps.
- **Previews render on-device** (type-5 RGB stream). No official JS implementation of the
  language exists; pattern-site previews are the stored JPEG frame-strips.

## 4. Reimplementation-critical gotchas

1. Two's-complement 16.16 everywhere; wrap on overflow; `~` zeros low 16 bits; other
   bitwise ops act on all 32 bits including fraction.
2. `%` truncated vs `mod()` floored; `frac`/`trunc` toward zero; `floor`/`ceil` true floor/ceil.
3. PB bytecode literals are 31-bit (we need not copy this quirk, but beware patterns
   composing full-precision values at runtime).
4. Implicit assignment = global; `var` in function = local; no closures; function arrays as
   dispatch tables.
5. `random()` true-random vs `prng()/prngSeed()` deterministic.
6. Wrapping semantics pervade `hsv`, `paint`, `wave`, `triangle`, `square`, `time`.
7. `time(interval)` period = 65.536·interval s (fixed-point epoch counter); synced via UDP
   timesync.
8. Render-fn selection priority + `render(index, x)` second parameter on current firmware.
9. Arrays never freed; literals self-allocate; `.length` is a property.
10. The compiler is browser JS shipped in the device's own web bundle (extractable per
    firmware version) — but Luxel should have its own clean-room compiler.

## 5. Existing FOSS partial implementations (reference/inspiration only — check licenses)

- **pixelblaze-client** (MIT, Python): protocol + PBP/EPE/PBB codecs + compiler extraction.
- **pb_emu / pixelblaze-pattern-emulator** (tarballz, 2026): browser emulator, Three.js,
  float64 math (self-declared fidelity gap).
- **PXLBLZ-IDE** (jon-whiteroomsoftware, 2026): browser IDE claiming hardware-accurate
  16.16 fixed-point software renderer, .epe import, push-to-device.
- **Firestorm** (official, public source, no license file), **PixelTeleporter**,
  **pb-examples** (jvyduna — canonical commented patterns).

## Sources
- https://github.com/simap/pixelblaze/blob/master/README.expressions.md (and v3 branch)
- https://github.com/simap/pixelblaze/blob/master/README.mapper.md
- https://electromage.com/docs/websockets-api/ · https://github.com/zranger1/pixelblaze-client
  (source + docs/pixelblazeProtocol.md)
- forum.electromage.com raw posts: topics 1092/6, 650/23, 1597/14, 413/4, 4574/6, 735/8, 3777/1
- https://github.com/simap/pixelblaze_sensor_board · https://github.com/jvyduna/pb-examples
