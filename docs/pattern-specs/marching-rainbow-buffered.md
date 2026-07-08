# marching rainbow (buffered)
kind: 1D
sensors: no

## What it looks like

Broad rainbow-colored bands of light march along the strip, while a much finer,
faster ripple travels in the opposite direction and periodically carves dark
notches through the bands. The interplay of the two opposing waves makes the
bright regions swell, split, and re-merge rather than simply scroll. Hues sweep
smoothly through the whole rainbow, but the color gradient along the strip is
warped — it compresses and stretches as the slow wave moves — so the rainbow
breathes rather than sliding rigidly.

## Algorithm

This pattern is explicitly a demonstration of frame buffering: all per-pixel math
is done once per frame in the pre-render step and stored in arrays, and the
per-pixel render callback is nothing but two array lookups feeding HSV. (The
stated motivation: clockless LED protocols need the render loop to be as fast and
jitter-free as possible, so precompute everything.)

State between frames: two arrays sized to the pixel count — one for hue, one for
brightness. (They are fully overwritten every frame, so they are buffers, not
evolving state.)

Per frame, loop over every pixel index:
- Two sawtooth clocks: a slow one (period of several seconds) and one twice as
  fast.
- Wave A: sine-shaped wave of (slow clock + pixel's fractional position). One
  spatial cycle across the strip, drifting in one direction.
- Wave B: sine-shaped wave of (fast clock − pixel position stretched by roughly
  tenfold, plus a small constant phase nudge). About ten spatial cycles across the
  strip, moving the opposite direction (note the minus sign on position), and
  faster.
- Brightness buffer entry = Wave A minus Wave B. Where the fine wave exceeds the
  broad wave the result is negative and renders as black (the renderer clamps),
  producing the moving notches; elsewhere it is the broad band attenuated by the
  ripple.
- Hue buffer entry = a sine-shaped wave of (a sine-shaped wave of Wave A, minus
  the pixel position). The double wave-warping of the slow wave is what bends the
  otherwise-linear position-to-hue rainbow, making the gradient undulate.

Per pixel (render): look up the buffered hue and brightness, emit HSV at full
saturation.

## Colors

Full-spectrum rainbow, fully saturated, over black gaps. No fixed palette.

## Controls

None.

## Timing

Slow band motion: one cycle over several seconds. Counter-moving ripple: roughly
twice the temporal rate and ten times the spatial frequency, so it feels
distinctly busier than the bands.

## Layout assumptions

Pure 1D; arrays sized from the pixel count and positions expressed as fractions of
strip length — scales to any strip. Nothing hardcoded.

## Notes

Modest visual complexity; its real point is the buffering idiom (heavy math in the
per-frame hook, O(1) lookups in the per-pixel hook). An implementation should
preserve that structure, not just the look. The one non-obvious visual ingredient
is using the difference of two opposing waves (relying on negative values clamping
to black) instead of a product, which gives harder-edged notches and richer
interference than multiplication would.
