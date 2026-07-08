# Light Organ - 2.0
kind: 1D
sensors: yes

## Purpose and overall look

A music-visualizing "light organ" for a 1D strip, designed for loud venues (bars, clubs) but usable at home. The strip shows a short repeating group of four solid-colored bars — each bar's brightness pulses with the loudness of one region of the audio spectrum (bass, low, mid, high). The bar group tiles across the whole strip, so the entire strip flashes in synchronized multi-colored segments. On every detected musical beat the bars change width and shuffle which color/band goes where, so the layout visibly "dances" with the song's rhythm. Brightness tracking is aggressive and faithful: quiet passages go dim, hits flash hard. Between songs (or when the room falls back to chatter), the music display shuts off entirely and a very dim, slowly-drifting rainbow shimmer plays as an idle screen until music returns.

This is by far the most elaborate pattern in this batch; most of its complexity is adaptive gain control, not rendering.

## Sensor inputs used

- A 32-band audio spectrum array (magnitudes per frequency bin, streamed from a sensor board).
- A scalar overall sound-energy level.
- A scalar "magnitude of the loudest frequency bin".
(It also configures one analog input pin for battery-voltage sensing, but everything that reads it is commented out — reimplementation can drop it.)

## Per-frame signal pipeline (all in the pre-render step)

1. **Energy cleanup.** The overall energy value is recomputed to exclude the lowest few (bass) bins — bar chatter and rumble live there — then scaled up by an order of magnitude so later divisions stay numerically healthy.

2. **Equal-loudness weighting.** Every spectrum bin is multiplied by a fixed per-bin weight table shaped like the human equal-loudness (Fletcher–Munson / ISO-226-style) curve: bass and extreme treble are attenuated, the vocal midrange is emphasized, with a secondary emphasis bump in the low-treble presence region. Rationale: the ear's sensitivity varies with frequency but the eye's response to color doesn't, so the spectrum is flattened to perceived loudness before display. The whole table is also multiplied by a large scale factor (roughly a thousand) to bring raw values into a workable range.

3. **Top-peak tracking with harmonic rejection.** Considering only bins from just above the bass region up to a high cutoff (the top handful of bins are ignored as containing little fundamental energy), the frame's few largest bins are found. Implementation trick in the original: each bin's amplitude and bin index are packed into a single integer (amplitude in the high bits, index in the low bits) so a single array sort yields the top peaks; the indices are unmasked afterwards. Then the top peaks are filtered: a candidate is rejected if its center frequency is within a modest relative tolerance (~15%) of an integer multiple (2nd through 10th) of an already-accepted peak's frequency — this suppresses harmonics and spectral leakage so the three survivors represent distinct instruments/voices. Duplicates are also rejected. A small lookup table of each bin's center frequency (tens of Hz up to about ten kHz) supports the ratio test. The surviving (up to three) bin indices are sorted ascending.

4. **Band assignment.** The strip's four display channels correspond to four spectral regions: a "bass" region (the lowest few bins, handled separately), then "low", "mid", and "high" regions splitting the remaining considered bins into three contiguous index ranges. Each surviving tracked peak is assigned to whichever region its bin falls in; per region, the strongest assigned peak's magnitude becomes that region's current data point. Bass ignores peak tracking and simply takes the maximum over the raw bass bins. (Note: the original has an apparent bug here — when comparing tracked peaks it indexes the spectrum with a stale loop variable rather than the peak's own bin, so the per-region "strongest" choice is effectively arbitrary among the survivors. A faithful reimplementation can just do the obviously-intended thing: compare each peak's own magnitude.)

5. **Fast peak-hold / decay per band.** Each band keeps a held envelope value: when the new data point exceeds the held value it snaps up instantly (and a short hold timer starts); otherwise the held value decays exponentially on a short timer (tens of ms per step), with a per-band decay multiplier giving each band a time constant of a few hundred ms. This deliberate smoothing exists because raw audio flickers faster than the eye tolerates. A new peak briefly lengthens the hold before decay resumes.

