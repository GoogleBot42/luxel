# 4th
kind: 1D
sensors: no

## What it looks like
A Fourth-of-July strip effect. The background is a static, dim repeating stripe
sequence of red, white (gray), and blue blocks. Over that background, a couple
of bright warm-orange "rockets" streak along the strip in random directions,
gradually slowing under friction. As a rocket loses nearly all its speed it
"dies": a small crackling burst of stochastic pure-white glitter spreads a few
pixels outward from its resting point and slowly fades. Then that rocket
relaunches from a fresh random position in a random direction. Each rocket's
flight lasts on the order of a few seconds; the dying crackle lingers a moment
after.

## Algorithm
State kept between frames:
- Two "rocket" entities (a fixed small count — exactly two). Each has a signed
  energy/velocity value and a fractional position on the strip.
- Two full-strip float buffers:
  - a "rocket trail" buffer that decays quickly (roughly 10% loss per frame), and
  - a "crackle" buffer that decays noticeably slower (a couple percent per frame),
    so death-bursts outlast rocket trails.

Per frame (before rendering):
1. Multiply every element of both buffers by their respective decay factors.
2. For each rocket:
   - If its energy magnitude has fallen essentially to zero, respawn it: give it
     a fresh energy magnitude drawn uniformly from a moderate band (roughly a
     quarter-spread above a fixed minimum), flip its sign with 50% probability
     so about half the launches travel leftward, and place it at a uniformly
     random pixel.
   - Apply friction: subtract a small constant amount per unit time from the
     energy magnitude, preserving its sign. The friction constant is inversely
     proportional to the pixel count (halved again on the theory that each
     rocket covers about half the strip), so flight length scales with strip
     length.
   - Advance position by energy × elapsed time (the frame delta is first scaled
     down by an order of magnitude), wrapping around both ends of the strip.
   - Deposit into the buffers depending on the energy magnitude:
     - While the magnitude is above a small threshold ("still flying"): set the
       rocket-trail buffer at the current pixel to full intensity.
     - While the magnitude is between that threshold and essentially zero
       ("dying"): compute two mirror positions on either side of the current
       position, offset by an amount that grows from zero up to a handful of
       pixels as the energy decays toward zero, and write a random value
       (uniform, ranging up to about double full intensity) into the crackle
       buffer at each mirror position. This produces a widening pair of
       sparking points.

Per pixel (render):
1. Background stripe: divide the pixel index by (stripe-spacing × 3), floor it,
   take it modulo three, and use that to pick one of three fixed base colors:
   a dim red, a dim neutral gray-white, and a dim blue. Each color therefore
   occupies a contiguous block, and the three colors repeat down the strip.
2. Add the rocket-trail buffer value to the red channel, and one third of it to
   the green channel — flying rockets and their fading trails read as bright
   orange over the background.
3. Crackle: compare the crackle buffer value against a fresh uniform random
   number each frame; if the buffer value wins, override the pixel to full
   white. Because buffer values can start above one and decay, this yields a
   flickering, sputtering white sparkle whose flicker probability fades over
   time — much livelier than a smooth fade.

Randomness: launch energy, launch direction sign, launch position, crackle
deposit intensities, and the per-pixel-per-frame sparkle comparison.

Layout assumptions: fully 1D, index-based. The dying-burst spread is a fixed
handful of pixels regardless of strip length; on very long/dense strips it will
look tiny (obvious fix: scale the spread with pixel count). The rocket count is
hardcoded at two; scaling it with strip length would suit large installs.

## Colors
- Background palette (three stops, block-striped, all quite dim): muted red /
  muted neutral white-gray / muted blue.
- Rocket trails: bright warm orange (strong red plus a minor green component)
  added over the background.
- Death crackle: pure full white flashes.

## UI controls
- One slider, "spacing": widens the background stripes. It maps to an integer
  block-width multiplier from one up to about six, so at minimum the colors
  alternate in narrow bands and at maximum in wide bands.

## Timing feel
Rocket flights decelerate over a few seconds; trails vanish within a fraction
of a second behind the head; the white crackle takes a second or two to fizzle
out. Motion speed is moderate — visibly fast at launch, gliding to a stop.

## Non-obvious details
- The stochastic render-time comparison (buffer value vs. fresh random number)
  is what makes the death burst crackle instead of merely fade.
- Two separate decay buffers with different half-lives cleanly separate the
  fast comet trail from the lingering glitter.
- A frame-time-based value from the global time helper is computed each frame
  but never used; omit it.
