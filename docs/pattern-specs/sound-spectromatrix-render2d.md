# sound - spectromatrix render2D
kind: 2D (with a 1D fallback that fakes a small square matrix)
sensors: yes

## What it looks like
A dark field over which colored blobs flare up wherever the music has energy. The
blobs are arranged along wavy, plasma-like contour bands that slowly drift and fold
across the matrix. Loud transients in a given frequency region make the band(s)
associated with that region flash bright and whiten at the peak; the flash then decays
over a fraction of a second, leaving short-lived glowing trails. Hue slowly cycles
through the rainbow over several seconds, and quiet passages leave the display nearly
black. The pattern self-adjusts gain, so after a few seconds it looks similarly active
for quiet or loud rooms.

## Conceptual sensor inputs
- A spectrum analysis array of about thirty-two frequency bands (low frequencies at
  one end, high at the other), each a magnitude that updates every frame.

## Algorithm
State kept between frames:
- A per-band rolling average of the (gain-scaled) spectrum, smoothed over roughly a
  second and a half using an exponential moving average whose blend factor is the
  frame time divided by the averaging window.
- A per-pixel brightness persistence buffer (one slot per pixel).
- An automatic-gain state: a proportional–integral controller. The setpoint is that
  the average lit fraction of the display should be small (several percent of full
  brightness across all pixels). Each frame it compares that target against the total
  brightness actually emitted last frame (accumulated during rendering, divided by the
  pixel count) and nudges a global sensitivity multiplier up or down. The integral
  term is clamped to a wide range so it can wind up enough to amplify very quiet
  signals. This is what makes the pattern volume-independent.

Per frame:
- Sample two free-running sawtooth phases from the global clock: a slower one (a
  dozen-ish seconds per cycle) and a somewhat faster one; these drive the spatial
  drift and the band-scan drift respectively.
- Update the gain from the PI controller, reset the brightness accumulator, and fold
  the new spectrum frame into the per-band rolling averages (floored at a tiny
  positive value so later ratio math never divides by nothing).

Per pixel:
1. Compute a "band index" in the range zero to the top band from the pixel's
   position: take a sine-shaped wave of x offset by a moving phase, plus a
   sine-shaped wave of y offset by the same phase moving the other way, average them,
   add the faster time phase, and fold the result through a triangle wave. This maps
   the 2D plane into smoothly curving, drifting iso-bands, each band corresponding to
   one region of the spectrum. The index is fractional; spectrum and average lookups
   linearly interpolate between adjacent bands.
2. Instantaneous brightness is (current gain-scaled band energy minus that band's
   rolling average), i.e. only above-average energy shows, so it responds to beats
   and transients rather than sustained tones. This difference is scaled up by an
   amount that grows with the band's own average level (quiet bands get boosted so
   highs aren't drowned out by bass), then squared (negative values clamp to zero),
   which gives punchy contrast.
3. The new brightness is blended into the per-pixel persistence buffer: previous
   value times a decay factor a bit under one, plus the new value. The buffer value is
   what's displayed, producing the decay trails.
4. Hue is the band index scaled down (so the whole spectrum spans roughly half the
   hue wheel) plus the slow time phase, giving a slow overall color rotation.
   Saturation is one minus brightness, so the hottest peaks blow out toward white.
5. The displayed brightness (clamped to at most full) is added to the feedback
   accumulator used by the auto-gain next frame.

## Colors
Fully cycling rainbow hues distributed along the drifting bands; peaks desaturate to
near-white; background is black.

## Controls
None exposed as sliders. There are a couple of top-of-file constants (a matrix width
and a zigzag flag) that appear vestigial — the actual fallback mapper hardcodes its
own size.

## Layout assumptions and the obvious fix
Designed for a 2D pixel map. A fallback plain-1D renderer synthesizes coordinates for
a small square matrix (eight by eight) with serpentine/zigzag wiring, with the zigzag
flip on a single commented-out-able line. That size is hardcoded; the obvious fix is
to derive width/height from the actual pixel count or the top-of-file constants
instead of a fixed square.

## Timing feel
Band drift: gentle, order of ten seconds per full spatial cycle. Flash decay: a few
tenths of a second. Hue rotation: order of ten seconds. Auto-gain settles over a few
seconds.

## Clever bits
- The PI-controller auto-gain closed loop on *emitted brightness* (not input level)
  keeps the visual density constant regardless of volume.
- Subtracting a per-band rolling average turns the display into a transient detector.
- Boosting each band by its own average level acts as a crude per-band equalizer.
