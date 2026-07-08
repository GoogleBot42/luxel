# Christmas string lights
kind: 1D
sensors: no

## Overview
A simple, mostly-static pattern that makes a dense LED strip look like an
old-fashioned string of widely spaced colored Christmas bulbs. Most of the strip
is dark; small bright "bulbs" appear at regular intervals, each a solid classic
Christmas color, with an occasional near-imperceptible flicker. Author's note in
the original: looks best at low global brightness.

## Visual behavior
Evenly spaced glowing bulbs a few pixels wide, separated by dark gaps several
times longer than the bulbs (bulb-to-gap ratio roughly one to five, so the lit
fraction is small). Each bulb is a single saturated color; the colors cycle
through a fixed five-color sequence down the strip: red, blue, green,
yellow/orange, purple — the classic C9 bulb set. Within a bulb, brightness peaks
at the center pixel and falls off toward the edges, giving a rounded "filament
glow" rather than a flat block. Very rarely a pixel blinks off for a single
frame — a subtle twinkle.

## Algorithm
Stateless between frames; everything is computed per pixel from the index.
- Define a repeat period = bulb width (a few pixels) plus gap width (several
  times that). The strip is divided into consecutive periods; each period holds
  one bulb at its start.
- Hue: take the pixel's period number, wrap it modulo the palette length (five),
  and use that palette entry. (In the original this is done with fractional
  arithmetic whose result is truncated by array indexing; the intent is simply
  "bulb number mod palette size". Note the wrap is applied to a fractional
  value, so implement it as floor-then-mod on the bulb number.)
- Lit test: the pixel is part of a bulb only if its offset within the period is
  less than the bulb width; otherwise brightness is zero.
- Bulb shading: evaluate a triangle wave over the pixel's fractional position
  across the bulb (offset by half a pixel so the peak centers properly), map it
  onto a range from a small floor up to full, then cube it to sharpen the peak.
  Result: bright center, dim shoulders, never fully dark within the bulb.
- Twinkle: per pixel per frame, draw a uniform random number; with a very small
  probability (a fraction of a percent) the pixel's brightness is zeroed for
  that frame. The original comments that this is frame-rate dependent (faster
  render loops twinkle more often); a reimplementation might scale the
  probability by frame delta instead.
- Saturation is always full.

## Colors
Five fixed fully saturated stops, in order along the strip: warm red, medium
blue, green, orange-leaning yellow, and purple/violet. No color animation.

## Controls
None. Bulb width, gap width, and the palette are top-of-file constants; sliders
for spacing would be a natural extension. No hardcoded pixel count — it adapts
to any strip length (a partial period at the end just truncates).

## Timing
Effectively static; the only motion is the random single-frame twinkles, each
lasting one frame and occurring rarely per pixel (order of once per many
seconds per pixel at typical frame rates).
