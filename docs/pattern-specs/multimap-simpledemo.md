# multimap simpledemo
kind: 2D (matrix with a 2D map required; a token 1D fallback maps the strip onto one row)
sensors: no

Note: despite being tagged sound/sensor-reactive in some catalogs, this pattern reads no sensor inputs at all. It is a framework demo, not a finished effect.

## What it looks like
Most of the matrix is dark. Two regions are alive:
- A small circular disc at the center of the matrix (radius about a fifth of the panel) pulsing blue — the whole disc brightens and dims together, roughly one pulse per second or two.
- The quarter of the panel where both normalized coordinates are below their midpoints (one corner quadrant), filled with green whose brightness pulses at a rate that varies per pixel (the pulse period depends on the product of the pixel's coordinates), giving the quadrant a shimmering, out-of-sync twinkle rather than a uniform blink.
Where the disc overlaps the quadrant, the disc wins. Everything else stays off.

## Purpose and algorithm
This is a minimal demonstration of a "multiple sub-maps" technique: partitioning a single mapped display into named regions, each running its own mini-pattern, with priority ordering and optional coordinate remapping per region.

Structure:
- A parallel pair of lists, one of region-membership tests and one of mini-pattern functions, index-aligned (region N pairs with pattern N). The demo has two of each; the design intent is that users append more.
- A handful of shared "out parameters": a membership flag, plus replacement values for the pixel index, pixel count, and both coordinates. Before testing each pixel, these are seeded with the pixel's real index/count/coordinates; a region test may overwrite the coordinate values to re-express the pixel in that region's local frame (e.g., re-centered, or renormalized to 0..1 within the region).

Per pixel (2D): iterate the region tests in list order. The first test that claims the pixel stops the search ("first match wins"), and its paired mini-pattern is invoked with the possibly-remapped index and coordinates. If no region claims the pixel, it is painted black explicitly. Because earlier entries win, overlapping regions layer with earlier = on top, and one could add a catch-all last entry as a background layer.

Demo region one: shifts coordinates so the panel center is the origin, then tests squared distance from center against a squared-radius threshold (circle test without a square root). It passes the center-relative coordinates through as the remapped ones (so its pattern sees coordinates centered on zero — the demo pattern happens not to use them).

Demo region two: claims pixels whose (original) coordinates are both under one-half, without remapping anything.

Demo pattern one: fixed blue hue, full saturation, brightness driven by a smooth 0..1 waveshaped sawtooth with a period around one to two seconds — a synchronized pulse.

Demo pattern two: fixed green hue, full saturation, brightness from the same kind of waveshaped sawtooth but with a period offset per pixel by the product of its two coordinates, so nearby pixels pulse at slightly different rates and drift in and out of phase — a gentle shimmer.

State between frames: none (all animation comes from the global time phase functions).

The 1D fallback simply feeds the pixel's normalized strip position as the horizontal coordinate with the vertical coordinate pinned at zero, so on a strip you'd see the green shimmer over half the strip (the disc, needing a vertical center, never matches).

## Layout assumptions
Region geometry is expressed in normalized 0..1 coordinates, so it scales to any matrix. The remapped-index and remapped-pixel-count hooks exist but are unused in this demo. Nothing hardcodes pixel counts.

## Colors
Pure saturated blue (the disc) and pure saturated green (the quadrant) on black. Qualitatively "primary-ish" hue anchors; no palettes.

## Controls
None.

## Timing
Blue disc: a smooth full pulse roughly every second or two. Green quadrant: pulses of a couple seconds each, desynchronized across pixels.

## Non-obvious bits
- The whole value of the pattern is the dispatch scaffolding: ordered region tests with first-match-wins, communication through shared out-variables (membership flag plus remapped coordinates), and explicit black for unmatched pixels so stale colors never linger.
- The circle test avoids a square root by comparing squared distance to a squared radius.
- A reimplementation should preserve the extensibility shape (parallel region/pattern lists a user can grow) rather than hardcoding the two demo regions.
