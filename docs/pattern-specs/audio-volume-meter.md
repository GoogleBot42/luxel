# Audio Volume Meter
kind: 1D
sensors: yes

## Purpose and overall look

A multi-section VU meter for a horizontal strip, driven by a sensor board's audio spectrum. The strip is divided into five (or, via toggle, three) equal sections; each section is a bar-graph that fills with the loudness of one frequency range. The outer sections fill inward from the strip's ends; the middle sections fill outward from their centers. Bars rise and fall smoothly (heavily smoothed on purpose), with color either slowly cycling through the rainbow, fixed to a chosen hue, or a static per-pixel rainbow. An automatic gain loop keeps roughly a third of the strip lit on average regardless of source volume.

## Sensor inputs

A 32-band audio spectrum array only. (No energy scalar, no accelerometer.)

## Section layout

Five-section mode assigns frequency ranges, left to right: sub-bass, treble, mids, low-mids, mid-bass — i.e. bass at the two outer ends, brighter content in the middle. Fill directions: first section fills start→end, the three middle sections fill from their midpoints outward in both directions, the last section fills end→start. Three-section mode keeps the two outer bass sections and merges the middle three into one wide center section (filling from its midpoint) driven by the loudest of the low-mid/mid/treble measurements. All section boundaries, midpoints, each pixel's "fill fraction" within its section, and each pixel's frequency-range assignment are precomputed once at startup into per-pixel lookup arrays sized from the actual pixel count (nothing hardcoded; comments show how to hand-edit boundaries).

## Audio pipeline (runs on a periodic refresh timer, roughly every few dozen ms, not every frame)

1. Subtract a tiny quiet-room noise floor from each raw spectrum bin (clamped at zero), then average adjacent pairs of bins into half as many values, multiplying by the current auto-gain sensitivity.
2. Apply four EQ multipliers by region of the paired bins: the lowest couple of pairs get the bass multiplier, then one pair low-mids, several pairs mids, the rest treble. Defaults strongly de-emphasize bass and strongly boost treble (bass energy dominates raw spectra).
3. Squelch: any value below a small threshold snaps to zero (kills flicker). Surviving values are coarsely quantized (truncated to about two significant digits) and rescaled as a fraction of a "max energy" cap.
4. Collapse to five display measurements: the two lowest pairs stand alone (sub-bass, bass); the next regions each take the **maximum** (not average) of a couple of pairs for low-mids, mids, and treble. All clamped to the unit range.
5. Temporal smoothing: each display measurement is blended with its previous value, previous value weighted a few times heavier than the new one.
6. A sixth measurement = the loudest of the low-mid/mid/treble trio, used by the wide center section in three-section mode.

Between refreshes, every frame multiplies the display measurements by a decay factor just under one, so bars slide downward smoothly rather than stair-stepping.

## Auto-gain (the clever part)

A tiny proportional-integral controller adjusts the sensitivity multiplier every frame. The render pass accumulates the total brightness actually emitted across the strip; the error term is (target lit fraction — about a third — minus actual average brightness). The integral term accumulates the error (clamped between a floor of one and a cap of a few thousand); output = proportional gain × error + integral gain × accumulated value, floored at a small minimum. Quiet sources therefore drift up in sensitivity until about a third of the strip is dancing; loud sources drift down. A read-only gauge control reports current sensitivity as a fraction of its maximum.

## Render (per pixel)

Look up the pixel's section fill-fraction and its section's display measurement; the pixel is nominally full-on if its fill fraction is below the measurement, else off. Then blend with the pixel's previous frame value, previous frame weighted by the user's blending slider (heavier = longer trails/softer edges); store the blended value for next frame and add it to the auto-gain feedback accumulator *before* clamping it to the unit range for display. This gives every bar a soft leading/trailing edge instead of a hard cutoff.

Hue selection:
- **Cycle mode (default):** a global hue slides back and forth along a triangle wave over several seconds; each section is offset from it by a small per-section multiple of a slowly oscillating shift, and a pixel's brightness nudges its hue a hair, so lit bars shimmer with near-neighbor hues rather than being flat.
- **Static mode:** a color slider snaps to one of a few preset hues (red / green / blue / purple regions of the slider); sections still get the small per-section shimmer offsets.
- **Rainbow mode:** static mode with the slider at its very bottom maps each pixel's fill fraction across the color wheel plus the slow drift — a fixed rainbow along each bar.

Everything is fully saturated.

## State kept between frames

Per-pixel previous brightness array; the five-plus-one smoothed display measurements and their previous-refresh copies; refresh timer; PI-controller accumulator and current sensitivity; brightness feedback total; UI values.

## UI controls

- **Toggle — static color:** cycle hues vs. fixed color.
- **Slider — color:** picks among preset hues (bottom of range = per-pixel rainbow mode).
- **Sliders — bass / low-mid / mid / treble balance:** four EQ gains, each mapped over roughly an octave-to-several-fold range (treble reaching the largest boost).
- **Toggle — three sections:** switch between the five- and three-section layouts.
- **Slider — blending:** inter-frame brightness smoothing, from crisp to long soft fades.
- **Slider — decay:** how slowly measurements fall between refreshes (low values can flicker).
- **Gauge — sensitivity:** displays the auto-gain level.

## Timing feel

Bars respond within a refresh tick (a few dozen ms) but glide up and down over a few tenths of a second thanks to the triple smoothing (refresh blend + per-frame decay + render blend). Hue cycling sways back and forth over a slow triangle cycle lasting tens of seconds. Auto-gain adapts over seconds.
