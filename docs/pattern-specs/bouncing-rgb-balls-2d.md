# Bouncing RGB Balls - 2D
kind: 1D+2D (native 2D; 1D fallback treats the strip as one horizontal line through the middle of the plane)
sensors: no

## What it looks like
Three large soft-edged glowing discs — one red, one green, one blue — drift smoothly around the display, each bouncing back and forth independently in x and y like slow Lissajous figures. Where discs overlap, their colors mix additively: red+green overlap glows yellow, red+blue magenta, green+blue cyan, and all three together approach white. Each disc also slowly breathes, its radius swelling and shrinking. Motion is fluid and never quite repeats, because every axis of every ball moves at its own slightly different rate. Each ball takes a few seconds (roughly one to eight, ball-dependent) to cross the display and come back; the radius breathing is much slower, tens of seconds per cycle.

## Algorithm
State: six motion periods (x and y for each of the three balls) chosen randomly ONCE at pattern startup — this is the only randomness. Each period is drawn uniformly from a moderate range and divided by a global speed constant, so every run of the pattern gets a unique but fixed set of drift rates.

Per frame: for each ball, compute its center position: x and y are each a smooth 0→1→0 wave of the global clock at that axis's startup-chosen period (so balls glide between the display edges and reverse smoothly — "bouncing" is really sinusoidal reflection, not physics). Also compute each ball's radius: a base of roughly a third of the display width plus a smaller oscillating term (peak radius around half the display), each ball breathing at its own fixed period in the tens-of-seconds range, the three periods deliberately different so the breathing never syncs.

Per pixel (2D, with coordinates normalized 0..1): start from black. For each ball, take the Euclidean distance from the pixel to the ball's center. If the pixel is inside the ball's current radius, set that ball's color channel to a falloff of (distance ÷ radius): the red and green balls use a linear falloff (bright center fading straight to zero at the rim), while the blue ball uses one minus the cube of the normalized distance — giving blue a fuller, plateau-like core with a faster drop near its rim. Outside the radius the channel stays zero. Emit the three channels directly as RGB; additive mixing in overlaps falls out for free since each ball owns one channel.

1D fallback: the pattern maps a plain strip to the horizontal line at mid-height (x = position along strip normalized by pixel count, y = middle), so on a strip you see the balls as passing bright blobs when their vertical drift brings them near the centerline.

## Layout assumptions
Uses normalized 2D world coordinates throughout; no pixel-count or grid-size hardcoding. Works on any mapped 2D layout; the 1D fallback works on any length.

## Colors
Strictly the three additive primaries, one per ball, mixing to secondaries and near-white in overlaps, on a black background. Brightness within each disc is its distance falloff.

## Controls
None exposed in the UI. There is an internal speed constant (edit-the-source style) that uniformly scales all six drift rates; a natural improvement is exposing it as a speed slider, and optionally re-rolling the random periods via a trigger.

## Non-obvious bits
- Choosing the six axis periods randomly at startup (rather than fixed) makes each power-up a different choreography, while keeping per-frame work purely deterministic.
- Incommensurate periods for motion and breathing prevent visible looping.
- One-color-channel-per-ball makes overlap blending trivial and cheap — no color-space math needed.
- The source's comments mis-label which ball is blue vs green in a couple of places; behavior as specified above (cubic falloff on the blue channel's ball) is what actually runs.
