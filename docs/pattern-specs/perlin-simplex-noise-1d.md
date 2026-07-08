# Perlin/Simplex Noise 1D
kind: 1D
sensors: yes (sound — frequency spectrum from the sensor expansion board)

## Concept

The pattern treats the LED strip as a profile view of a random landscape. It generates a one-dimensional slice through 2D gradient noise (Perlin or simplex, user's choice), normalizes it to a per-strip "heightmap," and then renders animated colored stripes that sweep along contour lines of equal altitude. Where the terrain is steep the stripes look narrow and fast; where it is flat they look wide and slow — the strip shimmers with bands of color flowing through an invisible landscape. Optionally the whole viewport pans back and forth through the noise field, and optionally a bass-detection mode makes the stripes lurch forward on musical hits.

## State kept between frames

- A per-pixel array holding the noise heightmap for the current viewport (recomputed only when needed).
- The running minimum, maximum, and range of that heightmap (used to renormalize every rendered map to the full 0..1 span).
- A "needs recalculation" flag, set by any control that changes scale or horizontal offset.
- A stripe-sweep phase (sawtooth clock) plus a sound-driven extra phase offset.
- Bass-reaction bookkeeping: whether a bass burst is currently active and how long it has been running.
- All the internal tables of the noise implementation (see below).

## Per-frame work

1. Advance the stripe-sweep phase: a sawtooth whose period is proportional to the stripe count and inversely proportional to the stripe-speed control (baseline on the order of tens of seconds for one full sweep at default settings; faster speed or fewer stripes shortens the visible cycle).
2. If bass reactivity is enabled, run the sound step (below) and add its accumulated offset to the sweep phase, wrapping at one.
3. Compute a very slow panning clock (period measured in minutes). If the panning control is nonzero, derive a horizontal viewport offset that moves sinusoidally, with excursion proportional to the panning amount and inversely proportional to the noise scale.
4. If the recalc flag is set or panning is active, rebuild the heightmap: for each pixel, evaluate the chosen 2D noise function at (normalized pixel position minus viewport offset, all times the scale factor; second coordinate held at zero), storing the value and tracking min/max. Note this is a full O(pixelCount) noise pass per frame while panning — the pattern is heavy, and the original author warns it can stall the interpreter.

## Sound reactivity

Conceptual input: a multi-band audio frequency spectrum array from the sensor board. The pattern sums a few adjacent low-frequency (bass) bands and compares against a user threshold. When bass exceeds the threshold, a burst mode latches on for a short fixed duration (a couple hundred milliseconds): during the burst, each frame adds an extra increment (proportional to elapsed time and scaled up by the stripe-speed setting) to the stripe-sweep phase, wrapping at one. Net effect: every bass hit fast-forwards the stripe flow briefly, so the bands pulse forward with the beat. With the threshold control at zero the sound path is disabled entirely.

## Per-pixel render

1. Read the pixel's stored height, and normalize it to 0..1 using the current map's min and range ("altitude").
2. Hue, auto-color mode: quantize (altitude minus sweep phase) into as many equal bands as the stripe count, so each sweeping stripe is one solid hue; subtract a user palette-offset so the whole palette can be rotated. Result: several distinct rainbow-spaced hues.
3. Hue, fixed-color mode (auto-color off): force three stripes and pick between three preset hues using two square-wave selectors on (altitude minus phase) — the presets are fire-like: a red, a deep orange, and a yellow-orange.
4. Brightness: a triangle wave of (altitude minus phase) multiplied by both the stripe count and a sub-stripe count, then squared for contrast. The sub-stripe mechanism ("stripe weight" control) divides each stripe into several slots and blanks all but one of them using an integer-modulo gate, which makes the visible stripe thinner without changing its spacing.
5. Optional progress bar: if enabled, pixels within a couple of positions of (sweep phase × strip length) are overridden with white whose brightness peaks at the exact phase position — a debugging aid showing the sweep clock along the strip.

## The noise implementation

The pattern inlines a self-contained 2D Perlin-noise and 2D simplex-noise implementation (a port of a well-known public-domain JavaScript noise library): a fixed permutation table of the byte values, a seed-mixing step that XORs the table with a chosen seed, a set of twelve 3D gradient direction vectors, the standard quintic fade curve and bilinear interpolation for the square-grid variant, and skew/unskew corner accumulation for the triangular-grid variant. Both are rescaled so outputs land in the 0..1 range instead of the conventional signed range. A fixed seed constant is baked in, so the "landscape" is the same every run; the x-offset control lets the user scrub to a different part of it.

Implementer note: the reimplementation should just use a correct, standard 2D Perlin/simplex noise (seeded, output remapped to 0..1). Be aware the original contains a bug in its gradient-table construction — all three components of each gradient vector are written to the same slot, so the effective gradients are degenerate (only one nonzero component). The pattern still looks fine because any smooth pseudo-random heightmap satisfies the visual intent; do not try to replicate the bug.

## Layout assumptions

Fully pixel-count-relative; works on any strip length. Designed for 1D (or a matrix mapped as a serpentine treated per-row); only a 1D renderer is provided.

## UI controls (all sliders; several act as toggles)

- Noise type (toggle-style slider): below midpoint = square-grid Perlin, above = triangular-grid simplex (slightly different terrain character).
- Scale: noise frequency — larger = finer-grained terrain (roughly one to five noise cells across the strip); triggers heightmap recalculation.
- Motion: viewport panning speed/amount through the noise field (zero = static terrain).
- Auto-color (toggle-style): switch between rotating rainbow stripes and the fixed three-hue fire scheme.
- Auto-color palette: rotates the hue offset used in auto-color mode.
- Number of stripes: one to a handful of simultaneous sweeping stripes.
- Stripe speed: how fast stripes flow along the contours.
- Stripe weight: stripe thickness, implemented as the sub-stripe blanking described above (max = solid, lower = progressively thinner).
- X offset: scrubs the viewport horizontally to a different part of the fixed landscape; triggers recalculation.
- Show progress (toggle-style): superimpose the white sweep-phase indicator.
- Bass threshold: zero disables sound reactivity; higher values require louder bass to trigger the fast-forward bursts.
