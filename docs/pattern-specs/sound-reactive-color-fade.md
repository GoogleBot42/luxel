# Sound Reactive Color Fade
kind: 2D (but the output is a single solid color, so a 1D renderer is trivially
equivalent and should be offered too)
sensors: yes

## What it looks like
The entire display is one solid, fully saturated, full-brightness color. With no music,
the hue creeps slowly and continuously around the color wheel (a full lap takes on the
order of minutes at mid slider setting). When music with a clear bass beat plays, every
detected bass hit makes the hue *jump* forward to the next of six anchor hues, so the
display snaps color on the beat and then resumes its slow drift from there.

## Sensor inputs (conceptual)
- A multi-band audio spectrum array (roughly 32 bands) from the sensor board. Only the
  lowest few bands (skipping the very lowest/DC band) are used — summed together as a
  "bass energy" signal, targeting kick-drum fundamentals.
- An ambient-light reading is used *only* as a presence probe: it is initialized to an
  impossible negative value, and if the sensor board never overwrites it, all sound
  processing is skipped (the pattern then just does the slow fade).

## Algorithm

### State kept between frames
- Current hue (the single output value).
- Bass-detector state: a very slow exponential moving average and a fast exponential
  moving average of bass energy; a decaying recent-maximum used for automatic gain
  control; a small circular buffer of the recent normalized frame-to-frame changes of
  the fast average, plus its running mean; a debounce countdown; and a ring buffer of
  the last several inter-beat intervals with an interval timer (recorded but never
  actually used to alter the visuals — vestigial tempo-inference scaffolding).
- A hand-rolled elapsed-time tracker: instead of trusting the per-frame delta argument,
  the pattern derives milliseconds-per-frame from a wrapping global sawtooth timer
  (handling the wraparound case). The stated motivation is better sync across multiple
  networked devices, since the global timer is shared but per-frame deltas are not.

### Per frame
1. Update the elapsed-time tracker.
2. If a sensor board is present:
   - Sum the low spectrum bands into a bass value; track a recent maximum of it. If the
     maximum is far above the slow average and above a small noise floor, decay the
     maximum slightly each frame (auto gain control).
   - Update the slow EMA (very long time constant, ~thousand-frame scale) and fast EMA
     (~ten-frame scale).
   - Push the change in the fast EMA since last frame, normalized by the recent
     maximum, into the circular buffer and maintain a running average of the buffer.
     When that average rises above a threshold slightly over half, "bass is rising" —
     a beat candidate.
   - Debounce: a beat only fires if a countdown has expired; on firing, the countdown
     is reloaded with a fraction (about a fifth) of a quarter-note at an assumed
     default tempo of ordinary dance-music speed. This allows re-triggering on fast
     doubled kicks while rejecting chatter. There is also a several-second "no beats
     for a long time" reset that clears the interval history.
   - On a fired beat: record the inter-beat interval, then advance the hue to the next
     anchor color (see Colors) — i.e. find the first anchor strictly greater than the
     current hue and snap to it, wrapping to the first anchor after the last.
3. Independently, the hue drifts forward continuously: each time a very fast wrapping
   timer reads its zero tick, the hue is advanced by a tiny increment. The increment is
   a small base amount plus a several-times-larger amount scaled by the fade-speed
   slider, all wrapped into the unit hue circle. Net effect: steady slow rotation whose
   rate the slider multiplies severalfold.

### Per pixel
Nothing position-dependent — every pixel is painted the current hue at full saturation
and full brightness. Layout-independent; no pixel-count assumptions.

## Colors
Six anchor hues spaced around the color wheel, in order: red, yellow, green, cyan, blue,
magenta (the magenta anchor sits a bit past pure blue rather than exactly evenly
spaced). Beats snap to the next anchor in this cycle; between beats the hue slides
smoothly through the intermediate colors. Always fully saturated and full brightness.

## Controls
- **Beat sensitivity** (slider, with its current numeric value displayed): intended to
  set the length of the derivative-averaging buffer — quadratic mapping from a couple
  of samples (twitchy, catches rapid kicks) up to around fifteen (slower to react,
  longer decay, better for sparse bass). NOTE: in the original, the buffer length is
  computed once at startup *before* the control's default is assigned, so it always
  ends up at the minimum and the slider has no real effect. A reimplementation should
  recompute/resize when the control changes — that is the clear intent.
- **Fade speed** (slider, value displayed): multiplies the idle hue-drift rate,
  from a barely perceptible crawl up to several times faster.

## Timing
Idle hue drift: minutes-per-lap scale. Beat response: effectively instantaneous jump,
with a debounce window of a small fraction of a second. The auto-gain and slow average
adapt over tens of seconds.

## Non-obvious bits
- The beat detector is edge-triggered on the *rate of rise* of a fast-smoothed bass
  signal (normalized by recent peak level), not on absolute loudness — this is what
  makes it volume-independent.
- Deriving frame time from a shared wrapping global timer instead of the local delta is
  a deliberate multi-device synchronization tactic.
- The tempo-inference machinery (inter-beat interval history) is present but unused;
  it can be omitted without changing behavior.
