# SOUND - lavablob
kind: 1D+2D+3D (a single 3D renderer; the 1D and 2D entry points just call it
with the missing coordinates as zero — 1D uses the normalized pixel index as x)
sensors: yes

## What it looks like
A sound-reactive lava-lamp effect, best on a matrix. Soft organic blobs of
color drift and undulate across the surface; in silence the display goes dark,
and with music the blobs flare up, pulsing in brightness with the audio. Blob
size breathes over a period of tens of seconds and also swells with overall
loudness. Colors sweep slowly through the spectrum with a gentle diagonal
gradient across the surface; dim fringe areas wash toward
white/desaturated while bright blob cores are fully saturated. The author
admits this was made by mangling an existing fire-style pattern and contains
known bugs — reimplement the observed behavior, quirks included, but see notes.

## Sensor inputs
- A frequency-spectrum array (the standard sensor-board spectrum, a few dozen
  bands). Only one band from the middle of the array is actually used per
  pixel, as a brightness gate: no energy in that mid band means the display is
  essentially off, and the pattern pulses with that band.
- An overall sound-energy scalar, which scales the spatial frequency of the
  blob field: louder sound stretches/multiplies the blob texture.

## Algorithm
State: three time phases recomputed each frame, plus a spatial scale factor.

Per frame:
- Three sawtooth time phases with slightly different, mutually incommensurate
  periods (so their combination never visibly repeats).
- A spatial scale = (a tiny floor plus a triangle wave with a period of tens
  of seconds) multiplied by the overall sound-energy scalar and a large gain.
  Silence collapses the scale toward zero (blobs become one huge flat field);
  loud sound raises it (finer, busier texture).
- **Bug preserved from the source**: the periods of the three time phases are
  divided by a "speed" value computed once at startup from the spectrum array
  itself used as if it were a number, times large constants. Depending on how
  the language coerces an array to a number this is effectively either zero or
  garbage; in practice on the original platform it makes the three phases run
  extremely fast or degenerate. A faithful reimplementation should compute
  this speed once at startup the same coerced way; a sane fix is to treat
  speed as a constant of about unity (giving phase periods of several seconds)
  or drive it from live sound energy.

Per pixel:
- Hue = the first time phase plus a small fraction (about a fifth) of each of
  x, y, z summed — a shallow diagonal rainbow gradient that drifts over time.
- A raw intensity is formed as the product of three waves and then amplified
  about tenfold:
  - a triangle wave of (y times the spatial scale, offset by a sine-shaped
    wave of the first time phase),
  - a sine-shaped wave of (y times the spatial scale, offset by a wave of the
    second time phase),
  - a sine-shaped wave of (x times the spatial scale, offset by a wave of the
    third time phase).
  The product of interfering waves in x and y is what makes blob-shaped
  bright regions.
- Saturation = that amplified intensity minus one, clamped by the color
  conversion: regions of low intensity get zero-ish (white/desaturated but
  dim), regions above the threshold saturate fully. (An earlier cube-of-hue
  saturation assignment in the source is immediately overwritten and has no
  effect — do not implement it.)
- Final brightness = the chosen mid-spectrum band value, heavily amplified
  (several-fold), times the square of the raw intensity — squaring sharpens
  blob cores, and the audio band multiplies the whole field so brightness
  pulses with the music.

No randomness; all motion comes from the time phases and audio input.
No layout assumptions beyond preferring a 2D map; works degenerately on
strips (x-only variation).

## Colors
Slowly cycling full-spectrum hues with a soft diagonal gradient. Blob cores:
saturated, vivid. Fringes: pale/whitish and dim. Off when the driving audio
band is silent.

## Controls
None.

## Non-obvious bits
- Brightness can exceed the nominal maximum by a wide margin before clamping;
  the deliberate over-amplification produces hard-edged, blown-out blob cores.
- Coupling saturation to (intensity minus one) is what gives the molten
  white-hot rim look without a palette.
- If the startup speed bug is "fixed" to a sensible constant, expect visibly
  slower, smoother phase drift than the original.
