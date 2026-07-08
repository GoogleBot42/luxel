# Halloween color twinkles
kind: 1D
sensors: no

## What it looks like
Confetti-like twinkles in a Halloween palette: pockets of deep purple/violet and pumpkin-orange sparkle against black. Adjacent pixels differ strongly (the spatial "wavelength" is only a few pixels), so it reads as random glitter rather than waves, though it's fully deterministic — the shimmer drifts and evolves smoothly, with individual pixels swelling up, glinting, and winking out. There's a layered motion feel: a slow drift over roughly a second-and-a-half cycle modulating the colors, and a several-times-faster cycle driving the brightness shimmer. A fair fraction of pixels are fully dark at any moment (brightness is gated), which is what makes it twinkle instead of glow.

## Algorithm
No per-frame state beyond two phase values; no randomness at all — the organic look comes from nested incommensurate sinusoids of the raw pixel index.

Per frame: compute two sawtooth phases from the global clock, one slow (period on the order of a second and a half, expressed as a full-circle angle) and one a few times faster (roughly 3–4× the slow rate).

Per pixel (using the raw 0-based pixel index, deliberately NOT normalized by pixel count):
- Hue driver: a sine of (index divided by a few) plus a full-circle-scaled sine of (index divided by roughly two, plus the slow phase). The nesting of one sine inside another, each with a different short spatial period, is what scrambles neighboring pixels. The result is a signed value.
- Brightness: a triangle/sinusoidal wave of (a similarly short spatial term of the index) offset by a sine of (index at another short spatial period, plus the fast phase). The wave output (0..1) is raised to the fourth power as gamma correction, crushing mid-tones so only near-peaks are visibly bright.
- Twinkle gate: if the gamma-corrected brightness is below a small threshold (about a tenth), snap it to zero. Raising the threshold yields sparser, twinklier output.
- Palette split: the sign of the signed hue driver picks the color family. Positive values are folded through a triangle function (for a smooth, non-wrapping ramp) and squeezed into a narrow purple/violet band of the hue circle; negative values are folded the same way into an even narrower band of warm orange just above red. Saturation is always full.

Output is hue/saturation/value with the computed hue and gated brightness.

## Colors
Two families only, on black: rich violets-to-purples, and pumpkin/amber oranges. Each family occupies a deliberately narrow hue band (the purple band a bit wider than the orange one), so there's tonal variation within each family but never a rainbow — the triangle fold prevents hue wrap-around from producing stray colors.

## Layout assumptions
Intentionally index-based rather than length-normalized: the design goal is "adjacent pixels differ", so the pattern looks the same density on any strip length. No hardcoded pixel count. On a mapped 2D/3D display it would still run (as a 1D index pattern) but the structure follows wiring order.

## Controls
None.

## Non-obvious tricks worth keeping
- Nested sinusoids of index at two different short spatial periods stand in for randomness — cheap, deterministic confetti.
- Fourth-power gamma plus a hard low-end gate is what turns a smooth wave field into discrete twinkles.
- Sign-splitting one signed oscillator into two narrow hue bands (via a triangle fold to avoid wraparound) is how a single hue driver yields a strict two-color palette.
