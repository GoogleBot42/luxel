# Contour Blobs

Idea spec written from Patternflow preset preset_0519_1.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

Organic islands float on a black sea. Inside each island, the terrain is drawn as a topographic map: nested contour bands that follow the island's shape, coloured from green at the shoreline through teal to blue toward the high ground. In "sharp" mode each band is a dim fill with a bright thin contour line at its upper edge — a classic elevation map. In "smooth" mode every band is a sawtooth ramp from dim to bright, giving a terraced, faceted look. The islands swell, merge, split and drift as the underlying wave sum evolves; a tension control raises or lowers the "sea level" so the land floods to a few small islets or grows into a continent with small lakes. Calm, cartographic, rather beautiful.

## The idea

Treat a smooth animated scalar field as terrain: threshold it at sea level for the coastline and draw evenly spaced iso-contours above it. It works because contour lines make any smooth field look like a living landscape.

## Algorithm in words

- State: time accumulator from the speed knob.
- Per pixel at a zoom-scaled position: field = average of three wave terms — a sine-of-x plus cosine-of-y pair, a diagonal sine, and an anti-diagonal cosine — each moving at its own rate. Add the tension offset (a signed shift, negative to positive).
- Where the field is at or below zero: black (sea).
- Above zero: multiply the field to get a number of contour bands; take the fractional part as the position within a band.
  - sharp mode: bright only in the last part of each band (the contour line), dim elsewhere;
  - smooth mode: brightness ramps across the band.
- Hue rises with the field from green toward blue; saturation fixed; value from the band shading.

## Knobs

- K1 — tension / sea level: low drowns the land to islets, high fills the panel.
- K2 — undulation speed.
- K3 — zoom: few large continents to many small islands.
- K4 — contour style: below the middle smooth terraces, above it sharp contour lines (a switch rather than a dial).
- No audio.

## Making it better in Luxel

- Use `perlinFbm` or `simplex3` with two octaves as the terrain — real coastlines with bays and peninsulas rather than sine blobs.
- Colour with a hypsometric palette via `setPalette`/`paint`: deep-to-shallow blues for water (instead of pure black), sand, greens, browns, snow-white peaks — the sea can show depth contours too. OKLCH ramps keep it perceptually even.
- Anti-alias contour lines with `smoothstep` on the band fraction and make line thickness a real-unit control; let sharpness be a continuous dial between terrace and line.
- Slowly "erode" by animating sea level with `easeInOutSine` tides; add a scene variant with a sprite "ship" or text map labels for fun.
- Strips: 1D works as a "cross-section" profile — a decent variant but secondary. Main form: one 2D pattern.

## Payoff

Score: 4/5 — topographic maps of moving noise are gorgeous on a panel; with fbm and a map palette this is a flagship.
