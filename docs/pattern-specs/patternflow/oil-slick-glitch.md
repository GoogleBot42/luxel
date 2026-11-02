# Oil Slick Glitch

Idea spec written from Patternflow preset preset_0602.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A molten, slowly flowing liquid surface drawn as many tightly packed contour bands, like an oil slick or a soap film. Each colour channel follows the same banding with its own phase offset, so where the offsets are large the bands become iridescent thin-film rainbows — magenta, gold, cyan, green fringes rippling through each other; where the offsets are zero all three channels coincide and the image is a pure greyscale marbling (the default state on the device, since the offset knob starts at zero). Squaring the channels gives deep blacks and punchy highlights. Over this, a glitch: the panel is cut into thin horizontal slices a few pixels tall, and each slice is shoved sideways by its own oscillating amount, with occasional slices stuttering much further — a VHS / data-moshing tear running through the liquid. Hypnotic when calm, aggressive with the glitch up.

## The idea

Iso-contour banding of a smooth flowing field rendered as thin-film interference (per-channel phase offsets), plus horizontal slice displacement for glitch. It works because contour-banded fields with RGB phase offsets look strikingly like real oil-on-water iridescence, and the slicing adds a contemporary glitch edge.

## Algorithm in words

- State: time accumulator from the speed knob.
- Per row: slice index = row divided by a small fixed slice height. Shift = sine of (slice hash + time) × glitch amount; if another, faster per-slice sine is near its peak, the shift is multiplied several times — the stutter.
- Per pixel at the shifted x: fluid = sine(y) × cosine(x) both drifting with time, plus a slanted sine — a smooth, slowly morphing field at a broad scale.
- Contour = sine of (fluid × a moderate multiplier) — many bands.
- Red, green, blue = sine of ((contour + core shift) × a factor), each with the colour-separation offset added (red), none (green), subtracted (blue), mapped to 0..1 and squared.

## Knobs

- K1 — core shift: slides the colour phase of all bands, changing which colours sit where.
- K2 — melt speed.
- K3 — glitch intensity: zero is a smooth liquid; high tears the image into horizontal slices.
- K4 — colour separation: zero is greyscale marbling, higher gives the iridescent thin-film rainbow.
- No audio.

## Making it better in Luxel

- Use a domain-warped `simplex3` (or `curl2` advection) as the fluid so it flows like real liquid rather than a sine grid; thin-film colour can be computed as a proper interference palette (`setPalette` with an iridescence ramp indexed by the contour) instead of three free sines — cheaper (one `paint` per pixel) and more physical.
- Make the default iridescent, not grey; ease the separation slowly over time for a living shimmer.
- Glitch as a scene effect or engine feature: per-row horizontal offset of a layer (a "scripted layer positioning" / row-displacement post stage) would let any pattern be glitched — propose it; within the pattern, trigger stutters on `beat(bpm)` or audio peaks.
- Render the field to a half-resolution canvas and upscale with `fillCanvas` for 128x128 performance; the slices can then be applied by per-row `blit` offsets.
- Strips: a 1D iridescent band flow is lovely on a strip (without the slicing). Pattern form for panels; strip variant worth having.

## Payoff

Score: 4/5 — the iridescent thin-film banding is gorgeous and cheap; split into a calm "oil slick" pattern and a glitch mode.
