# Matrix Green Waterfall 2D
kind: 2D
sensors: no

## What it looks like

The classic "digital rain" from The Matrix, minimal edition: on a 2D panel, columns of green streaks fall continuously down the display. Each streak has a bright head that reads as whitish-green (desaturated) and a green tail fading to black behind it. Different columns have different streak lengths and repeat spacings, and those spacings very slowly drift over time, so the columns never lock into a visible fixed pattern. Speed is moderate-to-fast by default and adjustable to a crawl or a torrent. The author notes it is a loose impression, deliberately tiny and cheap so it can serve as a background layer for other patterns.

## Algorithm

Almost stateless — the entire effect is a per-pixel closed-form function of time.

Per frame, compute two values:

- A fall phase: a sawtooth clock with a period around a quarter of the engine's long time unit, plus a tiny constant offset, multiplied by the speed setting (an integer up to about fifty).
- A column-frequency value: a base spatial frequency plus a very slow small drift (a fraction of a slower sawtooth clock), which is what slowly reshapes the columns over time.

Per pixel:

- Compute a per-column "cycle length" as a smooth 0..1 wave of (the pixel's x coordinate times the column frequency). So each column gets its own repeat interval, varying smoothly across x, and slowly changing as the frequency drifts.
- Take (y minus the fall phase) modulo that per-column cycle length, using a floored/always-positive modulo (important: with truncated modulo the negative operand breaks the effect; original Pixelblaze v2 needed a hand-rolled positive mod, provided in a comment). This produces, in each column, a value ramping from zero up to the column's cycle length as you move along the column, scrolling over time — a repeating falling ramp.
- Square that value: this darkens the tail and sharpens the bright head (gamma).
- Draw in a fixed green hue at that brightness, at full saturation — except where the squared value is quite high (above roughly four-fifths), where saturation is knocked down slightly (by about a twelfth), which whitens just the head of each streak.

Because different columns' cycle lengths differ, heads in different columns move at effectively different visual rhythms and repeat distances, which is what sells the "independent drops" illusion despite there being no per-drop state at all.

## Layout assumptions

Uses normalized 2D coordinates, so no hardcoded pixel count in the ramp math itself — but the default base column frequency is derived from the square root of the pixel count halved (i.e., assumes a roughly square matrix). A source comment says to hand-tune that base for strongly non-square layouts; the obvious generalization is to derive it from the actual x-axis resolution (half the number of columns) instead of assuming squareness. 2D renderer only; no 1D/3D fallback.

## Colors

Fixed pure "terminal" green throughout; streak heads bleach slightly toward white via the saturation dip. Background black (tail fade to zero).

## Controls

- One slider, "speed": scales the fall rate from stopped/very slow up to fast, quantized to integer steps (up to about fifty).

## Timing

At default speed, streaks traverse the panel in well under a second per repeat cycle; the column-shape drift evolves over several seconds.

## Non-obvious details

- The whole waterfall is one line of math: floored-modulo of (y minus time) by a per-column wave-derived period, squared. No arrays, no per-drop state.
- Multiplying the sawtooth clock by a largish integer speed factor makes the phase sweep through many cycles per clock period; the modulo folds it, so quantized speeds avoid visible seams.
- The saturation dip gated on a brightness threshold is a very cheap way to get "hot" streak heads without a second color computation.
