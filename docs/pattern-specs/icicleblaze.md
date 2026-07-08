# Icicleblaze
kind: 1D+2D
sensors: no

## Overview
Simulates falling "icicles" / shooting-star drips. With a 2D map, a single icicle at a
time sweeps down the vertical axis across the whole installation (every column shows
it at the same height, regardless of x). Without a map, the 1D renderer simply treats
strip position as height, so a bright segment with a fading tail shoots along the strip.
Between icicles the display goes completely dark for a random pause. Each new icicle
rolls fresh random parameters — speed, length, tail style, sparkle, and color scheme —
so the effect stays varied: sometimes classic cold white drips, sometimes warm white,
deep blue, blue-and-white shimmer, or Christmas candy-cane stripes.

## Timing feel
- A fall nominally takes a few seconds; each icicle's actual duration is randomized
  between one and two times that nominal value.
- After an icicle finishes, the pattern waits a random time (from zero up to several
  seconds) before launching the next one.
- Overall cadence: one drip every few-to-several seconds, each lasting a few seconds.

## State kept between frames
- A master progress value in [0, 1) driving the current icicle's fall. It is derived
  from a global sawtooth timer whose period equals the icicle's chosen duration, minus
  a captured phase offset (taken when the icicle was created) so every icicle's
  progress starts at zero regardless of when it begins. Wrap-around of this progress
  (new value less than previous frame's value) signals the icicle has completed.
- A "waiting" flag plus an elapsed-milliseconds accumulator and a randomly chosen
  wait length, used for the dark gap between icicles.
- Per-icicle randomized parameters (rolled once when each icicle spawns):
  - duration (nominal to twice nominal),
  - icicle length as a fraction of the height (nominal about a fifth of the height,
    randomized up to twice that),
  - whether sparkle is on (roughly a 1-in-5 chance),
  - whether the intermittent "wave tail" is on (well over half the time), and if so
    which of four tail styles,
  - which color scheme to use (uniform random among the enabled schemes),
  - two random values in [0,1) that the color schemes use to vary hue/brightness.
- The pattern initializes one icicle at startup so it does not begin dark.

## Per-pixel rendering (2D form; 1D delegates)
The 1D renderer just calls the 2D renderer with both x and y set to the pixel's
fractional position along the strip.

For each pixel, compute a signed "position within the icicle":
  pos = y + length − progress × (1 + length)
so the icicle enters from above the top and exits below the bottom. The head of the
icicle is where pos equals the icicle length; the tail end is where pos is zero.
A pixel is lit only when pos is between zero and the icicle length; everything else is
black (brightness multiplied by that in-icicle boolean). Skipping the heavier math for
out-of-icicle pixels is an intentional performance win.

Inside the icicle, starting from full brightness:
- **Wave tail (if enabled):** a spatial sinusoid over height (many cycles across the
  unit height) is phase-shifted by the fall progress times a per-icicle "tail period"
  coefficient. Brightness is reduced more strongly toward the tail: it fades linearly
  from head to tail, and additionally the wave value is subtracted scaled by the
  square of the fraction-toward-tail, clamped at zero. Net effect: a solid bright head
  that breaks up into moving bright/dark bands along the tail. The four tail-period
  choices give distinct looks:
  1. zero — the bands are fixed in space, so the icicle appears to slide through a
     stationary striped mask;
  2. a coefficient matching the spatial wave frequency — the bands ride along with
     the icicle;
  3. slightly less than that — a slowly drifting interference ("mach diamond") look;
  4. a much larger coefficient — bands cycle so fast they read as shimmer.
- **Sparkle (if enabled):** multiply brightness by a random value that is biased
  brighter near the head — draw a uniform random number scaled by the pixel's
  fraction of the way toward the head, then raise it to a power below one (around
  two-thirds) to lift the expected value. Gives a subtle glittering shimmer that is
  most intense at the head.

## Color schemes
Six schemes, chosen randomly per icicle. A config value limits which are eligible;
the lower-indexed schemes are calmer/classic, the higher ones more colorful and
frenetic (so setting the limit low keeps it tasteful). Two per-icicle random values
add variation within a scheme. Qualitatively:
1. Plain cool white (zero saturation).
2. Cool white that dims as it descends — brightness falls off with height twice over
   (fading quickly through the upper half, never fully to zero until near the bottom).
3. Warm whites: a hue in the warm orange-ish band (randomly picked within a narrow
   range), strongly but not fully saturated, with a random overall brightness.
4. Deep blues: hue randomly picked within the blue band, fully saturated, quite dim
   (low brightness with small random variation).
5. Blue with white shimmer: hue drifts from cyan-blue depending on position in the
   icicle; alternate pixels (index parity) flip between saturated color and white,
   with the white pixels dim and the colored pixels bright — a glittery two-phase look.
6. Christmas candy cane: a hue picked from a handful of evenly spaced stops around
   the wheel starting in the blue region; alternate pixels are white vs colored
   (again via index parity), and brightness falls off steeply (cubically) with height
   so the icicle dims dramatically as it falls, the white pixels slightly dimmer.

Note schemes 5 and 6 depend on raw pixel-index parity, which assumes physically
adjacent pixels have adjacent indices — fine on strips, odd on serpentine matrices.

## Controls
No UI controls. All tuning lives in named configuration constants at the top:
nominal fall duration, nominal icicle length fraction, sparkle probability, wave-tail
probability, maximum gap between icicles, and the color-scheme limit. A
reimplementation could sensibly expose these as sliders.

## Layout assumptions
Designed for a mapped installation with y normalized 0 (top) to 1 (bottom of the
fall). The 1D fallback works on any strip with no hardcoded pixel count. Icicles
span all x, so on a 2D matrix the effect is a full-width horizontal band sweeping
downward.

## Clever bits worth preserving
- Reusing one global sawtooth timer for differently-timed icicles by capturing a
  phase offset at spawn time and taking the wrapped difference.
- Detecting end-of-fall by watching for the sawtooth to wrap (current < previous).
- The head-biased random sparkle via a fractional power of a scaled uniform draw.
- Gating all expensive math on the cheap "inside the icicle" test.
