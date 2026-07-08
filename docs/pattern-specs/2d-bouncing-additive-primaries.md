# 2D Bouncing Additive Primaries
kind: 2D
sensors: no

## What it looks like
Three soft glowing disks — one pure red, one pure green, one pure blue —
ricochet around a 2D panel, reflecting off all four edges like billiard
balls. Where disks overlap, their light adds: red over green makes yellow,
green over blue makes cyan, red over blue makes magenta, and all three
together burn to white. Each disk is brightest at its center and fades
smoothly to nothing at its rim. At default settings the motion is fast and
energetic — a disk crosses the panel in well under a second.

## Algorithm
State per disk (three disks, stored as parallel arrays over the disk index):
x position, y position, x velocity, y velocity, all in the normalized unit
square. Fixed per-disk colors: the three additive primaries at full strength.

Initialization (randomized once at startup):
- Positions are stratified so the disks start spread out: the unit range is
  split into thirds, and disk N starts somewhere uniformly random within the
  Nth third — independently for x and y.
- Velocities: each disk's horizontal velocity is a moderate baseline (about a
  quarter of the panel per step) plus a symmetric random jitter of roughly
  two-thirds that size (so it can be slower, faster, or occasionally
  reversed). The vertical velocity is initialized as that disk's horizontal
  velocity plus another independent jitter of the same size — biasing the
  disks toward diagonal motion.

Per frame: for each disk and each axis, add the velocity (scaled by the
speed control) to the position. If the position leaves the unit range, clamp
it to the boundary and negate that axis's velocity (a hard reflective
bounce).

Important quirk: motion is applied once per frame with no scaling by the
frame's elapsed time, so the speed depends entirely on frame rate (faster
hardware = faster balls). The obvious fix is to scale each step by the frame
delta and re-tune the baseline as distance per unit time.

Per pixel: start with black, then for each disk compute the Euclidean
distance from the pixel's (x, y) to the disk center. If it is inside the
disk radius, compute a shade of one minus distance-over-radius, squared (a
soft quadratic falloff), and add shade times the disk's color into the
running red/green/blue accumulators. Emit the accumulated color (overlaps
simply sum; the engine clamps anything over full).

Randomness: startup placement and velocities only; motion afterwards is
deterministic.

Layout: any 2D mapped layout; nothing hardcoded. There is no 1D renderer.

## Colors
Exactly the three additive primaries, one per disk, on black. All secondary
colors and white arise purely from additive overlap — never chosen
explicitly. This is the whole point of the pattern (a demo of additive color
mixing).

## Controls
- Slider "Ball radius": disk radius in panel units, from vanishingly small up
  to a disk spanning the whole panel; default about half the panel.
- Slider "Ball speed": multiplies all velocities; zero freezes the disks.

## Timing
Frame-rate dependent (see quirk). At the default half-strength speed on
typical hardware, disks bounce across the panel several times per second —
lively, almost frantic; turn the speed slider down for a lava-lamp feel.

## Non-obvious points
- The quadratic edge falloff makes the disks look like glowing spots rather
  than hard circles, and makes the additive overlaps blend smoothly.
- Stratified starting thirds guarantee the disks never spawn on top of each
  other, so the additive mixing reveals itself progressively.
- Disks do not interact with each other — only with the walls.