6. **Per-band adaptive thresholds (the heart of the AGC).** On a fast periodic timer (a few dozen ms), the arithmetic mean of each band's bins is folded into a per-band exponential moving average; these averages serve as the "zero brightness" thresholds. Critically, each band's smoothing factor is itself recomputed periodically from the band's dynamic range: it's proportional to the logarithm of (that band's long-held peak divided by its moving average), clamped to a modest range. Loud dynamic music ⇒ faster-moving threshold (punchy response); quiet steady sound ⇒ slow threshold (no jitter). The author calls this the single most important tuning point.

7. **Per-band slow peak reference.** Each band also tracks a slowly-diminishing all-time-recent peak (decaying by a tiny fraction on a fast timer, i.e. a time constant of tens of seconds), snapped up instantly by new maxima. Gain for each band is effectively one over (peak minus threshold).

8. **Normalized pixel drive.** Each band's display magnitude = (held envelope − threshold) / (peak − threshold), clamped to the unit range, **then squared**. The squaring is a perceptual correction (Stevens' power law): the eye and ear have different power-law exponents, and their ratio is about two, so squaring the normalized loudness makes light changes *look* the way volume changes *sound*.

9. **Song-gap detection (auto standby).** Two exponential moving averages of overall energy run in parallel: a short-term one (sub-second to a couple seconds time constant, slower when the display is already off so applause doesn't retrigger it) and a long-term one (tens of seconds rising, faster falling, with a small floor to squelch the silent-room noise floor). When short-term falls well below (about half of) long-term, the pattern declares a gap: the music display is disabled, the long-term average freezes, band peaks are refreshed, and the render layout advances. When short-term climbs back above long-term, music mode resumes and the long-term average is knocked down so it can re-adapt.

10. **Beat detection.** A beat is declared when the squared bass drive jumps to several times its previous frame's value (plus a small absolute margin), with a minimum spacing of a few tens of ms between beats; if none is seen for a few seconds one is forced. Each beat: advances a 4-step beat counter, picks a new random bar width (a few pixels), decrements a marching white-flash position, re-rolls several random hue offsets, and after several dozen beats switches the render layout. 

## Render (per pixel)

Two layout families alternate (switched by beat count accumulation and by song gaps):

- **Layout A (equal bars):** the pixel index is folded modulo four times the current random bar width, giving four consecutive equal-width bars; each bar gets one band's (magnitude, hue) pair. The group tiles across the strip.
- **Layout B (proportional bars):** each band's bar width is proportional to that band's current squared drive (roughly one to eight pixels each, one derived width can vanish entirely), and the pixel index is folded modulo the sum, so louder bands occupy visibly more of each repeating group. In this layout the blue-ish bar's brightness is boosted about four-fold because the eye is less sensitive to blue.

**Colors.** Each beat step assigns the four bands a mix of fixed and randomized hues: bass tends to sit on red or a near-red orange; the other bands get hues derived from a slowly-cycling phase plus fixed offsets (spread roughly a quarter and half of the hue wheel apart) or occasional random rolls; high band often lands on blue (with the brightness boost noted above). Everything is fully saturated. Net effect: bold primary-ish colored bars whose palette reshuffles every beat.

**Overlays:**
- A sparse white-dot overlay: pixels at a marching position (every eighth pixel, position stepping backward on beats) show a dim desaturated flash when the overall loudness envelope is high — a subtle strobe accent riding on the beat grid.
- **Idle mode** (when a song gap is active): the whole music display is replaced by a very dim, slow-moving rainbow ripple — hue is a function of pixel position scaled by a slowly oscillating spatial frequency plus a drifting phase, folded into a half-wheel range; brightness is a triangle wave across position and time raised to a high power and scaled way down (around a percent), giving tiny drifting glints rather than a lit strip.

## State kept between frames

Per-band: held envelope, moving-average threshold, adaptive smoothing factor, slow peak, hold/decay timers. Global: short- and long-term energy averages, gap flag, beat counter and beat timer, layout selector, random bar width and hue rolls, marching white-dot position, several periodic update timers (threshold update, smoothing-factor update, peak-diminish, overall peak-hold), previous-frame bass drive (for beat detection).

## Layout assumptions

Pure 1D. No hardcoded pixel count — everything tiles by modulo, so it works at any strip length. Developed at ~high frame rate (100+ FPS on a short strip); the many delta-driven timers make it frame-rate independent, but the author warns responsiveness degrades at low FPS.

## UI controls

None. All tuning is via constants in the source (the comments include extensive calibration tables for the adaptive-smoothing gains per band at different listening levels). A reimplementation could expose the threshold multiplier and per-band gain constants as sliders, but the original ships fixed.

## Timing feel

Brightness follows the music within tens of milliseconds with a few-hundred-ms decay tail; thresholds breathe over seconds; peaks relax over tens of seconds; standby engages a couple seconds after music stops and the idle shimmer drifts on a multi-second cycle.
