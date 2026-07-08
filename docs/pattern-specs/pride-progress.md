# Pride Progress
kind: 1D
sensors: no

A simple scrolling-stripes pattern rendering the Progress Pride flag. Short spec accordingly.

## What it looks like

The strip is filled with a repeating sequence of eleven equal-width hard-edged color stripes — the Progress Pride flag's chevron colors followed by the six rainbow stripes — slowly and continuously scrolling along the strip, one full cycle every several seconds. Because the stripe position is derived from a triangle wave of normalized position, the sequence runs "up" the first half of the strip and mirrored back "down" the second half (symmetric about the midpoint), and the two halves appear to scroll in opposite directions.

## Algorithm

Stateless apart from one global clock. Per frame: sample a repeating clock several seconds long. Per pixel: take the triangle wave of the pixel's normalized position (rises to the midpoint then falls), subtract the clock value, wrap into the unit interval. Divide the unit interval into eleven equal bins; each bin selects one fixed HSV stripe color via a chain of threshold comparisons. No randomness, no per-pixel state.

## Colors (in stripe order)

The eleven stops, qualitatively — note the brightness levels are wildly unequal *on purpose* (see hardware note):

1. black (a stripe rendered as fully off)
2. extremely dim warm brown (barely-glowing ember level)
3. dim pastel light blue
4. moderately bright pink/magenta
5. faint desaturated warm white
6. red at modest brightness
7. orange, the brightest stripe by a wide margin
8. golden yellow at modest brightness
9. very dim pure green
10. very dim pure blue
11. very dim violet/purple

## Hardware note (important)

A header comment states the colors are hand-tuned for HDR-capable LED drivers (APA102/SK9822-class with the extra brightness channel) and will probably look wrong on ordinary WS28xx strips. Many of the values sit at "a percent or less" of full brightness, relying on HDR dithering to stay smooth and to make perceptually different colors read at comparable apparent brightness. On a reimplementation without an HDR path, the dim stripes will banding-crush to black or flicker; the obvious fix is to re-balance the value levels for the target LED type (or gamma-correct), keeping the qualitative ordering: black stripe darkest, brown barely visible, orange the standout brightest.

## Layout assumptions

None hardcoded; everything is normalized by pixel count. Stripe count is a code constant.

## Controls

None.

## Timing

One full scroll cycle every several seconds; motion is slow and steady.
