# sparks
kind: 1D
sensors: no

## Visual behavior
Small white-hot points of light shoot along the strip from the start toward the end, like embers blown down a channel. Each spark starts fast-ish, accelerates while it is energetic, and leaves a short fading orange trail behind it. Dim sparks look deep orange; energetic ones saturate toward white at their core. Many sparks (on the order of a couple dozen) are in flight at once, at staggered phases, so the strip shows a continuous shower. The overall feel is quick and lively — a spark crosses a typical strip in a second or two.

## State kept between frames
- A per-pixel "energy" buffer, one entry per pixel, persisting across frames.
- For each spark: a scalar energy level and a fractional position along the strip.
- The spark count is a fixed small constant in the original (roughly two dozen). Suggested fix: make it a control or scale it with pixel count.

## Per-frame work (before rendering)
1. Decay the whole pixel buffer multiplicatively: only a small fraction (roughly a fifth) of each pixel's value survives each frame. Note this decay is **per frame**, not time-based, so trail length varies with frame rate — a faithful port may want to convert it to a time-based decay.
2. The frame time delta is scaled down by about an order of magnitude before use, which sets the overall speed feel.
3. For each spark:
   - If its energy has reached zero or below, respawn it: energy is set to a random value a bit above unity (unity plus up to a modest random fraction), and position is set to a random spot within the first few pixels of the strip.
   - Reduce energy by a friction term proportional to the time delta and **inversely proportional to the strip's pixel count** — this makes sparks travel roughly the full strip length regardless of strip size (a nice built-in layout adaptation).
   - Advance position by an amount proportional to the **square of the energy** times the time delta — energetic sparks move fast, and motion slows as friction drains them.
   - If the position passes the end of the strip, reset position to zero and zero the energy (it will respawn next frame).
   - Deposit the spark's current energy **additively** into the pixel buffer at the spark's position (the fractional position is truncated to an integer index).

## Per-pixel render
Read the pixel's buffer value. Brightness is that value squared (gives a punchy gamma). Saturation starts slightly above full and has the squared value subtracted, so hot pixels desaturate toward white while faint trails stay fully saturated. Hue is a fixed warm orange/ember tone, near the red end.

## Randomness
Used only for respawn: initial energy jitter and initial position within the strip's first few pixels.

## Non-obvious points
- Squared-energy velocity plus linear friction produces a convincing "flare then die" motion profile.
- Additive deposit into a decaying buffer is what creates the trails; the spark itself is just the brightest, freshest deposit.
- Saturation-minus-brightness coupling gives white-hot cores with orange tails from a single fixed hue.

## Controls
None.
