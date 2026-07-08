# Flash Posterize + Music Sequencer framework
kind: 1D+2D+3D (all three renderers exist; 2D/3D delegate to a common dispatcher, and the bundled demo effect is index-based 1D)
sensors: yes (sound: a 32-band frequency spectrum array, an overall sound-energy scalar, and dominant-frequency info from the sensor expansion board; also reads the ambient-light value once, only to detect whether a sensor board is present)

This is two things in one file: (A) a reusable music-sequencing framework, and (B) one demo effect ("flash posterize") that the shipped sequence actually plays. Both must be reimplemented.

## What it looks like when running
A smooth, slowly drifting rainbow-ish gradient runs along the strip. Each time a bass beat is detected in the music, the display toggles between two modes: the smooth gradient, and a "posterized" version of the same gradient chopped into solid-color segments separated by single dark pixels. The segment lengths slowly breathe over the course of a musical phrase. With music playing, the effect is a strip that flips between smooth and stained-glass looks on every kick drum. Without a sensor board it just shows the smooth drifting gradient (no toggling). The demo runs this one effect for a very long block of beats at a default dance tempo, then loops.

## Part A — the sequencer framework

### Dispatch model
There is one active "frame-prep" function and one active "per-pixel" function at any time; both are indirect references that sub-patterns reassign. The global frame hook runs: (1) sound processing if a sensor board is present (detected by whether the light reading ever changes from an impossible sentinel), (2) musical timer updates, (3) the current sub-pattern's frame-prep. The 1D and 2D entry points forward into the 3D entry point (index mapped to a fractional position along the strip for 1D; z forced to zero for 2D), which calls the current per-pixel function.

### Queue
A sequence is defined declaratively at load time by appending entries to a fixed-capacity queue (a couple hundred entries). Each entry holds: a frame-prep function, a duration in beats, and a "continue mode". Modes:
- run for the fixed duration, then advance;
- run until a bass beat is detected, or the duration expires, whichever first;
- run until the overall volume spikes (e.g. silence ends), or the duration expires;
- execute-once commands: the function runs immediately with the stored value as an argument, then the queue advances (used to set tempo, set phrase length, set globals like a theme hue, or seed the bass gain);
- anything else is treated as a predicate function — advance when it returns truthy.

A start call resets the index and plays entry after entry. When the queue ends, the configured end behavior runs (loop by default; alternatives: hold black forever, or repeat the last entry). Between entries, shared per-pixel scratch arrays (hue/saturation/value, one slot per pixel plus one spare for interpolation) are cleared, a "setup already ran" flag is reset, an edge-trigger latch is reset, and the three instrument-callback hooks are reset to no-ops. When an entry is cut short by a detected beat or volume spike, the next entry starts slightly pre-advanced (a small fraction of a second) to compensate for detection latency.

### Musical timers
From the current tempo (beats per minute; default a standard dance tempo of ~120) and a phrase length in beats (default a few dozen; a measure is four beats), the framework maintains, every frame: elapsed seconds within the current entry; fraction-complete of the current entry (ramps up 0→1 like a sawtooth); fraction-complete of the current phrase (ramps up); a running fractional beat counter; and countdown ramps (1→0 sawtooth, i.e. inverted phase) for the measure, whole note, half note, quarter-note beat, eighth note and sixteenth note. Sub-patterns read these to animate musically.

### Sound processing (sensor board required)
- **Volume normalization**: the raw energy scalar is scaled up by roughly an order of magnitude to a workable range, then tracked with two exponential moving averages — a fast one and the raw sample — plus a slowly-decaying "loudest recently heard" maximum (held for on the order of a minute-plus before decaying, so it survives quiet bridges). Outputs: a normalized 0–1 volume (smoothed level relative to recent maximum, gated by a silence floor) and an instantaneous-to-average ratio useful for transient detection.
- **Instrument detectors**: three band-energy detectors, each comparing current energy to its own slow moving average with a multiplicative threshold:
  - bass/kick: sum of the few lowest spectrum bins (kick-drum fundamentals);
  - claps/snare: sum of a band of upper-middle bins;
  - hi-hat: sum of a couple of bins near the top of the spectrum, boosted because raw values there are tiny.
  Each detector has a continuous "currently on" boolean and a debounced one-shot callback that fires at most once per event; the debounce window is a fixed small fraction of a quarter note at the current tempo (so fast sixteenth-note retriggers are still possible). Sub-patterns react by overwriting the callback hooks.
