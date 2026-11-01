# Vector Field Flow

Idea spec written from Patternflow preset preset_0629_2.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

Despite the name there are no particles. It is a full-panel interference plasma: bright, saturated bands of colour that curve and fold across the panel, with hard black gaps between them and white-hot clipped crests along the band centres. The bands look like contour lines of a liquid surface being stirred — they bend, pinch and reconnect as time runs. Hue varies along and across the bands in broad rainbow sweeps, so one moment the panel is mostly cyan-magenta and the next green-yellow. The white crest clipping gives a neon-tube highlight. At high turbulence the bands crumple into tight, chaotic squiggles; at low turbulence they relax into nearly straight parallel diagonal stripes sliding across the panel.

## The idea

Build a smooth "angle" field from a sum of a horizontal and a vertical travelling sine, turn that angle into a direction vector, and project each pixel's position onto its local direction to get a stripe phase. Because the direction varies across the panel, straight stripes become curved flow lines. It is a cheap, convincing fake of a vector-field visualisation.

## Algorithm in words

- State: one time accumulator driven by speed.
- Per frame: for each column precompute a travelling sine of x; for each row a travelling cosine of y (spatial frequency set by the density control).
- Per pixel: angle = column term + row term. Scale the angle by the turbulence control and take its sine and cosine as a direction vector. Dot the pixel's centred position with that vector, multiply up to a fairly high stripe frequency, add a time drift, and take the sine to get a 0..1 intensity.
- Anything below a low threshold is black (hard gaps). Above it, colour = HSV with hue = palette offset + the angle (wrapped) + a bit of the direction's x component, high saturation, value = a slightly boosted intensity. The very top of the intensity range is replaced by pure white.

## Knobs

- K1 Turbulence — how strongly the angle field bends the stripes, from straight sliding bars to chaotic folds.
- K2 Velocity — animation speed.
- K3 Density — spatial frequency of the angle field (how many bends fit on the panel).
- K4 Palette — global hue rotation.
- Audio: none directly; Turbulence is the expressive knob to map sound to.

## Making it better in Luxel

- Make it honest: replace the sine-sum angle field with `curl2` or `simplex3`-driven angles so the folds are organic and non-repeating, and optionally actually advect a few hundred particles along the curl field with `fade` trails — that is a real "vector field flow" and the existing `curl-flow-2d` / `flow-field-2d` library patterns already do half of it (merge candidates).
- Colour in OKLCH with chroma held constant so the bands do not get the HSV dark-blue dip; replace the hard white clip with `setGlow` so crests bloom instead of flattening.
- Cost: the per-pixel body is several trig calls; on a 128x128 chain render it to a quarter-resolution canvas and `fillCanvas` upscale (the bands are smooth so bilinear upscaling is nearly invisible).
- Strips: the 1D slice is just moving coloured stripes — not worth a strip version.

## Payoff

2/5 — a pleasant but generic sine-warp plasma whose name oversells it; near-duplicate of "a big hit".
