# New pattern and scene ideas for Luxel, from an online survey (2026-11-01)

79 ideas that are **not** in `inventory.tsv`. Where an idea overlaps a weak
existing pattern it says **"Reconception of …"** and why it is better. All
descriptions are written in my own words from effect *names and published
one-line descriptions* only. No source code was read, and no distinctive
constant was taken from any source. The Patternflow wall was read for
**titles only**. Ideas that share only a theme with a Patternflow title are
marked `PF-title` so the Patternflow clean-room track can dedupe against them.

## Sources surveyed

- **WLED** full effect list incl. 2D and the v16 particle-system (PS)
  effects (kno.wled.ge/features/effects; PS Volcano, Galaxy, Attractor,
  Waterfall, Pinball, Springy, Sparkler, Hourglass, Ballpit, Impact, Vortex,
  DripDrop; 2D Octopus, Drift Rose, Crazy Bees, Ghost Rider, Black Hole,
  Tartan, Game of Life, PacMan, Julia, Sun Radiation, Plasma Ball, Hiphotic).
- **FastLED** classics and demo reels (DemoReel100, Pacifica, TwinkleFox,
  Pride2015, Fire2012, ColorWaves, CircusMarquee, DiscoStrobe; Stefan
  Petrick's AnimARTrix polar-coordinate family).
- **LedFx** effect modules (blade power, melt and sparkle, water, glitch,
  lava lamp, smoke2d, flame2d, keybeat2d, image spin, concentric, crawler,
  game of life, digital rain, waterfall2d).
- **pixelmatix Aurora** pattern list (Attract, Cube, Electric Mandala,
  Incremental Drift, Invaders, Life, Maze, Munch, Noise Smearing, Pong
  Clock, Rainbow Smoke, Spiro, Spin, Swirl, Infinity, Bounce).
- **Tidbyt community** app directory, visual apps only (aquarium, fishbyt,
  maze, sort, langtonsant, life, 3cellularautomata, tetrisclock, flipclock,
  wordclock, qlock, hilbertclock, mondrianclock, fuzzyclock, pongclock-like
  games, dvdlogo, tvstatic, testpatterns, pleasestandby, yulelog,
  moonphase, daynightmap, yearprogress, starfield, fireflies, fireworks,
  virtualpets, idlegardener, northernlights). Plus Divoom Pixoo: clock faces,
  holiday faces, a pixel-art gallery, notifications and timers.
- **Patternflow community wall** (titles only): Candle light, Haunted
  Clearing, Train, Glimmer Field, Liquid Chrome, Iris Singularity,
  Bioluminescent Plumes, Soap Bubbles, Pampas Night, Fluoroscope, Bokeh,
  Lunation, Hyperspace Ribbons, Psychedelic Underwater Garden.
- **Demoscene** effect canon (Wikipedia "Demo effect": raster/Kefrens bars,
  shadebobs, rotozoomer, textured tunnel, voxel landscape, bump mapping,
  glenz, twister, sine scroller) and classic screensavers (Mystify, Pipes,
  bouncing logo, star trails).
- **Generative-art, physics and nature staples** from general knowledge:
  Truchet, quasicrystals, Lenia, physarum, attractors, differential growth,
  marbling, Chladni, stable fluids, Verlet cloth, the Galton board and others.

## Conventions

- **Dims**: `strip` (60–300 px), `grid` (64x64 / 128x128 panels), `both`.
- **Cool** 1–5 = visual payoff on a wall. **Effort** S/M/L (S ≈ an
  afternoon, M ≈ a day with tuning, L = several days or an engine
  question).
- **Form**: pattern / scene / sprite (+ engine wish if one would help).
- "Cheap because" names the builtins that keep it inside an ESP32 frame
  budget. The engine floor is about 28 ms for an empty per-pixel `render2D`
  at 16384 px. So at 128x128 anything per-pixel must be light, or it should
  simulate on a small canvas and upscale with `fillCanvas`/`paintCanvas`
  (nearest-cell sampling) or `canvasGet` (bilinear).
- 16.16 caveat: values saturate at ±32768. Anything with 1/r, r², or
  accumulating velocities needs softening or clamping.

---

## A. Demoscene and screensaver classics

### 1. Rotozoomer — grid — Cool 4 — S — pattern
A checkerboard or tiled emblem that spins and breathes in and out of the
screen at the same time. Its tiles stream past in hypnotic spirals, and
hues shift with the zoom depth. This is the most recognisable 90s demo
effect, and the library has none.
**Algorithm:** each frame set a rotate+scale (+translate drift) transform,
then per pixel sample a procedural texture at the transformed (u,v). The
texture can be an XOR checker, a hash2 tile pattern, or a 16x16 canvas
painted once. Zoom = an eased oscillation.
**Cheap because:** the coordinate transform stack (`rotate`, `scale`,
`translate`) is applied by the engine before `render2D` sees x,y, so the
pattern body is one `frac`/`floor` plus one colour call. A canvas texture
via `canvasGet` gives smooth bilinear magnification.

### 2. Textured Tunnel — grid — Cool 5 — M — pattern
You fly down an endless tube whose walls carry a scrolling texture: rings,
bricks or noise. The far end is a dark vanishing point with fog, and the
tube twists as the camera's centre wanders. It is distinct from the
library's *Tunnel of Squares*, which draws flat concentric outlines with no
texture, depth fog or camera motion.
**Algorithm:** precompute per pixel angle = atan2 and depth = k / radius
once, into two arrays (the geometry is static). Each frame, sample the
texture at (angle·N + spin, depth + t) and darken by 1/depth. Wander the
centre by offsetting the lookup with a cheap per-row/column correction, or
rebuild the LUT only every few frames.
**Cheap because:** the LUT turns atan2 and the division into array reads.
The per-pixel body is a hash2/wave sample and an `oklch` (lightness = fog).
At 128x128, sample the texture on a 64x64 canvas and `fillCanvas`.

### 3. Copper Bars — both — Cool 3 — S — pattern
Glossy horizontal bars like polished metal tubes swing up and down on
sines. They overlap in z-order and each has a bright specular core fading
to dark edges. The Kefrens mode stacks vertical bars into a writhing
staircase. On a vertical strip it reads as metallic droplets passing each
other.
**Algorithm:** N bars, each with position = beatSin-style oscillation and
phase offsets, drawn back to front as a lightness ramp over a few rows.
**Cheap because:** `fillGradient`/`fillRect` per bar row, and `oklch`
lightness ramps give the chrome look. There is no per-pixel work at all.

### 4. Shadebobs — grid — Cool 4 — S — pattern
A few invisible brushes trace Lissajous paths and *add* heat wherever they
pass. Where the heat overflows it wraps around the palette, so the panel
fills with ever-more-intricate psychedelic contour bands that build up and
then slowly dissolve.
**Algorithm:** a small accumulation canvas (32x32). Each frame stamp each
bob as a soft disc that adds a little, and apply a gentle global decay.
Display = palette(frac(value)), so the wrap-around is the effect.
**Cheap because:** `canvasAdd` (or splat in add mode on a canvas), then
`arrayScale` for decay and `paintCanvas` through a `setPalette` gradient.
The cost is O(bobs·disc) per frame, independent of panel size.

### 5. Voxel Landscape Flyover — grid — Cool 5 — L — pattern
A low flight over rolling mountains in the Comanche style. Ridges in
green/brown/snow scroll toward you under a sunset sky gradient, with
distant peaks fading into haze. It is stunning on a 128x128.
**Algorithm:** a heightmap from noise (procedural, sampled on demand). For
each screen column, march depth from near to far, project height to a
screen row, and draw the vertical span from the last drawn row up to the
new one (painter with a y-buffer). Colour by height and fog by distance.
**Cheap because:** work is columns × depth steps (64 × ~48 ≈ 3k noise
samples), not pixels, and each span is one `fillRect`/`fillGradient`. The
sky is a single `fillGradient`. Effort is L for tuning and for the
fixed-point projection.

### 6. Embossed Light — grid — Cool 4 — M — pattern
A surface like hammered copper or wet stone. A small point light glides
across it and every bump catches a highlight on one side and casts shade
on the other. The surface itself slowly morphs.
**Algorithm:** height = fbm noise. The surface normal comes from the noise
gradient. Note that `curl2` already *is* the gradient rotated 90°, so
gradient = (curl_y, −curl_x). Shade = dot(normal, light − p) with
falloff, plus a specular power term.
**Cheap because:** `curl2` returns the analytic derivative in one native
call (no finite differences), and `oklch` maps shade to a metallic ramp.
At 128x128, compute on a 64x64 canvas.

### 7. Wireframe Solids — grid — Cool 4 — M — pattern
A rotating wireframe cube that morphs into an octahedron, an icosahedron
and a torus knot. Front edges are bright, back edges dim and thin, and an
optional glenz mode fills faces translucently with additive colour.
**Algorithm:** a vertex table for each solid. Each frame rotate in 3D (two
angles), apply perspective, and draw edges with depth-scaled brightness.
Morphing lerps vertex positions between solids that share a vertex count.
**Cheap because:** at most ~30 `drawLine` calls per frame in add mode,
plus `fade` for an optional ghost trail. There is no per-pixel cost.

### 8. Fibonacci Dot Globe — grid — Cool 4 — S — pattern
A planet of 200–400 glowing dots, evenly spread by the golden-angle
spiral, spins slowly and tilts. Near dots are large and bright, far ones
are pinpricks. Optionally a noise "continent" field colours land vs ocean
dots, so it reads as a pointillist Earth.
**Algorithm:** golden-angle sphere points (generated, not stored), rotated
about Y and X. Draw if z > −0.2, with size and brightness from z.
**Cheap because:** `splat` in max mode per dot is O(dots), and the
continent tint is a `simplex3` read at the dot's fixed sphere position
(cache it in an array at init).

### 9. Amiga Twister — grid — Cool 4 — M — pattern
A vertical square column twists like taffy. Its four faces each have their
own colour and the twist angle ripples along its height, so it writhes and
untwists. It is an iconic Amiga effect.
**Algorithm:** per row, angle = t + amp·sin(y·k + t2). Compute the four
corner x positions of the rotated square and, for each visible face
(left x < right x), draw a span shaded by the face's facing angle.
**Cheap because:** at most 4 `fillRect` spans per row. On 128 rows that is
~500 rect calls of one row each, or use a single `fillGradient` per face
span.

### 10. Sine Scroller — grid — Cool 4 — M — pattern (or scene with engine wish)
A greeting (from `textSlot`) scrolls across the panel while each letter
bobs on a sine wave and wears a rainbow raster gradient. It sits on top of
a starfield or copper bars.
**Algorithm:** walk the string per character with `textWidth` advances,
and `drawText` each glyph at x − scroll with y + sin(x·k + t). Then
colour by row through a palette (draw white, then multiply with a gradient
pass, or set the brush per glyph).
**Cheap because:** `drawText` is native, there are only ~16 glyph draws
per frame, and `textSlot` makes the message live-editable from HA.
**Engine wish:** a per-character callback, or a "wave" option on scene text
layers, would make this a zero-code scene.

### 11. Mystify — grid — Cool 4 — S — pattern
The classic screensaver: two bright polygons whose corners bounce
independently off the walls. Each one leaves a fan of fading echo copies,
so they read as ribbons of light folding through space while their colours
slowly drift.
**Algorithm:** 4 vertices × 2 shapes with velocity and wall bounce. Each
frame draw the polygon's edges, and the echo comes from frame persistence.
**Cheap because:** `fade` + 8 `drawLine` calls (add mode) per frame. This
is the textbook `renderBulk` shape and costs almost nothing at 16384 px.

### 12. Pipes — grid — Cool 4 — M — pattern
Shaded "3D" pipes grow across the panel. Each one advances cell by cell,
occasionally turning 90° with a ball joint, and has a cylindrical shading
across its width. When the screen is full everything fades and growth
restarts in new colours.
**Algorithm:** 2–4 growers on a coarse occupancy grid (16x16 cells), each
step choosing a free neighbour with a turn bias. Draw each segment as a
rect whose cross-section is a 3-step lightness ramp (dark edge, bright
centre).
**Cheap because:** per step it is one `fillRect` per shading band and the
frame persists, so nothing is redrawn. The `fade` reset is trivial.

### 13. Please Stand By — grid — Cool 3 — S — pattern
Broadcast nostalgia: colour bars with a pluge strip and a tone-ID box
(the text comes from `textSlot`). Every so often it glitches into analog
snow with rolling vertical hold and a horizontal tear, then snaps back. It
is a cheeky "device idle" screen.
**Algorithm:** a bars layout with `fillRect`. Snow = hash(x, y, frame)
greyscale. Rolling = a y offset and tear = an x offset on a band of rows.
**Cheap because:** the bars are 10 rect fills. Snow bursts are short and
can run at a reduced canvas (32x32 hash + `fillCanvas`).

### 14. Bouncing Logo — grid — Cool 3 — S — sprite + scene (engine wish)
A sprite (any library sprite, or a user's imported logo) drifts diagonally
and bounces off the edges, changing tint on each wall hit. There is a
burst of sparkles on the rare perfect corner hit, the moment everyone
waits for.
**Algorithm:** position + velocity with reflection. Tint cycles on bounce
and a corner check triggers a splat burst.
**Cheap because:** `blit` the sprite canvas in keyed mode. As a pattern it
needs the sprite pixels inlined.
**Engine wish:** *scripted layer position* (a scene layer whose x/y/tint
comes from a tiny expression or a bound pattern variable) would make this
a pure scene using the real sprite library. It is a good motivating
example for goal 8.

---

## B. Generative art

### 15. Truchet Weave — grid — Cool 5 — S — pattern
A woven field of quarter-circle arcs. Adjacent tiles join into long
meandering ribbons and closed loops, like a Celtic knot that never ends.
Every few seconds a random tile rotates 90° with an eased spin, re-routing
the ribbons, and colour flows *along* each ribbon as a travelling
gradient. A "10 PRINT" mode swaps arcs for diagonal slashes to give the
Commodore maze look.
**Algorithm:** cell = floor(xy·N), with tile orientation from hash2(cell)
(plus a per-cell flip timestamp for animation). The local distance to the
two arc corners gives a ribbon mask (|d − 0.5| < width), and the phase
along the ribbon (angle around the corner) + t drives hue.
**Cheap because:** `hash2` gives stateless tile bits, so this is pure
per-pixel arithmetic with one `atan2` (or use distance only for a cheaper
mode). `oklch` gives clean ribbon colours, and `easeOutBack` makes the
flips feel physical.

### 16. Maze Builder and Solver — grid — Cool 5 — M — pattern
Dark walls appear as a glowing "mole" carves a perfect maze by
backtracking, its head bright and its trail cooling. When the maze is
done, a rainbow flood fills it from the entrance (hue = distance), then
the shortest path lights white and pulses. Finally the maze dissolves and
a new one begins. It is satisfying to watch on a desk display (Aurora and
Tidbyt both ship one).
**Algorithm:** a 16x16 (or 21x21) cell grid with an explicit stack array
for the recursive backtracker, a few steps per frame. Then BFS with a
queue array and a distance array, then a backtrace.
**Cheap because:** state is ~1 KB of arrays, and rendering is
`fillCanvas`/`paintCanvas` of a 2× cell canvas (walls + floor + distance
hue). All the work is O(cells) per frame.

### 17. Sorting Visualizer — both — Cool 5 — S — pattern
A shuffled rainbow is put back in order *live*. On a strip each pixel is a
hue and you watch bubble sort ripple, insertion sort slide, shell sort hop
and heap sort pulse until the strip resolves into a perfect spectrum, holds
for a beat with a sparkle, then shatters into a new shuffle. On a grid,
each column is a bar whose height and hue are its value, which is the
classic "sound of sorting" picture without the sound. This is one of the
few genuinely *strip-native* ideas with a story arc.
**Algorithm:** an array of values. Each algorithm is written as a resumable
step machine with loop indices in globals, doing K comparisons or swaps
per frame (speed control). The two elements being compared are highlighted
white.
**Cheap because:** `fillHSV` from the value array is one native call per
frame for the strip. On the grid, use `fillRect` per bar, or
`fillGradient` per column. Quicksort needs an explicit stack, so ship it in
a second pass.

### 18. Turmites — grid — Cool 4 — S — pattern
Langton's ant and its multi-colour cousins. A single cell-walker follows
"turn left on colour A, right on colour B…" rules. Some rule strings grow
symmetric snowflake blobs, some chaotic clouds, and some suddenly build an
endless diagonal highway. It cycles through a curated set of rule strings,
with each run's colours from an OKLCH wheel.
**Algorithm:** a grid state array (64x64 bytes fits easily), plus ant
pos/dir. N steps per frame (hundreds). Wrap edges, and restart on a
highway exit or timeout.
**Cheap because:** each step is a few array ops, and only changed cells
need a `setPixel(gridIndex(c, r))`. The frame persists, so nothing else is
redrawn.

### 19. Life with Ghosts — grid — Cool 5 — M — pattern
Conway's Game of Life done beautifully, and **the library has no Life at
all**. Newborn cells flash white and age through an OKLCH ramp (cyan to
violet to deep blue). Dead cells leave a fading ghost, so gliders draw
comet tails. Stagnation (a repeated population hash) triggers a gentle
reseed "rain". Modes: HighLife, Day & Night, Brian's Brain (three-state
firing/refractory, very electric) and Seeds.
**Algorithm:** birth/survival rule masks on a canvas (64x64 native or
32x32 upscaled). Neighbour counts per generation, a per-cell age array,
and a ghost decay array.
**Cheap because:** **`stencil2D(dst, src, w, h, 0, 1, 1)`** computes every
neighbour count in one native call. Rule application is
`arrayMapTo`/loop, ghosts are `feedback`, and the display is one
`fillCanvas`.

### 20. Lenia Microbes — grid — Cool 5 — L — pattern
Continuous-valued "life" on a 32x32 canvas. Soft glowing creatures glide,
pulse and divide like microorganisms under a microscope, in bioluminescent
teal on black. It is the most alive-looking thing a matrix can show.
**Algorithm:** state in 0..1. A ring kernel convolution is approximated as
a difference of two blurs (wide blur minus narrow blur ≈ ring). The growth
function is a bell curve around a target density, and state += dt·growth,
clamped. Seed with a few blobs and reseed when extinct or saturated.
**Cheap because:** `blur2D` twice + `arraySub` + `arrayMix` are all native
whole-array ops, and the bell is a `smoothstep` pair. The only risk is
tuning the parameters into the "living" regime in 16.16, hence L.

### 21. Perlin Loop Ring — grid — Cool 4 — S — pattern
A single luminous ring whose edge boils with noise, but the animation is a
*perfect loop*: the noise is sampled on a circle in time, so every N
seconds it returns exactly to where it began. Several concentric rings with
offset phases give a breathing iris. It is the generative-art "perfect
loop" GIF aesthetic.
**Algorithm:** for pixel angle θ, radius r_θ = r0 + a·simplex3(cos θ,
sin θ, ·) where the third input moves on a circle (cos τ, sin τ) via a 4D
trick (two 3D samples blended), or just ride τ on one axis with a periodic
wrap. Brightness = falloff from |r − r_θ|.
**Cheap because:** precompute θ per pixel in an array. Noise is needed
only per *angle bucket* (64 angles), not per pixel, so the cost is 64
`simplex3` calls + a per-pixel lookup and falloff.

### 22. Circle Packing — grid — Cool 4 — M — pattern
Bubbles spawn at random free spots and inflate until they kiss a neighbour
or a wall, then freeze with a soft rim highlight. The panel fills with a
tight mosaic of circles in a harmonious OKLCH palette. Once full, they pop
one by one (an easeOutElastic shrink) and the packing regrows.
**Algorithm:** a list of ≤60 circles (x, y, r, growing flag). Each frame,
grow the active ones and test against the others (O(n²) = 3.6k distance
checks, which is fine). Spawn by rejection sampling.
**Cheap because:** `fillCircle` per circle with the frame persisting (only
growing circles redraw), and `dist` is native.

### 23. Differential Growth — grid — Cool 5 — L — pattern
A small closed loop of light grows into convoluted coral or brain-like
folds. Its line splits and buckles as it lengthens, filling the panel with
an intricate meander, and colour encodes each segment's age. When the
panel is saturated, it fades and a new seed loop starts.
**Algorithm:** a node ring (cap ~200). Forces are attraction to
neighbours, repulsion from nearby nodes (use a coarse spatial-hash bucket
grid to avoid O(n²)) and alignment smoothing. Insert a node when an edge
gets too long.
**Cheap because:** rendering is ≤200 `drawLine` calls. The sim is the cost
and runs at a low step rate. Effort is L because of the spatial hash and
the fixed-point force tuning.

### 24. Physarum Network — grid — Cool 5 — M — pattern
Thousands of invisible slime-mould agents leave trails and steer toward
each other's scent. Within seconds, glowing vein networks self-organise,
merge, reroute and pulse, like a living subway map in gold on indigo. It is
markedly different from the library's *Slime mold palette*, which is a
static colour-growth fill, not an agent network.
**Algorithm:** ~300–600 agents (x, y, heading). Each agent samples the
trail at three sensors (left, centre, right), turns toward the strongest,
moves and deposits. The trail map is blurred and decayed each frame.
**Cheap because:** `canvasGet` (bilinear) for sensors and `canvasAdd` for
deposits, then `blur2D` + `feedback` on a 64x64 trail canvas, then
`paintCanvas`. Agent count is the speed dial.

### 25. Strange Attractor Nebula — grid — Cool 5 — M — pattern
A cloud of luminous dust in the shape of a Clifford/de Jong attractor:
silky folded ribbons and wisps like a long-exposure photo of a nebula. Its
parameters drift slowly, so the nebula unfurls, twists and morphs into new
shapes.
**Algorithm:** iterate the 2D map for ~2000 points per frame from a
persistent point and accumulate a hit density in a canvas. Decay the
density slowly, and tone-map with log/sqrt into a palette. Parameters move
on slow sines.
**Cheap because:** `canvasAdd` per iteration (sin/cos are native), plus
`feedback` for decay and `paintCanvas` for display. It is independent of
panel size.

### 26. Chladni Plate — grid — Cool 4 — S — pattern
Sand on a vibrating metal plate. Bright grains gather on the nodal lines of
a standing wave, forming crisp symmetric figures (crosses, rings, lattices)
that melt and re-form as the "frequency" steps to the next mode. The
particles visibly migrate between figures.
**Algorithm:** f(x,y) = cos(nπx)cos(mπy) − cos(mπx)cos(nπy), with
(n, m) stepping through a list and crossfading. Simple mode: brightness =
1 − smoothstep(|f|). Particle mode: ~400 grains jitter with amplitude ∝
|f| and settle where f ≈ 0.
**Cheap because:** per pixel it is two cos products (or precompute cos
tables per column and row: 2 × 64 values per frame), and the particle mode
is O(grains) `setPixel` plus `fade`.

### 27. Living Topo Map — grid — Cool 4 — S — pattern
An ever-changing terrain seen as a hiking map: hypsometric tints (deep sea,
shallows, beach, green lowland, tan hills, snowy peaks) with crisp contour
lines every few metres, slowly drifting as the noise "geology" evolves. An
alternate mode is just the glowing contour lines in neon on black.
**Algorithm:** h = fbm(x, y, t). Tint from a palette by h. Contour = frac(h
· N) near 0 or 1 (anti-aliased with smoothstep).
**Cheap because:** `fillNoise3D` a row or a 64x64 canvas per frame, then a
palette lookup. Contours are one `frac`. `setPalette` holds the
hypsometric ramp.

### 28. Marbling (Suminagashi) — grid — Cool 5 — M — pattern
Ink drops fall onto water one by one. Each new drop pushes all the earlier
rings outward into concentric bands, then a comb drags through and pulls
them into feathered chevrons, like the endpapers of an old book. After a
while it slowly fades to clean water.
**Algorithm:** keep a list of drop events (centre, radius) and comb strokes.
Per pixel, invert the transforms newest-to-oldest (the drop's
area-preserving radial push has a closed-form inverse, and a tine stroke
is a displacement decaying with distance from the line) to find which drop
colour the point came from.
**Cheap because:** it is an analytic per-pixel inverse with no simulation
grid. Cap the history at ~12 events and run it on a 64x64 canvas. Colour
is a ring index into a 4-colour OKLCH set.

### 29. Harmonograph — grid — Cool 4 — S — pattern — *Reconception of "Lissajous curve tracer"*
A pen on two damped, slightly detuned pendulums draws a spiralling rosette
that tightens toward the centre as the swing decays. Then the "paper" fades
and new random ratios start another figure. It is warmer and more organic
than a plain Lissajous trace, because the decay gives every drawing a
beginning, middle and end.
**Algorithm:** x = Σ A_i·sin(f_i·t + p_i)·e^(−d_i·t), same for y, sub-stepped
several times per frame so the line is continuous. Restart when the
amplitude drops below a threshold.
**Cheap because:** `drawLine` between successive sub-steps in add mode,
with frame persistence plus a slow `fade`.

### 30. Stained Glass — grid — Cool 4 — S — pattern — *Reconception of "Voronoi 2D"*
Jewel-toned glass panes (ruby, cobalt, emerald, amber) held in dark lead
cames. A slow sunbeam sweeps across, so panes light up, glow and darken,
with a subtle per-pane noise texture like real cathedral glass. The panes
gently re-shape as the seeds drift.
**Algorithm:** Voronoi with F1 and F2 distances. Lead = F2 − F1 < width.
Pane colour = OKLCH hue from the seed id with lightness × sunbeam band ×
(1 + small noise).
**Cheap because:** few seeds (12–20), and nearest-two search per pixel.
At 128x128, run it on a 64x64 canvas, or use a cell-bucket grid so each
pixel tests only its 9 neighbouring buckets.

### 31. Hilbert Walk — both — Cool 3 — S — pattern
A comet travels the Hilbert space-filling curve, drawing the fractal
meander as it goes. On a grid the full curve emerges, then recolours as a
rainbow by curve index, which shows how a 1D rainbow folds into 2D with
locality kept. On a strip it is a "folding" rainbow whose segments flip
order at power-of-two boundaries.
**Algorithm:** the standard index-to-(x,y) Hilbert conversion (bit loop).
**Cheap because:** the d2xy mapping is a short integer loop per drawn
point, and only the comet head is drawn per frame (the frame persists).

### 32. Quasicrystal — grid — Cool 5 — S — pattern
Seven sets of plane waves at evenly spaced angles sum into a
never-repeating rosette lattice with 7-fold symmetry. As the phases drift
it blooms with flowers, stars and wheels endlessly, without ever tiling.
It is markedly different from *Interference 2D*: it uses plane waves with
rotational symmetry, not moving point sources.
**Algorithm:** v = Σ_k cos((x·cos θ_k + y·sin θ_k)·freq + t), normalised,
then a palette.
**Cheap because:** precompute (cos θ_k, sin θ_k). Per pixel it is k
`wave()` calls. Use k=5 on 128x128 and k=7 on 64x64. It can also run at
half resolution through a canvas.

---

## C. Physics toys

### 33. Double Pendulum Trio — grid — Cool 5 — S — pattern
Three double pendulums hang from the same pivot with starting angles
differing by a hair. For a few seconds they swing in perfect unison, then
chaos tears them apart, each tip trailing a fading ribbon in its own
colour. It is a live demonstration of the butterfly effect, and it
re-synchronises every minute or so.
**Algorithm:** the standard double-pendulum equations with semi-implicit
Euler, 4–8 substeps per frame and mild damping. Clamp angular velocity to
stay inside 16.16.
**Cheap because:** state is 12 numbers. Rendering is `drawLine` for the
rods, `splat` for the bobs and `fade` for the trails.

### 34. Orbits and Slingshots — grid — Cool 4 — S — pattern
A small star system: a glowing sun, planets on elliptical orbits that
speed up at perihelion, and a moon or two. Occasionally a comet dives in,
slingshots around the sun and flies off, with a tail that always points
away from the sun. Mode: the three-body figure-eight choreography.
**Algorithm:** softened inverse-square gravity from the sun (and between
bodies in three-body mode), with leapfrog integration and substeps. Comet
tail = a short line along the vector away from the sun.
**Cheap because:** ≤8 bodies, so the cost is `splat` add + `fade` +
`drawLine`.

### 35. Gravity Wells — grid — Cool 4 — S — pattern
Two or three invisible black holes wander the panel. A stream of particles
falls into them, spiralling into accretion discs and being flung out when
the wells pass close to each other. The colour of each particle encodes its
speed (red slow to white-blue fast). This is the WLED "PS Attractor" idea,
done with proper multi-well dynamics.
**Algorithm:** ~150 particles with softened gravity toward each well and
respawn at the edges or on capture. The wells move on slow noise paths.
**Cheap because:** `setPixel(gridIndex(...))` per particle with `fade`
trails, or `canvasAdd` into a small canvas with `feedback`.

### 36. Plucked String — strip — Cool 4 — S — pattern
The whole strip is a guitar string. A random pluck sends a bright kink that
splits and bounces between the fixed ends, decays into standing-wave
harmonics, and shimmers quieter and quieter until the next pluck. It is
coloured by local displacement (warm for up, cool for down) and is
genuinely physical motion on a 1D strip.
**Algorithm:** the 1D wave equation on a displacement array (current and
previous), with light damping and fixed ends. A pluck sets a triangular
initial shape.
**Cheap because:** `stencil2D(dst, src, n, 1, ...)` or `pixelState` gives
the neighbour sums in one pass, `arraySub`/`arrayScale` handle the update,
and `fillHSV` displays it.

### 37. Newton's Cradle — strip — Cool 3 — S — pattern
Five chrome balls on a strip. The end ball swings out (it travels along the
strip with eased motion), clacks into the row with a white flash at
contact, and the far ball leaps out. It slowly loses energy, then gets
re-lifted. On a grid it is drawn side-on with strings.
**Algorithm:** a scripted pendulum angle with easing, mapped to the end
balls' positions while the middle balls stay fixed. The flash is a short
splat at the impact point.
**Cheap because:** it is a handful of `fillRange` + `oklch` lightness
ramps per ball.

### 38. Slinky — strip — Cool 3 — S — pattern
Twenty beads joined by springs along the strip. A tap at one end sends a
compression pulse travelling through the chain, and it reflects back and
forth. Gaps between beads glow by spring tension (compressed = hot,
stretched = cold). This is the WLED PS "Springy" idea.
**Algorithm:** a mass-spring chain with Verlet integration and damping.
Render each bead as a 2–3 px dot, and spring segments as a gradient
coloured by strain.
**Cheap because:** O(beads) per frame using `fillGradient` per segment.

### 39. Galton Board — grid — Cool 4 — M — pattern
Balls trickle from a funnel through a triangular lattice of pegs, bouncing
left or right at each one, and pile into bins at the bottom. The bins grow
into a bell curve. When the curve is complete it pulses, the bins drain,
and it starts again.
**Algorithm:** each ball steps row by row with a 50/50 bounce and short
eased arcs between pegs. Bin counts are in an array.
**Cheap because:** O(active balls) splats with the static pegs drawn once.
The bins are `fillRect`s.

### 40. Smoke (Stable Fluids) — grid — Cool 5 — L — pattern
Colourful smoke billows from two or three wandering jets, curls into
vortices, swirls around and mixes into marbled clouds before fading.
Clicking the preview (an event) injects a puff.
**Algorithm:** Stam-style fluid on a 24x24 or 32x32 grid. Add forces (jets
+ a little curl noise), advect velocity and dye semi-Lagrangian, then
project with ~8 Jacobi pressure iterations.
**Cheap because:** advection is `canvasGet` bilinear back-tracing, Jacobi
iterations are `stencil2D` (one native call each), and display is
`paintCanvas`/`fillCanvas`. The risk is fixed-point stability, hence L.

### 41. Cloth Flag — grid — Cool 4 — L — pattern — *Reconception of "US Flag 2D"*
A real cloth simulation. A banner pinned on its left edge ripples, sags and
snaps in gusty wind, with folds catching light and shadow. Any design can
be shown: stripes, a pride flag, or a `textSlot` word printed on the cloth.
**Algorithm:** a Verlet grid of ~12x8 points with distance constraints and
pinned left column. Wind = noise-driven force on the normal. Render by
inverse-mapping each pixel into the deformed quads (or forward-splat a
denser mesh) with lighting from the quad normal.
**Cheap because:** the physics is ~100 points. The cost is the rasterising.
Forward-drawing a 24x16 sub-sampled mesh with `fillRect` cells is the
pragmatic route.

### 42. Jelly Blob — grid — Cool 4 — M — pattern
A wobbly jelly creature with two eyes (sprite pixels) bounces across the
floor, squashing on landing, stretching on take-off and jiggling. It
occasionally hops at the viewer or a click event.
**Algorithm:** a ring of ~16 spring-mass points around a centre with
pressure (area-preserving) force and gravity. Fill the polygon (scanline)
and blit the eyes at the centroid.
**Cheap because:** scanline fill is O(rows) `fillRect` spans, and the eyes
are a tiny `blit`.

### 43. Hourglass Timer — grid — Cool 4 — M — pattern (+ scene)
A functional timer. Sand drains from the upper bulb through the neck over a
set number of minutes (a control in real minutes). The pile in the lower
bulb grows as a cone, the stream glitters, and when it runs out the glass
flips over with an eased 180° rotation. It is different from *Falling
Sand 2D*, which is a free toy with no time function.
**Algorithm:** the glass outline is static. Fill levels are computed
analytically from the elapsed fraction (cone geometry), and grains are
drawn only as the falling stream plus surface jitter, not a full sand
automaton.
**Cheap because:** it is mostly `fillRect` rows plus ~20 grain splats. The
flip is the `rotate` transform with `easeInOutQuint`.

### 44. Domino Run — strip — Cool 3 — S — pattern
Dominoes stand along the strip as dim ticks. A push sends a toppling wave
racing down the line (each domino's lean animates and lights it), and it
accelerates slightly. At the end a robot sweep stands them back up one by
one.
**Algorithm:** each domino's fall time = previous + gap/speed. Its state is
an angle with an eased fall, and it is drawn as a 2–3 px segment whose
brightness and width follow the angle.
**Cheap because:** it is a time table computed once per run, then
`fillRange` per domino.

### 45. Fuse and Sparkler — strip — Cool 4 — S — pattern
A burning fuse creeps along the strip. A white-hot point throws showers of
orange sparks to both sides that arc, dim and wink out, with a dim smoke
glow trailing behind. At the end it pops into a short flash and the fuse
"rewinds". This is the WLED PS Sparkler idea, built as a strip story.
**Algorithm:** fuse head position plus a particle pool (~40) with velocity
(1D), drag and life. Sparks are additive points, plus a burnt-section
ember colour.
**Cheap because:** `fade` + `setPixel` per spark, and `fillRange` for the
burnt section.

---

## D. Nature and weather

### 46. Pool Caustics — grid (also strip) — Cool 5 — S — pattern
Looking down into a sunny swimming pool, a web of bright, sharp,
ever-shifting light lines dances over aqua tiles, with faint tile grout
lines underneath. This is one of the most "wow, that's real" effects
possible at very low cost.
**Algorithm:** caustic ≈ the inverse of ridged noise. Take two `simplex3`
layers moving in different directions, compute 1 − |n| for each, raise to
a power to sharpen, and take the min/product of the layers. Add over an
aqua base, plus faint grout from frac(x·N).
**Cheap because:** `perlinRidge` exists. Two noise calls per pixel at
64x64. At 128x128, run `fillNoise3D` on a 64x64 canvas, since caustics stay
convincing upscaled. On a strip it is a sunlit shallow stream.

### 47. Rain on Glass — grid — Cool 5 — M — pattern
A rainy window at night. Out-of-focus city lights glow as soft bokeh discs
behind the glass. Droplets bead up, grow, suddenly slide down in jerky
runs, merge and leave clear trails that slowly re-fog, and each drop shows
a tiny upside-down refraction of the lights.
**Algorithm:** background = static bokeh discs on a dark blue gradient,
drawn once into a canvas. Drops = a list with size, position and a
stick/slip state machine. Each drop is drawn as a bright-rimmed disc whose
interior samples the background at a flipped, magnified offset. A trail
mask array is cleared by sliding drops and decays back to "fog" (fog =
dimmer, blurred background).
**Cheap because:** the background is precomputed. Per frame it is
`fillCanvas` (background) + ~30 drops × small `canvasGet` samples + a
`feedback`-decayed fog mask.

### 48. Bokeh Night — grid — Cool 4 — S — pattern — `PF-title` ("Bokeh")
Defocused lights drift lazily: big soft discs in warm amber and cool teal,
with brighter rims as real lens bokeh have and hexagonal tips in one mode.
They overlap additively into glowing clusters and pulse faintly like
distant traffic. Very calming as ambient.
**Algorithm:** ~20 discs with size, depth (bigger = closer = slower drift
and dimmer) and slow noise drift. Each is drawn as a filled disc plus a
rim.
**Cheap because:** `splat` (add) + `fillCircle` per disc with `clear` or
`fade`. The cost scales with disc area, so cap the radius at 128x128.

### 49. Snowfall with Drifts — both — Cool 4 — M — pattern
Flakes in three parallax layers (big near ones fast and swaying, tiny far
ones slow) fall through a dusky blue night. They settle into growing drifts
along the bottom and on a ledge, and a gust occasionally blows a powder
cloud off the top. The drifts slowly melt so it never fills. On a vertical
strip it is gentle snowfall past a window.
**Algorithm:** a flake pool with x sway from simplex2(t, id), a height
map array per column for drifts with angle-of-repose smoothing, and gusts
as a global wind term.
**Cheap because:** O(flakes) `setPixel`/`splat`, and drifts are one
`fillRect` per column (64 calls) or `fillGradient`.

### 50. Wind in Grass — grid — Cool 5 — M — pattern — `PF-title` ("Pampas Night")
A meadow at golden hour. Dozens of blades rise from the bottom edge, and
visible gust fronts roll across the field, bending blades in waves. Seed
heads glint, and the sky behind is a sunset gradient with a few fireflies
at dusk.
**Algorithm:** blades as 2–3 segment curves (base x, height, stiffness).
Bend = wind(x − gustSpeed·t) from layered noise, more at the tips
(quadratic). Draw back-row blades darker for depth.
**Cheap because:** ~60–120 blades × 3 `drawLine` segments, the sky is one
`fillGradient`, and `bezierQuadratic` gives the blade curve.

### 51. Lava Lamp — both — Cool 4 — S — pattern — *Reconception of "SOUND - lavablob" / "Metaballs of Fire 2D"*
A real lava lamp rather than generic metaballs. Wax blobs heat at the
bottom, swell, rise slowly with a stretched teardrop neck, cool and drift
back down, merging and pinching apart. It glows within a tapered glass
silhouette with the classic red-in-yellow or purple-in-blue liquid. On a
vertical strip it is the same physics in 1D: blobs rising and falling in a
tube.
**Algorithm:** 5–7 blobs with temperature driving buoyancy (heat near the
bottom, cool near the top). The metaball field Σ r²/d² is thresholded with
a soft edge, and the vertical stretch scales with |velocity|.
**Cheap because:** at 64x64 the field sum is per pixel × 6 blobs. At
128x128, compute on a 32x48 canvas and `fillCanvas`. The glass mask is a
static per-row width table.

### 52. Real Candle — both — Cool 4 — S — pattern — `PF-title` ("Candle light")
A convincing candle for lamp use. On a grid it is a teardrop flame that
sways and stretches with drafts, with a blue base, a bright core and a
warm halo lighting the "wall". It occasionally gutters (dims and shrinks)
then recovers. On a strip the "tealights" mode makes every LED or group
its own candle with correlated flicker. It is not holiday-themed like the
*Blue Holiday Candle 2D*.
**Algorithm:** flicker = layered low-frequency noise + rare gutter events
with an exponential recovery. Flame shape is an implicit teardrop distance
in coordinates sheared by sway.
**Cheap because:** strip mode is one noise call per candle group + a
`fillRange`. Grid mode uses a small bounding-box per-pixel region
(`fillCircle`/`splat` for the halo).

### 53. Shoreline — strip — Cool 4 — S — pattern
A beach seen from above along the strip. Turquoise waves run up the sand,
leaving a lacy white foam edge and a darker band of wet sand. The wet band
dries back to pale gold, then the next set rolls in, with every seventh
wave bigger. It differs from *Ocean*, which is open water with no shore,
wash or drying sand.
**Algorithm:** wave run-up position from a sum of a periodic swell and a
noise term. A per-pixel wetness state rises when water covers the pixel
and decays after. Foam is a narrow bright band at the leading edge plus
hash sparkle.
**Cheap because:** `pixelState` (or one array) for wetness, then
`fillHSV`/`fillRGB` from arrays: one native call.

### 54. Jellyfish Drift — grid — Cool 5 — M — pattern — `PF-title` ("Bioluminescent Plumes")
Two or three translucent jellyfish pulse upward through dark water. Each
bell contracts and glides, and long trailing tentacles wave with delayed
sine motion, glowing cyan-magenta. Drifting plankton sparkle when a jelly
passes.
**Algorithm:** bell = squashed semicircle whose width oscillates with an
asymmetric pulse (fast contract, slow relax), giving upward impulse on
contraction. Tentacles are 4–6 polylines where each segment's x follows
sin(t − k·segment), so the wave travels down.
**Cheap because:** `fillCircle`/`splat` for the bell and `drawLine` for
tentacle segments (≈60 per frame), with `fade` for the glow trail.

### 55. Cloudscape Day Cycle — grid — Cool 4 — M — pattern (or scene)
A sky window. Fluffy cumulus drift across a sky whose gradient follows the
real time of day (`clockHour`): pale dawn, blue noon, orange dusk, starry
night. The clouds are lit from the sun's side, rose-tinted at sunset and
moonlit grey at night.
**Algorithm:** cloud density = fbm with a threshold and a soft edge.
Lighting = density difference toward the sun direction (one extra offset
sample). The sky colour comes from a time-of-day palette.
**Cheap because:** `fillNoise3D` on a 64x32 canvas (clouds are soft, so
upscaling is invisible), `fillGradient` for the sky, and `clock*`
builtins for the time.

### 56. Star Trails — grid — Cool 4 — S — pattern
A long-exposure photograph forms in real time. Stars wheel around an
off-centre pole star, and each one draws a concentric arc that lengthens
over minutes. A dark tree-line silhouette sits at the bottom, and an
occasional meteor streaks across. It resets with a slow fade.
**Algorithm:** ~150 stars at fixed (radius, angle, brightness, tint). Each
frame advance a global angle and plot each star's current position. The
trail is frame persistence (no fade until reset). The silhouette is a
static hash-noise skyline row.
**Cheap because:** O(stars) `setPixel(gridIndex)` per frame. The
silhouette is drawn once per frame with `fillRect` columns, or protected by
drawing it last.

### 57. Spiral Galaxy — grid — Cool 5 — M — pattern
A face-on spiral galaxy turns slowly. A bright yellow-white core opens into
blue star-forming arms laced with dark dust lanes and pink nebula knots.
The inner stars orbit faster than the outer ones, so the arms subtly wind.
This is the WLED "PS Galaxy" idea.
**Algorithm:** ~500 star particles placed on log-spiral arms with Gaussian
scatter, each with angular speed ∝ 1/√r (differential rotation, with arms
re-seeded slowly to avoid over-winding). Render with additive splats, plus
a per-pixel or canvas dust term from noise in polar coordinates.
**Cheap because:** stars are `setPixel`/small `splat`. The core glow and
dust are one canvas pass (`fillNoise2D` in (r, θ) space).

### 58. Volcano — grid — Cool 4 — M — pattern
A dark cone silhouette erupts in pulses. Incandescent particles fountain
up, arc under gravity, cool from white to yellow to red to black ash, and
roll down the flanks as glowing lava streams, under a red-lit smoke
plume. This is the WLED "PS Volcano" idea.
**Algorithm:** a particle pool with ballistic motion, colour by age through
a blackbody ramp, and landing on the cone's slope adds to per-column flow
streaks that drain downhill. The plume is noise-advected upward.
**Cheap because:** particles are splats in add mode. The plume uses
`fillNoise3D` on a small canvas, and the cone is a static triangle.

### 59. Waterfall Mist — grid — Cool 4 — M — pattern
Water pours over a rocky ledge as streaky white-blue sheets that accelerate
as they fall. They burst into churning white mist and spray at the plunge
pool, and a faint rainbow appears in the mist when "sunny".
**Algorithm:** column streaks are noise scrolled downward with speed
increasing by row (stretched noise y). The plunge mist is a particle pool
+ `blur2D`. The rainbow is an arc band with low alpha.
**Cheap because:** the fall is `fillNoise3D` with anisotropic scale (cheap
streaks), the mist is a small canvas with `blur2D`+`feedback`, and the
rainbow is `fillCircle`-like arcs with lighten blend.

### 60. Falling Leaves — grid — Cool 3 — S — pattern (+ sprite)
Autumn leaves tumble down. Each one flips (its width oscillates to
simulate rotation) and zig-zags side to side with pendulum sway, in maple
reds, oranges and browns. They pile at the bottom and are blown away by
an occasional gust.
**Algorithm:** a leaf pool with sway = sin(t·f + phase) and flip = |cos|.
Draw as a small diamond scaled by flip, and the pile is a column-height
array.
**Cheap because:** O(leaves) small `fillRect`/`splat`, or `blit` of a
5x5 leaf sprite in keyed mode.

### 61. Seasons Tree — grid — Cool 5 — M — pattern
A fractal tree grows from a seed branch by branch, with an eased growth of
each limb. It sways gently in the wind (the sway is larger at the tips),
blossoms pink in spring, goes green, turns gold and red in autumn, drops
its leaves (they flutter down) and stands bare under falling snow. Then
the year repeats. This is the L-system / recursive tree staple.
**Algorithm:** a recursive branching expanded into an explicit array of
segments at init (depth 7 ≈ 127 segments: parent, angle, length). Per
frame, accumulate angles with sway = noise(t)·depth-weight and draw. Leaves
are splats at the terminal segments.
**Cheap because:** ~127 `drawLine` + ~64 `splat` per frame, with the
season as a slow `time()` phase into OKLCH hue tables.

### 62. Moon Phase — grid — Cool 3 — S — pattern (or scene)
Tonight's actual moon. It is a cratered disc lit to the correct phase from
the date (`clockYear`/`clockMonth`/`clockDay`), with a faint earthshine on
the dark limb and a soft halo, over a starfield. This is a Tidbyt and
Divoom favourite.
**Algorithm:** phase from days since a known new moon, mod the synodic
month. The terminator is an ellipse (x-scale = cos phase), craters are
noise thresholded inside the disc, and the limb is darkened.
**Cheap because:** it is per-pixel only inside the disc's bounding box via
`fillCircle`-scoped work, or a one-time 32x32 canvas recomputed once a
minute and then `fillCanvas`.

---

## E. Ambient, home and clocks

### 63. Notification Ripple — both — Cool 4 — S — pattern
A home-automation workhorse. The panel or strip rests in a calm ambient
glow. An external event (HA/MQTT `luxel/<id>/event`, type = channel) fires
a colour-coded signal: doorbell = a blue ring bursting from the centre,
washer done = a green sweep, a message = a gentle amber double pulse. Then
it eases back to ambient. Unread events keep a small breathing dot in the
corner until a "clear" event.
**Algorithm:** a small queue of active animations (type, start time),
each a short scripted envelope (`easeOutBack`, `easeOutElastic`). Ambient
is a slow OKLCH breathe.
**Cheap because:** `eventCount`/`readEvent` + `fillCircle`/`fillRange`
envelopes. Idle frames are a single fill.

### 64. Day Ribbon — strip — Cool 4 — S — pattern
The strip becomes a 24-hour timeline: deep blue night at the ends, dawn
pinks, daylight blue at noon. A bright "now" marker sits at the current
time and hour ticks are faint notches. Past hours are slightly dimmed, so
the day visibly "fills". It is a glanceable clock for a shelf edge.
**Algorithm:** pixel → hour of day. Colour from a time-of-day palette
(optionally sunrise/sunset from a latitude control). The marker is a
splat at `clockHour + clockMinute/60`.
**Cheap because:** `fillGradient` for the sky bands + a few `fillRange`
calls, and `clock*` builtins.

### 65. Fibonacci Clock — grid — Cool 4 — S — pattern
A Mondrian-looking composition of five squares sized 1, 1, 2, 3, 5 that
secretly tells the time: squares counting hours are red, minutes (in fives)
are green, both are blue. It reshuffles its colouring among the valid
combinations every refresh. It is a puzzle-art clock.
**Algorithm:** a precomputed layout of the 5 squares in a golden
rectangle. Choose a subset summing to the hour, and another subset for
minutes/5, at random among the solutions.
**Cheap because:** 5 `fillRect` + thin dark borders. It redraws once a
minute.

### 66. Word Clock — grid — Cool 4 — S — pattern (or scene)
A letter grid in dim grey where the words "IT IS HALF PAST TEN" light up
in warm white. Transitions fade the old words out as the new ones fade in,
with a little sparkle on change. A minute-dot row covers the 1–4 minutes
in between.
**Algorithm:** a fixed letter layout (11x10 in the `tiny` font fits 64
wide). Word table with (row, col, len), and the active-word set from
`clockHour`/`clockMinute` in five-minute steps.
**Cheap because:** one `drawText` per row for the dim layer, then
re-`drawText` the lit words with a bright brush. That is ≤20 native text
calls.

### 67. Tetris Clock — grid — Cool 5 — M — pattern
Each digit of HH:MM is built live by tetrominoes that fall, rotate and
lock into the digit's shape. When the minute changes, the changed digit
dissolves and is rebuilt. It is the Tidbyt and LED-matrix crowd favourite.
**Algorithm:** a precomputed decomposition of each digit 0–9 into ordered
tetromino placements (authored by us). The falling animation runs per
piece with an eased drop.
**Cheap because:** each piece is 4 `fillRect` cells, and the static
locked pieces persist in the frame. Effort is mostly authoring the 10
decompositions.

### 68. Split-Flap Clock — grid — Cool 4 — M — pattern
An airport departures-board clock. When a digit changes, its top flap
falls with a foreshortening squash (easeInQuad then a bounce), revealing
the next digit, with a dark hinge line and faint flap shading. A
`textSlot` row can flip through messages the same way.
**Algorithm:** per character cell, render the top half from the
new/old glyph and the bottom half from old/new, with the falling flap
scaled vertically by cos(angle).
**Cheap because:** draw the glyph halves with `drawText` clipped via
`fillRect` masks (or into a small canvas and `blit`). It only animates on
change.

### 69. Pomodoro Ring — both — Cool 3 — S — pattern
A focus timer. On a grid a thick ring (or on a strip a bar) drains
clockwise over the work period (control in minutes), shifting from green
to amber in the last minutes. At zero it pulses and switches to a calm
blue break ring. An event restarts it, and the number of remaining
minutes shows in the centre.
**Algorithm:** elapsed fraction from `time()`/frame delta. The ring mask
compares the pixel angle against the fraction.
**Cheap because:** `fillCircle` ring + angle test per pixel inside the
ring's box only, or precompute pixel angles once. Uses `drawNumber`.

### 70. Night City — grid — Cool 4 — S — pattern
A city skyline at night. Building silhouettes of varied heights are dotted
with lit windows that flick on and off over time (warm tungsten, cool TV
blue, the occasional party colour). A red aircraft beacon blinks on the
tallest tower, a plane's lights cross the sky every so often, and a slow
dusk-to-midnight sky gradient tracks the real clock.
**Algorithm:** skyline heights from hash per column block. Windows are
hash2(cell) ≤ occupancy(clockHour), with a per-window flip schedule from
hash2(cell, slow epoch). The plane is a blinking pair of dots on a
straight path.
**Cheap because:** it is stateless via `hash2`. The windows can be drawn
with `fillRect` per lit window (only redrawn on change), and the sky is
one `fillGradient`.

---

## F. Games and attract modes

### 71. Pong Clock — grid — Cool 4 — M — pattern
Two AI paddles rally a square ball forever, and the score shows the time
(HH on the left, MM on the right). The hour-side paddle deliberately
misses when the hour changes, and the minute side misses each minute. It
is a beloved maker classic, rendered with a scanline-glow look.
**Algorithm:** the ball bounces, paddles track the ball's predicted
intercept with lag, and a deliberate miss is scheduled to land at the
minute boundary.
**Cheap because:** 3 `fillRect` + 2 `drawNumber` + a dashed centre line,
with a light `setGlow` for the CRT feel.

### 72. Pac-Chase — strip — Cool 4 — S — pattern
A yellow chomper eats a dotted line of pellets along the strip, chased by
four coloured ghosts. When it eats the power pellet the ghosts turn blue
and flee, and the chase reverses. Then the dots refill. It is a strip
animation with a story, after the WLED "PacMan" idea (shapes are our own,
with no game assets).
**Algorithm:** 1D positions with speeds, a mode timer for the power phase,
and a pellet bitmap array. Mouth "chomp" = the head pixel's brightness
pulsing.
**Cheap because:** `fillRange` per actor and `fillRGB` from the pellet
array. It is trivially O(n).

### 73. Invaders Attract — grid — Cool 4 — M — scene + sprites (engine wish)
Rows of two-frame alien sprites (our own designs) march, step down and
speed up as they thin out. A cannon at the bottom fires up, aliens burst
into splats, and a UFO crosses the top with a bonus flash. Sprite
animation frames already exist (`fps:` in `.sprite`).
**Algorithm:** a formation offset and alive bitmask, a bullet list, and a
simple cannon AI.
**Cheap because:** `blit` of 8x8 sprite frames in keyed mode, with the
formation drawn by loop. It is ≤40 blits.
**Engine wish:** this is the strongest argument for *scripted sprite
layers* (a scene layer bound to a pattern-computed x/y/frame), which would
let the art live in `library/sprites`.

### 74. Asteroids Attract — grid — Cool 4 — M — pattern
Vector-style rocks tumble across a wrapped screen while a little triangle
ship rotates, thrusts (flame flicker) and fires. Rocks split into smaller
ones on hits, with a phosphor afterglow on all lines.
**Algorithm:** a rock list (centre, radius, spin, jagged vertex offsets
from hash) and ship AI aiming at the nearest rock.
**Cheap because:** it is all `drawLine` (rocks are 8-gons) + `fade` for
phosphor. That is ≤100 lines per frame.

---

## G. Example scenes (layers of existing or new patterns, text and sprites)

### 75. Aquarium — grid — Cool 5 — M — scene (+ sprites, engine wish)
Layer 1 is the *Pool Caustics* pattern ramped to deep teal (a multiply
blend over a blue gradient). Layer 2 is a seaweed-sway pattern (a few
`bezierQuadratic` strands). Layer 3 holds fish sprites (new 2-frame
`.sprite` art) that swim across with sine bob. Layer 4 is a bubble-column
strip of rising bubbles in lighten blend. It is the top Tidbyt/Divoom
genre.
**Cheap because:** each layer is cheap on its own and the compositor does
the rest.
**Engine wish:** sprite layer motion paths (linear + sine bob + wrap)
would remove the need for a fish pattern.

### 76. Hearth — grid — Cool 4 — S — scene
An existing fire pattern (*Palette Fire 2D* or *Doom Fire 2D*) at the
bottom, under a new log-pile sprite (keyed black). An "ember glow" colour
wash in add blend breathes on the wall above, and an optional `textSlot`
line ("WELCOME HOME"). It is a yule-log screen built from parts.
**Cheap because:** it needs only one new sprite and reuses library fire.

### 77. Nocturne — grid — Cool 4 — S — scene
*Star Trails* (or *Starfield* run very slowly) as the sky, a moon sprite
(or the *Moon Phase* pattern in a box), a mountain-silhouette sprite
keyed black across the bottom, and occasional *Meteor Shower* in add
blend. A calm bedroom-wall piece.
**Cheap because:** it is composition only, plus one silhouette sprite.

### 78. New Year Countdown — grid — Cool 4 — S — scene
*Fireworks Finale* as the background, with a large `drawNumber` countdown
layer driven by a `textSlot` the HA automation updates each second (or a
countdown text layer if the scene grammar grows one). At zero, a
`HAPPY NEW YEAR` crawl appears.
**Cheap because:** it reuses fireworks.
**Engine wish:** a text-layer source `countdown <iso time>` alongside
`clock HH:MM` would make it fully self-contained.

### 79. Weather Window — grid — Cool 3 — S — scene
An HA automation writes a condition into a text slot or event. The scene
shows a sky gradient wash, a *Snowfall*/*Rainstorm*/*Cloudscape* layer
selected by the condition, and the temperature in large text. It is the
Divoom weather-face idea done with our own layers.
**Engine wish:** conditional layer visibility (a layer shown when text
slot n equals a value) would avoid one-scene-per-weather.

---

## Engine wishes that fell out of the survey (for goal 8)

1. **Scripted layer position/frame**: a scene sprite/text layer whose
   x, y, tint and frame come from a bound pattern variable or a tiny path
   spec (linear, bounce, sine bob, wrap). This unlocks Bouncing Logo,
   Aquarium, Invaders and the Sine Scroller as pure scenes.
2. **Countdown/elapsed text sources** next to `clock HH:MM`, for New Year,
   Pomodoro and Hourglass scenes.
3. **Conditional layer visibility** keyed on a text slot or last event
   type, for Weather Window and notification overlays.
4. **Palette-space bulk fill** (`paintCanvas` already exists for canvases.
   A `paintArray(valueArr)` index-space twin would make Plucked String,
   Shoreline and Sorting byte-exact palette patterns with no loop).

---

## Top 25, ranked by coolness ÷ effort (S=1, M=2, L=3)

Ties are broken by visual payoff, then by filling a gap in the library
(strips, ambient use, things the library entirely lacks).

| # | Idea | Dims | Cool | Effort | Ratio | Form |
|---|---|---|---|---|---|---|
| 1 | Pool Caustics (46) | grid/strip | 5 | S | 5.0 | pattern |
| 2 | Truchet Weave (15) | grid | 5 | S | 5.0 | pattern |
| 3 | Quasicrystal (32) | grid | 5 | S | 5.0 | pattern |
| 4 | Sorting Visualizer (17) | both | 5 | S | 5.0 | pattern |
| 5 | Double Pendulum Trio (33) | grid | 5 | S | 5.0 | pattern |
| 6 | Notification Ripple (63) | both | 4 | S | 4.0 | pattern |
| 7 | Real Candle (52) | both | 4 | S | 4.0 | pattern |
| 8 | Lava Lamp (51, reconception) | both | 4 | S | 4.0 | pattern |
| 9 | Plucked String (36) | strip | 4 | S | 4.0 | pattern |
| 10 | Shoreline (53) | strip | 4 | S | 4.0 | pattern |
| 11 | Night City (70) | grid | 4 | S | 4.0 | pattern |
| 12 | Mystify (11) | grid | 4 | S | 4.0 | pattern |
| 13 | Rotozoomer (1) | grid | 4 | S | 4.0 | pattern |
| 14 | Shadebobs (4) | grid | 4 | S | 4.0 | pattern |
| 15 | Chladni Plate (26) | grid | 4 | S | 4.0 | pattern |
| 16 | Stained Glass (30, reconception of Voronoi 2D) | grid | 4 | S | 4.0 | pattern |
| 17 | Harmonograph (29, reconception of Lissajous tracer) | grid | 4 | S | 4.0 | pattern |
| 18 | Turmites (18) | grid | 4 | S | 4.0 | pattern |
| 19 | Living Topo Map (27) | grid | 4 | S | 4.0 | pattern |
| 20 | Fibonacci Dot Globe (8) | grid | 4 | S | 4.0 | pattern |
| 21 | Bokeh Night (48) | grid | 4 | S | 4.0 | pattern |
| 22 | Star Trails (56) | grid | 4 | S | 4.0 | pattern |
| 23 | Word Clock (66) | grid | 4 | S | 4.0 | pattern |
| 24 | Fibonacci Clock (65) | grid | 4 | S | 4.0 | pattern |
| 25 | Pac-Chase (72) | strip | 4 | S | 4.0 | pattern |

**Just outside the top 25** (also ratio 4.0): Perlin Loop Ring, Orbits and
Slingshots, Gravity Wells, Fuse and Sparkler, Day Ribbon, and the Hearth,
Nocturne and New Year scenes. **Best M-effort bets** (ratio 2.5, highest
payoff): Life with Ghosts (the library has no Life at all, and
`stencil2D` makes it cheap), Physarum Network, Strange Attractor Nebula,
Marbling, Rain on Glass, Wind in Grass, Seasons Tree, Spiral Galaxy, Tetris
Clock, Textured Tunnel, Maze Builder and Aquarium.
