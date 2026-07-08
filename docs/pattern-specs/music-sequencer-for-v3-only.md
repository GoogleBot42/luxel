# Music Sequencer - for V3 ONLY
kind: 1D+2D+3D (all three renderers exported, but the content is one-dimensional: the 2D/3D entry points simply forward to a shared renderer that only uses the pixel index / first axis)
sensors: yes

This is not a single effect — it is a music-choreography framework plus roughly twenty demo mini-patterns and a scripted demo show. Think "sequencer/VJ engine": the pattern author writes a queue of mini-patterns, each played for a duration measured in musical beats at a known tempo, and the engine handles timing, tempo detection from audio, beat/instrument detection, and switching between mini-patterns. It expects the sensor expansion board (audio spectrum input); without one it degrades (some mini-patterns skip themselves, others fall back to clock-driven behavior).

## Sensor inputs used
- A 32-band audio frequency spectrum array (updated a few dozen times per second).
- Overall sound energy (a single loudness scalar).
- Dominant frequency and its magnitude (used for musical-note detection).
- The ambient light reading is used only as a "is a sensor board attached?" probe: it is initialized to an impossible negative value and, if it never changes, the code concludes no board is present.

## Part 1 — Sound analysis engine (runs every frame before anything else)

### Volume normalization
Raw energy is scaled up (by an empirical factor around an order of magnitude) and tracked three ways: a fast-ish exponential moving average (time constant around a second), a running maximum with very slow decay (the max only starts decaying after more than a minute without a new peak — longer than a typical song bridge), and a silence threshold. From these it derives:
- a smoothed 0..1 "volume" = smoothed energy relative to the recent max (zero when below the silence threshold), and
- an "instantaneous vs. recent average" loudness ratio, used to detect sudden spikes.

### Instrument detectors
Three band groups are watched, each compared against its own slow exponential average and flagged "on" when it exceeds that average by a factor:
- Bass/kick: the sum of the lowest few spectrum bins (kick-drum fundamentals). Tracked with both a slow and a fast exponential average, plus a slowly self-decaying running max (an automatic gain control).
- Claps/snare: a group of upper-middle bins summed.
- Hi-hat: a couple of bins near the top of the spectrum (scaled up because raw readings there are tiny).

Each detector is debounced: a one-shot callback fires at most once per configurable fraction of a beat (default: a fifth of a beat, so doubled sixteenth-note hits can still retrigger). Mini-patterns react by assigning their own functions to the three callback hooks (beat / claps / hi-hat detected); the hooks are cleared between mini-patterns.

Beat detection specifically is derivative-based: it keeps a short circular buffer (a handful of samples) of the frame-to-frame change in the fast bass average normalized by the bass max, and declares a beat when the running mean of those derivatives goes meaningfully positive (i.e., bass is rising). This "rising bass" approach is more robust than a simple threshold.

### Tempo estimation
The engine records the intervals between the last eight detected beats (reset if several seconds pass with no beat). When all eight are filled, it computes their mean and relative standard deviation; if the spread is small (under about ten percent), the tempo estimate = a minute divided by the mean interval, rounded to a whole BPM (round because released music usually has integer tempos; a comment suggests removing the rounding for live/DJ material), and a "reliable" flag is set.

## Part 2 — Beat clock and pattern queue

