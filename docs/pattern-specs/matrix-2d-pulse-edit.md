# matrix 2D pulse edit
kind: 1D (index-based renderer; intended to be shown on a small LED matrix, but it derives its own pseudo-coordinates from the pixel index rather than using a mapped 2D renderer)
sensors: no

## What it looks like
A dim, moody "twilight" plasma. Most of the display sits near black; soft glowing patches of color swell up, drift, and fade back into darkness. Because of how the coordinates are derived (see quirks), the glow tends to organize into short repeating cells/columns whose width visibly breathes over time — the texture periodically tightens into fine stripes and relaxes into broader blobs over a cycle lasting on the order of ten-plus seconds. Hues slide gradually through the color wheel but are folded so the very top of the hue range is avoided; the overall feel is shifting dusk colors rather than full rainbow.

## Algorithm
- No per-frame state carried over other than three clock phases computed each frame:
  - a fast phase (full-circle angle, cycling in a few seconds),
  - a slower phase (full-circle angle, cycling in roughly twice that),
  - a "zoom" factor that oscillates sinusoidally between about one and several, with a period of ten-plus seconds. (A fourth clock is computed but never used — omit it.)
- Per pixel, two pseudo-coordinates are derived from the linear index:
  - a "row" value: the index divided by a small constant (three) and floored — note this does NOT match the declared matrix width,
  - a "column" value: the index modulo the current (fractional, time-varying) zoom factor — this is what makes cell width breathe and produces the glitchy column texture.
- An optional serpentine-wiring flag, when enabled, mirrors the column value on alternate rows (using the declared matrix width, roughly eight).
- The hue field is a classic two-term plasma: a small positive bias plus the sine of (column scaled by zoom over matrix width, offset by the fast phase) plus the cosine of (row scaled by zoom over matrix width, offset by the slow phase), the sum halved. Result spans a bit beyond the 0..1 range in both directions.
- Brightness is derived from that same raw hue value: cube it (preserving sign), then divide down substantially (to roughly a third). Cubing makes the field mostly dark with sharp bright crests; negative values clamp to black, so only the positive half of the plasma glows.
- After brightness is taken, the hue is wrap-limited: values above roughly three quarters are folded back by taking them modulo a value a bit under two thirds. This keeps displayed hues out of one sector of the wheel and creates occasional abrupt hue jumps at the fold.
- No randomness anywhere; fully deterministic and periodic-ish.

## Colors
Continuously drifting hues at full saturation, folded to avoid part of the color wheel; overall dim (peak brightness intentionally well below full). Reads as deep twilight tones with glowing highlights emerging from black.

## Controls
None exported. Two internal constants exist: the matrix width (small, around eight) and a boolean for serpentine vs. straight wiring.

## Timing
Two independent drift phases (a few seconds and roughly double that) plus the slow zoom breathing (ten-plus seconds). The composite never exactly repeats quickly; it feels like slow lava-lamp motion with a periodic tightening of texture.

## Notes / quirks (important for faithful reproduction)
This pattern is "charmingly broken" and its look depends on the bugs:
- The row divisor (three) disagrees with the declared matrix width (eight), so on a real matrix the pattern does not form a coherent 2D plasma — it forms diagonal/striped moiré.
- Taking the index modulo a *fractional, time-varying* number for the column is the source of the breathing cell-width glitch; do not "fix" this into integer matrix coordinates or the character changes completely.
- Reusing the pre-fold hue as the brightness source couples color and intensity: certain hues are always bright, others always dark.
If a cleaned-up version is ever wanted, the obvious fixes are: derive row/column from the actual matrix width, and drive brightness from its own field — but that is a different pattern.
