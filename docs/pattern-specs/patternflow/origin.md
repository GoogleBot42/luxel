# Origin

Idea spec written from Patternflow preset preset_origin.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description. (It is Patternflow's failsafe boot pattern; its lineage note says it was designed in Blender and converted.)

## What it looks like

A centred arrangement of square tiles on black — a gallery wall of little screens. A mode knob picks the layout: two big squares side by side, a small grid of medium squares, a denser grid of smaller squares (in two variants with coarser or finer inner pixels), or a full wall of small tiles with no gaps. Every tile shows its own ripple: rings radiating from the tile's centre. But each tile is drawn in deliberately chunky "macro-pixels" (a coarse grid inside the tile), and each successive tile (in reading order) uses a higher ring frequency than the last. Because the ring frequency can be pushed far beyond what the coarse sampling can represent, most tiles alias: instead of clean rings you see moiré — crosses, diamonds, checkerboards, rosettes, flickering symmetric mandalas — and every tile shows a different one. The colour ramp is black → dark grey → one saturated hue (picked by the hue knob) → white, so each tile has a black/grey ground, coloured mid-tones and white peaks. Animation is the ripple phase running, which makes each tile's moiré pulse and churn. It reads like a wall of oscilloscope test patterns or a sampling-theory demo, and the "every tile different" variety is the charm.

## The idea

Radial ripples sampled on a deliberately coarse grid with a per-tile frequency ladder, so aliasing turns a single simple function into a wall of distinct symmetric moiré patterns. The aliasing is the feature.

## Algorithm in words

- Layout presets: each defines rows and columns of tiles, the gap between them, the tile size, the inner macro-pixel size and the number of macro-pixels per tile side. The layout is centred on the panel.
- When the mode changes: precompute, for each macro-pixel position inside a tile, its distance from the tile centre.
- Per frame: advance the phase by the speed control. Build a colour ramp with stops at black, dark grey, the hue-knob colour at full saturation and value, and white (white held to the top).
- Per pixel: find which tile it is in (gaps and outside the layout are black), then which macro-pixel inside the tile. Tile frequency = the base frequency control + the tile's index × a fraction of that base. Wave = sine(macro-pixel distance × tile frequency + phase), scaled down a little, mapped to 0..1 and sampled through the ramp.
- The original wires the four push-buttons to "reset this knob" (hue to red, speed to zero, mode to the first layout, frequency to its minimum).

## Knobs

- K1 hue — the ramp's mid colour (in whole-degree steps).
- K2 speed — ripple phase speed (zero freezes the moiré).
- K3 mode — tile layout preset.
- K4 freq — base ring frequency, over a very wide range; low values give real rings, high values give the aliased moiré zoo.
- Audio: none directly; mapping sound to freq makes the moiré jump between forms on every hit, which is likely the intended "LED synthesizer" use.

## Making it better in Luxel

- Compute each tile at its macro-pixel resolution into a small canvas per tile (or one canvas for the whole wall) and draw macro-pixels with `fillRect` / `blit` in `renderBulk` — the per-pixel work collapses to a handful of values per tile. That also makes 128x128 trivial.
- Lay tiles out from `gridWidth()`/`gridHeight()` (any panel, any aspect) and let the mode set a tile count rather than fixed pixel sizes. Let the source function per tile vary too (radial, angular, x·y products) for an even wider zoo.
- Animate the frequency ladder slowly by itself (eased drift through the moiré "sweet spots") so it is interesting without anyone turning a knob; colour via OKLCH ramp or `setOutputPalette` presets.
- As a scene: each tile could be a separate pattern layer at a different frequency, but a single pattern is simpler and cheaper.
- Near-duplicate of "Tile Waves" (tiled radial ripples with grout) and shares its four-stop black/grey/hue/white ramp with "Warped Wave"; a combined tiled-ripples pattern with an aliasing mode covers both. Strips: a strip of segments each aliasing a different frequency would work as a novelty.

## Payoff

3/5 — the deliberate aliasing makes it far more interesting than it sounds; a cheap, distinctive "moiré gallery" for big panels.
