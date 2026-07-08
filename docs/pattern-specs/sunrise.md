# Sunrise
kind: 1D
sensors: no audio/motion sensors; reads one GPIO digital input pin as a mode
switch

## What it looks like
A one-shot wake-up-light sunrise that plays once when the pattern starts and
then holds. From black, a warm sunrise-orange glow appears at the middle of the
strip and spreads outward like a rising sun until the whole strip is evenly
orange (first half of the show). Then, again starting from the middle and
spreading outward, the orange desaturates to pure white until the entire strip
is full-brightness white (second half). It stays white indefinitely until the
pattern is restarted. There is also an alternative mode, selected by a hardware
switch, that just shows a constant dim ambient orange.

## Algorithm
### Timebase and latching
The pattern derives everything from the engine's free-running sawtooth clock
configured for the total rise duration. Because that clock is NOT reset when a
pattern loads, the code samples its value once at startup and thereafter uses
the difference from that initial sample (wrapped into the unit range) as its
own zero-based progress value. Total nominal duration is about ten minutes of
real time, but a configurable speed-up divisor (default around a hundred)
compresses that to several seconds for testing; setting the divisor to one
gives the real-time sunrise.

Two phase progress values are maintained, each latched with a running maximum
so they can only increase (this is what makes it one-shot — when the underlying
sawtooth wraps, the maxima hold and the show never replays):
- Phase-one progress: the overall progress doubled and clamped to the unit
  range (so it completes during the first half).
- Phase-two progress: the overall progress doubled, shifted down by one, and
  clamped (so it runs during the second half, starting exactly when phase one
  finishes, and sticks at full when done).

### Per pixel
Compute a spatial dome: a triangle function of the pixel's normalized position,
peaking at the strip's midpoint and falling to zero at both ends.

- While phase two has not begun: brightness is the dome, offset downward by a
  full unit and raised by twice the phase-one progress, clamped to the unit
  range. At the start only the very tip (strip center) is lit; the lit region
  grows outward and brightens until, at the end of phase one, the offset has
  risen enough that every pixel is fully bright regardless of the dome. Color:
  fixed sunrise-orange hue, full saturation.
- Once phase two is running: the same dome-plus-rising-offset construction is
  recomputed with the phase-two progress, but now it drives desaturation
  instead of brightness: brightness is pinned at full, and saturation is one
  minus that value. So whiteness spreads from the center outward until the
  whole strip is pure white, then holds.

### Alternative mode
A digital input pin is configured as an input. Each pixel render checks a
config flag and the pin: if the flag enables alt mode and the pin reads low,
the whole strip instead shows the sunrise-orange hue at a constant modest
brightness (an "ambient nightlight" mode for non-sunrise hours). By default the
config flag disables alt mode entirely, so the pin is ignored.

## Colors
A single warm orange hue (classic sunrise orange, just off red toward amber),
transitioning to pure white in phase two. Nothing else.

## Controls
No UI sliders or pickers. Configuration is via exported constants at the top of
the source (speed-up divisor, total rise time, hue, alt-mode brightness,
alt-mode enable flag) plus the physical GPIO switch. An implementer may
reasonably promote these to sliders/toggles, but the original has none.

## Timing feel
Nominally a ten-minute sunrise (five minutes of orange growth, five of fade to
white) when run in real time; a few seconds total at the default test speed-up.

## Notes for the implementer
- The essential trick is one spatial triangle wave reused twice: once as a
  brightness ramp (rising sun) and once as a desaturation ramp (fade to
  white), each with a rising vertical offset large enough to eventually swamp
  the spatial variation.
- The startup-phase capture and the running-maximum latches are load-bearing:
  without them the effect would start mid-way through and would loop.
- Written against a real device with a specific pixel count and RGBW LEDs, but
  the logic itself is fully normalized; no changes needed for other strip
  lengths.
