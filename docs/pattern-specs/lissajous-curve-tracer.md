# Lissajous curve tracer
kind: 2D
sensors: no

## What it looks like
A single bright dot traces a Lissajous figure (a woven loop/figure-eight family of curves) across a 2D mapped surface, leaving a glowing trail behind it. Depending on a persistence setting, the trail either fades away within a fraction of a second (a classic oscilloscope-tracer look) or never fades, so the dot gradually "paints" the whole closed curve as a solid glowing figure. The dot's color slowly drifts back and forth around a user-chosen base color, so long-lived trails show a subtle rainbow gradient along their length. At default settings the dot completes its circuit in on the order of a second; the speed slider spans from a blur to a leisurely crawl of many seconds.

## Algorithm
State kept between frames:
- One brightness value per pixel (the persistence/trail buffer).
- One hue value per pixel (remembers the color the trail was painted with at that spot).
- The dot's current 2D position.
- The current frame's hue.
- Two "adjustable-speed clocks" (see Clever bits).

Per frame:
1. Compute the dot position from the first clock's phase: the horizontal coordinate is a sine of (phase × integer ratio A of a full turn, plus a phase offset "delta"); the vertical coordinate is a sine of (phase × integer ratio B of a full turn). This is the standard Lissajous parametrization; both coordinates span roughly −1..+1.
2. Compute the frame hue: the picked base hue plus a triangle-wave excursion centered on it. The excursion's amplitude is the hue-shift slider; its rate comes from the second clock. So the hue sweeps smoothly up and down around the base color.
3. Set up the coordinate transform: shift the mapped 0..1 coordinates so the origin is at the center, then scale up by a bit more than 2× so the ±1 curve fills the map, with slightly extra scale when the dot is large so fat dots don't clip at the edges.

Per pixel (2D renderer):
1. Euclidean distance from this pixel to the dot.
2. "Closeness" = one minus (distance × a density factor), clamped to 0..1. The density factor is inversely controlled by the dot-size slider, so a big dot means a gentle wide falloff and a small dot means a tight point.
3. New stored brightness = the maximum of (old stored brightness × a per-frame fade factor) and the closeness. The fade factor comes from the persistence slider; near the top of that slider it is effectively 1 (no fade, permanent paint).
4. If closeness is nonzero (the dot is touching this pixel this frame), overwrite that pixel's stored hue with the current frame hue.
5. Output: stored hue, the picked saturation, and stored brightness scaled by the picked brightness.

Randomness: none — fully deterministic.
Layout: no pixel-count hardcoding; works on any 2D map (needs a 2D map — there is no 1D renderer).

## Colors
Entirely user-chosen: a color picker sets the base color (hue, saturation, brightness). The hue then oscillates around that base by an adjustable amount, giving trails a gentle two-toned or rainbow-edged gradient. Unpainted background is black.

## Controls
- Color picker ("dot color"): base hue/saturation/brightness of the dot and its trail.
- Slider ("hue shift amount"): how far the hue wanders from the base color (zero = fixed color, high = broad rainbow sweep).
- Slider ("hue shift speed"): how fast the hue wanders (strongly eased — most of the range is slow).
- Slider ("persistence"): from short-lived tracer trails to permanent paint that never fades.
- Slider ("dot size"): from a tight point to a broad soft blob (eased response).
- Slider ("speed"): how fast the dot travels (strongly eased, cubic-feeling response).
- Slider ("A"): the horizontal frequency ratio, quantized to small integers (roughly one through eight).
- Slider ("B"): the vertical frequency ratio, quantized to small integers (roughly one through eight).
- Slider ("delta"): the horizontal phase offset, quantized to eight evenly spaced steps around a full cycle.

Changing A, B, or delta clears the trail buffer (a built-in flag can disable this), so the newly shaped curve starts on a clean canvas.

## Timing feel
Dot circuit time: from nearly instantaneous to several seconds via the speed slider. Trail decay: from a fraction of a second to infinite. Hue wander: from a fraction of a second per sweep to many seconds.

## Clever bits
- Adjustable-speed clocks that don't jump: the pattern wraps the engine's sawtooth time source in a tiny utility storing (phase offset, interval). When a speed slider changes the interval, it measures the old and new sawtooth values and folds the difference into the stored phase offset, so the dot's position is continuous across speed changes instead of teleporting.
- Trails remember their own color: because hue is stored per pixel at paint time and brightness decays independently, a slowly drifting hue leaves a color-history gradient along the trail rather than the whole trail changing color at once.
- The max(faded old, new closeness) update is what lets trails coexist with the freshly drawn dot without additive blowout.
