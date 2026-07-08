# Time Flies 2D
kind: 2D
sensors: no

## What it looks like
A handful of small, brightly colored dots — "flies", each a different hue evenly spread around the color wheel — wander erratically around a black 2D display. Each fly speeds up, slows down, and veers unpredictably like a real insect, and its dot rapidly pulses in size several times a second, reading as flapping wings. Flies loosely congregate around the middle of the display: the farther one strays toward an edge, the harder it turns. Requires a mapped 2D display; the 1D renderer is a deliberate no-op (blank).

## Algorithm
State between frames: a fixed-size collection of flies (about half a dozen; a source constant). Each fly stores: position (x, y in the unit square), heading (a fraction of a full turn), current speed, a private phase into a shared noise function, hue, and current dot radius. Also two accumulated-time counters that gate updates.

Noise source: a hand-rolled 1D "wave noise" — the sum of a triangle wave plus three sine-shaped waves at incommensurate frequencies (roughly 2x, 2.2x, and 5x a base frequency, with fixed phase offsets and differing amplitudes), rescaled to span roughly -1 to +1 with a jagged profile. Deterministic but chaotic-looking. Each fly samples it at its own phase, so all flies share one function but move independently.

Per frame (movement is decoupled from render rate):
- A slow global sawtooth (period in the several-tens-of-seconds range) supplies the per-tick advance of each fly's noise phase. A much faster sawtooth (a few cycles per second, tiny amplitude) supplies the "wing flap": every fly's dot radius = a small base (several percent of the display) plus this fast sawtooth, so the dots rhythmically swell and snap back.
- Positions update on a fixed tick of a few tens of milliseconds; headings update on a slower tick of a few hundred milliseconds (i.e. only every few movement ticks does a fly also steer).
- Movement tick per fly: advance the noise phase (wrapping); add the noise sample to the current speed and clamp between a tiny minimum and a maximum of roughly a tenth of the display per tick (so speed random-walks, often riding its bounds); step the position backward along the heading direction (cosine/sine of the heading as a full-turn fraction) by the speed; clamp position into the unit square.
- Steering tick per fly: compute distance from display center; turn amount = the noise sample times a maximum-turn constant times a factor that grows linearly with that distance (several times larger near the corners than at center). Add to heading, wrap to a full turn. This is the soft containment: central flies fly straight-ish, edge flies whip around.

Initialization: random positions, random headings, random noise phases; hue = fly's index divided by the fly count (even rainbow spread).

Per pixel: scan the fly list; cheap first-pass reject using the absolute value of the sum of the coordinate deltas against the fly's radius; then a true Euclidean distance test. On the first fly whose radius contains the pixel: brightness = one minus (distance scaled so it hits zero at about an eighth of the display), cubed — a sharp bright-centered dot; hue = that fly's hue; stop scanning (no blending, first fly in list order wins overlaps). Pixels owned by no fly are black.

Randomness: only at initialization; all in-flight "randomness" is the deterministic wave-noise function.

Layout assumptions: unit-square 2D mapping. Fly count and sizes are constants — the obvious upgrade is a slider for fly count (and scaling dot size to display resolution).

## Colors
Black background. Each fly a fully saturated pure hue, the set spaced evenly around the whole rainbow. Dots are brightest at center with a steep cubic falloff.

## Controls
None exported.

## Timing
Positions update dozens of times per second; steering a few times per second; wing-flap size pulse a few times per second; the noise-traversal rate itself cycles over several tens of seconds so flies' temperament slowly evolves. Update gating is delta-time based, so behavior is frame-rate independent.

## Clever bits
- One shared additive-wave noise function with per-fly phase offsets replaces per-fly RNG, giving smooth-but-jagged organic motion cheaply.
- Turn-rate proportional to distance-from-center keeps flies on screen without walls or reflection logic (position clamping is just a backstop).
- The "wing flap" is nothing but a fast global sawtooth added to dot radius.
- The Manhattan-style pre-test before the Euclidean distance keeps the per-pixel fly loop cheap.
