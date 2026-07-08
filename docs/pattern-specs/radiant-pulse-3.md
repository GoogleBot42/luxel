# radiant pulse 3

kind: 3D (implements only the 3D entry point, but ignores depth — effectively a planar/2D effect that any 3D or 2D mapping can run)
sensors: no

## What it looks like

(Author's own summary: "Slow pulses radiate outward/inward. Morphing between beams and three leaves.")

On a mapped 2D/3D fixture, soft luminous bands pulse concentrically around the center of the layout — sometimes flowing outward, sometimes inward, sometimes hanging almost still — while the shape of the bands morphs between concentric rings, straight radial beams, and a rotating three-lobed clover/flower form. The brightest cores of each band wash out toward pastel/near-white; edges stay richly saturated. The base color drifts through the entire rainbow over the course of several minutes. Individual pulses pass on the order of seconds; pulse speed itself breathes — accelerating, decelerating, and reversing direction — over the long cycle. The whole composition repeats only after several minutes.

## Algorithm

Almost stateless: one master clock, everything else is per-pixel math.

Setup: shift the world coordinates by half in both planar axes so the origin sits at the center of the mapped layout.

Per frame: sample a single very slow sawtooth master clock — full period on the order of five minutes.

Per pixel:
- Convert the two planar coordinates to polar form: radius from center, and angle around center. (The 3D entry point's third spatial coordinate and its incoming angle argument are both unused — the angle is recomputed from the planar coordinates, with the two arguments to the arctangent given in swapped order versus convention, which just rotates/mirrors where "angle zero" points. Depth being ignored means on a true 3D map every horizontal layer shows the same image.)
- Build a phase value as the sum of three terms, then wrap it modulo one and take a triangle wave of it; that triangle is the raw brightness:
  1. **Global pulse driver**: a sine-shaped function of the master clock, amplified by a large factor (around twenty). Because a full unit of phase equals one pulse, this term alone sweeps through roughly twenty pulse cycles per swing of the slow sine — that is where the seconds-scale pulsing comes from, and why the pulsing speeds up, slows, stalls, and reverses as the slow sine crests.
  2. **Angular lobes**: a sine wave of the angle scaled to three lobes around the circle, with its angular phase driven by another sine of the master clock times a large factor (mid-teens) — so the three-leaf shape spins, again with breathing speed.
  3. **Radial term**: the radius multiplied by a signed coefficient that oscillates (via the master clock's sine, recentered around zero) between roughly minus-three-and-a-half and plus-three-and-a-half. Sign determines whether pulses read as radiating outward or inward; magnitude sets ring density. When the coefficient passes near zero the radial dependence vanishes and the angular lobes dominate — this is the "morph between rings and beams/leaves."
- Square the triangle-wave brightness to sharpen the pulses and deepen the gaps.
- Saturation: a constant of about one-and-a-half minus the (squared) brightness, clamped by the HSV call — so dim regions are fully saturated and the brightest cores desaturate toward roughly half, giving pastel/whitish pulse centers.
- Hue: the sum of (a triangle wave of the normalized angle, weighted to about a fifth of the hue wheel) + (the radius, also weighted to about a fifth of the wheel) + the master clock itself. Result: at any instant, hue varies gently with direction and distance from center; over minutes the whole palette cycles once around the entire wheel.

No pixel-count or dimension hardcoding; everything derives from normalized world coordinates, so it scales to any mapped fixture. Only assumption: a coordinate map exists and is roughly centered after the half-unit shift.

## Colors

The full rainbow, visited slowly: at any moment the display is dominated by one region of the hue wheel with adjacent hues fanned across angle and radius. Bright pulse cores go pastel toward white; troughs go black. Every color eventually appears over the multi-minute cycle.

## Controls

None.

## Non-obvious details

- All motion at every timescale is derived from **one** slow clock by multiplying its sine by large factors — fast pulses, lobe rotation, and radial-direction flips are all harmonically locked, and their speeds all breathe together as the slow sine changes slope.
- The mod-then-triangle step turns an unbounded, multi-term phase into repeating soft-edged bands; the squaring then shapes them into pulses.
- The brightness-dependent saturation (constant minus brightness) is a cheap way to get white-hot pulse centers without a palette.
