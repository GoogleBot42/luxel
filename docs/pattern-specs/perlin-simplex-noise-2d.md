# Perlin/Simplex Noise 2D
kind: 2D
sensors: yes (optional; 32-band audio spectrum from the sensor board, low bands only)

## What it looks like
Think of a random topographic map: a smooth 2D gradient-noise heightfield covers the matrix. The pattern doesn't show the heightfield directly; instead it draws colored contour stripes — bands of equal "elevation" — that continuously sweep through the terrain from low ground to high ground and wrap around. The visual effect is organic, flowing rings and ribbons that ooze across the display, like animated elevation contours. Optionally the whole "camera" drifts in a slow circle over the terrain, so the underlying landscape itself glides by. In auto-color mode each sweeping stripe gets its own solid hue (adjacent stripes step around the color wheel); in the alternate fixed-palette mode the stripes use a small warm set — deep red through orange to amber, a "fire" look. With bass reactivity enabled, each bass hit makes the contour flow lurch forward in a brief fast-forward burst, so the stripes pump with the music.

## Layout assumptions
Matrix width is hardcoded (a 16-wide matrix by default); height is derived as pixel count divided by width. The per-pixel lookup converts the renderer's normalized coordinates back to integer grid indices by multiplying by width/height, so it assumes a regular, full grid whose mapped coordinates land exactly on cell centers. Obvious fixes: make width a configurable constant or derive width/height from the pixel map's extents; or sample the noise field directly at each pixel's normalized coordinates (bilinear or direct evaluation) instead of the integer-grid cache.

## Core structure: cached heightfield
A 2D array sized width-by-height stores one noise value per matrix cell. It is recomputed only when needed: at startup, whenever a zoom/translation control changes, or every frame while camera panning is active. Recomputation evaluates the selected noise function (classic square-grid gradient noise, or simplex/triangular-grid noise — user's choice) at each cell: the cell's fractional position across the matrix, minus the current pan/translation offset, times the zoom factor. While filling the array it tracks the minimum and maximum values; every pixel later normalizes against this observed range, so the rendered field always spans the full zero-to-one range regardless of zoom or seed — every stripe color always appears. This caching is the key performance trick: the (expensive) noise is per-cell per-recalc, not per-pixel per-frame.

The noise is seeded through a permutation-table scramble so a fixed seed gives a repeatable landscape. A reimplementation should simply use standard, textbook 2D Perlin and simplex noise with a seedable permutation table, rescaled to output in the zero-to-one range. (Caution if porting behavior exactly: the original's gradient-table initialization writes all three vector components into the same slot — a bug — so its effective gradients are degenerate; do not replicate that. Standard noise gives the intended look.)

## Per-frame work
- A sweep phase advances on a sawtooth clock. Its period is proportional to the number of stripes and inversely proportional to the stripe-speed control, landing at several-hundred-milliseconds to a few seconds per stripe at typical settings.
- A second, much slower sawtooth (period on the order of a couple minutes) drives circular camera panning: the pan offsets are sine and cosine of that phase, with amplitude proportional to the motion control and inversely proportional to zoom (so panning distance feels similar at any zoom).
- If a bass threshold is set, react to sound (see below) and add the accumulated sound offset to the sweep phase, wrapping.
- Recompute the heightfield if flagged dirty or if panning is on.

## Per-pixel work
1. Look up the cell's cached noise value and normalize it to n in [0,1] using the frame's observed min/max range.
2. Brightness: take a triangle wave of (n minus sweep phase) multiplied by (stripe count × sub-slot count). This creates several bright bands along equal-elevation contours that migrate as the phase advances. Then apply the stripe-thinning gate: conceptually each stripe is divided into a few sub-slots, and all but one sub-slot per stripe is blanked (a modulo test on the floor of the same quantity), leaving thinner, more separated stripes as the weight control decreases. Finally square the brightness for contrast.
3. Hue, auto-color mode: quantize (n minus sweep phase) into stripe-count buckets so each stripe is one flat hue, spread evenly around part of the color wheel, then shift the whole set by the palette-offset control.
4. Hue, fixed mode: exactly three warm hues (deep red / orange / amber) selected by which third of the sweep cycle the pixel's offset phase falls in (implemented with two step functions stacked); stripe count is forced to three.
5. Optional progress bar: if enabled, the last row overlays a small white marker whose horizontal position shows the sweep phase, brightness feathered over a couple pixels.

## Sound reactivity
Conceptual input: the sensor board's multi-band audio spectrum array. The pattern sums a few of the low (bass) bands and compares against a user threshold. When exceeded, it enters a short "fast-forward" state lasting a couple hundred milliseconds, during which each frame adds an extra increment (scaled by frame time and the stripe-speed control) to the sweep phase. Net effect: bass hits kick the contour stripes forward in bursts. A threshold of zero disables all audio processing.

## Controls (all sliders)
- Noise type: below-half chooses classic grid noise, above-half chooses simplex.
- Zoom/scale: larger = finer-grained terrain (more, smaller features); triggers a field recalc.
- Motion: amplitude/enable of the circular camera panning.
- Auto-color on/off (threshold at half): toggles between per-stripe rainbow hues and the fixed three-color fire palette.
- Palette offset: rotates the auto-color hues around the wheel.
- Number of stripes: one to a handful of simultaneous sweeping stripes.
- Stripe speed: how fast contours flow.
- Stripe weight: fatter or thinner stripes (via the sub-slot blanking count).
- X offset and Y offset: manual translation across the noise landscape; trigger recalc.
- Show progress: toggles the white sweep-phase bar on the bottom row.
- Bass threshold: zero disables audio; higher values require louder bass to trigger the fast-forward.

## Timing feel
Contour stripes drift continuously, each band taking roughly a second or a few to migrate one stripe-spacing at default speed; camera panning is glacial (a couple of minutes per orbit); bass bursts are short punches well under half a second.

## Notes
Non-obvious pieces worth preserving: the min/max normalization per recalc (guarantees full palette coverage), the dirty-flag recalc strategy, the sub-slot blanking trick for stripe thinning, and forcing exactly three stripes in fixed-palette mode.
