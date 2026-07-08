# Spring Colors
kind: 1D (index-based; works unchanged on any layout since every pixel is independent)
sensors: no

## What it looks like
A gentle, unsynchronized twinkle field. Every pixel independently glows in one of four palette colors, slowly dims over many seconds, and the moment it fades to black it instantly relights in a (possibly different) palette color at a random starting brightness. Because start brightnesses and colors are random per pixel, the strip shimmers softly with no visible waves or motion — just a calm ever-changing mosaic. Pace is deliberately slow and gentle.

## Algorithm
State kept between frames:
- Per-pixel brightness array.
- Per-pixel hue array.
- A small accumulator that throttles the update loop.

Per-frame: accumulate frame time; only when a few tens of milliseconds have passed does the housekeeping loop run (then the accumulator is decremented by that interval, not zeroed). In that loop, for every pixel:
1. Decrease its brightness by a small amount. (Quirk: the decrement is also scaled by the instantaneous frame delta even though the loop is already time-throttled, so the fade rate is mildly frame-rate-dependent. A clean reimplementation should just fade at a fixed rate per unit time such that a full-brightness pixel takes on the order of several seconds to reach black.)
2. If the brightness has reached zero or below, respawn the pixel: draw a uniform random number and pick a palette slot with weighted probability — roughly 30% first color, 30% second, just under 40% third, and a rare few-percent chance of the fourth ("accent") color. Then set the pixel's brightness to a fresh uniform-random value anywhere from off to full.

Per-pixel render: output the pixel's stored hue at full saturation, with brightness equal to the stored value squared (perceptual/gamma shaping so the fade looks smooth).

Layout assumptions: none beyond per-index arrays sized to the pixel count; no hardcoding.

## Colors
A four-entry hue palette, fully saturated. Defaults are a warm autumnal/spring-bloom set: red, red-orange, orange, and golden yellow — three common hues plus one rare accent that appears only occasionally, giving sparse pops of the fourth color.

## Controls
Four color pickers (HSV-style), labeled conceptually primary / secondary / tertiary / quaternary. Each replaces one palette slot's hue. Only the hue component of the picker is used; the picked saturation and value are ignored (rendering stays fully saturated). The quaternary picker sets the rare accent color.

## Notes
- The probability weights are fixed in code (the original author invites editing the branch thresholds to retune ratios); a reimplementation could expose them but matching behavior only requires the ~30/30/37/3 split.
- Respawning at a random brightness (rather than always full) is what keeps the field looking organic instead of blinky.
