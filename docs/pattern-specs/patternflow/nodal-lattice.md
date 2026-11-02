# Nodal Lattice

Idea spec written from Patternflow preset preset_0515_3.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A glowing web of thin, curving lines on a dim coloured background, constantly reshaping. Four plane waves — one along x, one along y, one along each diagonal — run through the panel at different speeds and directions, and light appears only where they nearly cancel each other out. Those cancellation lines (the "nodal lines" of an interference pattern) form a shifting lattice of loops, diamonds and wandering filaments, like light caustics or Chladni-plate figures in motion. Hue slides along a diagonal gradient across the panel and also shifts with the local field strength, so the lines take on rainbow-ish tints while the background between them is a dark wash of the same hue. Mesmerizing when slow, frantic when fast.

## The idea

Sum several travelling sine waves and draw only their near-zero set, with a sharp falloff — an interference / caustic lattice. It works because the zero set of a sum of waves is a rich, continuously morphing curve network from very cheap math.

## Algorithm in words

- State: time accumulator from the speed knob.
- Per pixel: four sines — of x, of y, of (x + y), of (x − y) — each with its own spatial scale (all multiples of the frequency knob) and its own phase speed/direction in time.
- Field = |sum of the four| × a gain that rises with the chaos knob.
- Brightness = (a little over one − field), clamped to 0..1, then cubed for a sharp falloff → thin bright lines where the sum is near zero, quickly fading to dark. A small floor keeps the background faintly lit.
- Hue = base hue + a gentle diagonal gradient across the panel + a shift proportional to the field.
- Saturation high and fixed.

## Knobs

- K1 — base hue.
- K2 — speed; the device mapping lets it run very fast at the top.
- K3 — spatial frequency: low gives a few big curves, high a dense fine mesh (the top of the device range is too fine to resolve).
- K4 — chaos: raises the field gain, making lines thinner and more fragmented.
- No audio.

## Making it better in Luxel

- Anti-alias and tune line thickness with `smoothstep` on the field (or an `exp` falloff) so lines stay a consistent pixel width as the frequency changes.
- Colour the lines with OKLCH at constant lightness and let hue follow the *gradient direction* of the field (which wave dominates) — lines then change colour by orientation, like dichroic glass.
- Use `setGlow`/`setBlur` post-process to bloom the thin lines; it reads much better on LEDs than one-pixel filaments.
- Add a mode where wave directions slowly rotate (or come from radial sources around the panel) for true Chladni/caustic figures; `beatSin` could pulse wave amplitudes to music tempo, or `frequencyData` bands could drive the four wave amplitudes.
- Strips: a 1D version (two or three waves along the strip, show near-zero points) gives travelling sparkle nodes — worth a variant. Main form: one 2D pattern.

## Payoff

Score: 4/5 — cheap, hypnotic interference lattice; a keeper with glow and orientation-based colour.
