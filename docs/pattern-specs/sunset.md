# Sunset
kind: 1D (a 1D renderer that internally maps the strip onto a 2D matrix; no mapper used)
sensors: no

Catalogued as sound/sensor-reactive but the source uses **no sensor input** — it is a pure time-driven ambient plasma. The author's own comment says it's a tweak of a classic 2D honeycomb/plasma pattern, tuned for indirect wall lighting.

## What it looks like

A soft, slowly-breathing plasma of warm blended color washes drifting across a LED matrix — think ambient mood lighting, not an effect that draws attention. Blobs of brightness swell and shrink over many seconds; the palette at any instant spans only a limited wedge of the color wheel (deliberately, so the bounced ambient light stays colored instead of averaging to white), and that wedge itself slowly rotates through the whole rainbow over tens of seconds. Despite the name there is no literal sunset gradient; it earns the name by its mellow, warm, ever-shifting glow.

## Layout assumptions

The pattern assumes the strip is physically arranged as a serpentine (zigzag) matrix and does its own index→(column,row) math: row = index divided by a **hardcoded width** (a few dozen columns), column = remainder, with every other row reversed when a source-level boolean says the wiring zigzags. Width and zigzag are source constants, not UI controls. Obvious fix: use the engine's 2D coordinate mapping (or expose width/zigzag as controls). The author notes that "wrong" widths produce interestingly sheared results, so the width is as much an artistic knob as a wiring fact.

## Algorithm

Per frame, compute a handful of slow oscillators from sawtooth time sources of different multi-second-to-minute periods (a global speed multiplier in the source slows everything about five-fold from the pattern it was derived from):
- two phase angles (smoothed waves scaled to a full circle) with slightly different periods, so they beat against each other;
- a spatial zoom factor that swings between a couple and several waves-per-matrix over the slowest period;
- a slow hue-drift value and an independent slow brightness-phase value.

Per pixel:
1. Compute a scalar plasma field: a constant plus the sine of (normalized column × zoom + first phase) plus the cosine of (normalized row × zoom + second phase), scaled to roughly the unit range. This is the classic two-axis interference plasma.
2. Brightness: take a smooth periodic wave of (field + brightness phase), then **cube it**. Cubing crushes the midtones so most of the matrix sits dim with soft bright islands — this is what keeps it calm.
3. Hue: fold the field through a triangle wave (halved, so hue varies over half as much as the field), add the slow hue drift, then add the brightness value, and divide the sum by a contrast divisor of about two. The divisor compresses how much of the color wheel is visible at once — the author's comment: small divisor ≈ monochrome, large ≈ so many hues the ambient blend goes white; about two is the sweet spot. Finally subtract a steadily advancing slow time ramp so the entire palette rotates continuously through the wheel over tens of seconds.
4. Output fully saturated at a fixed overall brightness scale (a source constant, defaulting to full).

Adding the brightness value into the hue is the clever bit: bright cores get pushed to a slightly different hue than their dim surroundings, giving two-tone blobs (bright center of one color fringed by a neighboring color) instead of flat spots.

## State kept between frames

None beyond the frame's oscillator values — the pattern is a pure function of time and pixel position.

## Colors

Ever-rotating: at any moment roughly a two-tone wash of adjacent hues (e.g. amber cores over rose, later teal over blue), fully saturated, cycling through the entire rainbow over tens of seconds.

## UI controls

None. Tunables (matrix width, zigzag wiring, master brightness, hue-contrast divisor, overall speed) are constants in the source; exposing width/zigzag/speed/contrast as controls is the obvious enhancement.

## Timing feel

Very slow: individual blobs morph over several seconds to tens of seconds; the zoom breathes over about half a minute; a full palette rotation takes on the order of tens of seconds. Nothing moves fast enough to catch the eye.
