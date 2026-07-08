# tixy
kind: 1D+2D (2D is the real pattern; the 1D renderer is vestigial/buggy — see below)
sensors: no

## What it is

A port of the tixy.land creative-coding toy to a 16x16 LED matrix, bundled with a built-in catalog of about sixty tiny formulas. Each formula maps (time, pixel index, column, row) to a signed value that becomes a pixel's color and brightness. The pattern acts as a self-running demo reel: it plays one formula for a while, then automatically advances to the next one in the catalog.

## What it looks like

A rotating gallery of tiny generative sketches on a 16x16 grid: random static, sines and ripples, expanding rings, scrolling checkerboards, Sierpinski-triangle bit patterns, fireworks, rotating pinwheels, waves, a bouncing blob, a Pac-Man, frames and diagonals, etc. Each sketch runs for a stretch (several seconds to a fraction of a minute depending on the speed slider), then the display switches to the next sketch. In the default two-color mode, positive formula values render in one picker-chosen hue and negative values in another; a "color" mode instead maps the formula value itself to a hue ramp.

## Algorithm

State kept between frames:
- A time accumulator (the formula's time input), advanced each frame by the frame delta scaled down by a factor proportional to the speed slider (larger slider = slower).
- The index of the currently playing catalog formula. It starts at a fixed entry partway into the catalog (the "mondrian squares" sketch).

Per frame: grow the time accumulator; when it exceeds a small threshold (a few units), reset it to zero and increment the formula index. Note: the index increments without bound — after the last defined catalog entry it walks into undefined slots and would fail. The obvious fix is to wrap the index modulo the number of defined formulas.

Per pixel (2D renderer): convert the renderer's unit-square coordinates into integer column and row indices on a 16-wide, 16-tall grid (multiply by sixteen and floor). Evaluate the current formula with (time, raw pixel index, column, row). The signed result is displayed one of two ways:

- Mono/two-color mode (default): positive → the "positive" picker hue; negative → absolute value, the "negative" picker hue. Brightness is the magnitude (plus a user brightness offset) cubed, full saturation.
- Color mode: hue is the formula value scaled by a user "color shift" amount; brightness is the magnitude plus the brightness offset; full saturation.

The formulas themselves use the whole tixy.land idiom set: per-dot randomness (some formulas call the RNG every pixel every frame — random static, noise), trigonometry of time/index/coordinates, integer/bitwise tricks (AND-based Sierpinski, bit-mask text rows pulled from a small lookup array of magic integers), distance-from-center rings, polar-angle pinwheels, and small lookup arrays cycled by index and time. A tiny scratch array of eight slots is shared by the array-based formulas. Two helper functions are defined for hypotenuse and a full-quadrant arctangent (the platform's built-ins were apparently insufficient at the time).

1D renderer (vestigial): it derives column/row by treating the strip as consecutive 16-pixel groups, but it always evaluates one fixed early catalog entry (per-pixel random brightness) instead of the selected formula, and ignores the color pickers (both signs get the same hue at the top of the hue wheel). It looks like leftover scaffolding; a faithful-but-fixed implementation should route it through the current formula and the same coloring rules as 2D.

Layout assumptions: hardcodes a 16x16 grid (both the coordinate quantization and many formulas' center points, edge indices, and bit widths assume sixteen columns/rows and a center around seven-and-a-half). The quantization could use the actual map dimensions, but the catalog formulas genuinely target 16x16; documenting "intended for 16x16" is the honest fix, with grid size as a constant to change in one place.

Randomness: only inside the handful of noise/static formulas (fresh RNG call per pixel per frame); the framework itself is deterministic.

## Colors

- Mono mode: two user-chosen hues on black — defaults read as red for positive and cyan-ish for negative (mimicking tixy.land's white/red with adjustable hues). Full saturation.
- Color mode: hue sweeps proportionally to the formula value (a rainbow ramp whose span is set by the color-shift slider), full saturation, on black.

## Controls

- Color picker, "positive color": hue for positive values (mono mode).
- Color picker, "negative color": hue for negative values (mono mode).
- Slider, "mono or color": acts as a two-position switch between the two display modes (rounds to off/on).
- Slider, "color shift": in color mode, scales value→hue mapping (range roughly zero to two turns).
- Slider, "speed shift": scales the time accumulator; higher = slower playback and longer time per sketch. At its zero end the divisor collapses to zero (degenerate); clamp to a small positive minimum.
- Slider, "brightness shift": adds an offset (roughly minus-a-half to plus-a-half) to displayed magnitude — lifts dim sketches or crushes bright ones.

## Timing

Each catalog sketch plays for a fixed span of the formula-time clock (a few time units), so wall-clock duration per sketch scales with the speed slider — typically several seconds to tens of seconds. Within a sketch, animation rates vary wildly by formula, from strobing static to slow drifts.

## Non-obvious details

- The signed-value contract (positive/negative → two hues, brightness = magnitude raised to a power for contrast) is the heart of the tixy convention.
- The auto-advance-on-timer turns a formula sandbox into a self-running demo reel; the unbounded index is the one real bug to fix (wrap it).
- Several catalog entries rely on integer truncation and bitwise operations on the platform's numeric type; a reimplementation needs consistent floor/truncation semantics for those sketches to look right.
