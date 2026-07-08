# sound - rays Frequency-BPM Reactive 1
kind: 1D
sensors: yes

## Conceptual sensor inputs
- A multi-band audio spectrum array (a few dozen bands, low frequencies first), updated a few dozen times per second by the sensor board.
- The single most prevalent frequency in the sound and its magnitude (provided directly by the sensor board).
- An ambient light reading — not used visually; it is only a presence probe: it is initialized to an impossible sentinel value, and if the sensor board never overwrites it, the pattern knows no board is attached and switches to simulated sound.

## What it looks like
"Rays" of color are born at one end of the strip and stream toward the other end, leaving a scrolling history of the music behind them. Each ray's hue encodes the dominant pitch at the moment it was born (a swept tone from deep bass up to the mid-kilohertz range would paint a full rainbow along the strip), and its brightness encodes how loud that dominant frequency was, with strong contrast (quiet moments are dark gaps between rays). The novel twist versus the classic version of this pattern: the speed at which rays travel is proportional to the detected tempo of the music, so faster songs visibly stream faster. With no sensor board attached, a plausible-looking simulation produces gently varying rainbow rays.

## Algorithm
Two mostly independent subsystems share the frame loop.

### A. The ray renderer (per-pixel history buffers)
State: two arrays sized to the pixel count — one holding a hue per pixel, one a brightness per pixel — used as circular buffers; a fractional write-head position; the last written brightness (feedback for gain control); and an adaptive-gain accumulator.

Per frame:
1. Advance the write head by (elapsed milliseconds × speed), wrapping around the pixel count. Speed is set by the tempo subsystem below (with a small constant fallback default).
2. Adaptive gain: a proportional-integral controller chases a target where recent written brightness levels average out to the middle of the range. The error (target minus last written brightness) is accumulated into a clamped integral term; output gain = proportional term + integral term, with a modest lower floor so it never mutes entirely. This is automatic gain control so both quiet and loud sources fill the visual range.
3. Write at the head position: brightness = (dominant-frequency magnitude × gain), squared; hue = dominant frequency divided by a mid-kilohertz full-scale value, so pitch maps onto one trip around the hue wheel.

Per pixel at render: reverse the index (so motion flows in the desired direction), offset it by the write-head position modulo pixel count, look up hue and brightness from the buffers, square the brightness once more as gamma correction, and emit at full saturation.

(The original contains a commented-out extra hue drift over time and position — not active, ignore.)

### B. Beat/tempo detection (drives the travel speed)
All from the sum of a few of the lowest spectrum bands (skipping the very lowest), treated as "bass" — roughly the kick-drum fundamental range.

State: a slow exponential moving average and a fast one of the bass level; a "recent maximum bass" tracker with slow automatic decay (it bleeds off a little each update while bass is well above the slow average and above a small noise threshold); a small ring buffer of the frame-to-frame first derivative of the fast average normalized by the recent maximum, plus a running average of that buffer; a debounce countdown; a ring buffer of the last several (about eight) inter-beat intervals in milliseconds; and the interval timer since the last beat.

Per frame:
1. Update the averages: the slow one blends in a tiny fraction (order one part in a thousand) of the new bass value, the fast one a moderate fraction (order one part in ten).
2. Push the normalized derivative of the fast average into the ring buffer and maintain its running mean. When that mean exceeds roughly the midpoint, bass is "rising" — a beat candidate.
3. Debounce: a candidate only fires if the countdown has expired; firing reloads the countdown to a small fraction (about a fifth) of a quarter-note at the current tempo estimate, allowing rapid double-kicks through while suppressing chatter.
4. On a confirmed beat: record the elapsed interval since the previous beat into the interval ring buffer, reset the interval timer, average all stored intervals, convert that mean interval to beats-per-minute (via an intermediate rescaling done in the original only to dodge the engine's limited numeric range), and set ray speed proportional to (estimated BPM × the user's speed-factor slider).

The original also reconstructs its own milliseconds-per-frame from a fast wrapping sawtooth clock, handling the wraparound by hand. A reimplementation can simply use the engine-supplied per-frame delta in milliseconds — that is all this machinery produces.

### C. Sound simulation fallback
If no sensor board is detected, each frame synthesizes: a dominant frequency that sweeps slowly over the low-to-mid kilohertz range (a slow wave scaled and jittered with a small random factor), and a magnitude formed from the logarithm of just-above-one plus the product of three sine-shaped waves at mutually incommensurate small-prime multiples of a slow clock (giving irregular, beat-like swells), again jittered by a small random factor. Randomness elsewhere in the pattern: none.

## Known defects / gotchas an implementer should handle deliberately
- The derivative ring buffer's length is meant to be a quadratic function of the beat-sensitivity slider (from very short for fast retriggering up to the mid-teens for sluggish, sparse bass). In the original it is computed once at startup **before** the sensitivity's default value is assigned, so it always ends up at the minimum length, and moving the slider never resizes it. A faithful-but-fixed reimplementation should size (or effectively re-window) the buffer from the current slider value.
- The tempo estimate is undefined until several beats have been observed; the fallback constant speed covers the gap. There is disabled code hinting at a "reset intervals after a long silence" behavior; the shipped version never resets.
- A couple of variables are declared twice (harmless in the original engine); don't replicate that.

## Layout
Fully 1D and pixel-count agnostic (buffers sized from the pixel count). No hardcoding beyond the mid-kilohertz hue full-scale.

## Colors
Full-saturation spectrum: hue is a direct pitch-to-rainbow mapping (bass at one end of the wheel through to the other for high mids). Brightness from sound energy, double-squared overall, so it reads as vivid rays on black.

## Timing feel
Rays take from several seconds to under a second to traverse the strip depending on detected tempo and the speed-factor slider. Beat response feels immediate (a frame or two); the tempo estimate settles over the last several beats, so speed changes glide rather than jump.

## UI controls
- **Slider** ("beat sensitivity"), with a numeric readout: intended to trade responsiveness vs. stability of beat triggering (see the defect note above — in the original it is inert).
- **Slider** ("BPM speed factor"), with a numeric readout: scales how strongly the detected tempo drives ray travel speed.
