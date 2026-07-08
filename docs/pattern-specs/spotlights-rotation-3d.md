# spotlights / rotation 3D
kind: 3D (primary), with 2D and 1D fallbacks that project into the 3D math
sensors: no

## What it looks like
On a 3D-mapped installation: a bright double cone of light (two cones tip-to-tip
at the center of the volume, like a searchlight beam through the middle) sweeps
and tumbles through the space. The cone's core is white-hot, its surface fades
through vivid color to black with a soft sub-pixel edge, and hue varies across
the installation by physical position, so the beam picks up different rainbow
colors as it crosses different parts of the space. The tumbling is continuous
but not perfectly smooth — the rotation axis itself wanders, giving a lively,
slightly erratic spotlight feel. On a flat 2D map it looks like someone waving
a flashlight across a projection surface; on a strip it becomes a frenetic
swooping bright region.

## Algorithm
State between frames: a three-by-three rotation matrix, rebuilt every frame.

Per frame:
- Three oscillators produce the components of a rotation axis. Each is a
  triangle wave rescaled to swing between minus one and plus one, and the three
  run at slightly different periods (all on the order of one to a few seconds at
  default speed, deliberately close together so the axis precesses slowly and
  never repeats obviously). Triangle waves are used instead of sinusoids on
  purpose — the author found pure sines "almost too smooth."
- A fourth oscillator, a plain sawtooth with a somewhat shorter period, supplies
  the rotation angle, sweeping through a full turn each cycle.
- The axis vector is normalized to unit length and a standard axis-angle
  rotation matrix (the classic Rodrigues / Wikipedia form) is computed and
  stored.

Per pixel (3D):
- Shift coordinates so the origin is the center of the mapped volume (subtract
  a half from each of x, y, z).
- Apply the frame's rotation matrix to the shifted point, yielding rotated
  coordinates.
- Compute a signed cone field: the absolute value of the rotated vertical
  component, minus the radial distance in the rotated horizontal plane where
  both horizontal components are divided by a width parameter before the
  root-sum-square. Positive means inside the double cone; negative outside.
  (Using different divisors per axis would make elliptical cones — the shipped
  version uses the same divisor for both.)
- Clamp the field to the range minus one to plus one (without the clamp the
  brightness curve explodes — the author flags this as a fun thing to try).
- Color with HSV where:
  - hue = the pixel's centered, UNrotated horizontal coordinate, so hue is
    painted onto the world (spanning the hue wheel once across the width, with
    the wraparound putting magenta-ish tones near center) and stays put while
    the cone sweeps through it;
  - saturation = one minus the field value, so deep inside the cone
    (field near plus one) it desaturates to white, and the surface is vivid;
  - brightness = (one plus the field) raised to the fourth power, normalized
    scale — a very steep curve, so outside the cone falls to black almost
    immediately, the boundary gets a naturally antialiased soft edge, and the
    interior blooms bright.

2D fallback: call the 3D renderer with depth fixed at zero (a planar slice of
the tumbling cone). 1D fallback: call the 3D renderer with the pixel's
normalized strip position stretched to double range as the x coordinate and the
other two coordinates zero.

## Controls
- Slider, "scale" / spotlight width: quantized to about six discrete steps;
  larger values widen the cones considerably. Internally the chosen step is
  divided by roughly ten (pi squared) to get the width divisor.
- Slider, "speed": quantized to two or three coarse steps; scales all four
  oscillator periods together, from a gentle sweep to a fast tumble.

## Timing feel
At defaults the beam completes a full rotation in a second or two while the
axis wanders over several seconds; the combination reads as continuous chaotic
tumbling with no visible loop.

## Notes for the implementer
- The rotation matrix should be computed once per frame, not per pixel — this
  is the pattern's main performance trick.
- The hue coming from the unrotated coordinate (not the rotated one) is what
  makes colors appear fixed to the installation while the light moves.
- No layout hardcoding; everything works in normalized world units and assumes
  the map occupies the unit cube.
