# Gradient blue  purple pink
kind: 1D
sensors: no

## Visual behavior
A smooth, full-strip color gradient in the cool blue → purple → pink/magenta range that continuously flows along the strip. The hue banding drifts by fairly quickly (a few seconds for a full pass), while an independent, slower ripple of saturation (several-fold slower, on the order of ten-plus seconds per cycle) makes regions alternately go pastel and richly saturated. The two motions beat against each other, so the look never exactly repeats on a short timescale. Everything is at full brightness; the effect is soft and ambient, like slowly sliding aurora bands.

## Algorithm
Stateless — everything is computed per pixel from global clocks; there is no pre-render step and no retained state.

Per pixel:
- Compute two triangle waves, each phased by the pixel's fractional position along the strip plus a global clock. Using fractional position means exactly one spatial cycle of each wave spans the strip, at any pixel count (no hardcoding).
- The first (faster) wave drives hue. Its output is compressed to about half its range and offset so the hue never leaves a band roughly a quarter of the color wheel wide, centered on the blue-violet region: the low end is a true blue, the high end a pink/magenta, with purple in between. The wheel's warm hues are never reached.
- The second (slower) wave drives saturation, similarly compressed and offset upward so saturation swings between "fairly saturated" and fully saturated (the upper part of its range would exceed full saturation and is effectively pinned there, so there are plateaus of pure color).
- Brightness is constant at full. (The code squares the brightness, but since it is one this is a no-op — ignore it.)

Both waves scroll in the same direction along the strip. No randomness.

## Colors
Continuous sweep from clear blue through violet/purple into pink-magenta and back, with saturation independently breathing between rich and slightly pastel. Never desaturates to white and never dims.

## Timing
Hue band: a few seconds per full traverse. Saturation ripple: several times slower.

## Controls
None.
