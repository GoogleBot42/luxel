# Pew-Pew-Pew!
kind: 1D
sensors: no

## What it looks like
A volley of laser bolts continuously firing down the strip in one direction,
like blaster fire. Around ten bolts are in flight at once, each in a color
drawn from a hot-pink-through-violet-to-blue palette, each moving at its own
random speed and dragging a rapidly fading trail. When a bolt flies off the
end it instantly re-fires from the start at a new random speed. A faint warm
red ambient glow underlies everything. In its default configuration there is
also a dramatic side effect: every time a bolt passes the very start of the
strip, the *entire* strip flashes blue for an instant ("blue lightning").

## Algorithm
State kept between frames:
- A fixed roster of bolts (ten of them). Each bolt has: a fixed color assigned
  at startup by cycling through the palette (round-robin, so with a bolt count
  that's a multiple of the palette size every color appears equally), a
  fractional position (initialized uniformly at random along the strip), and a
  velocity drawn uniformly from a band spanning roughly a base speed up to a
  bit over double that base.
- A full-strip color buffer holding the composited trail image. In the
  original each entry packs three 8-bit channels into a single number using
  integer and fractional bit positions (a fixed-point packing trick); a
  reimplementation should just use three arrays or per-channel storage —
  the packing is an artifact of the single-number array, not the effect.

Per frame:
1. Fade: every pixel's channels are multiplied by a strong per-frame decay
   (about one-fifth lost per frame) and floored to integers, so trails are
   short and dim components snap fully to black quickly. Note this decay is
   applied per frame, not per unit time, so trail length is frame-rate
   dependent.
2. Advance each bolt by (elapsed time × a small speed constant × its
   velocity). Crucially, the bolt paints *every* integer pixel between its old
   position and its new position — not just the head — so no gaps appear at
   high speed or low frame rate. Painting is additive per channel with
   saturation at the channel maximum.
3. A bolt whose position passes the end of the strip resets to position zero
   and rerolls its random velocity. (Bolt colors never change.)

Per pixel (render):
- Optionally mirror the index so the whole effect runs backward (a code
  constant, not a UI control).
- Output = buffer color plus a constant faint red ambient, normalized and
  clamped.
- Blue lightning quirk (on by default via a code constant): instead of the
  pixel's own blue channel, every pixel reads the blue channel of the *first*
  pixel of the strip. Since every bolt color contains blue and every bolt
  periodically restarts at the first pixel, the whole strip pulses blue in
  unison whenever a bolt launches. With the constant off, blue is read
  per-pixel like the other channels and the bolts simply show their true
  colors.

Randomness: initial positions, and each bolt's velocity at spawn/respawn.

Layout assumptions: purely index-based 1D; speeds are in pixels per unit time,
so on longer strips bolts take proportionally longer to traverse (arguably a
feature). Bolt count is hardcoded; scaling it with strip length is the obvious
generalization. The frame-rate-dependent fade could be made delta-aware.

## Colors
Palette of five fixed stops sweeping from hot pink through magenta and purple
to violet-blue — a neon "laser" gamut. Trails are the same colors fading fast.
Ambient base: very dim warm red across the whole strip. Blue-lightning flashes
read as a full-strip blue wash layered over everything.

## UI controls
None exposed. Behavior toggles live as top-of-file constants intended for
hand-editing: direction flip, bolt count, fade strength, speed factor, and the
blue-lightning toggle.

## Timing feel
Individual bolts cross a typical short strip in a second or two, each at a
noticeably different speed; trails persist only a fraction of a second. The
blue flashes are sudden and brief, at irregular intervals as bolts recycle.

## Non-obvious details
- Painting the full swept range each frame (old position through new) is what
  keeps fast bolts solid instead of dotted.
- The blue-lightning effect is achieved by deliberately mis-indexing the blue
  channel to pixel zero — a one-line quirk, easy to miss, but it defines the
  default look.
- Additive-with-saturation blending makes overlapping bolts brighten toward
  white rather than replacing each other.
