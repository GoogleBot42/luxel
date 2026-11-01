# Glow Tendrils

Idea spec written from Patternflow preset preset_0520.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A network of luminous, squiggly filaments on black — like bioluminescent plankton, neon veins or a deep-sea creature's nervous system. The filaments are the level lines of a gently domain-warped wave field, so they form loops, chains and branching worms that writhe slowly. Each filament has a soft glow falloff and a whitish, desaturated core, shading out to saturated colour at its edges. Hue sweeps across the panel from left to right (how much of the wheel it spans is a control), wobbles with the warp, and drifts slowly over time, so the network is rainbow-tinted and gradually shifting. Organic and alive; on a 128x64 panel it fills the whole screen with roughly a few dozen tendril loops at the default density.

## The idea

Domain-warp a simple wave field, then draw a thin glowing band around one of its level sets — giving organic glowing filaments. It works because warped iso-lines look biological, and a Gaussian-ish falloff plus white cores sells "glow".

## Algorithm in words

- State: time accumulator from the speed knob.
- Per pixel at density-scaled coordinates: two warp fields, each a product of a sine and a cosine of the coordinates and time.
- Displace the coordinates by the warp fields.
- At the displaced position, take a sine of x and a cosine of y (doubled frequency, each drifting in time) and add them; take the absolute value.
- Brightness = a Gaussian bump centred at a small positive level of that value, with width set by the thickness knob; then boost and clamp. (A larger knob value actually makes the bump narrower — thinner tendrils.)
- Hue = a starting hue + the mutation knob × horizontal position + a little of the warp + slow drift in time.
- Saturation falls as brightness rises, so cores go whitish.

## Knobs

- K1 — density: spatial frequency, sparse big loops to a dense mesh.
- K2 — flow speed.
- K3 — thickness/cohesion (higher = thinner, sharper tendrils).
- K4 — colour mutation: how much of the hue wheel spans the panel width.
- No audio. The device's knob ranges let density and thickness go well beyond what reads well.

## Making it better in Luxel

- Warp with `curl2` (divergence-free) or `simplex3` instead of sine products — tendrils that swirl and fold like real fluid instead of a regular sine lattice.
- Add persistence: render into the persistent frame in `renderBulk` with `fade()` (or use `setPixelState` feedback) so filaments leave faint trails — a bioluminescent wake.
- Colour in OKLCH with a deep-sea palette (cyan, teal, violet, a hint of magenta) and a white-hot core; `setGlow` to bloom the filaments.
- Name the thickness control honestly (line width in pixels) and give density real units.
- Audio option: pulse glow width with `energyAverage`. Strips: 1D level-crossings make pulsing glowing dots — a weak variant. Best as one 2D pattern.

## Payoff

Score: 4/5 — genuinely beautiful organic glow; with curl warp and trails it would be one of the best 2D pieces.
