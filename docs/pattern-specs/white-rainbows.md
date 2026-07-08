# White Rainbows
kind: 1D
sensors: no

## What it looks like

Several bright, near-white "comet heads" travel along the strip — half of them moving one direction, a mirrored set moving the opposite direction, so heads continually cross each other. Each head leaves a trail behind it that starts white-hot, quickly re-saturates into vivid color, and fades out over a fraction of a second to a second. With the rainbow mode on (default), trails are drawn from a many-times-repeated rainbow laid along the strip, so each trail segment shows the local rainbow color; with it off, all trails share one single color that drifts slowly around the color wheel. The overall feel is energetic weaving streams of light.

## Algorithm

State between frames: two per-pixel persistence buffers sized to the pixel count — one holding each pixel's current **saturation**, one holding its current **brightness**. These act as decaying "paint" layers.

Per frame (before rendering):
- Get a sawtooth phase cycling over several seconds, and its reversal (one minus it) — these are the forward and backward sweep positions.
- Get a second, independent sawtooth used only for hue drift. Its period is several seconds normally, but when rainbow mode is on it runs a few times faster.
- For each of N comets (N is the user-set count): compute an evenly spaced offset (comet index divided by N), add it to the forward phase, wrap into unit range, scale by pixel count, and floor to a pixel index. At that pixel, **stamp** the buffers: brightness to full, saturation to a low value (roughly one-fifth — i.e. nearly white). Do the same using the reversed phase for the mirrored, opposite-direction set.

Per pixel, in render:
- Hue = the drifting hue phase, plus — only when rainbow mode is on — the pixel's fractional position along the strip multiplied by a largish repeat count (on the order of a dozen-and-a-half rainbow repetitions across the strip).
- Multiply the pixel's stored saturation up by roughly a third (it clamps at full saturation in the color call), and store it back. This is the trick: a freshly stamped head is nearly white, and over the next several frames its saturation compounds back up to fully saturated color — white head, colored tail.
- Multiply the pixel's stored brightness down by a few percent, store it back — an exponential fade-out tail.
- Emit hue/saturation/brightness.

No randomness. Layout: pure 1D, properly scaled by pixel count; no hardcoding worth fixing (the rainbow repeat count is a fixed constant, acceptable as a stylistic choice).

**Frame-rate caveat**: both the saturation regrowth and the brightness decay are applied once per rendered frame, not per unit time, so tail length and the white-to-color transition distance depend on frame rate. A faithful reimplementation may keep that, but a delta-time-scaled decay would be the robust fix.

**Off-by-default gotcha**: the comet-count slider maps its range to zero through about a dozen — at the very bottom the count is zero and the display goes dark. Suggest flooring at one. Default count (before touching the slider) is a handful.

## Colors

Heads: near-white (barely tinted). Tails: fully saturated spectrum colors. Rainbow mode paints the full rainbow repeated many times along the strip and drifting; single-color mode uses one saturated hue for everything, slowly cycling through the whole wheel.

## Controls

- **Rainbow** (slider used as a toggle — on above midpoint): switches between per-position rainbow trails (on) and a single shared drifting trail color (off). Also speeds up the hue drift when on.
- **Number** (slider): how many comets per direction, from zero up to about a dozen.

## Timing

Comets take several seconds to traverse the strip. Trails fade over well under a second at typical frame rates. Hue drift: several seconds per full cycle (faster in rainbow mode).
