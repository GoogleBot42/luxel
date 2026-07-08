# Snake 2D
kind: 2D
sensors: no

## What it looks like
On a matrix, a glowing snake head roams around, leaving a smoothly fading heat trail behind it. The head is white-hot; the trail cools through a saturated hue that slowly cycles around the color wheel over several seconds. The snake slithers in S-curves rather than straight lines, avoids the border of the panel (curving away as it approaches an edge, harder in corners), occasionally changes its slither direction on a whim, speeds up near the walls and loafs near the center, and — if all else fails — bounces off a wall like a billiard ball. Trail length looks constant regardless of how fast it is moving.

## Algorithm
Layout assumption: the matrix width in pixels is hardcoded (a square-ish panel, sixteen wide in the original), with height derived as pixel count divided by width. Obvious fix: derive width/height from the 2D pixel map or expose width as a setting. The render pass uses normalized 2D map coordinates scaled back up to integer cell indices, so the pixel map is assumed to be a regular grid.

State kept between frames:
- Head position (continuous x, y in pixel units) and a bearing angle (direction of travel).
- Current per-frame turn rate ("deflection", signed — its sign is which way the snake is curling), an accumulator of how far the current curl has turned, and a separate corrective turn rate used while escaping edges.
- A 2D "heat" array with one scalar per matrix cell.

Per frame (pre-render), in order:

1. Two slow clocks: one cycling every several seconds drives the global hue; one cycling on the order of half a minute drives speed wandering.
2. Random mood change: with a small delta-scaled probability (works out to flipping roughly once every few seconds), negate the turn direction.
3. Edge avoidance: a border zone along each wall, a modest fraction of the panel's smaller dimension thick. If the head is inside the zone, compute the direction toward the nearest wall (or corner, when in two zones at once). Near a single edge, set the corrective turn to a fixed larger-than-normal turn rate, signed to rotate the bearing away from that wall (sign taken from which side of the toward-the-wall direction the current bearing lies). In a corner, keep whatever corrective turn is already running (or start one). If the bearing is already pointing away from the wall (within a modest angular tolerance of directly away), stop turning and go straight. On leaving the zone, reset to the normal gentle turn rate with a randomly chosen sign and clear the turn accumulator. Angle differences are computed wrapped into the signed half-circle range.
4. Speed: the commanded speed (slider) is multiplied by a smooth pseudo-random wander — the product of several triangle waves at mutually prime frequencies, offset so it stays positive, giving an organic non-repeating ebb and flow — and by a factor that grows with distance from panel center (about half speed at center, roughly full extra near a corner). So it hurries along walls and lingers mid-panel.
5. Turn: the effective per-frame turn is the current turn rate scaled proportionally to both actual speed and frame delta (so curvature per unit distance is stable). Add it to the bearing, wrapped to a full circle.
6. Slither: accumulate the turn; once the accumulated curl exceeds half a full turn, flip the turn direction and reset the accumulator. This makes S-curves instead of circles and keeps the snake from eating its own tail.
7. Move the head along the bearing by speed × delta.
8. Billiard bounce as a hard backstop: if the head crosses a panel boundary anyway, reflect the bearing about that wall and clamp position inside.
9. Heat update over every cell: multiply each cell's heat by a decay factor, where the decay exponent is scaled by the ratio of current speed to nominal speed — faster travel decays faster so the visible tail length stays roughly constant. Then, for cells within the head radius (radius scales with the square root of pixel count, a few pixels on a moderate panel), add heat proportional to a high power (around sixth) of closeness to the head — a tight hot core with soft falloff — scaled up by the speed ratio (so fast passes still deposit enough heat), capped at full.

Per pixel (render2D): read the cell's heat. Brightness is heat squared. Saturation is one minus a very high power of heat, so only near-maximum heat whitens (white-hot core, saturated body). Hue is the slow global hue cycle plus a small fraction of the heat, so the core is slightly hue-shifted from the tail.

Randomness: the occasional turn-direction flip and the sign chosen when leaving the border zone. The speed wander is deterministic but noise-like.

## Colors
Full-spectrum: the base hue circles the entire color wheel continuously over several seconds. At any instant the trail is one saturated hue grading to black, with the head bleached to white and a slight hue lead over its tail.

## Controls
- Slider, "speed": scales the snake's travel speed from stopped up to about double the nominal speed.

## Timing feel
Hue makes a full rainbow loop every several seconds. Speed mood swings over tens of seconds. Slither reversals every few seconds. Tail persists a second or so behind the head.

## Clever bits worth preserving
- Delta- and speed-compensated turn rate and heat decay: curvature and tail length are invariant to frame rate and travel speed.
- Product-of-incommensurate-triangle-waves as a cheap smooth noise for speed wandering.
- Skipping the heat-deposit distance math for cells outside the head radius (decay still applied) as a per-frame performance win.
- The half-turn accumulator ("slither") is what turns simple constant-rate turning into lifelike S-motion.
