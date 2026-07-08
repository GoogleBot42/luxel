# Ice Floes 2D
kind: 2D
sensors: no

## What it looks like
A top-down view of a river carrying broken ice. The display is divided into a few large irregular cell-shaped regions ("floes") that glow icy blue-white at their centers and fade to deeper blue toward their edges. Thin dark saturated-blue seams — the "cracks" between floes — separate the regions. All floes drift steadily in one direction (the river's current), each at a slightly different rate and with a gentle sideways wander, so the cell boundaries continuously shear, merge, and split. Motion is smooth and continuous; at the default speed a floe takes a few seconds to traverse the display, and the whole scene feels like slowly churning pack ice.

## Algorithm
This is an animated Voronoi diagram over a small set of moving control points.

State between frames: a small fixed set of control points (a handful — the original uses four), each with a 2D position in the unit square and a 2D velocity, plus a countdown/accumulator for the simulation tick.

Initialization: each point gets a uniformly random position. Its horizontal velocity is random but always in the same (negative/upstream) direction within a narrow band — this is the river current, so all floes drift the same way at slightly different speeds. Its vertical velocity is random, small (roughly a quarter the magnitude of the horizontal drift), and centered on zero — the sideways wander.

Per frame: accumulate elapsed time; when it passes a fixed tick interval (a small fraction of a second, giving roughly fifteen-ish simulation steps per second), advance the simulation and reset the accumulator. Rendering happens every frame, but point positions only change on ticks, decoupling simulation rate from frame rate.

Per tick: each point's horizontal position advances by its horizontal velocity scaled by the user speed setting, wrapping around the unit interval (fractional part; anything that lands negative is snapped to just under one, i.e. wraps to the far side). Vertical position advances by its vertical velocity unscaled. The intent is that floes bounce off the top and bottom "riverbanks" (position clamped to the edge, vertical velocity negated). Note: in the original, an ordering quirk in the wrap-vs-bounce checks makes the lower bank effectively wrap rather than bounce; only the top bank truly bounces. Either behavior looks fine — a faithful reimplementation may simply bounce at both banks.

Per pixel (2D render with normalized coordinates in the unit square): exhaustively compute the distance from the pixel to every control point and track the minimum. Distances are toroidal Euclidean: for each axis take the absolute coordinate difference, and if it exceeds half the unit range use its complement, then combine the two deltas as a Euclidean length. The exhaustive nearest-first-free scan is deliberate — no spatial optimization — because the crack detection needs to compare candidate distances against the running minimum:

- Whenever a point is at least as close as the best so far, compare its distance to the previous best. If the two are nearly equal (within a smallish tolerance — roughly a tenth of the unit scale), the pixel lies near the boundary between two cells: set its hue to a fixed deep pure blue "crack" marker. Otherwise set the hue to a cool cyan-leaning blue nudged slightly bluer with increasing distance.
- After the scan, brightness is one minus the minimum distance, then cubed — a steep falloff that makes floe centers glow and edges go dim.
- Saturation: crack pixels are fully saturated; all other pixels get a saturation of roughly (slightly more than one) minus the brightness, so bright floe centers desaturate toward icy white while dimmer edge areas stay richly blue.

No per-pixel state; all randomness happens at initialization only.

Layout assumptions: requires a 2D mapping; coordinates are treated as the normalized unit square, so any 2D layout works. The number of floes is hardcoded to a small constant — the obvious improvement is to expose it as a control or scale it with display area.

## Colors
Whole palette lives in the blue family: floe interiors run from near-white icy pale blue (bright centers) through cool cyan-blue midtones to dim deeper blue at the edges; crack seams are a dark, fully saturated pure blue; the far reaches between floes fade toward black.

## UI controls
- Slider, "speed": scales the current's drift rate, from frozen (near zero) up to a few times the default flow. Only affects horizontal drift; vertical wander is unscaled.

## Timing
Simulation ticks many times per second so motion appears continuous. Default drift carries a floe across the display in a few seconds; the crack topology reshapes on a similar timescale.

## Non-obvious notes
- The "cracks" are not drawn as lines; they emerge from flagging pixels whose two nearest control points are nearly equidistant during the distance scan — a cheap Voronoi-boundary detector that falls out of the exhaustive minimum search.
- The toroidal distance metric keeps cells seamless as points wrap horizontally, avoiding visible seams at the display edges.
- Cubing the inverted distance for brightness is what gives the floes their glassy, center-lit look.
