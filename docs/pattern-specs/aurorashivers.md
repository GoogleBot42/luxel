# aurorashivers
kind: 1D
sensors: no

## What it looks like
Soft aurora-like curtains on black. Two families of glowing blobs, each blob roughly a sixth of the strip wide, bloom and fade over a couple of seconds while drifting slowly along the strip — one family tinted violet drifting one direction, the other tinted aqua/cyan drifting the opposite direction. Each blob also quivers with a tiny per-frame positional jitter (the "shivers"). Wherever blobs have recently been, a pure-blue afterglow lingers and decays over a few seconds, so the strip carries ghostly blue trails of past activity. New blobs appear about once a second per family, so several are usually alive at once and the two color families continually cross through each other.

## Algorithm
This is generated code from a declarative pulse-compositor; describe it as three layers summed per pixel.

### Pulse layer (two independent instances)
Each instance maintains a small fixed pool of pulse slots (about four). Per-slot state kept between frames: alive flag, birth time, and a base position drawn uniformly at spawn. Instance-level state: the next allowed spawn time. A running clock in seconds (accumulated from frame deltas) drives everything.

Per frame, per instance:
- Spawning: if the clock has passed the next-spawn time and a slot is free, activate a slot, draw its base position uniformly at random, record its birth time, and schedule the next spawn a random interval later — uniformly about one second, give or take a fifth.
- For each live pulse: compute its age; if it exceeds the fixed lifetime (a couple of seconds) the pulse dies. Otherwise:
  - Temporal envelope: a triangle wave over the normalized lifetime — linear rise to full at mid-life, linear fall to zero at death.
  - Position: base position, plus a linear drift proportional to age (total drift over a full lifetime is about a fifth of the strip; one instance drifts positive, the other negative), plus a small zero-mean roughly-gaussian jitter re-drawn every frame (built from a few summed uniform draws). The jitter amplitude is well under one percent of the strip — enough to shimmer, not to wander.
  - Spatial envelope: a triangle-shaped bump centered on that position, with total width roughly fifteen percent of the strip; intensity contribution = temporal envelope × spatial envelope, added into that instance's per-pixel scratch buffer (overlapping pulses of the same instance sum).
- The two instances differ only in their spawn-position range and drift direction: one spawns in the lower four-fifths of the strip and drifts toward the far end; the other spawns in the upper four-fifths and drifts toward the start. So the drift keeps each blob mostly on-strip.

### Afterglow layer
A per-pixel persistent buffer updated each frame as: max( its own value decayed exponentially with a half-life of a couple of seconds , the larger of the two pulse buffers at that pixel ). I.e. it envelopes the peak activity and lets it die away slowly. Important ordering: the decay/refresh uses the pulse buffers from the previous frame (it runs before the pulse buffers are rebuilt).

### Color composition
Per pixel, the three scalar fields are mixed into RGB with fixed tints:
- one pulse instance contributes a violet (strong blue, moderate red, no green);
- the other contributes an aqua/cyan (full blue, strong green, no red);
- the afterglow contributes pure blue.
Channels sum (can exceed nominal range where layers overlap; clipping is acceptable).

Per pixel (render): read the three summed channels and square each before output — a gamma-like curve that deepens the fades and makes the triangle envelopes look smooth rather than linear.

## State summary
Between frames: the clock, per-slot pulse state for both instances, the afterglow buffer, plus scratch buffers (two pulse-intensity fields and three color-channel fields) that are rebuilt each frame — kept as buffers only so render stays a cheap lookup.

## Randomness
Uniform draws for spawn intervals and base positions; an approximate gaussian (sum of a few uniforms, recentered) for the per-frame shiver.

## Colors (qualitative)
Black background; violet and aqua drifting blooms; pure-blue lingering afterglow. Everything cool-toned — no reds or warm hues except the red component inside the violet.

## Layout assumptions
All positions are normalized fractions of the strip; scales to any pixel count.

## Controls
None exported.

## Timing
New blob roughly every second per family; each blob lives a couple of seconds; afterglow half-life a couple of seconds, so trails visibly persist for several seconds.

## Notes
- The signature trick is the afterglow: taking a per-pixel running max of pulse activity with exponential decay yields organic trailing light without storing any pulse history.
- The machine-generated original tracks its live-pulse count with a buggy counter and cuts slot scans short at the first dead slot; neither matters much in practice because pulses have a fixed lifetime and die in birth order, but a clean reimplementation should just scan all slots and count properly.
- Squaring each color channel at output is load-bearing for the look; without it the triangle envelopes read as harsh linear ramps.
