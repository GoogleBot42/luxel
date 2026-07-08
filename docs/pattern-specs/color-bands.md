# color bands
kind: 1D
sensors: no

## What it looks like

A double rainbow laid along the strip (the full hue wheel repeats twice end to end) but only visible through a shifting mask of short bright bands: clumps a few pixels wide separated by dark gaps, crawling slowly along the strip with an interference/moiré shimmer — some structure drifts one way while other structure drifts the opposite way, so bands appear to slide, merge, and re-form. Sprinkled through it, narrow spots briefly wash out to white and recover. Overall a relaxed, watery, glittering rainbow that never sits still; the component motions repeat on cycles of roughly ten to fifteen seconds.

## Algorithm

Purely per-pixel; the only per-frame work is sampling a few free-running sawtooth clocks (with periods around ten and fifteen seconds). One additional clock is computed but never used — dead code that a reimplementation can drop.

Per pixel, three quantities are computed from the pixel index and the clocks. All spatial oscillations below use a smooth sinusoidal wave that runs zero to one over each cycle of its argument.

- Hue: the pixel's normalized position along the strip, scaled so the hue wheel is traversed twice over the full length (values past one wrap around the wheel).
- Saturation: a wave over pixel position with a short spatial period (a very few pixels), drifting slowly along the strip on the ~fifteen-second clock. The wave is raised to the fourth power and subtracted from full saturation, so saturation stays essentially full everywhere except narrow moving spots near each wave crest, which briefly desaturate toward white.
- Brightness: an interference pattern built from three waves over pixel position with three different short, mutually non-harmonic spatial periods (each a handful of pixels, all different). Two of them drift in one direction on the ~ten-second clock and are multiplied together; the third drifts in the opposite direction on the same clock and is added to the product. The sum is then raised to the fourth power, which crushes everything mid-level to black and leaves only the constructive-interference peaks as distinct bright bands. Note the sum can exceed unity before shaping; the fourth power then exceeds unity too, relying on the renderer clamping brightness at full — the clipped plateaus are part of the look.

The pixel is emitted as hue/saturation/brightness. No state carries between frames, no randomness anywhere.

## Layout assumptions

The band widths are expressed in raw pixel units (spatial periods of a few pixels), so bands stay physically small on any strip length; the hue gradient, in contrast, is normalized to strip length. On very short strips the bands become coarse relative to the rainbow. This is arguably intended; if band count relative to strip length matters more than physical band size, normalize the spatial terms by pixel count.

## Colors

Full rainbow (twice along the strip), mostly at full saturation, with narrow traveling white flashes; black between bright bands. No palette table — direct hue-wheel usage.

## UI controls

None.

## Timing

Two independent motion clocks, roughly ten and fifteen seconds per cycle; the visible drift of bands is slow and continuous, with the interference giving faster apparent shimmer than either clock alone.
