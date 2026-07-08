# sound - spectroblots - pow fade
kind: 1D+2D+3D (native renderer is 3D; the 2D and 1D entry points delegate to it with the missing coordinates zeroed)
sensors: yes

## What it looks like
Organic, amoeba-like blobs of color float through the mapped space (strip, matrix, or volume). Each blob region corresponds to one band of the audio spectrum: when that band spikes above its recent average, its blobs flare up bright and near-white, then decay back down with a smooth persistence fade. Quiet regions sit dark. Hues drift slowly through the rainbow, and neighboring blobs carry neighboring hues. The whole blob field slowly drifts and "breathes" — the apparent blob size/complexity swells and shrinks over tens of seconds. The pattern is left/right mirror-symmetric. With music playing, bass hits light one family of blobs, mids and treble light others, giving a spatialized spectrum analyzer that looks like lava-lamp splotches rather than bars.

## Sensor inputs
- A 32-band audio spectrum array supplied by the sensor expansion board.
- An ambient-light variable is read *only* as a board-presence sentinel (it keeps a special negative value when no board is attached; light level is never used visually).
- Fallback: with no board attached, the pattern synthesizes fake spectrum data at around forty updates per second, emulating a four-on-the-floor dance loop at 120 BPM: kick energy splayed across the lowest handful of bands four times per measure, clap-like bursts in the low-mid bands on offbeats, hi-hat ticks in upper-mid bands on beats two and four, and a "lead synth" that excites one wandering band (plus, randomly, a band a few steps higher) whose position meanders smoothly over an eight-measure phrase using a product-of-detuned-triangle-waves pseudo-noise walk.

## Algorithm
State kept between frames: per-band running averages (32 values), per-band current "excitement" values (32), a per-pixel brightness persistence buffer, and the feedback/sensitivity values of an automatic gain controller.

Per frame:
1. Rebuild the coordinate transform: center the unit cube on the origin, then scale all three axes by a factor that oscillates slowly (period on the order of a couple of minutes at default speed) between a small and a moderate zoom — this is the breathing. (Quirk: on one specific networked node ID the scale is pinned to a constant, an ad-hoc multi-device sync hack; safe to omit.)
2. Advance two slow clocks: one drives global hue rotation (tens of seconds per cycle), another very slow one drives a continuous offset applied to the noise field's third axis, making the blobs morph and crawl over minutes.
3. If no sensor board is present, run the sound simulator at its fixed cadence (accumulating elapsed time and firing whenever a fortieth of a second has passed).
4. Automatic gain: a proportional-integral controller nudges a sensitivity value so that the average band excitement seeks a modest target — *however, in this version the controller's output is immediately overridden by a fixed constant sensitivity*, so the PI machinery is vestigial (implement the constant; the controller can be kept inert or dropped). Read-only gauge-style UI outputs expose the feedback level, gain, and current zoom/complexity for monitoring.
5. Per band (all 32): update an exponential running average of the scaled band energy, with a frame-time-compensated smoothing weight (time constant of several seconds). Compute an excitement value proportional to how far the current scaled reading exceeds several times its running average, additionally boosted for bands whose averages are larger (so habitually loud bands still punch through). Blend this half-and-half with the band's previous excitement value (one-frame smoothing) and clamp to a generous positive range (well above unit brightness, allowing overdrive). Sum of excitements feeds back into the AGC.
6. Compute this frame's persistence factor: a base retention fraction (large, ~most of the brightness kept per tenth of a second) raised to a power proportional to elapsed frame time — the "pow fade" of the title, giving frame-rate-independent exponential decay.

Per pixel (3D): take the absolute value of one horizontal coordinate (cheap mirror symmetry). Sample fractal (multi-octave) Perlin-style noise at the transformed coordinates, with the very slow drift offset added on the third axis. Fold the noise through a triangle wave to get a well-distributed value in the unit range. That value, scaled to the band count, selects which of the 32 excitement values this pixel displays — this is the core trick: a smooth noise field partitions space into contiguous blobs, one per spectrum band. The selected excitement is blended into the pixel's persistence buffer (old value times the retention factor plus new value times its complement), and the blended result is used as brightness. Hue is the noise value plus the slow global hue clock — so each blob is a coherent color, adjacent bands are adjacent hues, and everything drifts through the rainbow together. Saturation is a large constant minus the brightness, so overdriven (very loud) pixels desaturate to white while modest ones are richly colored. Brightness is clamped to unit range and squared for gamma.

Layout: fully map-driven, no pixel-count assumptions; the per-pixel persistence buffer is sized to the pixel count. Works unmapped (1D) too, since the 1D entry point receives a normalized position for the first coordinate.

## Colors
Full rainbow, slowly rotating. Each blob holds one coherent hue; loud moments push blob cores from saturated color to near-white; decay returns them through color to black. Background is black.

## Controls
No interactive controls. Three read-only meter-style outputs report the AGC feedback level, the effective gain, and the current field zoom ("complexity").

## Timing feel
Blobs respond to hits essentially instantly and decay over a large fraction of a second. Hue rotation takes tens of seconds per cycle; blob-shape morphing and the zoom breathing evolve over minutes.

## Non-obvious points
- Mapping a folded noise field to *band index* (not to brightness) is what turns a spectrum analyzer into spatial blobs; the triangle-wave fold both keeps the index in range and doubles the spatial variety.
- The exponent-based persistence fade and the exponential-average smoothing weight are both computed from frame delta, so the look is stable across frame rates.
- Excess-over-running-average (rather than raw level) is what makes it beat-reactive and self-calibrating across quiet and loud material, even with the fixed gain override.
- The mirror symmetry via absolute value costs nothing and makes matrix layouts look deliberately composed.
