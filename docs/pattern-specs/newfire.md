# Newfire
kind: 1D
sensors: no

## Visual behavior
A realistic one-dimensional fire for strips and strings, in the style of the classic "Doom fire" algorithm collapsed to 1D. A hot base (white-hot core if allowed) feeds flames that lick along the strip, cooling and flickering as they travel, fading through the flame color into darkness at the tips. Random sparks flare and dark sputters interrupt the column, so it reads as organic fire rather than a smooth gradient. Flame color is user-pickable (default warm fire); flame height, base heat, and sputter rate are adjustable. Motion feels like real fire — flicker on the order of tens of milliseconds per simulation step.

## State kept between frames
- A **heat array one element longer than the strip**: element zero is the constant heat source that drives everything (default fully hot; adjustable), and elements one through pixel-count hold the per-cell temperature in the unit range.
- A timer that runs the simulation at a **fixed tick of a few dozen milliseconds** (roughly 25 steps per second), independent of the render frame rate — the strip re-renders faster, but heat only propagates on ticks.

## Per-tick simulation
1. **Advection with jitter**: iterate from the far (cool) end down toward the source. Each cell takes the heat of a cell a randomized **one-to-two positions closer to the source** (clamped at the source), minus a random cooling amount drawn between zero and the cooling parameter; result floored at zero. Sampling at a randomized distance instead of always the adjacent cell replaces 2D Doom fire's convolution-plus-wind and makes the flame less predictable.
2. **Sparks and sputters**: with probability equal to the variability parameter per tick, pick a random cell within roughly the **first eighth of the strip** (near the base) and add a random amount that is *centered above zero but can be negative* (drawn from a range about three times as wide above zero as below), so the event is usually a bright spark but sometimes a dark spot. The result is capped at the source heat (or a moderate floor slightly above half, whichever is greater). Turning variability up gives a sputtering-burner effect.

## Per-pixel render
1. Map the pixel index (offset by one so index zero reads the first simulated cell, not the source itself) through the selected **layout mode** into the heat array:
   - Mode A: flame rises from the start of the strip to the end (normal).
   - Mode B: reversed, base at the far end.
   - Mode C: mirrored, base at the **center**, two flames radiating outward.
   - Mode D: mirrored, bases at **both ends**, flames meeting in the middle.
   (In the mirrored modes both halves read the same half-length portion of the heat field.)
2. Cube the heat value (gamma correction).
3. Color: hue = picked hue plus a small upward shift proportional to the (cubed) heat, so the hottest parts lean slightly toward the next hue over; saturation = picked saturation minus the cubed heat, so the hottest cells desaturate to white; brightness = picked brightness times the cubed heat.

## Controls
- **Color picker (HSV)**: hue sets the flame color; saturation is deliberately repurposed — it is scaled up (nearly doubled internally, allowing over-saturation) and controls **how much white-hot core appears at the base** (lower picked saturation = more white heat); value sets overall brightness.
- **Flame height (slider)**: inversely mapped to the cooling parameter across a wide range (small cooling = tall flames filling the strip; large = short stubby flames). Response is a linear blend between a small and a moderately large cooling value.
- **Heat (slider)**: sets the source cell's temperature, from a bit under half up to fully hot.
- **Sparks (slider)**: sets the per-tick sputter probability, from zero up to about one-half.
- **Mode (slider)**: quantized to the four layout modes.

## Layout notes
Fully pixel-count-agnostic: array sizes, spark zone, and midpoint all derive from the strip length. The mirrored modes assume the "center" is the halfway index.

## Non-obvious points
- Fixed-tick simulation is essential: without it the fire's apparent speed would depend on frame rate.
- The single hottest trick is the **saturation headroom**: internally allowing saturation above full means only heat values near maximum can pull it down below full and show white — so the white core automatically appears only at the very base, and the picker's saturation sets how much headroom there is.
- Iterating from the cool end toward the source lets the update run in place on one array without a second buffer.
- The original's mirrored-mode index expressions contain a precedence quirk that reduces to a plain conditional; implement the described mirror intent directly.
