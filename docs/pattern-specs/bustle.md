# bustle
kind: 1D
sensors: no

## What it looks like
A busy two-way stream of comet-like pulses on a strip, all in the red/magenta family on black.
Two visually distinct populations, each spawning from both ends of the strip:

- **Wide, fast magenta pulses** (each about a fifth of the strip long) that cross the whole
  strip in a couple of seconds, launched every second and a half or so.
- **Narrow, slower crimson/pink-red pulses** (about a tenth of the strip long) that take
  several seconds to cross, launched about once a second.

Every pulse is brightest at its leading edge with a quadratic tail fading behind it, so they
read as comets. Multiple pulses of each kind can be in flight at once and freely pass through
each other; where they overlap, the display shows whichever is brighter rather than adding, so
crossings never blow out to white. The result is a continuous, slightly irregular "traffic"
feel — hence the name.

## Algorithm
This is a generated/compiled pattern (four instances of one "pulser" template plus a
max-combiner), all computed in the per-frame stage into full-strip buffers; the per-pixel
renderer just reads the buffers.

State kept between frames, per pulser (there are four): a fixed-size pool of pulse slots
(around ten) with an alive flag and a birth timestamp per slot, plus the scheduled time of the
next launch, and a shared running clock accumulated from frame deltas (in seconds).

Per frame, for each of the four pulsers:
1. Clear that pulser's full-strip intensity buffer.
2. If the clock has reached the next launch time and there's a free slot, mark one slot alive,
   stamp its birth time, and schedule the next launch at the current time plus a randomized
   interval: a fixed mean (about one and a half seconds for the wide pulsers, about one second
   for the narrow ones) plus modest approximately-Gaussian jitter built by summing three
   uniform random draws and centering.
3. For each live pulse, compute its age and from it a center position that moves linearly along
   the strip. Each pulser is one direction/speed combo: the two wide ones start just off one
   end or the other and cross the full strip in roughly two-and-a-bit seconds; the two narrow
   ones likewise start off-strip at each end but travel at half that speed. Start positions sit
   slightly beyond the strip ends so pulses slide on and off smoothly. When a pulse has fully
   exited the far end, kill its slot.
4. Rasterize the pulse into the buffer: over the window of pixels covered by the pulse width,
   compute the pixel's relative position within the window and apply a one-sided quadratic
   ramp — squared ramp rising toward the direction of travel — so intensity peaks at the
   leading edge and falls off as the square toward the trailing edge. Contributions from
   overlapping pulses of the *same* pulser add together.

Then combine: for each pixel, build R/G/B buffers by scaling each pulser's intensity by its
color (see below) and taking the **per-channel maximum** across the four pulsers.

Per pixel in the render stage: read the three channel buffers and output them squared
(gamma-style shaping for LED response).

Randomness: only in launch scheduling (interval jitter). Pulse paths and shapes are
deterministic.

## Colors
Black background. Wide fast pulses: magenta (full red with blue at most of red's strength, no
green). Narrow slow pulses: crimson / pinkish red (full red with only a small amount of blue,
no green). The squaring at output deepens the tails toward black.

## Controls
None.

## Layout assumptions
Fully 1D and resolution-independent: everything scales off the strip's pixel count. The pulse
slot pool size is hardcoded to a small constant, which is fine in practice. One latent bug
worth knowing: the code that tracks how many pulses are alive per pulser increments/decrements
a *shared* counter instead of each pulser's own, so the per-pulser "pool full" limit is never
actually enforced by the counter — the free-slot search is what really bounds it. A
reimplementation should just track live counts per pulser correctly (or rely solely on the
slot search).

## Non-obvious tricks
Using per-channel max instead of additive blending is the signature choice: crossing comets
occlude rather than sum, keeping colors saturated. The near-Gaussian launch jitter (sum of
three uniforms) keeps spawning organic without ever bunching too hard.