- **Beat detection**: rather than thresholding raw bass, it watches the *first derivative* of a fast bass moving-average, normalized by a slowly-decaying recent bass maximum (auto gain control). A short ring buffer (about five samples) of these derivatives is averaged; when the average says bass is rising past a midpoint threshold, that's a beat (then debounced as above).
- **Tempo estimation**: intervals between the last eight detected beats are kept in a ring buffer (reset if several seconds pass with no beat). When the buffer is full, compute mean and relative standard deviation of the intervals; if the spread is tight (within about a tenth), publish an estimated tempo (converted from mean interval, rounded to an integer BPM) and mark it reliable. A queue command can adopt the detected tempo as the sequence tempo.
- **Pitch helper**: a utility converts the dominant-frequency reading to a semitone number relative to a reference pitch via a logarithm — usable for melody-reactive effects.

### Other shared helpers (present for sub-patterns, lightly used by the demo)
- A proportional-integral feedback controller intended for automatic brightness gain: it accumulates rendered brightness, compares to a target fill level, and computes a sensitivity gain. (Provided as infrastructure; the demo effect does not use it.)
- An exponential decay helper that fades the shared value array so a starting value dies away over a chosen number of seconds, frame-rate-compensated.
- An edge-trigger helper that fires a function once on the rising (or falling) edge of any 0–1 ramp.
- An "all off" sub-pattern that renders black.

## Part B — the demo effect ("flash posterize")
State: a boolean posterize toggle; a per-pixel hue scratch array; a running "current segment start index" and "previous sign" used while scanning; a segment-length parameter animated each frame by a triangle wave over the phrase progress (varying it by a moderate fraction around its base).

Each frame: the beat callback is set to flip the posterize toggle. The gradient's hue function is: a base offset around a third of the wheel, plus the pixel's fractional position, plus a time drift that loops in several seconds — i.e. a full-width rainbow slice that slides along the strip.

Per pixel (scanned in index order):
- If not posterized: render the gradient hue at strong-but-not-full saturation and moderately high brightness.
- If posterized: render the hue previously stored for this pixel's segment at full saturation, with brightness forced to zero exactly when this pixel's stored hue differs from its neighbor's — which blacks out precisely the first pixel of each segment, drawing thin dark separators.
- Regardless of mode, the scan also *precomputes next frame's segments*: it evaluates a quasi-random "gap" function at a point proportional to the pixel position — a sum of two products of periodic waves at incommensurate frequencies, offset downward so it crosses zero irregularly; the segment-length parameter stretches two of the frequencies. Each zero crossing ends a segment: all hue slots from the segment's start through the current index are filled with the gradient hue sampled at the segment's midpoint, and a new segment begins.

The clever bit: segmentation is done inline during rendering with one frame of latency, using zero-crossings of interfering waves instead of stored random boundaries — segments therefore drift and breathe smoothly as the wave parameter animates over the phrase.

## Shipped sequence
Set the default tempo, play the flash-posterize effect for a run of several hundred beats (several minutes), then loop. (A commented-out option waits in darkness until sound is heard.)

## Colors
Full rainbow gradient (all hues, cycling), shown either smoothly blended or as solid posterized bands with black separators. The "off" pattern is black.

## Controls
None exposed by default. (A commented-out slider exists for tuning the hi-hat detector threshold.) All configuration is by editing the sequence code.

## Timing
Beat-locked: everything is expressed in beats at the current tempo (default standard dance tempo). The gradient drift loops in several seconds; segment lengths breathe once per phrase (tens of seconds); mode flips occur on each detected kick, typically every half-second-ish with dance music.

## Layout assumptions
Works on any pixel count and any 1D/2D/3D mapping, but the demo effect uses only the pixel index, so on a matrix it appears as wiring-order stripes. The queue arrays have a fixed capacity of a couple hundred entries. The original targets a platform with a hard cap on global variable count and warns it is near the limit — a reimplementation should keep the framework modular rather than replicating that constraint.
