# slowflies
kind: 1D
sensors: no

## What it looks like
Soft green "fireflies" — small glowing humps of light — appear at random places along the strip,
drift slowly (some leftward, some rightward), swell to full brightness and fade away over a few
seconds. Where a fly has been, it leaves a dim blue-violet ghost trail that lingers and fades
over several more seconds. The overall feel is calm and organic: at any moment a handful of
green glows are creeping in both directions over a faint blue afterimage of their recent paths.
(The source is machine-generated from a declarative pipeline description, but the behavior below
is complete.)

## Algorithm
All heavy work happens once per frame into full-strip scalar buffers; the per-pixel render just
reads them out.

### Two pulse generators
There are two identical particle generators, differing only in drift direction. Each maintains a
small fixed pool (about eight slots) of live pulses. State per pulse: birth time, a random start
position (uniform over the strip, in normalized 0..1 coordinates), and a random drift velocity.
One generator draws velocities from a narrow positive band (a few percent of the strip length per
second, rightward); the other from the mirrored negative band (leftward).

Each generator also keeps a "next spawn time". When the running clock (accumulated from frame
deltas, in seconds) passes it and a free slot exists, a new pulse is born in the first free slot
and the next spawn time is set roughly one second ahead (uniformly jittered by about ±20%). So
each generator emits about one new fly per second, up to its pool cap.

### Pulse life and shape
A pulse lives for a few seconds (about three). Its intensity envelope over its lifetime is a
triangle: linear rise to a peak at half-life, linear fall to zero. Its current center position is
start + age × velocity. A pulse is also culled early once it has fully drifted off the end of the
strip it is moving toward.

Each frame, each generator clears its own intensity buffer (one float per pixel) and, for every
live pulse, adds a spatial hump: the hump spans a fixed width of about one tenth of the strip,
centered on the pulse position and clipped to the strip; across that span the profile is half a
sine arch (zero at both edges, peak in the middle), scaled by the triangular lifetime envelope.
Overlapping pulses add.

### Combining and trails
- A "flies" buffer = per-pixel sum of the two generators' buffers.
- A "trail" buffer implements peak-hold with exponential decay: each frame every pixel becomes
  the maximum of (its old value decayed exponentially with a half-life of a few seconds) and the
  current flies value. So trails are seeded at the flies' brightness and halve every few seconds.
- Final RGB per pixel: channelwise maximum of two colorings —
  (flies value × pure vivid green) versus (trail value × a dim blue-violet whose blue component
  dominates, with a whisper of green and no red).
- At output, each channel is squared (simple gamma correction), which makes the dim trails read
  as properly faint.

## Randomness
Uniform draws for: spawn position (full strip), drift speed (narrow band, sign fixed per
generator), and inter-spawn interval (about one second ±20%).

## Colors
Flies: saturated pure green. Trails: dim indigo/blue-violet (mostly blue, a trace of green).
Because the combination is a per-channel max rather than a sum, a bright fly simply outshines its
trail rather than blending with it.

## Timing feel
New flies about once a second per direction; each fly lives about three seconds; trails halve in
brightness every few seconds so a path stays faintly visible for maybe ten seconds.

## Layout assumptions
Fully normalized — buffers are sized from the pixel count; nothing hardcoded except the pool size
and shape parameters, which are fine as constants.

## Quirks worth knowing (from the generated code; reimplement the *intent*)
- The generated slot-management is sloppy: the intended live-pulse counter is never actually
  updated (a stray unrelated global is incremented instead), so the pool cap is enforced only by
  the "find a free slot" scan. Also, both the free-slot search and the live-pulse iteration stop
  at the first dead slot, which silently assumes pulses die in birth order; a pulse culled early
  from the middle of the pool can orphan later slots. A clean reimplementation should just manage
  the pool correctly (proper live flags or compaction) — the visual intent is simply "up to N
  concurrent pulses per direction".
- Off-strip culling checks only the edge the pulse drifts toward, which is correct given the
  fixed sign of each generator's velocity.
