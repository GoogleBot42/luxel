# sound - spectrokalidamandala
kind: 2D
sensors: yes (32-band audio spectrum from a sensor expansion board; graceful fallback to simulated sound when no board is present)

## What it looks like
On a 2D mapped display, concentric ring-like bands of color pulse outward-and-inward from the center in a kaleidoscope/mandala arrangement. Each ring corresponds to a band of the audio spectrum; when a frequency band spikes above its recent average (a kick drum hit, a clap, a synth note), the corresponding rings flash brightly and then fade over several frames, leaving glowing trails. Hue varies both with frequency (which ring) and with angle around the center, so the mandala has angular color symmetry. The whole ring pattern slowly drifts (rings appear to flow radially) on a cycle of several seconds, and independently the entire image slowly "breathes" — zooming between roughly half-size and several-times-size over about a minute. Loud sustained sound doesn't blow the image out: an automatic gain control keeps average brightness near a target, so the pattern stays expressive at any volume. Very intense moments bleach from color toward white.

## Sensor inputs
- A 32-element frequency-spectrum array, updated continuously by the sensor board, low frequencies first.
- An ambient-light variable used **only** as a presence detector: it is initialized to a negative sentinel, and if it never changes, no board is attached and the pattern substitutes simulated audio.

### Simulated-sound fallback
When no board is detected, a synthesizer fills the spectrum array at roughly forty updates per second (using an accumulator on frame deltas), imitating a four-on-the-floor dance loop at a typical dance tempo (~two seconds per measure): kick-drum energy splayed across the lowest ~third of bands with a sharply concave attack four times per measure; clap energy in the low-mid bands on the offbeats with a little randomness in which bands and how much; hi-hat energy in a couple of upper-mid bands on beats two and four; and a "lead synth" that excites one wandering band (plus, randomly about forty percent of the time, a band a few steps higher). The wandering is a smooth pseudo-random walk built by multiplying/averaging a few incommensurate triangle/sine-like waves of relatively prime periods, scaled so the walk is continuous when its time input wraps.

## State kept between frames
- Per-band exponential moving averages of spectrum energy over a window of about a second and a half.
- A per-pixel brightness persistence buffer (one value per pixel).
- The PI controller's accumulated (integral) term.
- A running sum of clamped pixel brightness from the previous frame's render pass, fed back to the gain controller.
- A delta accumulator for the fixed-rate simulated-sound updates.

## Per-frame work
1. **Automatic gain control.** A proportional-integral controller compares last frame's average pixel brightness against a target fill level (roughly a third) and adjusts a sensitivity gain. Error scales the proportional term; the integral term accumulates and is clamped between zero and a large ceiling. The resulting sensitivity has a small floor so the pattern never goes fully dead. The brightness feedback sum is then reset for the coming render pass.
2. A slow phase value advances on a cycle of several seconds (about ten); this drives the radial drift of the rings.
3. If no sensor board, run the simulated-sound update at its fixed rate.
4. **Band averaging.** Each of the 32 bands' moving averages is updated with a weight equal to frame time divided by the averaging window (an exponential moving average), incorporating the current reading times the sensitivity; a tiny floor keeps averages from reaching zero. (Consequence: the pattern needs a second or two after startup to stabilize, plus controller convergence time.)
5. **Breathing zoom.** The coordinate transform is reset, the map is re-centered so the origin is the middle, and a uniform scale is applied that oscillates sinusoidally between roughly one-half and about three-and-a-half over the platform's long default time cycle (~a minute).

A read-only numeric display in the UI shows the current zoom factor. Another exported inspectable value exposes the controller state for tuning (if its integral term pins at the ceiling, the input is too quiet for the gain range).

## Per-pixel rendering (2D)
For each pixel with (transformed) x, y:
- **Ring index.** The distance from center is folded through a triangle wave, the slow drift phase is subtracted, and the result is folded through a triangle wave again, then scaled to a fractional index into the 32 bands. The double folding plus the centered/zoomed coordinates yields mirrored, repeating spectrum copies — the kaleidoscope. (The source lists alternative index mappings, commented out, for a static 1D spectrum analyzer, a center-mirrored version, and a bouncing version.)
- **Band lookup with interpolation.** Both the current spectrum and the averages are sampled at the fractional index by linear interpolation between adjacent bands, so 32 bands render smoothly across many pixels.
- **Novelty value.** Value = (interpolated current energy × sensitivity − interpolated average), multiplied by a weight that grows with the band's typical average energy (roughly "ten plus ten times the average") so habitually strong bands flash harder. Negative results are clipped to zero; positive results are squared as a gamma-like emphasis. This "current minus running average" trick is the heart of the pattern: it highlights *changes* in each band rather than absolute loudness.
- **Hue** = the band fraction (frequency position) plus an angular term: the pixel's angle around the center folded through a triangle wave contributes up to about half the hue wheel. Hue is therefore a mandala: radial position picks base color, angle shifts it symmetrically.
- **Persistence.** The pixel buffer decays by keeping most (roughly two-thirds to three-quarters) of its previous value each frame, then adds the new novelty value scaled by an attack factor of similar magnitude. The decayed-plus-attacked buffer value is the displayed brightness.
- **Whiteout.** Saturation is full at moderate brightness but falls linearly once brightness exceeds unity, so hits bleach to white.
- The clamped brightness is accumulated into the feedback sum for the gain controller.

## Layout assumptions
Requires a 2D pixel map (uses per-pixel x, y in the unit square; the transform re-centers it). The persistence buffer is sized to pixel count, so any count works. No hardcoded pixel counts.

## UI controls
No interactive controls — only read-only numeric displays (current zoom factor; controller internals for tuning). Fade, attack, drift period, target fill, and averaging window are code constants that would make natural sliders.

## Timing feel
Ring drift: several seconds per cycle. Flash decay: a large fraction per frame, so hits fade in a fraction of a second. Band averaging window: about a second and a half. Zoom breathing: about a minute per cycle.

## Non-obvious bits
- Comparing each band to its own recent moving average (then weighting by that average) makes the display react to musical *events* rather than steady-state volume — much livelier than a plain spectrum analyzer.
- The PI-controlled sensitivity plus brightness feedback forms a closed loop across frames: render sums brightness, the next frame's controller nudges gain toward the target fill.
- Fractional interpolation into the band arrays hides the coarseness of only 32 bands.
- The simulated fallback means the pattern demos convincingly with no hardware attached.
