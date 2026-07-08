# b_lightning_flashes
kind: 1D
sensors: no

## What it looks like
The strip is almost entirely dark. At random moments a short segment of the strip flashes on like a lightning strike: it swells up to full brightness, crackles/flickers erratically at its peak, then dies away, followed by a dark pause of random length before the next strike appears somewhere else. Each strike is a bright bluish-white streak — nearly white at its dim fringes, tinted cold blue in its bright core — that fades off toward both ends of the segment. The whole on/off cycle feels sub-second for the flash itself, with dark gaps ranging from almost nothing up to a couple of seconds depending on settings.

## Algorithm
State kept between frames:
- An accumulated milliseconds timer for the current strike.
- The current strike's parameters: center pixel index, hue, flash duration, and following dark-gap duration.
- A boolean "flicker" flag that gets toggled chaotically.
- A global brightness envelope value computed each frame.

Per-frame (before rendering):
1. Advance the timer by the frame delta.
2. Compute a brightness envelope as a triangle wave over the flash duration: rises linearly from zero to full at the halfway point, then falls back to zero.
3. Chaotic flicker: with a small per-frame probability (derived by comparing a random draw against the reciprocal of the frame time — i.e. deliberately frame-rate-entangled and erratic), toggle the flicker boolean. Whenever the envelope is in roughly its top two-fifths of intensity and the flag is set, knock the envelope down by about half. This produces the crackling look only near peak brightness.
4. Square the envelope (gamma-style shaping).
5. Once the timer passes the flash duration, force the envelope to zero (dark phase). Once it also passes the dark-gap duration, start a new strike: pick a fresh hue (cold blue with a small random jitter to either side, so strikes vary subtly between blue-violet and blue-cyan), pick a new random center pixel (constrained so the whole segment fits on the strip, i.e. the center is kept at least a half-segment-width away from both ends), roll a new flash duration (a base of a couple hundred milliseconds plus a random extra, divided by the speed setting), roll a new dark gap (random up to a user-set maximum, multiplied by the speed setting), and reset the timer.

Per-pixel (render):
- If the pixel lies within the segment (center ± half-width), compute its distance from the center and scale the envelope linearly down to zero at the segment edge.
- Brightness is that interpolated value, roughly doubled (so mid-envelope values already clip to full brightness — the strike saturates quickly).
- Saturation is binary: pixels above roughly two-thirds interpolated intensity get a strongly-but-not-fully saturated blue; everything dimmer renders as pure white. So the core reads blue and the fading fringes read white — an inverted-from-usual choice that reads well for lightning.
- Pixels outside the segment are black.

Layout assumptions: pure 1D by pixel index. The segment half-width control is capped at an absolute pixel count (a few dozen pixels), which is effectively hardcoded for typical strip lengths; the obvious fix is to express the maximum half-width as a fraction of the total pixel count instead.

## Colors
Single-hue effect: cold blue, randomly jittered slightly per strike. Bright core = saturated blue; fringes = white; background = black.

## Controls
- Slider, "lightning length": sets the segment half-width, from a single pixel up to a few dozen pixels.
- Slider, "speed": raising it makes each flash shorter/snappier but simultaneously stretches the dark gaps proportionally (the two are inversely/directly coupled to the same knob).
- Slider, "off-duration randomness": sets the maximum possible dark gap between strikes, from a fraction of a second up to a couple of seconds.

## Notes / quirks
- The flicker toggle probability depends on the instantaneous frame time, so the crackle character varies with frame rate; a reimplementation could use a fixed small per-frame toggle probability or a time-based one.
- The source file contains its entire body pasted twice (an authoring accident); the duplicate is functionally inert — implement it once.
