# Stairmaster 2D
kind: 2D
sensors: no

## What it looks like

An escalator scene on a 2D matrix: a diagonal staircase of a handful of steps
(around four visible across the panel) marches steadily across/up the display,
each step showing a soft vertical brightness gradient. A glowing round ball sits
at the horizontal center and bounces up and down, brightest at its middle and
fading toward its rim, timed so it bounces roughly once per step of the
escalator — like something hopping on a stairmaster. Color is a rainbow sweep
across the panel's width (hue follows horizontal position); toward the base of
the stairs the color washes out to near-white, getting more saturated higher up.

## Algorithm

Setup: the whole display is shifted down by a modest fraction of the panel height
(a one-time translate) so the action sits better in frame.

State between frames: none beyond two per-frame scalars.

Per frame:
- A stair phase: a sawtooth clock with a period of several seconds. This one clock
  drives the staircase's motion.
- A ball height: a sine-shaped wave on a clock several times faster than the stair
  clock (faster by the same factor as the number of steps, so one bounce per step
  passing), scaled and offset so the ball oscillates over roughly the lower third
  of the panel, dipping slightly below its resting line at the bottom of the bounce.

Per pixel (2D renderer):
1. Staircase brightness: quantize the horizontal position (plus the stair phase)
   into step columns — multiply by the step count, floor, divide back — giving a
   staircase-shaped level per column. Brightness = (vertical position + stair
   phase) minus that quantized level. Because the same phase is added to both the
   vertical term and the horizontal quantization input, the whole staircase
   translates diagonally over time — the escalator effect. Within each step the
   result is a vertical gradient ramp; across steps the ramp resets, drawing crisp
   diagonal stair edges.
2. Ball brightness: distance from the pixel to the ball center (fixed at the
   horizontal middle, vertical position = the per-frame ball height). Inside the
   ball radius (roughly an eighth of the panel width) brightness ramps from full
   at the center to zero at the rim; outside it is zero.
3. Combine stairs and ball by taking the maximum of the two brightnesses.
4. Color: hue = horizontal position (rainbow across the width); saturation = a
   value slightly above one minus the vertical position, so pixels at one vertical
   extreme are oversaturated (clamped to full) and pixels at the other end
   desaturate toward white — per the design intent, the base of the stairs washes
   out. Value = the combined brightness.

## Colors

Rainbow across the width (left-to-right hue sweep), blending to whitish at the
stair base. The ball takes whatever hue sits at the panel's center column. Black
background between stair edges.

## Controls

None. Step count, ball radius, and the master speed are internal constants at the
top of the pattern, clearly intended for hand-tweaking.

## Timing

Escalator drifts through one full cycle over several seconds; ball bounces about
once a second.

## Layout assumptions

Needs a 2D pixel map with normalized coordinates; designed for square matrices.
The author's note: on non-square layouts, apply an axis scale to fix the aspect
ratio. Step count is a hardcoded small integer — the obvious generalization is to
expose it (and ball size / speed) as sliders.

## Notes / clever bits

- The whole staircase is one line of math: floor-quantizing x into levels and
  subtracting from y yields stair-shaped iso-lines, and feeding the same scrolling
  phase into both axes animates it diagonally.
- Bounce rate is derived from the stair clock divided by the step count, so ball
  hops and passing steps stay locked in sync regardless of the master speed.
- Deliberately over-driving saturation above its valid range and letting the
  renderer clamp is the trick that confines the desaturation to one end of the
  panel.
