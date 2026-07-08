# Sound & Music Spectrum Visualizer
kind: 1D
sensors: yes — requires the sensor expansion board (audio spectrum input)

## What it looks like
The strip is divided into around ten equal segments, arranged low-frequency-to-high along its
length, and each segment lights up with the loudness of its audio band — a classic spectrum
analyzer laid flat. With music playing, bass thumps light the first segments, vocals and
instruments dance through the middle, cymbals sparkle at the far end. Brightness changes are
punchy but smoothed, so segments flare quickly and decay over a fraction of a second rather
than strobing. Sensitivity self-adjusts within seconds, so it works across quiet and loud
rooms without tweaking. Default coloring is a rainbow gradient along the strip whose hues
slowly drift back and forth over tens of seconds.

## Sensor inputs
- The board's frequency spectrum array (a few dozen bands, low to high). That is the only
  sensor input; no direct energy/beat/accelerometer use.

## Algorithm
Setup (once): choose a segment count (around ten; the author suggests staying within roughly
half a dozen to a dozen) and precompute each segment's start/end pixel from the pixel count —
so strip length is handled generically, no hardcoding. Also compute three breakpoint segment
indices (as fractions of the segment count) that split the strip into bass / lower-mid /
upper-mid / treble EQ regions.

State between frames: the running clock for the spectrum-refresh timer, the reduced band
array, a per-pixel brightness array carried over for smoothing, the sensitivity controller's
accumulated term, and the previous frame's total lit-ness used as feedback.

Per frame:
1. **Auto-sensitivity (PI controller).** Maintain a proportional-integral controller whose
   error is (target average brightness − actual average brightness of the previous frame).
   The target corresponds to roughly a fifth of the strip fully lit. The integral term is
   clamped to a wide range and the output floored at a small minimum, then used as a gain on
   the incoming spectrum. Quiet room → gain climbs; loud room → gain backs off. This settles
   over a few seconds.
2. **Spectrum refresh, rate-limited.** Only every couple hundred milliseconds (several times a
   second — deliberately much slower than the frame rate, to cut flicker): average the raw
   spectrum in adjacent pairs to get half as many reduced bands, multiply by the sensitivity
   gain, then apply four fixed EQ gains by region — bass cut very hard (to a small fraction),
   lower-mids left about unity, upper-mids boosted moderately, treble boosted about double.
   (These are exported so the user can retune for their room.) Finally quantize: anything
   below a threshold snaps to zero; anything above is rounded up and mapped onto a coarse
   ladder of half-steps up to a ceiling several times full brightness. The zero-snap kills
   noise floor glow; the overshooting ladder makes loud hits slam to full and *stay* saturated
   while the smoothing decays.
3. **Per-pixel smoothing.** Every frame (not just on refresh), each pixel's brightness moves
   toward its segment's band value via an exponential moving average weighted heavily (about
   four to one) toward the previous value — this gives the fast-attack-ish, smooth-decay feel.
   The sum of all these pixel values becomes the feedback for the next frame's controller.

Per pixel (render): clamp the stored brightness to the displayable range, pick a hue by mode:
- **Rainbow mode** (default): hue is a triangle function of position spanning a bit more than
  half the hue wheel across the strip, offset by a slow triangle-wave drift with a period of
  tens of seconds, direction-flipped so it reads as a rainbow scrolling gently back and forth.
- **Color-shift mode** (when rainbow is off and this is on): near-monochrome — the same slow
  drifting hue everywhere, with only a very slight positional ripple (a few percent of the
  wheel), giving a slowly recoloring solid look.
- **Fixed color mode** (both off): a single constant hue from the color slider.
Saturation is always full.

## Colors
Fully saturated on black. Rainbow mode covers most of the wheel along the strip. Fixed-color
slider snaps to about five presets rather than being continuous: red at the bottom, then
green, then blue (the default, occupying the middle), a slightly violet-leaning blue, and
pink/magenta at the top.

## Controls
- Slider, "rainbow": acts as an on/off switch (anything above the bottom fifth counts as on).
- Slider, "color shift": also effectively on/off; enables the slow hue-drift monochrome mode
  when rainbow is disabled.
- Slider, "color": picks the fixed hue, snapping to the preset list above.
(EQ gains and band breakpoints are exported variables — tweakable but not sliders.)

## Quirks and non-obvious bits
- Only the lower segments'-worth of the reduced bands are ever displayed (segment count is
  smaller than the reduced band count), so the very top of the spectrum is silently dropped.
  A reimplementation could either match that or map all reduced bands across the segments.
- The treble EQ loop runs one index past the displayed segment range — harmless here (the
  reduced array is longer) but worth not replicating blindly.
- The clever core is the closed loop: quantized-and-overdriven band values feed per-pixel
  smoothing, whose total feeds the PI controller, which rescales the next spectrum read. The
  combination of slow spectrum refresh + fast per-frame smoothing is what makes it look fluid
  instead of flickery.
