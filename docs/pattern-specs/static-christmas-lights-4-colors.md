# Static Christmas Lights - 4 Colors
kind: 1D
sensors: no

This pattern is trivial: a completely static, non-animated repeating four-color sequence, like a classic incandescent Christmas light string.

## Algorithm
No state, no per-frame work, no randomness. For each pixel, divide the index by a block size (source default: one pixel per block), take that quotient modulo four, and pick one of four fixed hues by which quarter of the cycle it falls in. Render at full saturation and full brightness.

## Colors
The four hues, in repeating order: red, green, blue, yellow. (Chosen as evenly spaced primary/secondary hue-wheel stops plus yellow; describe them just as those named colors.)

## Controls
None exposed in the UI. Block size, saturation, and brightness are constants in the source meant to be hand-edited; the obvious improvement is to expose block size (and maybe brightness) as sliders.

## Layout notes
Pixel-count-agnostic; works on any strip length. On matrices it simply follows index order.
