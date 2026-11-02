# Normal-Map Plexus

Idea spec written from Patternflow preset preset_0527.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A handful of nodes (a few up to around ten) drift around the panel on smooth looping orbits, each marked by a small white plus-sign crosshair. Any two nodes closer than a link distance are joined by a thin, one-pixel straight line, so a constellation / "plexus" network forms, stretches and snaps as the nodes move — links appear and vanish as pairs cross the threshold. The twist is the line colour: each line is coloured by its *direction* the way a 3D normal map encodes vectors — horizontal-ish lines lean red, vertical-ish lean green, with a blue component set by a knob — so lines at different angles have different pastel/neon tints and a line's colour shifts as it rotates. Black background, hard pixel lines. It looks like a tech/sci-fi data graph, sparse and clean.

## The idea

A plexus (proximity graph of moving points), with each edge coloured by its orientation encoded as RGB. It works because the proximity rule makes the topology change continually, and orientation colour adds information for free.

## Algorithm in words

- State: time accumulator from the speed knob; node count from K1.
- Each frame, compute each node's position: x on a cosine and y on a sine around the panel centre, with radii a large fraction of the panel's half-width and half-height, each with a per-node pseudo-random phase and different x and y rates — so nodes trace independent elliptical / Lissajous-like loops.
- Clear to black.
- For every pair of nodes closer than the link distance (K3): compute the unit direction between them; colour = direction x mapped to red (pointing fully one way → dark, fully the other → bright), direction y mapped to green, and blue = K4 constant. Draw the segment by testing pixels in its bounding box for perpendicular distance under about a pixel and projection within the segment.
- Draw a small white plus at each node.

## Knobs

- K1 — node count ("cluster weight").
- K2 — orbit speed.
- K3 — link distance: short gives sparse pairs, long gives a fully connected web.
- K4 — blue component of every line (the "depth" of the normal map), from warm red/green tints to cool pastels.
- No audio.

## Making it better in Luxel

- Natural `renderBulk` pattern: `clear()`, a `drawLine` per linked pair (anti-aliased capsules, and line brightness fading with distance so links fade in/out smoothly instead of popping), a `splat` or small cross per node — trivially cheap.
- Move nodes along `curl2` flow or with gentle steering so the motion is less mechanical; let nodes enter and leave the panel.
- Colour by orientation using OKLCH hue = line angle (a full colour wheel, constant lightness) — a cleaner "normal map" that also works on dark backgrounds; alternatively colour by length.
- Add `fade()` persistence for faint trailing echoes of the network, and `setGlow` for neon lines. Audio: link distance can breathe with `energyAverage`.
- As a scene: node sprites + a lines pattern layer. Strips: not a fit.

## Payoff

Score: 3/5 — a classic plexus with a fresh colouring idea; with fading links and glow it becomes a nice ambient piece.
