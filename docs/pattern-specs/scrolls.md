# scrolls
kind: 1D
sensors: no

## What it looks like
On a black strip, soft glowing mounds of sea-green light fade up at random positions,
hold, and fade away — like bioluminescent patches surfacing and sinking. Several are
alive at once; where mounds overlap or reach full strength, the bright aqua-green core
shifts into a deeper blue-teal. Calm, ambient, ocean-like. No scrolling motion despite
the name — pulses stay where they spawn.

## Algorithm
This is generated code in a "field then palette" style: each frame builds a scalar
intensity field over the whole strip, maps it through a color gradient into per-pixel
RGB buffers, and the per-pixel render just looks its value up (an obvious
simplification is to fold the gradient lookup into the per-pixel render and skip the
RGB buffers).

### State kept between frames
- An accumulated wall-clock (summing per-frame deltas, in seconds).
- A fixed pool of roughly ten pulse slots, each holding: alive flag, birth time, and a
  position (fraction of strip length) drawn uniformly at random when spawned.
- The earliest time the next pulse may spawn.

### Per frame
1. Advance the clock by the frame delta.
2. **Spawning**: if the spawn cooldown has elapsed and a free slot exists, activate one
   pulse: give it a fresh uniform-random position along the strip, stamp its birth
   time, and set the next allowed spawn to a fixed short interval later (a large
   fraction of a second). At most one spawn per frame.
3. **Aging and accumulation**: clear the intensity field, then for each live pulse:
   - Compute its age as a fraction of a fixed lifetime of several seconds; retire it
     when the fraction passes one.
   - **Temporal envelope**: a trapezoid — a triangle wave over the life fraction,
     doubled and clamped at one. So the pulse ramps up over the first quarter of its
     life, holds at full for the middle half, and ramps down over the last quarter.
   - **Spatial profile**: a half-sine bump centered on the pulse's position, spanning
     about a fifth of the strip (clipped at the strip ends): zero at the bump's edges,
     one at its center.
   - Add (envelope x profile) into the intensity field for every pixel under the bump.
     Overlapping pulses sum.
4. **Coloring**: clamp each pixel's summed intensity to about two-thirds, then scale it
   back up by one-and-a-half (so the clamp ceiling lands exactly at the gradient's
   top), and evaluate a fixed six-stop gradient (see Colors) independently per color
   channel with linear interpolation between stops, storing per-pixel RGB.

### Per pixel (render)
Read the pixel's RGB from the buffers and output each channel *squared* — a gamma-style
curve that deepens the dark end and keeps the glow soft-edged.

### Steady state
Lifetime divided by spawn interval equals the pool size, so after the first several
seconds the pattern settles at a full pool: one pulse dying and one being born at every
spawn tick, keeping roughly ten mounds alive continuously.

### Layout
Fully proportional (positions and widths are fractions of pixel count); nothing
hardcoded to fix.

## Colors
A gradient from intensity zero to one, qualitatively:
black, staying black through the first fifth — then rising through a dark muted teal —
to a bright spring-green/aqua peak at mid-scale (the most luminous stop) — then easing
down into a deep blue-leaning teal that holds steady across the top of the range.
So a lone pulse at full strength shows a deep-teal core ringed by a bright aqua-green
band, fading through dark teal to black at its skirts. No red anywhere.

## Controls
None.

## Timing
New pulse a couple of times per second; each pulse lives several seconds with roughly
symmetric fade-in/fade-out quarters. The whole field feels slow and breathing.

## Non-obvious bits
- The clamp-then-rescale before the gradient means stacked overlapping pulses don't
  blow past the palette — they saturate into the top-of-gradient deep teal, which is
  what creates the darker cores inside bright overlaps.
- The dead zone at the bottom fifth of the gradient makes pulse skirts vanish into
  true black instead of a lingering dim haze.
- The original's slot bookkeeping is buggy (a live-count that never increments, and
  slot scans that stop at the first dead slot, briefly freezing later pulses out of
  rendering until the hole refills). A reimplementation should just do honest pool
  management — spawn into any free slot, age every live slot — which matches the
  intended behavior described above.
