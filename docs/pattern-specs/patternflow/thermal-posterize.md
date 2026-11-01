# Thermal Posterize

Idea spec written from Patternflow preset preset_0531.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A slowly flowing plasma rendered as a posterized thermal-camera image: the field is quantized into a few hard-edged bands with fixed "heat" colours — white-hot, orange, hot magenta-pink, deep purple and near-black navy — so the screen is covered in flat, nested contour regions with crisp boundaries that ooze and drift diagonally. Because the band index wraps around, the white-hot band sits directly against the coldest navy, giving a startling hard white-on-black edge in places. A second control swaps any part of the field above a chosen level into an "inverted" scheme — cyan-to-blue tones with the red channel suppressed — so the image can be split between a hot palette and an icy one. At the default setting almost all of the field is in the inverted icy scheme, which hides the thermal look until the knob is turned up. Bold, flat, poster-like.

## The idea

Posterize a smooth plasma into a small fixed "false colour" palette (thermal imaging), with a palette offset and a partial inversion threshold. It works because flat-colour banding of a smooth field reads as a heat map or contour map instantly.

## Algorithm in words

- State: time accumulator from the speed knob.
- Per pixel: average three sines — of x, of y, and of (x + y) at half frequency — each drifting with time, at a spatial scale set by the density knob; normalize to 0..1.
- Add the colour-shift knob and wrap into 0..1.
- Split into five equal bands and assign the fixed colours (hottest at the low end of the wrapped range, coldest at the top).
- If the wrapped value exceeds the inversion threshold: replace the colour with a cool variant — red reduced to a fraction of the old average brightness, green inverted, blue full.

## Knobs

- K1 — colour shift: rotates which band sits where in the field.
- K2 — speed.
- K3 — density: big smooth regions to fine stripes.
- K4 — inversion threshold: low inverts nearly everything, high leaves the thermal palette intact.
- No audio.

## Making it better in Luxel

- Do the posterization as a post stage: render a smooth field (`simplex3` / `perlinFbm`) in grayscale and apply a stepped thermal palette with `setOutputPalette` (hard steps via duplicated stop positions) — then the same "thermal" look can be applied to any pattern, and the pattern itself is tiny.
- Order the palette as a true ironbow (black → purple → red → orange → yellow → white) without the wrap discontinuity, or make the wrap optional; OKLCH-evened bands look better than raw RGB.
- Instead of the inversion trick, offer a palette selector (ironbow, rainbow-thermal, arctic) and a band-count control (from a few to many) — more useful than the threshold.
- For a real "thermal camera" scene: warm moving blobs (`splat` sprites, e.g. figures or hands) over a cool background, blurred, then thermal-palettized — much more evocative than a plasma.
- Strips: works fine in 1D (posterized flowing bands). Pattern form, plus a reusable output palette.

## Payoff

Score: 2/5 — a pleasant posterized plasma; the real value is the "thermal palette over anything" idea, better as a palette/post-process than a pattern.
