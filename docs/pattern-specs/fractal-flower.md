# fractal flower
kind: 2D
sensors: no

## What it looks like
A kaleidoscopic flower made of glowing dotted branches. Several identical fractal "petals" are arranged in a ring around the center of a 2D panel, each petal a recursive binary tree of light points whose branch angles slowly flex, so the flower continuously folds, unfurls, and morphs between spiky stars, ferns, and spiral pinwheels. Colors sweep through the rainbow, with hue shifting along branch depth so each petal shows a gradient; the brightest overlapping points can bloom to white. Motion is smooth and hypnotic — trails linger and fade behind the moving structure, and overall brightness self-adjusts so dense moments don't blow out.

## Algorithm

### Offscreen buffer
The pattern renders into an offscreen square grid (a couple dozen cells per side, **hardcoded** — the comments suggest matching it to your display's dimensions, or setting it lower for a deliberately pixelated mosaic look; the obvious fix is to size it from the actual mapped display). Two parallel arrays over that grid store per-cell brightness and per-cell hue. The visible renderer just samples this buffer.

### Per frame
1. **Fade.** Every cell's brightness is multiplied by a persistence factor (user "trails" slider; near one leaves long ghost trails, lower values crisp).
2. **Animate parameters.** Three slow oscillators (sine applied to sawtooth/triangle timebases at mutually incommensurate periods, all inversely scaled by the speed slider — think tens of seconds to a couple of minutes per cycle at defaults) drive: the first branch angle (swinging around a value near minus one radian by up to plus/minus pi times its range slider), the second branch angle (swinging around a small positive value similarly), and the starting heading of the whole figure (swinging across a full half-turn each way). The base hue advances on its own cycle of several seconds.
3. **Draw the fractal(s).** For each replica (count from the replicas slider), start a recursive tree at a point on a circle around the panel center (radius from the spacing slider; with a single replica it draws dead-center). Two placement modes: **pinwheel** (petal positions fixed on the ring; each petal's starting heading is the animated heading plus its angular slot, so petals spin in place) vs **orbit** (the animated heading is added to the ring position instead, so the whole ring of petals revolves around the center). 

   The recursion, given a position, heading, and remaining depth: on all but the outermost call, step the position forward along the heading by a distance proportional to the remaining depth times the scale slider (so steps shrink toward the branch tips). Optionally wrap coordinates toroidally ("wrap world" toggle). If the current depth is at or below the draw-levels setting and the point is on-screen, deposit light into the buffer cell there: the new hue is the animated base hue plus a small offset proportional to depth (the depth-gradient), and it is **blended with the cell's existing hue as a brightness-weighted circular average** (hues are rotated so the two are numerically close before averaging, correctly handling wrap-around on the color wheel); the cell's brightness is bumped up by a unit per deposit, accumulating where branches overlap. Then, if depth remains, recurse twice — once per branch angle added to the heading — forming a binary tree. A global counter tracks total node visits (exported for monitoring).

4. **Auto-exposure.** The brightest cell value observed during the previous render pass feeds a slowly-relaxed normalization divisor (an exponential blend, moving a few percent per frame toward the new max, clamped between one and a large cap). This gradually rescales output brightness so heavy overlap reveals structure instead of clipping, without visible flicker.

### Per pixel
Map the pixel's normalized coordinates to a buffer cell. Divide the stored brightness by the auto-exposure divisor, then square it for contrast. Saturation: in "white mode" saturation decreases as brightness increases (hot spots bleach to white); otherwise fully saturated. Hue comes straight from the cell.

## Layout assumptions
Needs a 2D mapping; the internal buffer resolution is a hardcoded square (fix: derive from display size). Only a 2D renderer exists.

## Colors
Continuously cycling rainbow base hue; along each branch, hue shifts progressively with recursion depth, producing gradient petals. Where branches overlap, hues mix smoothly (brightness-weighted). Optional white bloom at the hottest points. Background: black, with colored ghost trails when trails are long.

## Controls (all sliders; the last three act as on/off toggles at half-travel)
- **Iterations** — recursion depth (one to around nine levels; deeper = more, finer dots, exponentially more work).
- **Draw levels** — how many of the deepest levels actually deposit light (skipping the early/coarse levels near the trunk).
- **Scale** — overall petal size (with a squared response for finer control at the small end).
- **Speed** — speeds up all the oscillators and the hue cycle together.
- **Angle range 1 / Angle range 2** — how widely each of the two branch angles swings; small values give gentle sway, large values wild morphing.
- **Trails** — persistence of the fade buffer (zero disables persistence entirely).
- **Replicas** — number of petals in the ring (one to about a dozen).
- **Spacing** — ring radius, i.e. how far petals sit from center.
- **White mode** (toggle) — bleach hot spots to white vs keep them saturated.
- **Pinwheel mode** (toggle) — petals spin in place vs the ring revolving.
- **Wrap mode** (toggle) — branches leaving one edge re-enter the opposite edge.

## Timing feel
Shape morphing evolves over tens of seconds to minutes; the color cycle turns over in several seconds; the auto-exposure breathes over a second or two. Nothing is frame-rate coupled except the fade/exposure relaxation rates (per-frame multiplies — mild frame-rate dependence, could be delta-scaled).

## Clever bits
- Rendering the recursive structure once per frame into a coarse buffer decouples the (exponential) fractal cost from pixel count.
- The brightness-weighted circular hue averaging when depositing keeps overlapping colored branches from producing muddy or wrap-around-artifact hues.
- The slow-tracking max-brightness normalizer is an automatic exposure control: it preserves detail when many dots stack, and its slow relaxation prevents flicker.
- Using two independently oscillating branch angles at incommensurate periods yields a very long non-repeating morph sequence from trivially little state.
