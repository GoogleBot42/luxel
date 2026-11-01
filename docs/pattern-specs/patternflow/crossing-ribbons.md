# Crossing Ribbons

Idea spec written from Patternflow preset preset_0515.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

Two sets of vertical-ish wavy ribbons drift across the panel in opposite directions — one set travelling right with a fairly fine spacing, the other travelling left with wider spacing. Each set is a sine wave across x that wiggles sideways as it goes down the screen, then hard-thresholded into solid ribbons with crisp edges. The threshold of each set breathes in and out over time, so the ribbons fatten and thin rhythmically. Where ribbons of the two sets cross, the overlap lights in the complementary hue, giving a woven, interference look. In the device version only the first set and the overlaps are actually visible (the second set alone renders black), so on hardware it reads as base-hue ribbons with complementary-coloured knots where an invisible second set passes — a flat two-colour graphic, crisp but a bit thin.

## The idea

Two thresholded sine gratings moving in opposite directions, composited so that their overlap is a third colour — a moiré/woven-ribbon effect. It works because hard-edged overlapping gratings produce shifting diamond patterns that the eye reads as weaving.

## Algorithm in words

- State: time accumulator (base rate + speed knob).
- Layer A: sine of x (scrolling right) plus a sideways wiggle that is a sine of y and time; normalize to 0..1 and compare to a threshold that oscillates gently around the middle → on/off mask.
- Layer B: similar with a cosine, coarser spacing, scrolling left, offset horizontally by the layer-shift knob, with its own differently-timed threshold.
- Colour: both on → complementary hue at full; only A → base hue; only B → a nearby hue at reduced intensity (which the device's integer brightness rounds to black); neither → black.
- Saturation is fixed and fairly high.

## Knobs

- K1 — speed of drift and threshold breathing.
- K2 — intended as band width, but the device version never uses it.
- K3 — layer shift: horizontal offset of the second grating relative to the first (shifts where the knots fall).
- K4 — base hue.
- No audio.

## Making it better in Luxel

- Show all three regions as intended, with OKLCH colours chosen so layer A, layer B and the overlap are equally bright but clearly different (e.g. cyan, magenta, and a light violet overlap); this is effectively a two-layer **scene** with an "add" or "difference" blend — each grating a tiny pattern, the overlap colour coming from the blend mode.
- Let the gratings rotate relative to each other (one slowly turning) — the moiré between two rotating gratings is far more striking than parallel ones.
- Soft edges via `smoothstep` with a narrow width prevent stair-stepping on slanted ribbons; use `simplex2` for the sideways wiggle so it looks hand-drawn rather than sinusoidal.
- Wire the dead width control to the threshold offset (duty cycle) of both layers.
- Strips: works well in 1D — two dashed patterns scrolling opposite ways with a third colour on overlap is a nice cheap strip pattern.

## Payoff

Score: 2/5 — a classic moiré idea that is only half-delivered on the device; worth redoing as a rotating two-grating scene.
