# Austin FC
kind: 1D
sensors: no

## What it looks like
A field of green embers (Austin FC "verde" team color) drifting along the
strip in both directions. Many small sparks — roughly one per ten pixels —
each glide along, gradually slow down, and leave short fading trails; when one
coasts to a stop it silently respawns somewhere else at a fresh random speed
and direction. Sparks wrap from one end of the strip to the other. The overall
impression is a calm, continuously shimmering green particle drift. It is an
acknowledged rework of the classic "sparks" idea, tuned slower, longer-lived,
and end-to-end wrapping.

## Algorithm
State kept between frames:
- A population of sparks sized proportionally to the strip (about a tenth of
  the pixel count, plus one). Each spark has a signed velocity and a
  fractional position.
- A full-strip float buffer of accumulated intensity.

Per frame:
1. Scale the frame delta down by an order of magnitude, then decay the whole
   intensity buffer by about ten percent per frame (this forms the trails).
2. For each spark:
   - If its velocity has decayed into a small dead zone around zero, respawn:
     draw a new velocity uniformly from a symmetric band (up to a modest
     maximum in either direction — so direction is random and speeds vary),
     and place it at a uniformly random pixel.
   - Apply drag: multiply the velocity by a factor just under one each frame,
     so sparks exponentially coast to a stop over a couple of seconds.
   - Advance position by velocity × scaled delta; if it runs off either end,
     wrap to the opposite end.
   - Deposit the spark's velocity value (signed, as-is) into the intensity
     buffer at the spark's integer position, additively.

Per pixel (render):
- Brightness = the buffer value squared, boosted by roughly an order of
  magnitude; fixed green hue at full saturation. Squaring hides the faint
  residue and makes moving sparks pop; it also means the sign of deposits
  cancels out visually — though note that leftward (negative-velocity) sparks
  deposit negative values, which *subtract* from any positive residue they
  cross, an unremarked interaction that slightly dims collisions between
  opposing trails.

Randomness: respawn position and respawn velocity (magnitude and direction).

Layout assumptions: purely index-based 1D; spark count already scales with
strip length, so nothing needs fixing for different sizes.

## Colors
Single hue: a saturated green (soccer-club verde). Only brightness varies —
black background, dim green trails, bright green spark heads. No palette, no
hue motion.

## UI controls
None. Tuning values (spark hue, spark density, drag strength, top speed,
respawn dead-zone) are top-of-file constants meant for hand editing.

## Timing feel
Each spark's glide-and-fade life cycle lasts a couple of seconds; trails
evaporate in a fraction of a second. Continuous and unsynchronized — there is
no global rhythm, just steady ambient drift.

## Non-obvious details
- Spark brightness comes directly from its speed (the deposited value is the
  velocity), so sparks naturally dim as they decelerate and fade out right
  before respawning — no separate life/brightness bookkeeping.
- This is a simple pattern overall: particle drift with exponential drag,
  additive deposit, exponential trail decay, monochrome output.
