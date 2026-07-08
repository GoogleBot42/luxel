# Golden Tix
kind: 1D
sensors: no

## What it is

A deliberately minimal "live-coding sandbox" pattern — a 1D adaptation of the tixy.land creative-coding toy, written as a class/challenge exercise. The pattern itself is mostly scaffolding: it provides a single user-editable one-line formula that maps (time, pixel index, normalized position) to a signed brightness value, and renders that along the strip. This is close to trivial; the value is the framework, not the default visual.

## What it looks like (default formula)

The default formula is the sine of (pixel index times elapsed time). At the very start the strip is dark/uniform; as time grows, a sine pattern along the strip compresses into ever-finer stripes that shimmer and alias — a classic "accelerating moiré" look. Positive values light in one chosen color, negative values in a second chosen color.

## Algorithm

State kept between frames:
- Elapsed time in seconds, accumulated from frame deltas.
- A reset latch driven by a slider (see Controls).

Per frame: derive the formula's time input as elapsed time divided by (speed setting times a large constant), so the raw elapsed seconds are scaled way down and larger slider values make the animation slower.

Per pixel: evaluate the sandbox formula with three inputs — the scaled time, the raw pixel index, and the pixel's position as a fraction of the strip (index divided by total pixel count). The result is interpreted as a signed brightness:
- Positive: use the "positive" hue.
- Negative: take the absolute value; use the "negative" hue.
- Brightness is the (absolute) value squared, at full saturation. Values are implicitly expected in roughly the -1..+1 range.

Randomness: none in the default formula (user formulas may add it).

Layout assumptions: none hardcoded; scales with any strip length via the normalized-position input. 1D only (no 2D renderer).

Known wart: the speed divisor is the slider value times a large constant, so with the slider at its zero end the division is by zero (degenerate/very fast time). An implementation should clamp the divisor to a small positive minimum.

## Colors

Two user-chosen hues (via color pickers), one for positive formula values, one for negative. Defaults: positive maps to the top of the hue wheel (reads as red), negative to the middle of the hue wheel (reads as cyan-ish). Full saturation always; brightness is the squared formula magnitude on black.

## Controls

- Color picker, "positive color": hue used for positive formula results (only the hue component is used).
- Color picker, "negative color": hue used for negative formula results (hue only).
- Slider, "speed shift": scales time; higher = slower. (Subject to the divide-by-zero wart above.)
- Slider, "slide right to reset": moving it fully to its maximum zeroes the elapsed-time accumulator (a one-shot latch — the code clears the latch immediately so it re-arms).
- Implicit "control": the formula line itself is meant to be edited in the code editor.

## Timing

Depends entirely on the formula and speed slider; the default formula evolves continuously with no fixed cycle, getting busier the longer it runs (until reset).

## Non-obvious details

- The signed-value → two-hue mapping (positive color vs negative color, brightness = square of magnitude) is the tixy.land convention and is the core contract any reimplementation must keep.
- Squaring the brightness gives gamma-like contrast so mid values don't wash out.
