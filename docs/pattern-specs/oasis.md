# Oasis
kind: 1D
sensors: no

Note: this pattern is sometimes catalogued as sound-reactive, but the code contains no sensor input of any kind. It is purely time-driven. (It is inspired by FastLED's "Pacifica" ocean effect.)

## What it looks like
Peaceful ocean water at night: soft aqua/teal light rolling along the strip in overlapping swells. Multiple wave trains move in both directions at different speeds, so crests continually merge and separate. Bright crests wash toward white ("whitecaps") and shift hue slightly; troughs fall to near-black. The overall feel is slow, organic, and non-repeating.

## Algorithm
Four independent wave layers are superimposed. Each layer is described by a small record holding: a base speed, a direction (toward or away from pixel zero), a wavelength divisor, plus per-frame working copies (current phase offset and current wavelength).

Layer configuration (set once, and re-done when the wavelength slider moves):
- The four layers get distinct hand-picked speeds and wavelengths. Two travel one direction, two the other. Speeds span roughly a 2:1–4:1 spread; the slowest layer takes on the order of tens of seconds to traverse the strip, the fastest several seconds. Wavelengths also span a wide spread — one layer has many short waves, another a few long ones.
- Both speed and wavelength are normalized against a reference strip length (about a hundred and fifty pixels): speeds and wave densities are scaled by the actual pixel count so the effect looks the same on any strip length. This is deliberate and should be kept.
- Direction is handled elegantly: each layer stores a function reference — one returns a forward-running sawtooth clock, the other returns its complement (one minus it) — so per frame the phase just calls the layer's own direction function. Reimplementations can use a sign flag instead.

Brightness shaping lookup table (built once): a table of a few hundred entries containing a sine-shaped bump (phase-shifted to start at its trough) raised to the fourth power. This turns each layer's raw sawtooth phase into narrow bright crests with long dark troughs — the key to the watery look. A lookup table is used purely for per-pixel speed.

Per frame:
- Advance each layer's phase from its clock (converted to a pixel-space offset by multiplying by pixel count).
- A slow triangle oscillator (a several-tens-of-seconds full cycle) gently modulates one layer's wavelength by around ten percent either way — a subtle breathing that keeps the pattern from feeling mechanical.
- The same triangle also nudges the base hue by a small amount (a couple percent of the hue wheel).

Per pixel:
- For each of the four layers: take (pixel index + layer phase offset) times the layer's wavelength divisor, divided by pixel count; take the fractional part; look it up in the shaping table. Sum the four results and divide by four. That average is the pixel's brightness.
- Hue: base hue minus a term proportional to brightness (scaled by the "depth" control) — brighter water shifts hue slightly, giving crests a different cast than the deep troughs.
- Saturation: a whitecap threshold minus brightness — so saturation drops as brightness rises, and the brightest crests desaturate toward white. The threshold sits above one so only genuinely bright sums whiten.

## Color
Default palette feel: deep near-black teal troughs, through translucent aqua/sea-green mid-tones, to foam-white crests. The hue slider can move the whole scheme anywhere on the wheel (violet lagoon, golden desert pool, etc.) — the crest-whitening and depth-shading behavior is what defines the look, not the specific hue.

## Controls (all sliders)
- Hue ("aura"): sets the base water color directly.
- Speed: scales all layer speeds together; slider is inverted (higher = faster) and maps across roughly a 4:1 range.
- Whitecaps: how easily crests blow out to white — adjusts the saturation threshold; inverted (higher = whiter crests).
- Depth: how strongly brightness pulls the hue away from base — inverted; at zero the water is a flat single hue.
- Wavelength: scales all wavelengths together across roughly a 10:1 range (inverted: higher slider = longer waves); moving it rebuilds the layer configuration.

## Timing
Individual wave trains traverse the strip in several-to-tens of seconds. The wavelength/hue breathing cycle is slower still. Nothing visibly loops.

## Layout assumptions
1D, index-based. Explicitly scale-independent via the pixel-count normalization described above — no fixes needed.
