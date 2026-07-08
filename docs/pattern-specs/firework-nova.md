# firework nova
kind: 3D
sensors: no

## What it looks like
A repeating spherical firework blast inside a 3D volume. Roughly once a second
a thin, bright shell of color erupts from the center of the mapped space and
expands outward to the edges, over black. Just behind the expanding shell,
random pixels flash pure white like trailing sparks. The shell's color is not
uniform: hue varies smoothly across the volume along the main diagonal (a
spatial rainbow gradient), and the whole palette slowly drifts around the color
wheel over several seconds, so consecutive blasts come out in different colors.

## Algorithm
This pattern implements only the 3D renderer. It is essentially stateless — no
per-pixel buffers — driven entirely by two global sawtooth phases computed once
per frame:
- a fast phase cycling in roughly a second (drives the blast expansion), and
- a slow phase cycling several times slower (drives the hue drift).

Per pixel (given normalized 3D coordinates):
1. Recenter the coordinates so the origin is the middle of the unit cube, and
   compute the Euclidean distance from the center. Scale that radius down by
   half so the wave comfortably spans the volume.
2. Hue = the average of the three centered coordinates plus the slow drifting
   phase. This gives a smooth hue gradient along the cube's main diagonal that
   rotates over time.
3. Blast wave: evaluate a triangle wave of (radius minus the fast phase), then
   subtract three quarters. Only the top quarter of the triangle wave survives
   as positive; everything else goes negative. This isolates a thin radial
   band — the shell — whose position sweeps outward as the phase advances.
4. Sparks: evaluate the same clipped triangle expression but with the phase
   offset slightly so it peaks a bit behind the shell, and compare it against a
   uniform random number whose range is double the clipped peak. Where the
   expression exceeds the random draw (a chance rising to roughly one in eight
   at the trailing peak, zero elsewhere), render the pixel as pure white.
5. Otherwise: rescale the surviving quarter-wave back to a zero-to-one range
   (multiply by four) and then cube it. Cubing sharpens the shell into a
   crisp bright band with soft edges, and — crucially — preserves the sign, so
   the negative regions stay negative and clip to black. Render with the
   computed hue, full saturation, and that value as brightness.

Randomness: only the per-pixel-per-frame spark draw.

Layout assumptions: expects a 3D pixel map with coordinates normalized to the
unit cube; the blast originates at the cube center. There is no 1D or 2D
renderer — on unmapped strips it shows nothing. Obvious fix if broader support
is wanted: add 1D/2D fallbacks that treat distance-from-strip-center /
distance-from-plane-center as the radius.

## Colors
Background is black. The shell runs through the full rainbow, distributed
spatially along the volume diagonal and slowly cycling over time; fully
saturated. Sparks are pure white.

## UI controls
None. (A scale constant at the top of the code halves the radius; a natural
enhancement is exposing it as a slider, but the original has no controls.)

## Timing feel
The blast repeats a bit faster than once per second — continuous rhythmic
pulses from the center. The palette drift takes several seconds per full
rainbow revolution, so each blast is tinted differently from the last.

## Non-obvious details
- The whole "expanding shell" is just a triangle wave of radius-minus-time,
  clipped so only its top quarter is visible — cheap, smooth, and periodic
  with no particle state.
- Cubing the renormalized wave both sharpens the shell and doubles as the
  clip-to-black for the negative regions.
- The trailing white glitter reuses the identical clipped wave, phase-shifted
  backward, as a probability field for a random threshold test.
