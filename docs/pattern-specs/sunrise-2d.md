# Sunrise 2D
kind: 2D
sensors: no

Note: despite any library tagging, this pattern reads no sound or sensor inputs. Its only external input is one UI slider.

A 2D matrix simulation of a sunrise followed by an active sun: a glowing disc rises from the bottom of the display, then boils with plasma-like surface activity while flare particles erupt from its edge, loop around under gravity, and fall back in.

## Visual behavior
Three stages, managed by swapping which per-frame routine runs:
1. Sunrise: a warm disc slides up from below the bottom edge to the center over several seconds, its surface already shimmering.
2. Brief pause (a couple of seconds) once fully risen.
3. Active sun (runs indefinitely): the disc's surface churns with moving brighter/darker cells like solar granulation, and a couple dozen spark-like flare particles arc off the limb, curve under gravity, and streak back — leaving short-lived fading trails because the whole canvas cools each tick.

Moving the slider at any time fades the display to black over a second or two and restarts the sunrise.

## Architecture
The pattern renders into an off-screen 2D frame buffer at a fixed simulation tick (on the order of ten updates per second), independent of LED frame rate; the per-pixel render just samples the buffer. Each buffer cell packs both color and brightness into one number: the integer part encodes hue (in thousandths of the hue wheel) and the fractional part is brightness. Rendering splits the number back apart. This packing is the key non-obvious trick — one array carries the full image.

Display dimensions are hardcoded (a square in the mid-teens per side in the original). The obvious fix: derive width/height from the actual matrix or expose them as constants to edit; the buffer is indexed as if the y coordinate selects within a row array, which only works if the matrix is square — generalize the indexing when width ≠ height.

## Algorithm

### Sun disc
At startup, a brightness mask the size of the display is precomputed once: for each cell, distance from center; inside a fixed sun radius (roughly a third of the display width) the mask is a linear falloff from full at the center to zero at the rim; outside it is zero. Precomputing avoids per-frame distance math.

Each simulation tick, the disc is stamped into the frame buffer at a vertical offset (the "how far risen" amount; zero once fully risen). For each cell where the mask is nonzero, surface brightness is an additive plasma: a sine wave of a linear combination of the cell's normalized x/y (with a slowly drifting coefficient), plus a triangle wave of a different x/y combination (with a second slowly drifting coefficient), plus a sine of the mask value itself; average the three and cube the result for contrast. The stored value combines a fixed warm base hue component with brightness that mixes the plasma and the mask (mask-weighted so the center stays brightest). The two drift coefficients oscillate on periods of tens of seconds to a minute-plus, keeping the granulation pattern evolving without repeating obviously.

### Cooling
Every simulation tick, every lit cell's fractional (brightness) part is reduced by a small constant (a tenth-ish per tick), clamped at zero, with the hue part untouched. This makes everything not re-stamped each tick — i.e., the flare trails — fade over roughly a second.

### Flare particles
A couple dozen particles (capacity slightly higher than the active count), each with position, velocity, and a personal hue offset (a small random warm offset). They start at random positions inside the sun with zero velocity — all motion comes from gravity toward the sun's center. Each tick:
- The center of gravity is jittered by a small random amount per axis, which keeps orbits from settling ("stirs" the system).
- Acceleration points from the particle toward the jittered center. Important quirk: although the code looks like it computes a distance-based falloff, the falloff algebraically cancels, so the pull has constant magnitude regardless of distance (beyond a tiny inner cutoff). Reproduce constant-magnitude attraction — it's what produces the characteristic wide looping arcs.
- Velocity integrates the acceleration (scaled way down) and is clamped per-axis to a "speed of light" cap of a few cells per tick; position integrates velocity. Off-screen particles are skipped for drawing but keep simulating (they get pulled back).
- Drawing: a particle only draws when it is beyond roughly nine-tenths of the sun's radius from center, so flares appear to erupt from the limb rather than crawl across the face. It writes its hue (personal offset plus a base hue that drifts very slowly over the run) into the integer part of its cell, and a medium brightness into the fractional part (or keeps the cell's existing brightness if already lit).
- Two particle buffers are alternated (read from one, write to the other, swap pointers) so all particles see a consistent previous-frame state.

### Stage control
- Sunrise stage: vertical offset decreases with time (full rise takes a handful of seconds); when it reaches zero a pause timer runs a couple of seconds, then the active stage takes over. Stamps and cools each tick but runs no particles.
- Active stage: stamp, cool, and move/draw particles each tick.
- Fadeout stage (entered when the slider moves): only cools each tick, so the image dies away; after about a second and a half, reset the rise offset to the bottom and return to the sunrise stage.

## Controls
- Slider, concept "make the sun rise again": the value is ignored; any movement triggers the fadeout-then-sunrise restart.

## Colors
Warm sun palette: base deep orange-red, with the granulation reading as shimmering gold/orange highlights over the disc. Flares are sparks in nearby warm hues (reds through oranges) that drift very slightly over minutes. Background pure black.

## Timing
Simulation tick: order of ten per second. Sunrise: a handful of seconds. Post-rise pause: a couple of seconds. Trail fade: about a second. Surface-drift oscillations: tens of seconds. Restart fade: over a second.
