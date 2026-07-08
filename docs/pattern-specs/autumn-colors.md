# Autumn Colors
kind: 1D
sensors: no

A simple random-twinkle pattern; short spec.

## What it looks like
The whole strip twinkles in autumn-leaf colors: reds, russet brown-oranges, oranges,
and an occasional yellow. Each pixel independently glows, slowly fades out, then pops
back at a new random color and random brightness. The overall feel is a gentle,
unsynchronized shimmer, like leaves catching light.

## Algorithm
State per pixel, kept between frames: a hue and a brightness level. Also a frame-time
accumulator used to throttle updates.

Per frame (in the pre-render step):
- Accumulate elapsed time; only when a few hundredths of a second have passed does
  the update pass run (this throttling sets the gentle pace).
- Update pass, for every pixel:
  - Decrease its brightness by a small amount proportional to the frame time (a full
    fade takes on the order of several seconds).
  - If brightness has reached zero, draw one uniform random number and pick a new hue
    by weighted choice: red roughly three tenths of the time, brown roughly three
    tenths, orange a bit under four tenths, yellow rarely (a few percent). Then
    restart the pixel at a uniformly random brightness.

Per pixel (render): output the stored hue at full saturation, with the stored
brightness squared (squaring deepens the dim end so fades look smoother and the strip
reads less washed-out).

## Colors
Four fixed hues in the warm red-to-yellow range: pure red, a brownish red-orange,
orange, and yellow. The four hue values are exported variables, so they can be
retargeted live (e.g. to make a Christmas or Halloween variant) without editing the
selection logic. Changing the *ratios* requires editing the weighted-choice
thresholds.

## Controls
No sliders/toggles; just the four exported hue variables adjustable via the variable
watcher.

## Layout assumptions
Works on any pixel count (state arrays sized from the pixel count). 1D but looks fine
on any geometry since pixels are independent.

## Timing feel
Updates ticked a few hundredths of a second apart; each pixel's fade-out lasts
several seconds, so at any moment the strip shows a mix of bright, mid, and nearly
dark pixels.

## Notes
The per-tick decay amount is scaled by the frame delta even though the pass only runs
on throttled ticks, so effective fade speed varies slightly with frame rate; a
reimplementation could decay by elapsed time per tick instead.
