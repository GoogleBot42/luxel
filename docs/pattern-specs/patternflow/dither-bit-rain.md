# Dither Bit Rain

Idea spec written from Patternflow preset preset_0521.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A monochrome, 1-bit "computer art" screen: white pixels on black, with all shading done by an ordered (Bayer) dither so mid-tones are fine checker textures. Three things are layered:
- Falling rain: the panel is divided into column stripes, each with its own fall speed; in each stripe a block of "mass" falls down repeatedly, so chunks of dithered white rain down the screen at slightly different rates, like a blocky digital waterfall. The leading chunk of each falling stream is highlighted in hot neon pink — the only colour on screen.
- A fixed XOR bit-pattern: a lattice texture (a Sierpinski / plaid-like pattern from the bitwise XOR of block coordinates under a mask) adds a static, geometric "circuit" substrate whose structure is chosen by a knob.
- A faint concentric ripple from the centre, expanding outward, modulating the dither density so rings pulse through everything.
The whole image can be pixelated into larger blocks. The aesthetic is demoscene / 1-bit Macintosh / glitch zine — stark, crunchy, rhythmic.

## The idea

Combine a falling-streams signal, a bitwise-XOR fractal texture and a radial ripple into a single brightness, then render it through an ordered dither — a 1-bit glitch display with a single accent colour. It works because ordered dither turns any mix of signals into a cohesive retro texture.

## Algorithm in words

- State: time accumulator from the speed knob.
- Pixel size (block quantization) from K3; all coordinates are snapped to blocks.
- Optional tears: certain rows are shifted horizontally by a tangent-shaped amount — meant to produce horizontal cracks. (In the device version the trigger is effectively never met, so no tears appear.)
- Rain: group columns into fixed-width stripes; each stripe gets a pseudo-random speed from a hash of its index. A falling phase = (block row / a few − time × that speed) wrapped to a cycle of a fixed number of steps; "mass" is on for the first part of the cycle — a falling bar. Its first step is the stream's head.
- Bit field: XOR the block column and row indices and AND with a mask from K4; if the result is zero, add a mid-level contribution.
- Ripple: a sine of the distance from the panel centre moving outward over time adds a small ± modulation.
- Total signal = rain weight + bit field + ripple.
- Compare to the threshold of a small ordered (Bayer) dither matrix at the pixel: above → lit. Lit pixels are white, except stream heads, which are neon pink.

## Knobs

- K1 — tear severity (dead in the device version).
- K2 — cascade speed.
- K3 — block size: crisp single pixels to chunky macro-pixels.
- K4 — bit mask: selects which XOR lattice appears — from none, through fine checker, to large nested Sierpinski-like structures.
- No audio.

## Making it better in Luxel

- Make the tear work and tie it to a beat (`beat(bpm)` or audio `energyAverage`): rows snap sideways on hits, then ease back with `easeOutExpo` — the glitch becomes musical.
- Let streams be actual particles (a small array of heads with speeds and lengths) drawn in `renderBulk` with `fillRect` per chunk, rather than evaluating a modular phase at every pixel; dither the background once per frame via a small canvas.
- Offer an accent-colour control (OKLCH hue) and an optional two-colour mode (e.g. phosphor green + amber); allow per-stream accent hues.
- Keep the Bayer dither as the identity — a reusable "dither output" post stage (ordered-dither quantization of any pattern to 1-bit + accent) would be a good engine proposal.
- Strips: a 1D version (dithered falling chunks along the strip) is fine but loses the XOR lattice. Main form: 2D pattern; overlaps with the existing Matrix-rain idea, so pitch it as the "1-bit dither" variant.

## Payoff

Score: 3/5 — strong retro identity, but busy and partly broken; worth one good 1-bit glitch pattern combining this and its mirrored sibling.
