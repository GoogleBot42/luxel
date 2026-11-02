# Glyph Mosaic

Idea spec written from Patternflow preset preset_0514.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

The panel is a grid of small square cells (each only a handful of pixels wide, so a few dozen cover the panel). Every cell shows one glyph from a tiny alphabet, picked by a slowly flowing brightness field sampled at that cell: an inset solid square, nothing, a plus/cross, a diagonal slash, or a nearly cell-filling square. Each glyph has its own colour — four hues spread around the wheel from a base hue. As the underlying field flows, waves of glyph changes roll across the grid: regions turn from crosses to slashes to big squares and back, like a mosaic of animated typography or an ASCII-art plasma. The cell grid itself is fixed; the sampling points wobble so that neighbouring cells change at slightly different moments, which breaks up the regularity. The overall look is a colourful, pixel-perfect "glyph plasma", busy but orderly.

## The idea

ASCII-art rendering of a plasma: quantize a smooth field into levels and draw each level as a different glyph in a cell, with each glyph in its own colour. It works because the glyphs add structure and readability to an otherwise mushy plasma.

## Algorithm in words

- State: time accumulator (base rate + speed knob) and a hue that drifts at a rate set by K1.
- Fixed cell size of a handful of pixels.
- For each cell: take its centre and push it sideways/vertically by a sine/cosine of the cell's row/column and time (the "warp" amount from K4), giving a wobbled sample point.
- Field = average of a sine in x and a cosine in y at that sample point, both scrolling with time (constant per cell). A second, purely per-row, slow vertical wave is blended in according to the depth knob, which also reduces the first term's weight — at high depth the screen becomes horizontal bands of glyphs.
- Clamp the result to 0..1 and split into five levels: inset square (hue A), empty, plus sign (hue B), diagonal stroke (hue C), large square (hue D).
- Glyph geometry is evaluated on the pixel's position within its cell.

## Knobs

- K1 — hue drift: centred is still; turning it either way makes all four colours rotate slowly around the wheel in that direction.
- K2 — speed of the flowing field.
- K3 — depth: blends in horizontal banding; at the top of its device range it overwhelms the field and the grid saturates into stripes of the brightest glyph.
- K4 — warp: how far each cell's sample point wanders, adding jitter to the wave fronts.
- No audio.

## Making it better in Luxel

- Draw glyphs with `renderBulk` shape ops (one `fillRect`, two `fillRect`s for a plus, a `drawLine` for a slash) per cell — a couple of hundred ops per frame, versus evaluating the field at every pixel. The field is evaluated once per cell, not per pixel.
- Replace the sine field with `simplex3` sampled at cell centres, and grow the alphabet: circles (`fillCircle`), rings, X, dots, half-blocks, ordered by visual weight so the field reads as a true luminance ramp (classic ASCII-art density ordering).
- Use font glyphs from the Text extension (or a small sprite sheet) as an alphabet option — e.g. a "matrix" alphabet of characters ordered by ink coverage.
- Make cell size a real-unit control so a 128x128 chain gets readable glyphs; colour by OKLCH lightness tied to glyph weight so the image still reads as a gradient.
- A generic "glyph-ize any field" pattern could share code with Hatched Warp. Strips: not a fit.

## Payoff

Score: 3/5 — fun ASCII-plasma concept; with a weight-ordered alphabet and noise it becomes a good showpiece, as-is it is a busy colour quilt.
