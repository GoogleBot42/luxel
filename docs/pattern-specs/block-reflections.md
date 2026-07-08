# block reflections
kind: 1D
sensors: no

## What it looks like
Chunky, banded blocks of color arranged symmetrically outward from the center of the
strip. The bands repeat like reflections in facing mirrors: their count and width
continuously stretch and compress (sometimes a few fat blocks, sometimes many narrow
ones), the palette of the blocks slides around the color wheel, and brightness sweeps
across the blocks so edges between them read as dark seams. The motion feels like
slowly breathing, folding glass — geometric rather than flowing.

## Algorithm
No persistent state; each frame recomputes four phase values from free-running
sawtooth timers of different speeds (two of them scaled to full-circle angles):

- a base-hue phase (several-second cycle, used through a sine),
- a same-speed linear phase used for both the block-size wobble and a brightness bias,
- a much slower phase (tens of seconds) driving the dominant zoom via a triangle wave,
- a medium phase driving a secondary zoom wobble via a sine.

Per pixel:

1. **Signed center distance**: the pixel's offset from the strip midpoint, normalized
   by strip length (so it runs from about minus one-half to plus one-half). This is
   what makes the pattern mirror-symmetric about the center — everything downstream
   depends on distance from center with sign.
2. **Zoom factor**: the slow triangle wave scaled up to roughly ten-fold, plus a
   sinusoidal wobble of about half that reach. Multiplying the signed center distance
   by this zoom sets how many bands fit on the strip; as the triangle rises and falls
   the bands multiply and merge.
3. **Block quantization**: the zoomed distance is taken modulo a time-varying block
   size — a value hovering around one-third of the hue circle, wobbling by a modest
   fraction via a triangle wave. The modulo folds the smooth ramp into repeating
   sawtooth segments: these segments are the visible "blocks". (Note the modulo is
   signed, so the two halves of the strip fold in opposite directions — the mirror
   effect.)
4. **Hue** = sine of the base-hue phase (a value swinging through both signs, which
   simply wraps around the hue circle) plus the folded block value. So all blocks share
   a drifting base color, and each block spans a slice of adjacent hues.
5. **Saturation**: always full.
6. **Brightness** = the fractional part of (absolute hue + absolute block size + the
   linear phase), then squared. Because hue varies per block, each block gets a
   different brightness that ramps and wraps over time; the squaring deepens the dark
   phase so blocks blink through dark seams rather than dimming linearly. The wrap
   points of this fractional value create hard light/dark boundaries that travel
   through the pattern.

Output in hue/saturation/value space. Fully layout-proportional (uses pixel count and
midpoint only); no hardcoding to fix.

## Colors
Full-spectrum, but at any instant only a window of related hues is on screen: blocks
step through neighboring hues around a shared base color that itself drifts around the
whole wheel. Always fully saturated — no white or pastel, just vivid color and black
seams.

## Timing
Base hue drift and brightness sweep: several seconds per cycle. Block-count breathing:
slow, on the order of half a minute per full expand/contract. Secondary zoom wobble:
in between. The unrelated periods keep it from visibly repeating.

## Non-obvious bits
- The entire "blocks" structure comes from a single signed modulo of a zoomed
  center-distance ramp — there is no explicit block list.
- Feeding the (already position-dependent) hue back into the brightness formula is what
  makes brightness break up per-block instead of sweeping smoothly; that coupling is
  the pattern's signature.
- Signed distance plus signed modulo yields the mirror symmetry for free.
