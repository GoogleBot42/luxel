# heatshivers
kind: 1D
sensors: no

## What it looks like
Warm amber pulses of light bloom at random places along the strip, swell up, drift sideways while trembling slightly, and fade out — leaving behind a pure-red "heat" afterglow that decays over a couple of seconds wherever a pulse has been. Two families of pulses move in opposite directions (one family drifts leftward, the other rightward), so the strip feels like embers sliding past each other. The per-frame positional tremble is tiny but constant, giving the glow a nervous, shivering quality — hence the name. Overall: organic, fire-like, asynchronous; never two frames alike.

(Provenance note: the source is machine-generated from a small declarative pulse-composition toolkit; the spec below describes the resulting behavior, which is what should be reimplemented.)

## Algorithm
### Pulse generators (two independent ones)
Each generator maintains a small pool of pulse slots (a handful, four-ish) and:
- Spawning: whenever the current time passes a "next spawn" deadline and a slot is free, start a new pulse and schedule the next spawn roughly a second later (uniformly randomized around one second). So each generator emits about one pulse per second.
- Each pulse stores its birth time and a base position drawn uniformly from most of the strip: one generator spawns anywhere in the left roughly-four-fifths and drifts rightward; the other spawns in the right roughly-four-fifths and drifts leftward.
- Lifetime: a couple of seconds. A pulse's intensity envelope over its life is a triangle: linear rise to a peak at mid-life, linear fall to zero, then the slot is freed.
- Position each frame = base position + linear drift proportional to age (covering a substantial fraction of the strip, roughly two-fifths, over the full lifetime; sign differs per generator) + a fresh tiny random jitter each frame. The jitter is approximately Gaussian (implemented as the centered sum of a few uniform randoms, normalized) with a standard deviation around half a percent of the strip — this per-frame re-roll is the "shiver".
- Spatial profile: a triangular bump centered on the pulse position, total width around a sixth of the strip, clipped at the strip ends. Per pixel inside the bump, contribution = (time envelope) x (spatial triangle), and overlapping pulses add.
- Each generator renders its live pulses into its own full-strip intensity buffer, cleared every frame.

### Afterglow
A third full-strip buffer holds the "heat": each frame every element decays exponentially with a half-life of about a second and a half (computed from the real frame delta, so it is frame-rate independent), but is floored at the instantaneous maximum of the two pulse buffers at that pixel. Net effect: the afterglow instantly rises to match any pulse passing through and then dies away slowly after the pulse leaves.

### Compositing and output
Per pixel, the three fields are mixed into RGB:
- both pulse fields contribute a warm amber (full red channel, green at roughly four-fifths, no blue),
- the afterglow contributes pure red only.
The sums are computed once per frame into per-channel buffers; the per-pixel render just squares each channel (simple gamma shaping — deepens the tails, makes cores pop) and outputs RGB. Values can exceed full scale where pulses overlap; they clip at white-hot amber.

### State kept between frames
Per generator: slot occupancy flags, per-slot birth times and base positions, next-spawn deadline. Plus the two pulse buffers, the afterglow buffer, the three channel buffers, and a running clock accumulated from frame deltas.

## Colors
Two-layer fire palette, qualitative stops: black → deep red (afterglow) → amber/orange-gold (active pulse) → near-white amber where pulses stack. No blues ever.

## Controls
None exported.

## Timing
- New pulse roughly every second per generator (two generators).
- Each pulse lives a couple of seconds with a symmetric rise/fall.
- Afterglow half-life about a second and a half; a visited spot fades to nothing in several seconds.
- Pulse drift is slow and smooth; the shiver is per-frame and subliminal-fast.

## Notes / quirks
- The generated slot-management code has bugs worth knowing about: a shared alive-counter is never actually maintained (it increments an unrelated stray global), and both the spawn scan and the update loop stop at the first *empty* slot rather than skipping it. Consequence: only a contiguous run of live slots from the start of the pool is ever updated/drawn, so the effective simultaneous-pulse cap is usually lower than the nominal pool size, and a pulse can occasionally freeze un-drawn until earlier slots refill. At the actual spawn rate (~1/s) versus lifetime (~2 s) this rarely matters — about two pulses per generator are alive at once and they mostly stay contiguous. A reimplementation should just do correct slot handling (skip dead slots, real live-count); the visual difference is negligible and only for the better.
- The per-frame jitter re-roll (rather than a persistent random-walk) is essential to the shimmering character; smoothing it out would kill the effect.
- The floor-then-decay afterglow (decay toward zero but never below the current pulse maximum) is the clever bit that produces trails behind moving pulses without any explicit trail bookkeeping.
