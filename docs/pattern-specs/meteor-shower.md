# Meteor Shower
kind: 1D
sensors: no

## What it looks like
Meteors streak continuously along the strip in one direction, fast — the head travels tens of pixels per second. Each meteor has a bright, whitish-pastel head and a tail that both dims and *gains* color saturation as it trails off, so the tail reads as a colored streak fading into black. Meteor lengths vary randomly (typically a dozen or two pixels, occasionally much longer or back-to-back). Meteor hues drift gradually around the rainbow over roughly half a minute, and consecutive meteors have related hues rather than jumping randomly.

## Algorithm
State between frames:
- Three per-pixel arrays sized to the pixel count, holding hue, saturation, and brightness for every cell — a ring buffer of the trail history.
- A write-head index into that ring.
- A millisecond accumulator used as a frame-rate limiter.
- A slow clock (sawtooth cycling on the order of half a minute) that steers meteor hue.

Per frame: accumulate delta and bail out until a small fixed interval (a few hundredths of a second) has passed; this fixes the animation speed regardless of render frame rate, giving on the order of fifty steps per second. On each step:
1. Advance the write head one cell (wrapping at the pixel count).
2. Decide whether this cell starts a new meteor head or continues a tail:
   - Start a new head if the previous cell's brightness has decayed to near-black, OR with a small random chance each step (order of one in fifteen) — the random early-spawn is what makes meteor lengths irregular and sometimes overlapping.
   - New head: full brightness; saturation around half (pastel/white-hot look); hue set to the average of the previous cell's wrapped hue and the slow clock — the averaging low-pass-filters hue so successive meteors are neighbors on the wheel while still following the slow rainbow drift.
   - Tail continuation: copy from the previous cell with brightness multiplied by a factor a bit under one (about a tenth lost per step), hue nudged slightly downward, and saturation multiplied by a factor noticeably above one (it climbs quickly and effectively clamps at full). So the tail transitions whitish → vividly colored → dark.
3. Only the one cell under the write head is written per step; nothing else is touched.

Per pixel at render time: read the ring buffer at (pixel index plus write-head index, modulo pixel count) and emit that HSV. Because the read offset advances every step, the whole buffer appears to scroll along the strip — this is how meteors move without ever shifting array contents.

## Layout assumptions
1D; arrays are sized from the pixel count at startup, so it adapts to any strip length with no changes. Very long strips just show more simultaneous meteors.

## Randomness
A single uniform random draw per step gives the small probability of spawning a new head before the previous tail has fully faded.

## Colors
Heads: bright, half-saturated (pastel, near-white) in a hue that circles the full rainbow over tens of seconds. Tails: same hue family sliding slightly, rapidly saturating to vivid, decaying geometrically to black. Background: black.

## UI controls
None. Natural additions: sliders for step interval (speed), decay factor (tail length), and spawn probability (meteor density).

## Timing feel
Fast: heads move on the order of fifty pixels per second; individual tails persist under a second at any given spot; hue palette evolves over roughly half a minute.

## Clever bits
- Ring buffer plus render-time rotation: one array write per step instead of shifting the whole trail — cheap and scroll-perfect.
- Delta-accumulator step gate decouples animation speed from frame rate.
- Saturation *increasing* down the tail while brightness decays is what sells the "white-hot head, colored tail" meteor look.
- Hue averaging between the previous cell and a slow clock gives coherent, slowly-evolving meteor colors.