### Clock
A nominal tempo (default in the low hundreds of BPM) can be set explicitly mid-sequence or snapped to the detected tempo. From elapsed time in the current mini-pattern the engine continuously derives:
- ramp-up fractions (0→1): progress through the current mini-pattern, and progress through the current "phrase" (a configurable number of beats, default a few dozen);
- ramp-down sawteeth (1→0, opposite direction to the platform's usual clock): remaining fraction of the current measure, whole note, half note, quarter-note beat, eighth note, and sixteenth note; plus a decimal running beat counter.

Mini-patterns are written entirely against these timers, which is what keeps everything locked to the music.

### Queue
The queue is a set of parallel arrays (a few hundred entries): a per-frame setup function, a duration in beats, and a continuation mode per entry. Modes:
- fixed duration (if no duration was given, one full phrase);
- fixed duration, but advance early when a bass beat is detected ("hold on black until the drop");
- fixed duration, but advance early when volume spikes (silence → sound);
- one-shot command: execute immediately with an argument and advance (used for "set tempo", "set phrase length", "set the shared theme hue", "flip direction", "pre-seed the bass gain so the first beats detect well", etc.);
- alternatively a predicate function: advance when it returns true.

When an entry's time is up, the engine resets shared state (clears the shared hue/sat/brightness scratch arrays, re-arms a run-once setup latch, clears the edge-trigger latch and instrument hooks) and starts the next entry, optionally skipping a few tens of milliseconds into it to compensate for detection latency. When the queue is exhausted it loops from the start (halting or repeating the last entry are one-line alternatives). Each frame, the current entry's setup function runs and must assign the pixel-renderer function that the exported renderers delegate to.

## Part 3 — Shared helpers (used by many mini-patterns)
- Three shared per-pixel scratch arrays (hue, saturation, brightness), one element longer than the strip to make interpolation loops safe.
- An exponential decay helper that fades the brightness array toward zero with a chosen time-to-dark.
- A "proximity" helper: 1 when two normalized positions coincide, falling to 0 at a chosen half-width, with the result squared for a gamma-like soft edge.
- A hue-warping helper that redistributes the hue wheel to look more perceptually even (built from the platform's sine-shaped wave of a shifted, halved hue).
- A polar color-adder that mixes two fully-saturated hue/brightness pairs as vectors (cheaper than round-tripping through cartesian color space).
- An edge-trigger helper: run a function once on the rising (or falling) edge of any boolean/ramp.
- A musical note detector: converts the dominant-frequency reading to a semitone number relative to a low reference pitch via a log-base-2 ratio.
- A small proportional-integral controller used as an auto-gain for brightness: a target "fraction of the display lit," feedback accumulated from rendered brightness, and a gain that adapts so the display stays near the target fill regardless of music level.
- Shared globals: a "theme hue" and a "direction" flag that the sequence script can set between entries so consecutive mini-patterns coordinate color and orientation.

## Part 4 — The demo mini-patterns (each one sentence to a paragraph; all 1D over normalized strip position)

1. Off: all pixels dark (used for musical rests and drop build-ups).
2. Progress bar: fills the strip (respecting the direction flag) in theme hue over its queued duration, brightness pulsing with each beat.
3. Measure progress: same idea but fills over one measure, repeatedly.
4. Sweep: a soft dot in theme hue sweeps the strip once per beat (direction flag honored).
5. Quarters: whole-strip triangle-shaped brightness ridge pulsing to the beat, hue slightly graded along the strip.
6. Eighths: the strip divided into eight segments; the segment matching the current eighth-of-a-measure lights and decays with the eighth-note ramp, hue/saturation drifting through the measure.
7. Strobe: white full-strip flash on the leading fraction of every sixteenth note (photosensitivity warning applies).
8. Half-note surge: colors emanate from the center and withdraw sharply every two beats — built from a smoothed half-note ramp folded into a radial position term, with hue evolving over the mini-pattern's life.
9. Dancing pixel: a single bright dot whose position is a sum of layered oscillations (a slow wander, beat-cubed wiggle, eighth-note square-wave jitter that fades out, plus a bass-level term that grows in), and whose width breathes with the bass; hue snaps to the complement halfway through.
10. Half-note bass "oscilloscope": a dot oscillates about the center like a decaying bass waveform — oscillation frequency chirps downward while amplitude decays as a cubic of the half-note ramp; a similar variant does it every beat, with a width that breathes over the pattern and hue stepping per beat within the measure.
11. Paint fizzle: a "texture brush." On every detected beat a new random brush stroke starts: random start position, random signed length (usually one direction, occasionally reversed), hue drawn from a slow clock with a coin-flip small offset. The stroke instantly sputters a run of pixels with random sparkly brightnesses (about four fifths of alternating pixels biased bright) in the stroke's hue; claps extend the current stroke. Everything decays continuously — faster when the music is loud — and a slow spatial shimmer ripples the result. Rendered so that dimmer grains go white-ish (saturation drops when a grain is bright).
12. Build-up segments: divides the strip into 2^n segments where n grows as the remaining queued beats shrink (segments multiply as the drop approaches), flashing a random on/off bitmask of segments on each beat, then each eighth, then each sixteenth in the final bars; colors cycle with the phrase, switching to white for the last couple of beats. A guard re-rolls the mask when few segments exist so at least one is always lit.
13. "Hyper" retro chase: a swept-in pulse builds from one end over the first beats, strobes and modulates the far end by loudness mid-pattern, reverses direction late, and steps its hue in large jumps every couple of measures. Deliberately harsh/flashy.
14. Parallax starfield: a fixed set of a couple dozen particles at random offsets scroll past like objects seen from a moving train — each particle is projected with a pinhole-camera model (fixed focal length, particles distributed across a depth window several strips deep), so nearer particles are taller, brighter-edged streaks that move faster; the scroll follows the phrase, gaining a back-and-forth wobble in the second half. Trails decay quickly; hue drifts with the phrase and slightly with depth.
15. Rain: the strip is a vertical camera view — top half a dim warm-grey cloudy sky gradient. A handful of white drops fall (nearer drops are longer-tailed, brighter, faster, per a small projection model with a hardcoded focal-length-style ratio); when a drop passes its precomputed ground position it becomes a blue splash: four splash droplets arc outward following a little parametric splash-height curve, fading as they fly. With a sensor board, beats spawn near drops and claps spawn far drops; after several beats of silence (or with no board) drops spawn at random low probability. Drops are recycled from a fixed pool.
16. Flash sieves: on strong loudness spikes, paints a random stretch with an alternating red/blue-and-gap comb; on milder spikes occasionally paints an alternating white-dot comb; during silence, occasionally seeds dim blue pixels at a position swept by a fast clock. Everything decays (slowly in silence, quickly in sound).
17. Piano: maps three octaves of piano keys along the strip (a fixed natural/accidental bitmask pattern gives dim "white keys" and dark "black keys", with faint key dividers that pulse with bass). The note detector lights the key of the currently detected pitch in a hue derived from its pitch class, brightness following the note's magnitude, with fast decay. Fades in/out at the start/end of its slot. Skips itself without a sensor board.
18. Ocean ("budget Pacifica"): layered sums of several slow sine waves at different spatial frequencies and drift speeds (one frequency itself slowly modulated), cubed for contrast, in the theme hue with slight per-wave hue offsets, brightness scaled by overall volume; an occasional white crest pulse sweeps through (a narrow sine burst gated by a square window whose timing was hand-synced to the demo). Hue drifts away from theme late in the slot. Not truly sound-reactive beyond the volume scale.
19. Posterize flash: draws a smooth moving hue gradient; on every detected beat it toggles between the smooth gradient and a "posterized" version: the strip broken into irregular segments (segment boundaries are the zero-crossings of a hand-tuned product-of-waves gap function whose parameter oscillates with the phrase, so segment sizes breathe), each segment filled with the gradient hue sampled at its midpoint, with thin dark lines at boundaries. The segment table for the next frame is computed during rendering of the current one.
20. Splotch on beat: each detected beat throws a random soft blob (random position/width) — deep red/pink normally, cyan when the detector reports the high-frequency variant — which then decays exponentially; while hi-hats are on, every few pixels at one end flick warm white, and while claps are on, the other end does.
21. Spectrum analyzer: about twenty bins of the spectrum mapped along the strip, each bin's smoothed level driving brightness through the PI auto-gain (target fill tied to volume). Per-bin hue jumps toward a "peakiness" measure (instant vs. average) on sudden peaks and relaxes back slowly. Brightness smoothing oscillates over the phrase between a neighborhood running average and randomized nearby sampling (a sparkle/smooth crossfade), and hue similarly splays. In the second half, a desaturated white marker tracks the (log-smoothed) dominant frequency along the strip. Skips itself without a sensor board.
22. Elastic: a physics toy — a short chain of particles connected by springs with rest length, friction, and a spring constant that stiffens over the slot. The head particle seeks a target that jumps to a new random position (at least an eighth of the strip away) on every detected beat (or once per beat by clock without a board; wanders around the center during long silence). Particles are plotted with linear interpolation between adjacent pixels, hues splayed by volume from a violet base; trails linger periodically (the decay time itself oscillates over tens of beats).
23. Sound rays: three writing heads circulate around the strip at different speeds (fast, medium, slow — ratios roughly 7 : 3.5 : 2), continuously recording the current hi-hat, clap, and bass levels into three circular pixel-buffers; rendering reads all three buffers with their rotating offsets and combines them additively in RGB (bass→red-ish with contributions from the others, claps→a touch of green, hi-hats→blue), so each instrument leaves rays streaming around the loop at its own speed. (A commented alternative combines them in hue space via the polar mixer, at lower frame rate.) Skips itself without a sensor board.
24. Four diagnostic visualizers (bass bins, clap bins, hi-hat bins, volumes): bar-graph style meters showing raw bins vs. their moving averages, plus small indicator zones (edges/center of the strip) that blink in distinct colors when each instrument detector or its debounce window is active. Meant for tuning; they also skip without a board.

## Part 5 — The shipped demo sequence
The pattern ends with a scripted show (the queue): set a tempo in the low hundreds and a modest phrase length; hold dark until any sound (up to a few minutes); a stint of the piano; pre-seed the bass gain; set a light-blue theme; play the ocean until a beat is detected (up to a minute); then a fixed tour through most of the mini-patterns above — fizzle, rays, build-up, the diagnostics briefly, the retro chase, the surge, more fizzle; retheme to violet, more ocean; retheme to red and wait for a downbeat; then a rhythmic-precision section alternating progress bars, sweeps (with direction flips and tiny hue nudges between half-beat sweeps), quarters, eighths, the bass-scope hits interleaved with darkness and diagnostics, a half-beat strobe; a green dancing pixel; then the bass scope, elastic, splotches, rain, posterize, elastic again, parallax, sieves; and finally a long analyzer stint — then loop forever.

## Controls
- Slider, "choose manual pattern": selects one of the ~twenty demo mini-patterns by position (it also clears shared state when moved). It only takes effect if the sequence script includes the provided "manual pattern" queue entries (shipped commented out — it's a debugging aid).
- A commented-out slider exists for tuning the hi-hat threshold.

## Timing feel
Everything is quantized to the musical grid: pulses per quarter/eighth/sixteenth note, phrase-length arcs of tens of seconds, and pattern changes every few seconds to a minute according to the script.

## Layout assumptions
Strip-length-agnostic (everything normalized), 1D content. No hardcoded pixel counts, though several effects have counts tuned for a strip on the order of a hundred-plus LEDs (fixed particle pools, a 20-pixel drop tail, etc.). The original warns that it nearly exhausts the platform's global-variable budget; a reimplementation should not need to care, but it explains the deliberately dense style.

## Non-obvious points worth preserving
- The ramp-down (1→0) note timers, opposite in direction to the platform's usual ramp-up clock, are the framework's signature: "energy that decays until the next beat" is the natural shape for musical flashes.
- Beat detection keys on the rising derivative of a fast bass average (normalized by a self-decaying max), not on absolute level.
- Advancing the queue "early, skipping slightly into the next entry" compensates for audio-detection latency so transitions land on the beat.
- Tempo is accepted only when eight consecutive beat intervals agree within a small relative spread, and is rounded to integer BPM.
- The between-entries reset (scratch arrays, one-shot setup latch, instrument hooks, edge trigger) is what lets twenty mini-patterns share globals safely.
