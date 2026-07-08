# Blue Holiday Candle 2D
kind: 2D
sensors: no (despite the catalog tag, the code reads no sound or sensor inputs — it is purely time-driven)

## What it looks like
A stylized candle on a 2D matrix: a blue-cored flame with an orange/yellow rim sways gently side to side above a blue candle body at the bottom of the display. The flame's blue core shimmers internally with a fast fine-grained flicker; the sway is slow, organic, and non-repeating-feeling (multi-octave). The rest of the display is a very dim deep purple night background, in which a small handful of warm-white "stars" twinkle — each fades through a soft pulse over a second or few, then reappears at a new random location.

## Coordinate setup
The pattern uses the 2D map. At startup it shifts the coordinate origin to the center of the display and flips the vertical axis so that the candle sits at the bottom (the author notes the flip may be removed for displays that don't need it). All geometry below is in these centered coordinates, roughly −½..+½ on each axis.

## Algorithm
Per-frame state and work:
- A running seconds clock (wrapped after an hour to avoid precision loss).
- Flame sway signal: the sum of three sine oscillations at octave-related periods (each successive one twice as fast and contributing half as much), recentered around zero. Longest period is several seconds. This gives natural, non-metronomic movement.
- A fast internal-flicker phase derived from the clock (runs a few times faster than real time).
- Star twinkles: a small fixed number (three) of twinklers. Each holds: a target pixel index (uniform random over the whole display), a "life" value counting down from full to zero, and a per-twinkle random decay rate (so some twinkle fast, some slow — roughly a second to a couple of seconds per pulse). When life hits zero the twinkler respawns at a new random pixel with a new rate.
- Clever bit: each frame, an index array over the twinklers is sorted ascending by their target pixel index. Because the renderer visits pixels in ascending index order, render can keep a single cursor into this sorted list and advance it as matches are found — a one-pass merge instead of scanning all twinklers per pixel. The cursor resets to the start each frame.

Per-pixel work (given centered x, y):
1. Candle body: a brightness contribution that is nonzero only in the lower band of the display (below roughly the lower two-fifths line) and within a horizontal extent slightly narrower than the display, tapering toward the sides. This contribution feeds the blue channel only.
2. Aspect correction: stretch the x coordinate by nearly a factor of two so the flame reads tall and narrow rather than round.
3. Sway distortion: displace x by a sinusoidal function whose phase depends on the y coordinate times the multi-octave sway signal, with the displacement amplitude growing with height above the base — so the flame tip whips around more than its root.
4. Inner core (blue): a soft radial blob — distance from the origin minus a small radius, skewed by a vertical term so the blob stretches upward. Convert to a 0..1 softness over a narrow falloff band. Then modulate it by a triangle-wave flicker whose phase mixes the fast time phase with fine spatial frequencies in both x and y (different multipliers per axis), swinging the core intensity roughly ±half — this is the shimmering "burning" texture inside the flame.
5. Outer shell (orange rim): the absolute distance from a circle about a third of a unit in radius, passed through a smooth threshold over a narrow band — a soft annulus surrounding the core.
6. Color composition (RGB, channel by channel):
   - Red: a small share of the core plus a strong share of the shell (the rim is dominantly red/orange).
   - Green: a modest share of the core plus a shell share that increases toward the bottom of the flame — so the rim is yellow near the base and redder near the tip.
   - Blue: the full core plus the candle-body contribution.
7. If the summed color is above a tiny threshold, the pixel is "part of the flame/candle" and outputs that RGB. Otherwise:
   - If the pixel index matches the next active twinkler (via the sorted-cursor scheme), draw a warm near-white star whose brightness follows a smooth sine hump of the twinkler's remaining life (fades in, peaks, fades out) at moderate peak brightness; advance the cursor.
   - Otherwise output the background: fully saturated violet/purple at very low brightness.

## Colors (qualitative)
- Flame core and candle body: blue (core whitened somewhat by its red/green admixture — reads as bright blue-white at center).
- Flame rim: orange, shading to yellow at the flame's base.
- Stars: warm white (faintly peach-tinted), gentle pulse.
- Background: deep dim purple, just above black.

## Controls
None exported. Two internal tunables an implementer may expose: the number of twinkling stars (fixed at three) and their base twinkle speed.

## Layout assumptions
Requires a 2D mapped display; assumes normalized 0..1 map coordinates before the recentering transform. Star positions use raw pixel indices, which is layout-agnostic. Nothing depends on a specific matrix size.
