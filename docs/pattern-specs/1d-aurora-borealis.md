# 1D Aurora Borealis
kind: 1D
sensors: no

(Ported to Pixelblaze from an open-source Arduino aurora simulation. The author
notes it looks best diffused up a vertical surface, e.g. LEDs at the base of a
wall facing upward.)

## What it looks like
Several soft, glowing blobs of aurora color drift slowly along the strip over
black, each fading in as it is born, peaking in brightness mid-life, and fading
out again over tens of seconds. Blobs occasionally and randomly reverse their
drift direction, the way real aurora curtains waver. Where blobs overlap, their
colors blend translucently. Palette is northern-lights themed: greens,
turquoise, pink, purple.

## Algorithm
### State
A fixed pool of wave records (capacity around ten; a slider chooses how many
are active). Each record stores: age, lifetime, a palette color index, a base
opacity, a half-width, a center position (normalized to the strip), a drift
direction (plus or minus), and a drift speed.

### Per frame
For each active wave:
1. With a small probability per frame (a few percent), flip its direction.
2. Advance its center by speed times direction. (Note: this step is per frame,
   not scaled by elapsed time, so drift rate is frame-rate dependent —
   acceptable to reproduce, or fix by scaling with the frame delta.)
3. Accumulate the frame delta into its age.
4. The wave dies if its age exceeds its lifetime, or if it has drifted entirely
   off either end of the strip (center plus/minus half-width beyond the ends).
5. A dead wave immediately respawns with fresh random parameters:
   - lifetime: uniform in a range on the order of fifteen to thirty seconds;
   - color: chosen from the palette by weighted random draw using the currently
     selected weighting preset;
   - base opacity: uniform between about a half and one;
   - half-width: uniform between about a tenth of the strip and the
     slider-controlled maximum (up to about half the strip);
   - center: uniform anywhere on the strip;
   - direction: coin flip;
   - speed: proportional to the speed slider times a uniform factor, tuned so a
     blob takes many seconds to cross the strip.

Note the pool starts zeroed, so on startup every wave is instantly "dead" and
respawns randomly on the first frame.

### Per pixel
Start from black and alpha-composite each active wave in pool order:
- Skip waves whose center is farther from this pixel than their half-width.
- Compute an opacity for this wave at this pixel as the product of three
  factors:
  1. **base opacity** (the wave's own random constant);
  2. **radial envelope**: one minus the square root of (distance from center
     divided by half-width) — a dome that is broad in the middle with soft
     feathered shoulders;
  3. **age envelope**: a triangle function applied to the square root of
     (age divided by lifetime) — brightness ramps up quickly after birth,
     peaks around mid-life, and declines toward death (the square root skews
     the peak earlier and the fade longer).
- Blend the wave's palette color over the running mix using standard
  alpha-over compositing in RGB.

Output the final mixed RGB.

## Palette (qualitative)
Five fixed stops: a deep grass green; a bright chartreuse green; a
green-leaning turquoise; a warm rose pink; a soft violet purple. Three
weighting presets select how often each is picked: (1) all equally likely,
(2) pink and purple strongly favored, (3) greens strongly favored (this is the
default).

## Controls (all sliders)
- **Speed**: scales drift speed of newly spawned waves, roughly one-to-five
  range.
- **Width**: sets the maximum half-width new waves may spawn with, from narrow
  blobs up to about half the strip.
- **Palette**: a quantized slider acting as a three-way preset picker for the
  color weightings above.
- **Number of waves**: quantized slider from one up to the pool capacity.

Slider changes affect waves as they respawn (existing waves keep their
parameters until they die), except the wave count, which takes effect
immediately.

## Timing feel
Individual blobs live tens of seconds; the overall scene evolves slowly and
never repeats. Direction reversals give an organic shimmer on a several-second
cadence.

## Quirks worth knowing
- The active-count check is off by one in the original (the comparison is
  exclusive where it should be inclusive), so the effective number of active
  waves is one more than the slider's nominal value. Reproduce or quietly fix.
- Compositing is order-dependent (later pool entries paint over earlier ones),
  which is invisible in practice.
- No pixel-count hardcoding; all geometry is normalized.
