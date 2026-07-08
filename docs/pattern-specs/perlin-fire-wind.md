# perlin fire wind
kind: 2D
sensors: no

## What it looks like

A noise-driven fire effect on a 2D panel, with the whole flame column swaying
side to side as if a breeze is bending it. Tongues of black/red/orange/gold
flicker and stream continuously along the height axis; the flame is
concentrated in a column near the horizontal center of the panel and fades
toward the left and right edges. The sway is a slow, smooth back-and-forth on
a several-second feel; the fine flicker never visibly loops (the underlying
noise animation takes several minutes to repeat).

## Algorithm

Palette-based. A gradient palette is installed once at startup (see Colors).

Per frame:

- Sample a few sawtooth time bases: two on a several-second period driving
  the wind sway, one long one (several-minute period) that sweeps the noise
  field's third coordinate across its full repetition interval so the
  flicker animation is seamless and very long before repeating, and a
  medium-long one (on the order of a minute or two) that steadily advances
  the noise field's vertical coordinate, making the noise appear to stream
  along the height axis like rising flame.
- Set up a coordinate transform: recenter the horizontal axis on the panel
  middle and scale both axes up by about two, so pixel coordinates span a
  window into a larger noise space.

Per pixel (2D render, transformed coordinates):

1. Wind wobble: compute a sine of (the pixel's height, offset by the two
   several-second time bases — one used directly, one passed through a
   smooth oscillation) and add it to the x coordinate. The wobble amplitude
   is modest (a small fraction of the panel width) and is weighted so it is
   strongest at the low-y edge and shrinks toward the high-y edge — this is
   what makes the flame column bend rather than shift rigidly.
2. Sample multi-octave "turbulence"-style perlin noise (absolute-value
   fractal sum, a few octaves, roughly halving amplitude per octave) at
   (wobbled x, height compressed by half plus the streaming offset, the slow
   time coordinate). Roughly double the result.
3. Multiply by a triangle-shaped horizontal window centered on the panel
   middle: full strength at center, tapering linearly toward the sides.
   Clamp below at zero.
4. Multiply by normalized height (zero at one horizontal edge of the panel,
   full at the opposite edge), so intensity ramps linearly across the height
   axis — the flame is rooted-dark at one edge and brightest at the other.
5. Clamp the result at one so the palette lookup never wraps around from the
   top color back to black.
6. Paint from the palette: the clamped value indexes the palette, and the
   *square* of the value drives brightness — squaring darkens the low end so
   the dim red regions stay smoky instead of washing out.

No per-frame state beyond the time bases; no randomness other than the fixed
perlin noise field. No pixel-count assumptions — it is purely
world-coordinate driven. There is no 1D renderer.

## Colors

Fixed gradient palette, qualitatively: starts at black, rises quickly to a
pure deep red about a fifth of the way in, holds through red into orange
(red with growing warmth) across most of the range, and ends at a pale warm
gold approaching white. Classic fire ramp.

## Controls

- One read-only numeric display exporting a "mode" number. Vestigial: the
  source contains four selectable noise flavors (plain perlin, ridged,
  fractal-sum, turbulence) and code that would cycle among them rapidly, but
  the selection is immediately overridden so the turbulence flavor is always
  used. Implement only the turbulence path; the display and the other three
  modes are dead weight and may be dropped or kept as a debug indicator.

## Notes

The clever parts: (a) the height-weighted sinusoidal x-offset that bends the
flame like wind; (b) exploiting the noise field's smooth wraparound over its
natural repetition interval to get many minutes of seamless animation from a
single sawtooth time base; (c) squaring the palette index for the brightness
argument to keep the dark end moody.
