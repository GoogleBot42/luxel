# 80s kid show
kind: 2D
sensors: no

## What it looks like
A handful of brightly colored geometric shapes — circles, squares, triangles, hexagons, and six-pointed stars — drift around a 2D panel like a retro screensaver or a Saturday-morning cartoon title card. Each shape bounces off the edges of the display, slowly spins in place, and rhythmically swells and shrinks in size. Optionally the whole scene rotates as one, and shapes can be drawn as thin outlines or solid fills. Occasionally two shapes trade stacking order so the same one isn't always drawn on top. Background is black; shapes are saturated, evenly-spread rainbow hues.

## Algorithm
The pattern maintains a pool of shape objects (capacity of roughly a dozen and a half; a UI slider chooses how many are active). Each object carries persistent state: 2D position, 2D velocity, hue, which shape it is, a base rotation phase, a random per-object size multiplier (between one and three times the nominal size), a random period for its grow/shrink cycle, and a cached "current size" recomputed each frame.

At initialization, every object gets a random position anywhere on the unit square, a small random velocity in each axis, and a shape: either one fixed shape chosen by the UI, or (at the top of the shape-select slider) a random pick per object. Hues start at a random value and each subsequent object's hue is offset by a large fixed fraction of the color wheel (a bit under half), so the set spans the rainbow without adjacent duplicates.

Per frame (before rendering):
1. **Move & bounce.** Each active object's position advances by velocity scaled by the speed slider. If a coordinate leaves the unit square it's clamped to the edge and that velocity component is negated (classic wall bounce).
2. **Spin.** A global rotation phase sweeps a full turn every several seconds. Each object's effective rotation is this global phase plus its own base offset; the sine and cosine of that angle are precomputed and cached per object.
3. **Breathe.** Each object's current size is its per-object size multiplier times the size slider value, modulated by a triangle wave with that object's own random period (periods can range from around a second to on the order of a minute), offset so the size oscillates between half and one-and-a-half of nominal.
4. **Z-order shuffle.** With a small chance each frame (a few percent), two randomly chosen active objects swap entirely, changing which is drawn "on top".
5. **Scene spin (optional).** If the spin toggle is on, the global coordinate transform is reset and a rotation about the display center by the global phase is installed, so the whole canvas turns.

Per pixel: iterate the active objects in order. For each, translate the pixel into the object's local frame; do a cheap axis-aligned bounding-box rejection first (box half-size = current shape size times the cutoff slider factor). If inside the box, rotate the point by the object's cached sine/cosine, then evaluate that shape's **signed distance function** (SDF) at the rotated point with the current size as radius. Five SDFs are used: circle, square, equilateral triangle, hexagon, and hexagram (six-pointed star) — standard 2D SDF math of the kind popularized by Inigo Quilez's distance-function articles. A comparison predicate chosen by the "filled" toggle decides a hit: outline mode requires the absolute distance to be within the line width (a thin ring around the boundary); filled mode requires the point to be inside (distance below the line width). The **first** object that hits wins and the loop stops — that's what makes array order act as z-order.

Coloring on a hit: hue is the object's hue. In "flat" mode saturation and brightness are full. In shaded mode, brightness falls off with signed distance relative to the line width and saturation eases toward white near the exact boundary, giving a soft neon-glow edge. Final brightness is squared for contrast. Pixels with no hit end up black (brightness defaults to nothing).

## Layout assumptions
Pure normalized 2D coordinates; no pixel-count hardcoding. Needs a 2D mapping. There is no 1D or 3D renderer.

## Colors
Fully saturated rainbow hues spread evenly around the wheel across the object set, on a black background. In shaded mode, outline edges bloom toward white.

## Controls (all rendered as sliders; several act as toggles at half-travel)
- **Number of floaters** — how many shapes are active (a couple up to the pool max).
- **Shape type** — selects circle / square / triangle / hexagon / star; the top end of the slider gives a random mix. Changing it re-randomizes all objects.
- **Size** — nominal shape size (up to a large fraction of the panel).
- **Speed** — drift velocity scale.
- **Filled** (toggle at half) — outline vs solid shapes.
- **Line width** — outline thickness / fill edge softness.
- **Cutoff** — how far outside the nominal size the bounding box (and shaded glow) extends.
- **Spin** (toggle at half) — rotate the entire scene.
- **Flat** (toggle at half) — flat full-brightness color vs distance-shaded glow.

## Timing feel
Global rotation: several seconds per revolution. Size breathing: each shape on its own cycle from about a second to about a minute. Motion speed is user-set but reads as a gentle drift at defaults. Z-order swaps happen every few seconds on average.

## Clever bits
- SDF rendering makes arbitrary rotated vector shapes cheap on a per-pixel basis; the bounding-box early-out and "first hit wins" break keep the per-pixel cost low.
- The occasional pairwise object swap is a cheap way to vary occlusion order without a real depth sort.
- Precomputing each object's sine/cosine once per frame avoids per-pixel trig.
