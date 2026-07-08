# Utility: Perceptual hue
kind: 1D
sensors: no

## Purpose
A utility/demo pattern. The standard HSV hue axis is not perceptually uniform: equal hue steps do
not look like equal color steps to humans (greens and blues hog the range, reds and oranges get
squeezed). This pattern provides two reusable "hue remapping" helper functions that take a
*perceptual* hue in the unit range and return a corrected hue for the HSV call, and it
demonstrates them by cycling a live A/B/C comparison on the strip.

## What it looks like
A full rainbow is laid across the strip (position along the strip maps to hue) and the whole
rainbow scrolls continuously — a complete hue revolution takes on the order of a second, so it
drifts briskly. The pattern rotates through three display modes, each shown for a few seconds:

1. Raw linear hue (the ordinary rainbow — greens/blues visually dominate).
2. Hue passed through the table-based remap (best-looking, most even rainbow).
3. Hue passed through the cheaper wave-based remap (decent but bright greens still
   over-represented, deep blues under-represented).

At each mode changeover the strip blinks fully dark for a large fraction of a second so the
viewer can tell the modes apart. The whole three-mode cycle repeats every ten-or-so seconds.

## Algorithm
Per frame: advance two sawtooth phases from the built-in timebase — a fast one (roughly one
second per cycle) that scrolls the rainbow, and a slow one (roughly ten seconds per cycle) whose
value, split into thirds, selects the current display mode. Brightness is forced to zero during
a short initial slice of each third to produce the blackout blink; otherwise full brightness.

Per pixel: perceptual hue = (pixel index / pixel count) + fast phase. Depending on the mode,
either use it directly, or run it through remap A or remap B. Emit fully saturated HSV.

### Remap A — table interpolation (the good one)
A small hand-tuned anchor table of about ten entries maps evenly spaced *input* hues to chosen
*output* hues. Conceptually the anchor list runs: red, then orange only slightly above red,
yellow still quite low, then a big jump to green, then cyan, blue, indigo, purple, pink, and back
to red (the last entry duplicates the wrap value, plus one extra overflow entry guarding the
interpolation's out-of-range read). The input range is divided into equal arcs, one per gap
between anchors; the function wraps its input into the unit range, finds which arc the input
falls in and how far through it, and linearly interpolates between that arc's two anchor hues.
Because the early anchors are bunched near the red end, reds/oranges/yellows are stretched to
occupy more of the perceptual input range, while greens and blues are compressed. The table is
explicitly subjective — chosen by eye for perceived equal spacing — and meant to be user-tunable.

### Remap B — smooth wave (the fast one)
A one-liner: wrap the input into the unit range, then warp it through one arc of a sinusoidal
easing (shift the input by half, halve it, and take the standard unit sine-wave function of
that). This produces a smooth S-shaped stretch that approximates the table at noticeably lower
cost (roughly a third faster), at the price of over-representing bright greens.

## Colors
Full-spectrum rainbows, always fully saturated and (except during the blink) full brightness.
The point of the pattern is the *distribution* of hues, not a palette.

## Controls
None. The comments suggest edits (freeze the scroll, offset by half a revolution to inspect
reds) but there are no exported UI controls.

## Layout assumptions
Any 1D strip; hue is normalized by pixel count, so nothing is hardcoded.

## Notes for reimplementation
The deliverable is really the two helper functions with wrap-safe inputs; the render code is just
a self-documenting demo harness. Keep the two remaps as standalone, reusable pure functions.
