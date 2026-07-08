# Scary Pumpkin
kind: 2D
sensors: no

## What it looks like

A Halloween jack-o'-lantern rendered on a 2D matrix. A round pumpkin body fills most
of the panel with a slowly swirling, fiery orange texture that appears to spiral
inward like a tunnel. Cut into it are a classic face: two triangular eyes, a
triangular nose, and a wide crescent smile. The face features glow vivid green and
flash erratically — bursts of flicker with irregular pauses, like guttering
candlelight or a malevolent blink. Everything outside the pumpkin's circular
outline is black.

## Algorithm

Coordinate setup: the mapped 2D coordinates are re-centered so the panel origin is
the middle (a global translate applied once at startup). All geometry is expressed
in normalized panel units.

State between frames: a monotonically accumulating time base in seconds (wrapped
after about an hour to avoid precision loss), fed by the frame delta.

Per frame:
- Two noise-scroll offsets derived from the time base times a speed factor (the
  second axis scrolls at half the rate of the first, which keeps the fire texture
  from looking like a rigid translation).
- A "flash" scalar for the face: take the tangent of a cosine of (time plus a
  fast sine wiggle of time). The nested oscillators at very different rates make
  the argument sweep chaotically; the tangent then spikes and dips unpredictably.
  This yields organized-looking random flashing from a pure deterministic formula —
  no random number calls, so it is smooth in time and free of frame-to-frame strobe
  noise.

Per pixel (2D renderer):
1. Compute radial distance from center. If it is beyond the pumpkin's radius
   (a little over half the panel's half-width, i.e. the pumpkin nearly fills the
   panel), output black and return early — a cheap bounding test that skips all the
   expensive work for corner pixels.
2. Face mask via signed-distance functions (SDFs), all in centered coordinates:
   - Two triangle SDFs (point-up wedges) placed symmetrically left/right in the
     upper half: the eyes.
   - One triangle SDF at center, slightly below the eyes: the nose.
   - A crescent ("moon") SDF — a disc with a second disc cut out of it — evaluated
     with the x/y axes swapped so the crescent opens upward: the smile.
   A pixel is "face" if it is inside any of the four shapes (negative SDF).
3. If the pixel is in the face: fixed green hue, full saturation, brightness = the
   per-frame flash scalar. The whole face flashes in unison.
4. Otherwise (pumpkin body): tunnel-warp then noise.
   - Tunnel warp: rotate the pixel about the panel center by an angle inversely
     proportional to its radius (a fixed constant divided by the radius). Pixels
     near the center get twisted far more than the rim, so any texture sampled
     afterwards appears to spiral into a tunnel. (This trick turns any texture
     into a tunnel effect.)
   - Sample fractal Brownian motion built on 3D perlin noise at the warped
     coordinates offset by the two scroll timers, with the third noise axis also
     driven by time. A few octaves, gain a little under one.
   - Brightness = the absolute noise value scaled down substantially, with a small
     floor so the body never goes fully black.
   - Hue is derived from brightness: dim areas sit at deep red-orange, brighter
     areas shift toward yellow-orange (hue roughly proportional to brightness plus
     a tiny base offset). Saturation is near full, easing off slightly as
     brightness rises. Net effect: a fire palette from ember red through orange to
     hot yellow, driven by one scalar.

## Colors

- Body: fiery palette — near-black embers through deep red-orange to bright
  yellow-orange; never desaturates to white.
- Face: single vivid green, flashing in brightness only.
- Outside the pumpkin: black.

## Controls

One adjustable numeric setting exposed as an exported/watchable variable (not a UI
slider): the animation speed of the fire texture. Default is a gentle drift. The
face flash rate is independent of it.

## Timing

Fire texture drifts slowly (noticeable movement over a second or two, no hard
period). Face flashes at an erratic few-times-per-second cadence with irregular
dark gaps.

## Layout assumptions

Requires a 2D pixel map with normalized coordinates; intended for square-ish
matrices. All face geometry is in normalized units, so it scales with panel size,
though a strongly non-square aspect will distort the face (fixable by scaling the
coordinate axes to compensate for aspect ratio).

## Notes / clever bits

- The tangent-of-nested-oscillators flicker: deterministic chaos as a substitute
  for random flashing.
- Radius-dependent rotation as a universal "make it a tunnel" post-transform.
- SDF composition (triangles + crescent) for crisp vector-like face shapes on a
  low-res matrix.
- The early-out radius test as a performance guard.
