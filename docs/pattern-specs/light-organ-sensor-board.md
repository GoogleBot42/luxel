# Light Organ -- sensor board
kind: 1D
sensors: yes

## Sensor inputs used
- A **32-band audio spectrum array** (bin magnitudes from a few tens of Hz up to roughly 10 kHz).
- **Overall sound energy** (a single loudness scalar).
- **Peak spectral magnitude** (the strongest single bin's magnitude).

## What it looks like
A classic four-channel light organ on a 1D strip. The strip is tiled with a repeating group of four colored bars, each bar's brightness pulsing faithfully with one frequency band of the music (bass / low-mid / mid / high). On every detected bass beat the bars visibly reorganize — band-to-position assignments rotate, bar widths re-randomize, and some bar hues re-randomize — so the display "dances" in sync with obvious changes in the music. Every so often (after several dozen beats) the whole layout style switches between two modes: equal-width bars vs bars whose widths breathe with each band's loudness. A white accent pixel, repeated at a fixed pixel stride across the strip, flashes and marches along on strong overall peaks. When the music stops (even in a noisy room), the strip goes dark within moments and stays dark until music resumes. Designed for a strip on the order of a hundred pixels running at high frame rate.

## Algorithm

### Band splitting
The spectrum bins are grouped into four bands: bass (the lowest ~fifth of bins, up to about 200 Hz), low (the next few bins, roughly 200–500 Hz), mid (the next quarter or so, roughly 500 Hz–2 kHz), and high (the remaining upper bins, roughly 2–10 kHz). Each frame, for each band, compute the band's **maximum bin magnitude** and the band's **average magnitude**. Each band's running maximum is also decayed slightly every frame (a few percent — about a decibel per frame) so it tracks the recent loud content rather than an all-time record.

### Moving averages (the AGC core)
Several time-windowed moving averages are maintained by shifting fixed-length sample arrays at timed intervals (each has its own update timer driven by frame delta):

- A **short-term moving average** of overall energy: a few dozen samples taken several times a second (window on the order of a few seconds).
- A **long-term moving average** of overall energy: a couple hundred samples taken about every two-thirds of a second (window on the order of a couple of minutes). It is clamped to a small floor so a silent room doesn't drive it to zero.
- Per-band moving averages of the band average magnitudes: a handful of samples at a few-per-second rate (window of a couple of seconds), then multiplied by a fixed factor of a few (i.e. raised several dB) to act as a noise-squelching threshold.

### Silence gating
If the short-term energy average falls to well under half of the long-term average (a fixed ratio of a couple-and-a-half, i.e. several dB), the music is judged to have stopped: all gains are forced to zero (strip dark), the long-term average and band averages **stop updating** (their timers are held reset) so background chatter can't drag the reference down, and the layout mode is nudged forward so the pattern differs when music returns.

### Gain / scaling (per-band AGC)
Each band keeps a slowly relaxing **peak** value (the largest band maximum seen, decayed by a couple percent about once per second). When not gated, each band's gain is the reciprocal of (peak minus that band's threshold/average), clamped to a large maximum. The displayed magnitude for a band is (current band max minus band average) times that gain, floored at zero. This maps "at the noise threshold" to dark and "at recent peak" to full brightness — an automatic gain control that keeps brightness swings faithful to the music at any volume. A similar overall computation using the global peak and the larger of the two energy averages (scaled by the squelch factor) produces a "white flash" intensity value.

### Beat detection & re-randomization
A beat is declared when the bass display magnitude jumps upward from the previous frame by more than a fixed step (roughly a third of full scale). Each beat:
- advances a beat counter that cycles through four phases;
- steps a pixel offset backwards by a few pixels (wrapping around the strip length) — this makes the white accent march;
- re-randomizes the common bar width (a few pixels, between one and about four);
- draws fresh random numbers used for some bar hues;
- counts beats, and after several dozen beats advances the layout mode (two effective modes, cycling).

The four-phase beat counter rotates which band feeds which of the four bar slots, and sets each slot's hue: the bass slot is always a fixed warm red, the low slot a fixed orange, the mid slot a random hue, and the high slot either a fixed blue-violet or a hue derived from a slow time ramp (a cycling timebase with period around ten seconds, sometimes offset by a quarter or half of the wheel). The rotation means the red bass bar physically moves to a different slot each beat.

### Rendering (per pixel)
The strip is treated as tiles of a four-bar group repeated end to end:
- **Mode A (equal bars):** the tile is four bars of the current common width; the pixel's position within the tile picks one of the four slots; the pixel gets that slot's hue at full saturation with brightness equal to that slot's band magnitude.
- **Mode B (proportional bars):** each of the four bands gets a bar whose width (a pixel to several pixels) is proportional to its current display magnitude (each width derived by scaling the magnitude up several-fold and clamping to a small pixel range; the bass-driven one may collapse to nothing); the tile is the sum of those widths and pixels are assigned band/hue/brightness by which sub-range they fall into.

Finally, regardless of mode: pixels whose index matches the marching offset modulo a small fixed stride (every eighth pixel) are overridden with **white** at the overall flash intensity, but only when that intensity is above a substantial threshold — a strobe-like sparkle on big peaks.

## Layout assumptions
Uses the actual pixel count for wrap-around of the marching offset, so it adapts to strip length, but bar widths are in absolute pixels tuned for a strip on the order of eighty pixels; on very long/dense strips the bars would look thin. Obvious fix: scale bar widths with pixel count. 1D renderer only.

## Colors
Bass: pure red. Low band: orange. Mid band: a random hue re-picked on beats. High band: blue-violet or a slowly cycling hue. All fully saturated except the white peak sparkle (no saturation, brightness from overall peak level).

## Controls
None exported — behavior is tuned entirely by the audio.

## Timing feel
Brightness follows the music essentially instantly (frame-rate limited). Layout reshuffles land on bass beats. Layout mode changes arrive on the order of every minute of steady beats. Silence gating darkens the strip within a couple of seconds of the music stopping; recovery is immediate when music resumes.

## Clever bits
- Two-timescale energy averaging (seconds vs minutes) provides a robust music-stopped detector that ignores loud room chatter, and freezing the long-term reference while gated prevents chatter from re-arming it.
- Per-band "reciprocal of headroom" gain with slowly decaying peaks is a cheap, effective AGC that keeps brightness dynamics faithful across volume levels — the author considers faithful brightness change more important than the spatial pattern itself.
- The eye's and ear's roughly parallel logarithmic responses mean no explicit log/gamma correction is applied, deliberately.
