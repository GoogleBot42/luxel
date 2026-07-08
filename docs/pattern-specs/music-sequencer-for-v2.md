# Music Sequencer for v2
kind: 1D+2D+3D (all three renderers exported; 2D/3D just delegate — effects are essentially 1D over normalized index)
sensors: yes (optional — degrades gracefully without a sensor board)

## What this is
Not a single visual effect: it is a framework for choreographing a playlist of mini-patterns to music with a steady tempo, plus a demo playlist. It provides (a) a beat/tempo/instrument detection engine fed by a sound sensor board, (b) a queue/sequencer that plays mini-patterns for specified durations measured in musical beats, and (c) a set of example mini-patterns. A large header comment documents usage; the shipped file also contains a deliberately invalid line right after a safety warning, so the pattern refuses to compile until the user reads the warning and deletes that line (the warning: this pattern nearly exhausts the older hardware's global-variable budget and crashing it repeatedly can soft-brick a board). A reimplementation should note the budget concern but need not reproduce the tripwire.

## Sensor inputs used (all optional)
- A 32-band audio frequency spectrum array.
- Overall sound energy (instantaneous loudness).
- Dominant frequency and its magnitude (for pitch detection).
- Ambient light level — used only as a probe: it rests at an impossible sentinel value when no sensor board is attached, and that check gates all sound processing.

## Sound analysis engine (runs each frame when a sensor board is present)
Volume normalization: raw energy is scaled up (about sixteen-fold) into a useful range, then tracked with a fast exponential moving average and a "loudest recently heard" ceiling that decays very slowly (only after more than a minute without a new maximum, and only while above a silence floor). From these it derives:
- a normalized volume in 0..1 (smoothed level relative to the recent ceiling, zero below the silence floor), and
- a "local volume ratio" (instantaneous vs. recent average), handy for detecting bursts.

Instrument detectors, each comparing a band-group energy to its own slow exponential average and firing when it spikes several-fold above it:
- Hi-hat: the two bands near the top of the spectrum (around 9 kHz), scaled up for headroom, threshold roughly double its average.
- Claps: a mid-high group of about seven bands, threshold roughly triple its average.
- Bass/kick: the sum of the two or three lowest nonzero bands (kick fundamental range).
Each detector exposes both a level-style boolean ("currently above threshold", may hold for several frames) and a debounced one-shot callback that fires at most once per fraction of a beat (about a fifth of a quarter note at the current tempo, allowing fast sixteenth-note retriggers). Mini-patterns can override the callbacks to react to kick/clap/hi-hat events; the callbacks are reset to no-ops whenever the playlist advances.

Bass beat detection is derivative-based rather than threshold-based: keep a fast and a very slow exponential average of bass energy, plus a rolling ceiling of recent bass with a slow automatic-gain-control decay. Maintain a small circular buffer (about five entries) of the frame-to-frame change in the fast average normalized by the ceiling; when the running mean of those derivatives goes meaningfully positive (bass rising), that is a beat. Buffer size trades reaction speed vs. stability.

Tempo estimation: record the interval between successive detected beats in a circular buffer of about eight samples (resetting after roughly five seconds of no beats). When the buffer is full, compute the mean interval and its relative standard deviation; if the spread is tight (within about ten percent), publish an estimated BPM (rounded to an integer, since most recorded music has integer tempo) and mark it reliable. A helper computes the standard deviation on down-scaled values to dodge the engine's limited fixed-point numeric range — a reimplementation on wider arithmetic can do it directly.

## Sequencer / queue
Global nominal tempo (default in the low 120s BPM), beats-per-measure of four, and a configurable beats-per-phrase (default a few dozen; the demo sets it to sixteen). The playlist is a set of parallel arrays (generously sized, a few hundred entries): a per-frame "setup" function for each entry, a duration in beats, and a continue-mode. Entry kinds by continue-mode:
- run for the stated duration (defaulting to one phrase when no duration is given);
- run until either the duration expires or a beat is detected (used to start visuals exactly on a beat drop) — on early advance, skip a small fraction of a second into the next entry to compensate for detection latency;
- same but triggered by a volume spike instead (silence-to-sound entrances);
- execute-once "commands" that run immediately with an argument and advance instantly (used to set tempo, phrase length, theme hue, direction flips, or adopt the detected tempo, optionally falling back to the current tempo if none is reliable);
- a custom predicate: advance when a supplied function returns true.
A build-the-playlist API appends entries; convenience wrappers exist for commands, play-until-beat, play-until-loud, tempo setting, and pre-seeding the bass gain baseline. A final "begin" call rewinds to the start; hitting the end of the playlist loops by default (halting or repeating the last entry are one-line alternatives).

Per frame the framework: processes sound (if available), advances timers, calls the current entry's per-frame function, which must assign the "renderer" — a function of (index, x, y, z) that the exported render hooks delegate to. The 1D render hook synthesizes x as normalized index; 2D delegates with z of zero.

Musical timers published for mini-patterns each frame: seconds and percent through the current entry; a fractional beat counter; percent through the phrase; and countdown ramps (running one-down-to-zero, the reverse of the engine's usual sawtooth) for measure, whole/half/quarter/eighth/sixteenth notes, derived by frequency-multiplying the whole-note ramp and wrapping. The ramp tops are clamped just below one to avoid wraparound artifacts.

Shared scratch state for mini-patterns: three per-pixel arrays (hue/saturation/value, one spare slot beyond pixel count to make interpolation loops safe), a theme hue, a direction flag, and a run-once setup latch — all cleared between playlist entries. Helpers: a perceptual hue warp (smooth S-curve that widens the warm region); a "nearness" kernel returning a squared (gamma-corrected) brightness bump when two normalized positions are within a given half-width; a musical note detector converting the dominant frequency to a semitone number via a log-ratio against a reference pitch (reliable only in flute-solo-like registers); and an exponential per-pixel brightness decay with a time constant expressed in seconds.

## Demo mini-patterns (each is a per-frame function that installs a per-pixel renderer)
- Off: black.
- Progress: a theme-colored bar filling the strip over the entry's duration (direction-aware), whole bar pulsing in brightness on each quarter-note ramp.
- Sweep: a soft pulse of theme color sweeping the strip once per beat (direction-aware), using the nearness kernel.
- Quarters: whole strip breathing to the quarter note with a triangle brightness profile across the strip and a slight hue tilt along it.
- Eighths: the strip cut into eight segments; the segment matching the current eighth-note position lights, brightness shaped by the eighth-note ramp, hue and saturation drifting through the measure — a position-stepping strobe.
- Half-note bass hit: a fake one-dimensional oscilloscope — a bright dot oscillates around the center with frequency and amplitude that decay within each half note, like a plucked bass string settling; hue drifts over the entry and shifts near the dot.
- Half surge: colors emanate from the strip center and withdraw sharply every half note, built from wrapped ratios of phrase progress and a smoothed half-note ramp, with a triangle brightness mask centered on the strip; hue evolves over the entry. Deliberately psychedelic; matching the character is enough.
- Piano: skipped automatically without a sensor board. Draws a piano keyboard across the strip covering about three octaves: naturals as faint warm keys (brightening momentarily on clap/hi-hat hits), key dividers marked and pulsing with the bass envelope, and the currently detected note lighting its key in a hue keyed to pitch class, with a quick exponential fade (uses the shared hue/value arrays and the decay helper). The whole pattern fades in over its first beats and out over its last. The natural/accidental layout is looked up from the standard 12-semitone keyboard shape (a reimplementation can use any octave-shaped lookup).

The demo playlist: set tempo (low 120s) and a sixteen-beat phrase; theme red; a couple of progress bars with direction flips; a few beat-synced sweeps alternating direction; quarters; eighths; a run of eight fast half-beat sweeps stepping the theme hue by a large fraction of the wheel each time (direction alternating); the bass-hit oscilloscope; two beats of black; a full phrase of the surge; then piano; loop forever.

## What it looks like running (demo playlist, no music)
A tightly tempo-locked routine at a steady dance tempo: red progress bars snapping back and forth, beat-synced color pulses sweeping in alternating directions, segment strobes marching down the strip, a decaying bouncing "bass string" dot, a blackout, then a swirling color surge — repeating. With a sensor board and music, the same routine can start sections on detected beats, adopt the detected tempo, and finish with the pitch-reactive piano.

## Controls
None active in the shipped demo (one commented-out slider exists for tuning the hi-hat threshold). Configuration is by editing the playlist.

## Layout assumptions
Works on any strip length; everything is normalized. 2D/3D installations just get the same effect as a function of whatever x is passed. The renderer chain means every mini-pattern is agnostic to topology.

## Non-obvious points
- Beat detection keys on the rising slope of a fast bass average (a derivative buffer), not on absolute level — robust to volume changes; the paired slow AGC ceiling handles quiet/loud material.
- The debounce timers are expressed in fractions of a beat at the current tempo, so retrigger limits scale with the music.
- The advance-early modes skip a small time offset into the next entry to compensate for detection latency, keeping visuals on the grid.
- Countdown-style note ramps (one-to-zero) are deliberately the opposite polarity of the engine's sawtooth, so "hit then decay" effects come free by using the ramp as brightness.
