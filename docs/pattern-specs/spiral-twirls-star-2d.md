# spiral twirls star 2D
kind: 2D + 3D (isometric projection onto the 2D scene) + 1D (horizontal slice fallback)
sensors: no

## What it looks like
A six-pointed star (hexagram / Star-of-David outline shape) filled with rotating,
swirling rainbow spiral arms; everything outside the star is black. The spiral arms
(one to three of them, selectable) rotate continuously while their twist winds up in
one direction, unwinds, and rewinds the other way in a slow breathing oscillation.
Brightness falls off toward the star's edge and the arms have sharp leading edges with
dark gaps between them. Independently, the star mask itself spins at an adjustable
speed and direction. Colors drift slowly through the wheel unless the color-speed
control is parked.

## Algorithm
State between frames: only slider-set parameters plus a small "adjustable timer"
record (a phase offset and a current interval) used for the star's rotation.

Per frame:
- Sample three clock phases: a twist oscillation (a sine-shaped value swinging
  between minus one and plus one), a rotation phase (sawtooth), and a color-shift
  phase (sawtooth). Their periods come from the speed sliders.
- Reset the coordinate transform, recenter the map so the origin is the middle,
  scale so coordinates span roughly minus-one to plus-one, and rotate the whole
  scene by the star timer's current phase (a full turn per timer cycle).

Per pixel (2D):
1. Star mask: evaluate a signed-distance function for a hexagram of the
   slider-chosen size, centered at origin. If the signed distance exceeds a small
   slider-chosen threshold, output nothing (black). The threshold effectively fattens
   the star outline outward, softening/expanding its edge.
2. Inside the mask, convert to polar coordinates: radius from center and a
   normalized angle in zero-to-one.
3. Add to the angle the radius times half the current twist value — this shears the
   angular coordinate progressively with radius, producing the spiral wind/unwind.
4. Form an arm coordinate: twisted angle times the arm count, minus the rotation
   phase (plus a constant to keep it positive), keeping only the fractional part.
5. Brightness: (slightly more than one, minus the radius) times a nonlinear shaping
   of the arm coordinate — the lower half of the arm coordinate is cubed while the
   upper half is left linear. The cubic half creates a dark wedge with a soft ramp;
   the discontinuity at the wrap creates the crisp arm edge.
6. Hue: the arm coordinate plus the initial-color offset, compressed to about half
   the wheel, plus the drifting color-shift phase. Full saturation.

3D renderer: applies a fixed, experimentally tuned isometric projection — shrink the
3D coordinates, offset them, and combine them linearly into a 2D point — then renders
that point with the 2D logic. Gives a reasonable view of the star on 3D-mapped
installations.

1D fallback: renders the horizontal line through the scene at mid-height, pixel index
mapped linearly across x.

## Colors
Rainbow-based but only about half the wheel is on screen at once, starting from a
slider-chosen base color and continuously drifting through all hues over time. Black
background outside the star; dark gaps between arms; brightest at the star's center.

## Controls (all sliders)
- Star size: scales the hexagram mask from tiny up to nearly filling the display.
- Line width / edge thickness: how far beyond the exact star boundary pixels still
  light (response is squared so most of the travel is subtle).
- Twist speed: how fast the spiral winds and unwinds. Inverse mapping — higher
  slider means faster; at zero the twist effectively freezes.
- Rotation speed: how fast the arms sweep around; same inverse mapping.
- Initial color: the base hue offset; with color speed stopped the pattern holds
  this color family.
- Color speed: how fast hues drift; inverse mapping, zero freezes color.
- Arms: snaps to one, two, or three arms of symmetry.
- Star rotation: bidirectional and roughly logarithmic. Slider center is almost
  stationary; moving toward either end spins the star mask faster, with the left
  half spinning the opposite direction from the right half.

## Timing feel
Twist oscillation and color drift: tens of seconds at default settings. Arm rotation:
several seconds per revolution. All speeds strongly slider-dependent.

## Clever bits
- The star mask is a signed-distance function built by folding the plane across the
  hexagram's mirror symmetries (absolute values plus two reflection steps using
  hexagonal-lattice direction vectors), then measuring distance to one edge segment —
  the standard SDF trick, cheap enough to run per pixel.
- Changing the star's spin speed uses a phase-matching timer: when the interval
  changes, the stored phase offset is adjusted by the difference between the old and
  new clock phases, so the star never visibly jumps — it just smoothly changes speed.
  Negative intervals run the phase backwards for reverse spin.
- Note for reimplementers: the speed sliders' zero-guard comparison is buggy in the
  original (an assignment where a comparison was intended), so the "at zero" branch
  never actually runs; a division by zero yields an effectively infinite period,
  which coincidentally still freezes the motion. Implement the intended behavior:
  slider at zero means stopped.
