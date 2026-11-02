# Kaleido Bit Rain

Idea spec written from Patternflow preset preset_0522.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

The mirrored, warped sibling of Dither Bit Rain. The same 1-bit dithered vocabulary — white pixels on black with ordered-dither mid-tones — is folded into four-fold symmetry around the panel centre, so the screen is a kaleidoscope: everything on the left mirrors the right, top mirrors bottom. Instead of falling, the data streams flow horizontally away from (or toward) the vertical centre line, in rows of differing speed, so the image pulses outward like a techno "warp gate". The mirror axes are themselves domain-warped by sines — the whole picture wobbles and the centre lines bend — with the warp strength on a knob. Accents: where an active stream crosses a mirror axis, a neon sky-blue beam lights up along the centre lines; some nodes of the XOR bit lattice flash neon magenta; and a deep indigo halo shades the region close to the vertical axis. A faint concentric ripple pulses from the centre. Symmetric, hypnotic, very "club visual".

## The idea

Apply kaleidoscopic four-fold mirroring and a sine domain-warp to the dithered bit-rain construction, and colour the symmetry axes as beams. It works because mirror symmetry turns noisy bit textures into face-like, intentional-looking forms, and the centre axis gives a focal point.

## Algorithm in words

- State: time accumulator from the speed knob.
- Per pixel: warp offsets = sine of y and cosine of x, both moving with time, scaled by the warp knob.
- Mirrored coordinates = absolute distance from the panel centre (after warping) on each axis; quantize to the block size from K3.
- Streams: rows are grouped in bands, each with a hashed speed; a flow phase = (mirrored x / a few − time × speed) wrapped to a fixed cycle, "on" for a short part of it → bars flowing along the mirrored x axis.
- Bit field: XOR of the mirrored block coordinates masked by K4; zero → add a mid-level contribution.
- Ripple: sine of the mirrored radius moving with time adds a small modulation.
- Sum, then compare against an ordered-dither threshold. Lit pixels: sky-blue if a stream is on and the block lies on a mirror axis; magenta on a subset of bit-lattice nodes; otherwise white.
- Unlit pixels close to the threshold and near the vertical axis get a very dark indigo fill.

## Knobs

- K1 — warp amplitude: zero gives a crisp rectilinear kaleidoscope, high values a wobbling, melting one.
- K2 — flow speed.
- K3 — block size.
- K4 — bit mask, selecting the XOR lattice structure.
- No audio.

## Making it better in Luxel

- Merge with Dither Bit Rain into one "1-bit glitch" pattern with a symmetry mode control (none / mirror-x / four-fold / eight-fold) rather than two near-identical patterns.
- The symmetry is cheap to exploit: render one quadrant into a small canvas and `blit` it four times (with mirrored indexing), cutting per-pixel work to a quarter on a 128x128 chain.
- Replace sine warp with `simplex3` warp for organic melting; pulse warp and beam brightness on `beat(bpm)` or audio energy for a proper club visual.
- Accent colours as OKLCH controls (a pair), with `setGlow` on the beams only (draw beams in a second pass).
- Strips: mirror-from-centre streams map nicely onto a strip (flow outward from the middle) — a good cheap 1D variant. Main form: 2D pattern.

## Payoff

Score: 3/5 — the more striking of the two bit-rain presets thanks to symmetry; ship it as a mode of a combined pattern, not on its own.
