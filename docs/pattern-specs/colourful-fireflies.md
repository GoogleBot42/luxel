# colourful fireflies

kind: 1D
sensors: no

## What it looks like

A swarm of colored fireflies on a dark strip. Each firefly is a bright dot with its own fixed color that darts along the strip, dragging a short fading comet tail, gradually loses speed as if by friction, coasts to a stop, and then instantly respawns somewhere else with a fresh random velocity. Different fireflies move at different speeds and in both directions. The density is proportional to strip length (roughly one firefly per ten pixels). Fast fireflies are bright; as one slows its dot dims, so they fade out as they die. Individual firefly lifetimes are a few seconds to maybe ten-plus seconds feel; trails fade in a fraction of a second to a second.

(This is a community fork of the well-known "fireflies" pattern with per-spark colors added; the author's own comment admits the colors "don't necessarily work well, but it works.")

## Algorithm

State kept between frames:
- Per spark (spark count = one plus about a tenth of the pixel count): a signed **velocity**, a fractional **position** in pixel units, and a fixed **hue** assigned at startup by spreading spark indices evenly around the full hue wheel.
- Per pixel: an accumulated **brightness** value and a **hue stamp** (the hue of the last spark that touched that pixel).

Per frame:
1. Scale the elapsed-time delta down by an order of magnitude (a global speed trim).
2. Decay every pixel's brightness multiplicatively by about ten percent per frame. (Note: this is frame-rate-dependent; a faithful port may keep it, a better port would make decay time-based.)
3. For each spark:
   - If its velocity has decayed to within a small dead-band around zero, respawn it: pick a new velocity uniformly in a symmetric range (up to a modest max speed in either direction) and a new position uniformly along the strip.
   - Multiply velocity by a decay factor very slightly below one (friction — again per-frame, hence frame-rate-dependent).
   - Advance position by velocity times the scaled delta; wrap positions past either end around to the other end.
   - Deposit energy into the pixel under the spark's integer position: **add the signed velocity** to that pixel's brightness, and overwrite that pixel's hue stamp with the spark's hue.

Per pixel render: brightness is the accumulated value squared and then boosted by about an order of magnitude; saturation is fixed just below full (colors are vivid but not razor-pure); hue is the pixel's hue stamp.

Layout: fully 1D, scales with pixel count (spark count derives from it). No hardcoding to fix.

## Colors

Each firefly owns one fixed hue; together they cover the whole rainbow evenly. Background is black. Trails are the firefly's own color fading to black. Near-full saturation throughout.

## Controls

None.

## Non-obvious details

- Sparks moving "backward" deposit **negative** brightness each frame; the render squares brightness, so those show up just as bright as forward movers. The squaring makes the signed-energy trick invisible (whether by design or by luck), and also gives the trails a fast, nonlinear fade.
- Brightness deposited is proportional to speed, so a firefly naturally dims as friction slows it — its death fade is emergent, not scripted.
- The hue stamp is a single value per pixel with last-writer-wins, so when two fireflies cross, the crossing pixel takes one color, not a blend.
- Respawn is triggered by velocity entering the dead-band, not by a timer — the exponential friction guarantees this always eventually happens.
