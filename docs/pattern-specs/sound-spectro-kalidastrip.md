# sound - spectro kalidastrip
kind: 1D
sensors: yes

## Visual behavior
A music-reactive spectrum display painted along the strip with a kaleidoscope/mirror fold. Rainbow-colored regions of the strip flare up in response to transients in specific frequency ranges (bass at one hue region, treble at another), then decay away over a fraction of a second, leaving short glowing trails. The mapping of frequencies onto the strip slowly slides back and forth, so the whole "spectrum bar" picture drifts and mirrors over a couple of seconds. Strong hits bloom toward white; quiet passages go fully dark. Overall brightness self-regulates: in a loud room it desensitizes, in a quiet room it cranks up, always trying to keep roughly a small fraction (around a fifth) of the strip lit.

## Conceptual inputs
- A 32-band audio frequency spectrum array (from the sensor expansion board), each element the current energy in one band, low frequencies first.

## State kept between frames
- A per-band rolling average array (32 entries) — an exponential moving average of recent (sensitivity-scaled) band energy, with a time window on the order of one to two seconds. Each entry is floored at a tiny positive epsilon so it never reaches zero.
- A per-pixel persistence buffer (length = pixel count) holding each pixel's recent brightness for trail decay.
- An automatic-gain state: a proportional-integral (PI) controller whose integral term accumulates the error between a target lit-fraction (about one fifth of the strip) and the actual total brightness emitted last frame divided by pixel count. The integral term is clamped to a bounded range and starts at a moderate positive value. Sensitivity output = proportional-constant × error + integral-constant × accumulated error (proportional gain a bit larger than the integral gain, both small fractions).
- An accumulator that sums clamped per-pixel brightness during render, read and reset each frame (the feedback signal for the controller).
- A slow sawtooth phase used to scroll the frequency-to-position mapping; one full cycle takes roughly a couple of seconds.

## Per-frame work
1. Compute the new sensitivity from the PI controller using last frame's brightness feedback, then zero the feedback accumulator.
2. Advance the scroll phase.
3. Update each band's rolling average: blend the old average toward (current band energy × sensitivity) by a fraction proportional to elapsed time over the averaging window.

## Per-pixel work
1. Compute a fractional band index in the 0-to-31 range from pixel position via a nested fold: take the pixel's normalized position doubled and folded with a triangle wave, add the scroll phase, and fold that again with a triangle wave, then scale to the band range. The double triangle fold is what produces the mirrored/kaleidoscopic symmetry, and the added phase makes the mirror pattern slide.
2. Sample both the live spectrum and the rolling-average array at that fractional index using linear interpolation between adjacent bands.
3. Brightness = (live energy amplified by a few times, minus the rolling average) — i.e., the transient above the recent norm — multiplied by the sensitivity, and further multiplied by a boost factor that grows strongly with the rolling average of that band (plus a small constant), so bands that are generally active get extra emphasis. Negative results are clipped to zero; positive results are squared for contrast.
4. Blend into the persistence buffer: new stored value = about three quarters of the previous stored value plus the fresh brightness; the stored value is what's displayed. This makes the fast attack / slow decay trails.
5. Add the displayed brightness (clamped to a small maximum) into the feedback accumulator for the gain controller.
6. Color: hue = fractional band index mapped across the full hue wheel, plus a mild extra hue gradient along the strip (about a quarter turn end to end), so it reads as a rainbow keyed to frequency. Saturation = a value that decreases as brightness rises past full (computed as a constant of about two minus the brightness), so overdriven peaks whiten. Brightness is capped at full for output.

## Layout assumptions
1D strip; scales with pixel count. The spectrum is assumed to have exactly 32 bands (hardcoded); if the platform's band count differs, derive the band range from the actual array length.

## UI controls
None — sensitivity, target fill, scroll speed, and averaging window are internal constants.

## Non-obvious techniques
- The PI-controller auto-gain keyed to "fraction of strip lit" makes the pattern work across wildly different volume levels without a sensitivity knob.
- Subtracting each band's rolling average from its live value turns the display into an onset/beat detector per band, rather than a static level meter.
- Squaring the value and letting saturation fall as value climbs gives punchy, white-hot peaks.
- The triangle-of-a-triangle position mapping is the entire "kaleidoscope": it mirrors the spectrum around moving fold points.
