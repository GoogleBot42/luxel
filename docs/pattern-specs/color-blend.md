# Color Blend
kind: 1D
sensors: no

## What it looks like
Three independent smooth waves — one pure red, one pure green, one pure blue — drift along the strip at different speeds and (by default) in different directions. Where the waves overlap, the primaries mix additively into constantly shifting secondary and pastel colors; where all three peak together you get near-white, where all trough you get near-black. The overall impression is a slow, hypnotic, ever-changing gradient wash whose color combinations never quite repeat. At default settings each color wave takes several seconds to drift one full wavelength, and there are on the order of ten wavelengths visible across the strip.

## Algorithm
Per frame, compute three phase values (one per color channel) from the global animation clock, each advancing at its own signed rate. A negative rate makes that channel's wave travel the opposite direction. The phases wrap in the unit interval.

Per pixel:
1. Normalize the pixel position to 0..1 along the strip, then multiply by a "spread" factor. Spread is the number of wavelengths that fit across the whole strip.
2. For each of the three channels, evaluate a smooth periodic wave (sine-shaped, output 0..1) at (scaled position + that channel's phase). This yields that channel's intensity at this pixel.
3. Multiply each channel by 1 or 0 depending on whether its toggle is on.
4. Apply a gamma-like correction to each channel before output. The author approximates a standard display gamma curve by simply squaring the value (a deliberate speed-for-accuracy tradeoff that noticeably improves frame rate). Reimplementers can use squaring or a true power curve.
5. Emit the result as an RGB color.

State between frames: only the three phase values (recomputed from the clock each frame) and the UI-set parameters. No per-pixel state, no randomness. Layout: fully proportional to pixel count, no hardcoding — works on any strip length.

## Colors
Additive mixing of the three display primaries. The palette is emergent: full rainbow of blends from black through saturated primaries/secondaries up to white, depending on wave alignment. Toggling channels off restricts the gamut (e.g. only red+blue gives black-red-magenta-blue blends).

## UI controls
- Slider, "spread": how many wavelengths fit on the strip. The mapping is intentionally reversed so that pushing the slider up makes the waves broader (fewer, wider bands, down to a single wavelength across the strip); slider at the bottom packs in many tens of tight bands.
- Slider, "red speed" (and identical sliders for green and blue): controls that channel's drift speed and direction. Center of travel = stopped; moving toward either end increases speed in that direction. The mapping is reciprocal, not linear: speed stays very slow near the center and ramps up sharply toward the extremes, giving fine control over slow speeds. The implementation must guard the exact-center position against a divide-by-zero and treat it as "stopped".
- Toggle, "red on" (and identical toggles for green and blue): enables/disables that channel entirely.

## Timing
Default speeds give gentle motion: several seconds to tens of seconds per wave cycle, with the three channels deliberately unequal and one running counter to the other two (the author notes this gives the best results). At the fast extreme of a speed slider, a cycle takes on the order of a second.

## Non-obvious notes
- The reversed spread mapping and the reciprocal speed mapping are both purely for slider ergonomics; the render math is a plain "position*spread + phase" wave sum.
- The per-frame phases are explicitly wrapped to the unit interval; the wave function would tolerate larger values anyway, so this is belt-and-suspenders.
