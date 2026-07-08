# Slime mold palette
kind: 2D
sensors: no

## What it looks like

An organic, slowly-growing "paint blob" effect on a 2D-mapped panel. Starting from one (or a few) seed pixels, color creeps outward pixel by pixel, and because each new pixel is placed where its already-painted neighbors most closely match its color, similar colors clump into smooth, amoeba-like regions — like slime mold growth or the classic "rainbow smoke" all-colors images. The image fills in over some seconds, holds when complete, then (by default) wipes after roughly half a minute and regrows with a fresh randomly-chosen palette.

On first startup (and after a remap is triggered) there is a brief one-time "calibration" phase with its own diagnostic visuals: the pixel currently being analyzed glows pure blue, candidate neighbors flash dim yellow or dim red, and a faint gray fill rises across the panel like a progress bar. This lasts a few seconds (one pixel analyzed per frame) and then the drawing effect begins automatically.

## Algorithm

The pattern runs as a state machine with two phases. It swaps which per-pixel render routine is active depending on the phase (the 2D render entry point is reassigned at runtime — a notable trick).

### Phase 1: neighbor-map construction

Goal: for every pixel, precompute a small fixed-size list (about eight entries) of the indices of its spatially nearest neighbors, using the installation's 2D map coordinates, limited to a small radius (sized so that adjacent cells of a small square grid — roughly 8x8 — qualify; for other layouts this radius should scale with typical inter-pixel spacing, which is the obvious generalization).

There is no direct way to iterate the coordinate map from frame code, so the per-pixel render pass itself is used as the iterator: each frame, one "target" pixel is selected, and as the renderer visits every pixel it computes that pixel's distance to the target; pixels within the radius compete for a slot in a small keep-the-closest sorted list. At the start of the next frame the completed list is committed to the big map and the next target is chosen. Targets are visited in shuffled random order (a standard array shuffle), reusing the color buffer as scratch space for the shuffled index list during this phase.

Memory-saving detail: the neighbor map stores two small integer indices per array element by packing one index into the integer part and one into the fractional part of the fixed-point number, with a sentinel value meaning "empty slot". Distances in the temporary sorted lists are packed similarly — distance in the high bits, candidate index in the fractional part, plus a few random bits in between as a tie-breaker so equal distances don't sort in index order.

When every pixel has been mapped, the pattern automatically starts a fresh drawing.

### Phase 2: incremental drawing

Persistent state: one value per pixel holding either "unpainted" or a palette position (a fraction along the current gradient palette); a count of painted pixels; a search cursor; a small sorted best-matches list; a target palette position for the pixel currently being placed; a redraw countdown.

Each drawing step works like this:

1. A random target palette position is chosen.
2. The pattern scans across all pixels looking at each *unpainted* pixel's neighborhood: for each painted neighbor, it accumulates the absolute difference between that neighbor's stored palette position and the target position, and averages. Pixels whose neighborhoods contain no painted pixels are skipped.
3. Candidates compete for a small (about eight-entry) sorted list of lowest average distance (same packed-value + random-tie-break trick as above).
4. When the scan completes, the single best candidate is painted with the target position and the search restarts with a new random target.
5. If the scan completes but found no candidate while unpainted pixels remain (an isolated island with no painted neighbors anywhere near it), a new random seed pixel is planted instead.

The scan is budgeted: only on the order of a thousand neighborhood evaluations run per frame, so on large displays one placement can span multiple frames. This keeps the frame rate steady at the cost of slower growth.

Once every pixel is painted, an elapsed-time accumulator runs; when it exceeds the configured redraw delay (and auto-redraw is on), the display is cleared to unpainted, a new random palette is selected, the configured number of seed pixels are planted at random positions with random palette positions, and growth begins again.

Per-pixel rendering in this phase is trivial: if the pixel has a stored value, render it through the current gradient palette; unpainted pixels stay dark.

## Colors

All drawing-phase color comes from a gradient-palette lookup. The pattern carries a library of a few dozen gradient palettes (adapted from a well-known community gradient collection), including: a full rainbow (listed several times so it is picked disproportionately often); lava and fire ramps (black through deep reds and oranges to near-white); ocean/teal ramps; coral and pink-splash magentas; sunset (deep red through orange into violet-blue); pastel pink-purples; forest/landscape greens into sky blues; autumn rust reds; vintage sepia-golds; and several black-to-primary-to-white ramps (e.g. black-blue-magenta-white, blue-cyan-yellow). Each redraw picks one palette uniformly at random. An exported read-only value reports which palette index is active.

Mapping-phase diagnostics: analyzed pixel bright blue; accepted-neighbor candidates dim yellow; checked-but-rejected candidates very dim red; progress fill very dim gray (a fill whose covered fraction of the panel tracks mapping completion).

## UI controls

- Number input, "initial seed count": how many random seeds start each drawing (floored at one).
- Trigger, "seed a pixel": plants one extra random seed mid-drawing (gives up after a bounded number of attempts if the display is nearly full).
- Trigger, "new drawing": clears, picks a random palette, reseeds, restarts growth (ignored during mapping).
- Trigger, "rebuild neighbor map": restarts the phase-1 calibration (needed after changing the pixel map).
- Toggle, "auto redraw": whether a completed image wipes and regrows on its own.
- Number input, "redraw delay (seconds)": hold time on the finished image; defaults to about half a minute.
- Trigger, "random palette": switches to a new random palette immediately (recoloring is only visible on subsequently painted pixels).

## Timing feel

Mapping: a few seconds (one pixel per frame). Growth: the image fills over several seconds to tens of seconds depending on pixel count (throttled by the per-frame evaluation budget). Finished image holds around half a minute by default, then repeats.

## Layout assumptions

Requires a 2D pixel map. The neighborhood radius is tuned for a small dense grid; on sparser or larger layouts it should be scaled to the typical nearest-neighbor spacing. Index packing assumes pixel indices fit comfortably in the fractional half of the fixed-point representation (fine for realistic pixel counts).
