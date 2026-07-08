# Custom Sequences
kind: 1D
sensors: no

## What it looks like
A configurable repeating color sequence, designed for spaced bulbs/strands on house eaves rather than dense strips. The user picks up to a dozen colors; the strip is divided into equal-length runs of pixels, each run showing the next color in the sequence, repeating forever down the strip. The whole sequence can chase left or right at anything from a glacial crawl to a fast march, or hold still. Optional dressings: adjacent colors can blend smoothly into each other instead of hard-stepping; each run can fade out toward its trailing edge (comet-style); random individual pixels can sparkle up toward white or wink off momentarily; and the entire strip can give a brief blackout blink every so often. Default look: alternating runs of red and blue.

## Algorithm
Layout: 1D, fully pixel-count aware (run length limits, twinkle rates, and the random table all scale with strip length). The author warns that stacking many effects on very long strips can drop the frame rate.

State kept between frames:
- A master animation phase in the unit interval, advanced each frame by the frame delta times the signed chase speed (a small divisor keeps it slow); it wraps around. From it derive a signed pixel offset equal to the phase times the run length times the number of active colors — so one full phase wrap slides the pattern by exactly one full sequence, making the wrap seamless.
- A fixed table of one uniform random number per pixel, generated once at startup. These serve as stable per-pixel phase offsets for the twinkle effects.

Per pixel:
1. Sequence lookup: add the animation offset to the pixel index, divide by the run length, floor, and take it modulo the active color count to get this pixel's color slot; the following slot is the "next" color. All modulo operations here must be floored/sign-correct modulo (result takes the divisor's sign), or negative offsets (reverse chase) break the sequence — the original ships a helper for exactly this on older hardware.
2. Color: if smoothing is off, use the slot color directly. Otherwise compute the pixel's fractional position within its run and ease it, then linearly blend from the slot color to the next color by the eased fraction:
   - Normally an ease-in-out curve whose steepness comes from the smoothing slider (from near-instant step at one end to fully linear blend at the other), implemented as a signed odd-power curve mirrored about the midpoint.
   - When fade-out is active, use an ease-in (one-sided) curve instead, so blending doesn't fight the fade.
   - Special case: when fade-out is active and either the current or next color is essentially black, skip blending entirely and use the slot color — otherwise the fade and the blend-to-black double up and look wrong.
3. Fade-out: if enabled, compute a per-run sawtooth from the pixel's position within its run — full brightness at the leading edge falling toward the trailing edge, with the "leading" side chosen by the chase direction slider's side — then cube it for a perceptually pleasing tail. Amount slider scales how deep the tail falls.
4. Twinkles (applied only to pixels whose blended color, after fade, is reasonably bright — dark pixels never twinkle):
   - Twinkle-white: index into the random table (offset by the animation offset so twinkles ride along with the chase) to get this pixel's phase. A pulse function produces a resting value of about a quarter for most of a long cycle, with a brief smooth spike; the spike is allowed to overshoot past full, and any overshoot is converted into an additive white boost while the base multiplier wraps — so a twinkling pixel both brightens and whitens at its peak. The slider shortens the cycle (more frequent twinkles) as it increases; cycle length also scales with pixel count so density-per-strip feels constant.
   - Twinkle-off: same idea with a second, decorrelated index into the random table (offset by roughly half the strip so the two twinkle types don't coincide), but inverted: a longer pulse that multiplies brightness down toward zero, making pixels briefly wink out. Its slider has a very aggressive response (double square root), so even small values wink noticeably.
5. Blink: if enabled, on a period from a couple of seconds up to about a minute, the whole strip gates through a very fast square-wave strobe for a brief fraction of the period — a quick global flicker/blackout.
6. Output: the blended color times (fade cubed, times both twinkle multipliers, times the blink gate), plus the additive white boost on each channel.

Randomness: only the startup-time random table; all runtime animation is deterministic, so twinkles per pixel repeat on their cycle rather than being freshly random (visually indistinguishable, much cheaper).

## Colors
Entirely user-chosen: up to a dozen arbitrary picked colors used in order, repeating. Defaults to red and blue alternating. Twinkle-white pushes pixels toward white at spike peaks. Black is a legitimate sequence color (gaps), with the blending special-case above to keep gaps crisp.

## Controls
- A dozen color pickers ("Color 1" … "Color 12") — the sequence entries, used in order.
- Slider "number of colors used" — how many of the pickers participate (one up to all twelve).
- Slider "color length" — pixels per run, from a single pixel up to several times the strip length (cubic slider response, so fine control at short lengths).
- Slider "chase speed" — bidirectional: centered is stopped (a deliberately wide dead zone so "off" is easy to hit); pushing either way chases in that direction, with a strongly nonlinear (roughly fifth-power) response so very slow creeps are reachable.
- Slider "fade out" — depth of the per-run trailing fade (squared response).
- Slider "color smoothing" — hard steps at one end through progressively softer eases to fully linear cross-blend at the other.
- Slider "twinkle white" — density/frequency of white sparkle twinkles.
- Slider "twinkle off" — density/frequency of momentary wink-outs.
- Slider "blink" — enables and speeds up the periodic whole-strip blink (from about once a minute up to every couple of seconds).

## Non-obvious notes
- Everything hinges on sign-correct floored modulo; a truncating remainder breaks reverse chase and the wrap seam.
- Indexing the random table by (index plus animation offset) makes twinkle assignments travel with the pattern instead of sitting still while colors slide underneath — a subtle but important cohesion trick.
- The twinkle pulse's "overshoot past full becomes added white" trick gets a hot-white peak out of a single scalar multiplier without a separate white channel.
- The brightness gate ("only twinkle bright pixels") keeps black/gap runs clean.
