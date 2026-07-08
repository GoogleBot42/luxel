# A Peak Integrator
kind: 1D
sensors: yes

## What it looks like
The strip is divided into a small fixed number of equal segments (eight), one per coarse frequency band, low frequencies at the start of the strip. The strip is dark at rest. When a beat/peak is detected in a band, that band's segment flashes on and then holds/decays over time — the flash duration is "integrated" energy: each detected pulse adds on-time proportional to the pulse's duration and to a user slider, so sustained or repeated hits keep a segment lit longer. Color runs as a continuous rainbow gradient along the whole strip (red at the start through blue/violet near the end); a peak's strength (relative to the loudest peak ever seen in that band) sets both how saturated and how bright the segment appears. Overall brightness is deliberately modest (value is scaled down to roughly a third of the strength), so it reads as colored pulses rather than a blinding VU meter.

This is primarily a beat-detection engine with a minimal debug-style visualization bolted on.

## Sensor inputs
- A 32-band audio frequency spectrum array (from the sensor expansion board). This is the only sensor input.

## Algorithm
Per frame (throttled — the analysis only runs a few tens of milliseconds apart, not every frame):

1. A free-running millisecond clock is accumulated from frame deltas and wraps at a large power-of-two value; all timestamps use wrap-aware subtraction.
2. The 32-band spectrum is intended to be collapsed into eight coarse bands (four raw bands averaged per coarse band). Each raw value is compressed with a logarithm of (one plus a large multiple of the value) so quiet signals are boosted and loud ones compress; only rising energy (value above the previous sample) is meant to contribute. The loudest coarse band and its energy are tracked.
3. Per coarse band, two circular-buffer moving averages are maintained over the compressed energy: a long window (a few dozen samples, ~1–2 s of history) and a short window (roughly a fifth as long, a fraction of a second). Running totals are updated incrementally (subtract the outgoing sample, add the incoming one).
4. Peak state machine per band: a circular buffer of recent peak magnitudes with their center timestamps. If the current energy exceeds the stored peak slot, the slot is updated, the band enters a "peaking" state (recording the trigger time), the expected peak period is stretched, and the peak magnitude is remembered. Once enough time has passed (proportional to the band's adaptive peak period) and the energy has fallen back to only modestly above the short-term average, the peak is declared over and committed; a watchdog also force-ends a peak that lasts more than a fraction of a second so it can't wedge.
5. Committing a peak ("adding a pulse width"): the time span from trigger to now is written into a per-band history array covering roughly the last few seconds at a resolution of a few hundredths of a second per slot (zero-filling the gap since the previous pulse, then filling the pulse's span with its magnitude). It also tracks the all-time max magnitude per band, then: adds on-time to that band's LED timer (pulse span × integration slider × a large gain), and sets the band's LED strength to magnitude ÷ that band's all-time max.

Per pixel (render): figure out which band segment the pixel belongs to; if that band's LED timer is zero, output black. Otherwise decrement the timer by the frame time (note: this decrement happens per-pixel inside render, so it actually drains segment-count times faster than intended — see quirks) and output the rainbow hue for this position with saturation = band strength and value = band strength scaled down by about a third.

Analysis doesn't begin producing peaks until the long history has partially filled, and it bails out entirely when the lowest band's short average is nearly silent (auto-mute in quiet rooms).

## Layout assumptions / hardcoding
- Segment width is hardcoded assuming a strip of about a hundred and a half pixels (a specific fixed count divided by the band count). Obvious fix: derive pixels-per-segment from the actual pixel count.
- 1D only; no mapped rendering.

## Controls
- One slider, "integration time" concept: scales how much lit time each detected pulse contributes — low = short blips per beat, high = long sustained glows that accumulate.
- There is also an internal debug flag (not exposed as a UI control) that freezes analysis when set.

## Quirks / faithfulness notes (the source is visibly buggy)
An implementer should know the original has several apparent bugs; decide whether to reproduce or fix them:
- The band-aggregation inner loop indexes the spectrum by the coarse-band number instead of the raw-band loop variable, so each coarse band just samples one low spectrum band four times — effectively only the lowest eight of the 32 bands drive anything, and low bands dominate.
- The "previous sample" used for the rising-edge test is overwritten with the raw value before comparison against the log-compressed value, so the rising test almost always passes.
- Several helper writes use a stale loop variable from the outer scope instead of the passed band index (the zero-fill index advance and the LED time/strength accumulation), so some state meant to be per-band lands on the wrong band.
- The per-pixel render mutates the pixel index while computing an unused history lookup, and decrements the band timer once per pixel rather than once per frame.
- A "sample delta" fed to the analyzer lags one frame behind the actual delta (previous frame's delta is used).
A clean reimplementation that fixes these will look better than the original; a strict clone should keep the effective behavior (mostly bass-driven flashes, timers draining fast).
