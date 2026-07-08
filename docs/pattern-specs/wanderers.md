# wanderers
kind: 2D (implemented through the 1D renderer with a hardcoded matrix mapping — no map file used)
sensors: no

## What it looks like
A few dozen small colored dots scattered over an otherwise dark matrix, each performing an independent slow random walk — drifting one cell at a time in random directions, like fireflies or ants meandering. Each dot keeps its own fixed color for its whole life, and together the dots span the entire rainbow. Motion is unhurried: any given dot steps only every couple dozen frames, so the field shimmers with sporadic single-cell hops rather than continuous sliding.

## Algorithm
Layout: the pattern hardcodes a matrix geometry (a short-and-wide panel, e.g. the common flexible 8-row by 32-column type) plus a zigzag/serpentine flag, and does its own x/y ↔ pixel-index math, including un-zigzagging alternate columns. Pixel count, rows, and columns must agree; the obvious fix is to derive geometry from the installed pixel map or expose rows/cols as controls.

State kept between frames:
- An array of walker positions (a configurable count of walkers, default a few dozen), each stored as a raw pixel index. Initialized to uniformly random pixels at startup.
- A full-pixel-count scratch framebuffer of hue values, where a sentinel "negative" value means unlit.

Per frame:
1. Clear the framebuffer to the "unlit" sentinel.
2. For each walker, draw one uniform random number against a "speed" constant. With probability of roughly a few chances in that constant, the walker takes one step; the specific sub-range of the draw picks the direction: one step right, left, up, or down (each direction equally likely at about one part in the constant; otherwise it stays put). Horizontal motion wraps around (cylinder topology — commented alternative clamps at the edges instead); vertical motion clamps at the top and bottom rows. The step is computed in decoded x/y space and re-encoded through the zigzag mapping.
3. Write the walker's identity, expressed as its index divided by the walker count (a value spread evenly over the unit interval), into the framebuffer at its new position. Later walkers overwrite earlier ones on collisions.

Per pixel: if the framebuffer holds the sentinel, output black; otherwise output a fully saturated, full-brightness hue equal to the stored value. Walker 0 is at the red end and the rest fan out across the wheel.

Randomness: one uniform draw per walker per frame; that is the whole animation driver — there is no time base at all, so apparent speed is frame-rate dependent (worth noting: a faithful reimplementation may want to scale the step probability by elapsed time instead). The "speed" constant works inversely — larger means slower — and must stay at least as large as the number of direction sub-ranges or walkers would move every frame.

## Colors
Black background; dots are maximally saturated hues evenly distributed around the full rainbow, one fixed hue per walker.

## Controls
None exposed. Walker count, speed (inverse), matrix dimensions, and zigzag are edit-the-source constants; all four are natural slider/toggle candidates.

## Notes
The clever bit is cost: nothing accumulates or fades — the frame is fully redrawn from walker state each frame, so per-frame work is O(pixels + walkers) with a single random draw per walker deciding both whether and where to move.
