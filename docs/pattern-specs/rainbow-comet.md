# Rainbow Comet
kind: 1D
sensors: no

## What it looks like
A bright, near-white comet head sweeps back and forth along the strip (bouncing at the ends, not wrapping). Behind it trails a tail that is both fading in brightness and shifting through the rainbow — the pixels nearest the head are whitish, then they bloom into the head's color and drift through neighboring hues as they dim out. Because the base color the head stamps also drifts slowly on its own, successive passes lay down different parts of the rainbow. Reads as a classic "rainbow comet" bouncing like a metronome, one full back-and-forth every several seconds at default speed.

## Algorithm
State kept between frames: three per-pixel arrays — a brightness value, a hue, and a saturation for every pixel — plus the head's pixel index from the previous frame.

Per frame (pre-render):
- A slowly cycling base hue is derived from a sawtooth clock with a period on the order of ten seconds.
- The head position is a triangle wave of time (0 → end → 0), scaled to the pixel count and floored to an integer index. The triangle period is set by the speed control (default: several seconds per full bounce).
- Every pixel between last frame's head index and this frame's head index (inclusive, in either direction) gets "stamped": brightness set to full, hue set to the current base hue, saturation set noticeably below full (this is what makes the head whitish). Stamping the whole span, not just the new index, prevents gaps when the head moves more than one pixel per frame. As a guard, the stamp is skipped if the span would cover essentially the entire strip (protects against a bogus huge jump).
- Remember the new head index for next frame.

Per pixel (render) — unusually, this pattern mutates its state inside the per-pixel pass, relying on the renderer calling it exactly once per pixel per frame:
- Output brightness is the stored value squared (gamma-ish curve for a snappier tail).
- Then decay/evolve that pixel's state for next frame: hue decreases by a small fixed step each frame (this is what smears the tail through the rainbow; the step size controls how long the rainbow is), saturation is multiplied slightly upward and capped at full (so the whitish head "cures" into fully saturated color within a fraction of a second), and brightness is multiplied by the fade factor (exponential decay).

Randomness: none.

Layout: resolution-independent (positions are normalized then scaled by pixel count). No fixes needed, though implementers should note the per-frame (not per-second) hue/saturation/fade steps mean trail length varies with frame rate; a delta-scaled equivalent is the obvious improvement.

## Colors
Full rainbow over time. The head is desaturated (near white with a pastel tint of the current base hue); the tail passes through the fully saturated base hue and then walks through adjacent hues as it fades to black. Background is black.

## Controls
- Slider, "speed": how fast the comet sweeps (higher = faster bounce).
- Slider, "fade": trail persistence — higher slider gives a faster fade / shorter tail (it sets the per-frame exponential decay of pixel brightness).

## Timing feel
Default: a full bounce takes several seconds; the tail persists for roughly a second or two; the whole rainbow's base hue drifts around the color wheel in about ten seconds.
