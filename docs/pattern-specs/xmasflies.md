# XmasFlies
kind: 1D
sensors: no

A fork of the classic "sparks" pattern, re-tuned so the sparks move slowly, live a long time, and wrap from one end of the strip to the other — the effect reads as colored fireflies drifting along the strip rather than fast shooting sparks.

## What it looks like

Many small points of light (about one spark per five pixels, plus one) crawl along the strip in both directions at gentle, individually-random speeds. Each spark gradually slows down and dims, eventually stalling out and being reborn at a random position with a fresh random speed and direction. Sparks that run off either end reappear at the opposite end. Lit pixels are driven hard into full brightness, with a very short fading tail behind each spark. Colors come from a small fixed set of hues (see quirks — in practice mostly two of them show).

## Algorithm

State between frames:
- Per spark: a signed velocity and a fractional position. Spark count scales with pixel count (about a fifth of it, plus one).
- Four per-pixel energy buffers, one for each of four color groups; sparks are assigned to a group round-robin by spark index (every fourth spark shares a group).

Per frame (pre-render):
1. Scale the frame's elapsed time down by roughly a tenth (this is the "slowed down" part of the fork).
2. Decay every cell of all four energy buffers by a constant factor of about nine-tenths per frame — a fast decay, so trails are short.
3. For each spark:
   - If its velocity is within a small band around zero (it has coasted to a stop — note this band includes freshly-negative near-zero speeds too), respawn it: velocity drawn uniformly from a symmetric range (half the maximum speed in either direction; max speed is a modest fraction of a pixel per scaled time unit), position uniform over the strip.
   - Multiply velocity by a factor just under one (slow exponential slowdown — a spark's speed halves over a few dozen frames).
   - Advance position by velocity × scaled elapsed time.
   - Wrap: position past the top snaps to the bottom, below zero snaps to the top (loop, not bounce).
   - Deposit the spark's **velocity value** (signed!) into its color group's energy buffer at the integer cell under the spark. Faster sparks are brighter; leftward-moving sparks deposit negative energy, which still displays (brightness is squared later) but can partially cancel a rightward spark sharing the cell.

Per pixel (render):
- Read the four group energies at this pixel. The hue is the fixed hue of whichever group has the (signed) maximum here. The four candidate hues are: red, a green, a blue, and an orange-gold — a Christmas-y set. Saturation is full.
- Brightness: the sum of the squares of the **first two** groups' energies only, multiplied by a huge factor (order of a hundred), so any pixel with even slight group-one/group-two energy slams to full brightness.

## Quirks a reimplementer must decide on

- Only the first two color groups contribute to brightness. Pixels whose only energy is from groups three or four get assigned those groups' hues but essentially zero brightness — so in practice the strip shows mostly the red and green families, with blue/orange appearing only where they overlap a lit cell. There is also a per-group squared brightness computed in a variable that is then never used. Both look like leftovers from the fork of the single-color original. A "bug-compatible" port reproduces this (mostly red/green fireflies — arguably the intended Christmas look); a "fixed" version would sum all four groups' squared energies.
- The signed deposit means leftward sparks are as bright as rightward ones (squaring), but collisions between opposite-direction sparks can momentarily dim a cell.
- The huge brightness multiplier means output is effectively binary (full-on speck with a one-or-two-pixel decaying tail) rather than a smooth gradient.
- Because a spark deposits only its raw velocity each frame against the fast buffer decay, a spark visibly dims as it slows, then vanishes just before respawning — the firefly "fade out, reappear elsewhere" feel.

## Layout assumptions

Pure 1D; everything scales from the actual pixel count. Nothing hardcoded.

## Colors

Fixed palette, fully saturated: red, green, blue, orange-gold (in practice dominated by the red and green, per the quirk above). Black background.

## UI controls

None; all tuning is source constants.

## Timing feel

A spark crosses noticeable distance over several seconds; individual sparks live a handful of seconds before stalling and respawning; trails extinguish in a fraction of a second. The overall scene is calm and continuous, like fireflies.
