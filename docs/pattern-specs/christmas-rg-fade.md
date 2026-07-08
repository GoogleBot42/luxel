# Christmas RG Fade
kind: 1D
sensors: no

This one is simple — a short spec suffices.

## What it looks like
Every pixel independently glows either pure red or pure green and slowly fades to black; the moment it reaches black it instantly relights (possibly switching color) at a random brightness and begins fading again. The whole strip is a gently shimmering mix of red and green points at all different brightnesses — a classic Christmas-lights sparkle. A pixel starting from full brightness takes several seconds to fade out; because rebirth brightness is random, individual pixels have staggered, varied lifetimes and the field never pulses in unison.

## Algorithm
State: two per-pixel arrays — current brightness, and current hue.

Per frame, for every pixel: decrease its brightness linearly by an amount proportional to elapsed frame time (rate tuned so a full-brightness pixel takes several seconds to die). When brightness reaches or crosses zero: flip a fair coin to pick red or green as the pixel's new hue, and set brightness to a fresh uniform random value between zero and full.

Per pixel at render time: draw the stored hue at full saturation, with brightness equal to the stored value squared (squaring stretches the low end so the fade looks perceptually smoother and pixels spend more of their life dim).

Randomness: fair coin for color choice, uniform random for rebirth brightness. No layout assumptions; any pixel count, 1D by index.

## Color
Exactly two colors, fully saturated: red and green. The two hue values are held in exported (externally adjustable) variables rather than literals, so they can be retargeted live, but there are no UI controls. (Quirk of the original: the two variables' names are swapped relative to the colors they actually produce; the rendered result is still plain red and green. A reimplementation should just produce red and green.)

## Controls
None in the UI; the two hue variables are exported for external adjustment.
