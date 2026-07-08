# GlowFlow (3D coord transform API port)
kind: 3D (only; requires a 3D pixel map)
sensors: yes — accelerometer (required), overall sound energy (on by default), ambient
light level (off by default)

## What it looks like
The fixture appears to be a container half-full of glowing rainbow liquid. Whichever
way you physically tilt the object, the liquid "surface" stays level with the real
world: the lower half of the volume glows in saturated layered rainbow bands (hue
changes with depth, like strata in the liquid), and just above the surface there is a
reddish-orange glow that fades quickly to black with height, like a sharp sunset
horizon. When sound plays, the liquid "fizzes": scattered pixels briefly flash
brighter and slightly whiter, like carbonation popping, with intensity tracking the
music. Response to tilting is smooth and slightly damped — the liquid sloshes into
place over a fraction of a second rather than snapping.

## Algorithm
State between frames:
- Smoothed gravity vector: each accelerometer sample is folded into a running value
  with a first-order low-pass (exponential/IIR) filter weighted mostly toward the
  previous value, so jitter is suppressed. The axes are also remapped and sign-flipped
  to match the author's physical build orientation (a re-implementer should expose or
  document this remap; it is build-specific).
- A per-pixel array of "spark" values for the sound fizz.
- A proportional-integral controller's accumulated state for automatic microphone
  sensitivity.

Per frame:
1. From the smoothed gravity vector, compute two spherical angles: how far the vector
   tilts from vertical (polar/latitude) and which way around it points (azimuth/
   longitude). Both computations use arctangent with explicit guards for the
   degenerate axis-aligned cases (avoids divide-by-zero and is faster). Both angles
   are then offset by a quarter turn to center them.
2. Set up the coordinate transform for this frame: reset it, translate the map so the
   unit-cube coordinates are centered on the origin, optionally apply two fixed
   rotations that stand a cube fixture on its corner (a source-level flag, off by
   default), then rotate about two axes by the azimuth and (negated) polar angles.
   The net effect: the renderer's vertical axis is aligned with real-world gravity.
3. Sound processing (when enabled): a PI controller adjusts a sensitivity gain so
   that, averaged over the display, roughly a percent of full brightness comes from
   sparks — this auto-adapts to quiet rooms and loud parties alike. The controller's
   error input is a small target fill level minus last frame's measured average spark
   brightness; its output is floored at a modest minimum. Then every pixel's spark
   value decays each frame (a small time-proportional leak plus an amount scaled by
   current sound energy and sensitivity), and any value that reaches zero is re-seeded
   to a random amount proportional to current sound energy times sensitivity. Louder
   sound therefore both re-seeds sparks hotter and burns them down faster — that churn
   is the fizz.
4. If the light sensor option is enabled, an overall brightness multiplier is taken
   from ambient light (clamped away from zero); otherwise the multiplier is one.

Per pixel (3D renderer, receiving transformed coordinates):
- Spark brightness for this pixel = its spark value, scaled up a few-fold and squared
  (squaring makes only strong sparks visible). The clamped result is also accumulated
  into the feedback total the PI controller reads next frame.
- If the pixel's transformed vertical coordinate is below the mid-plane (inside the
  liquid): hue is proportional to depth below the surface, clamped just short of
  wrapping the color wheel, at full brightness — producing the layered rainbow.
- If above the surface: hue is fixed at the extreme red/orange end; brightness is one
  minus height, cubed, so the glow hugs the surface and cuts off sharply.
- Saturation is full, minus a small amount from the spark brightness (capped), so
  fizzing pixels whiten slightly.
- Final brightness is multiplied by the ambient-light factor.

Layout assumptions: needs true 3D map coordinates in the unit cube; nothing depends on
pixel count except the per-pixel spark array. Requires the sensor expansion hardware.

## Colors
Liquid: full saturated rainbow, red near the surface descending through the spectrum
toward violet at the deepest points (never wrapping back to red). Above the surface: a
narrow reddish-orange horizon glow fading to black. Sparks: brief brightening with a
slight shift toward white.

## Controls
No UI controls. Four behavior switches exist only as constants in the source: enable
sound reactivity (default on), enable ambient-light dimming (default off), corner-
stand rotation for a cube build (default off), and the accelerometer smoothing amount.
A re-implementation could reasonably promote these to toggles/sliders.

## Timing feel
Tilt response settles in a fraction of a second (smoothing filter). Sparks flicker at
frame rate with lifetimes of roughly a fraction of a second to a couple of seconds
depending on loudness. With no sound and no tilting, the image is static.
