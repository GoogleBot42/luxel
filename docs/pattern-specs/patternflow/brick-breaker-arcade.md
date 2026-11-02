# Brick Breaker Arcade

Idea spec written from Patternflow preset preset_0712_2.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT). The preset's display name, "Breakout Arcade", carries an arcade trademark, so the name here is generic. Prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A self-playing brick-breaking game, composed for a portrait (tall) playfield and turned a quarter turn onto the landscape 128x64 panel, so on the wall the bricks are at one short end and the paddle at the other. A wall of bricks — a handful of rows, a column count that fills the width — sits near the far end; each brick is a filled rectangle with a darker one-pixel outline, and the rows are shaded from pale peach-white at the back row through warm orange toward the front. Faint grey-violet side walls frame the field, and the background has a barely visible alternating line texture. A small plus-shaped ball bounces around; the paddle is a short bright bar near the near end.

The fun is in the juice. Every brick hit leaves a soft glowing blob that fades over a moment, the ball leaves a faint comet trail, and the whole background flashes brighter on hits — more so when consecutive hits build a combo without touching the paddle. Some broken bricks drop a blinking power-up that falls toward the paddle: multiball (the ball splits into several), wide paddle (the paddle grows, with pulsing ends), or fire ball (the ball gets a bigger halo, ploughs straight through bricks without bouncing, and the remaining bricks shimmer). When the last brick falls the wall rebuilds with a big central burst of glow; when every ball is lost a new one serves from mid-field with a flash. The colour ramp runs black → slate violet → indigo → electric blue → magenta → hot red → orange → peach → white, so brightness reads as heat.

An AI plays when nobody is touching the paddle knob: it predicts where the nearest descending ball will cross the paddle line and moves there at a capped speed, and it detours to catch a power-up if it can do so before the ball arrives. Turning the paddle knob takes over manual control instantly (a small pulsing marker appears on the paddle), and the AI resumes after a few seconds of no input.

## The idea

An attract-mode arcade game as an ambient light show: a deterministic little physics sim with power-ups, combos, and a glow buffer that turns every collision into a burst of light. Humans can grab the paddle at any time.

## Algorithm in words

- State: a brick-alive grid; a small pool of balls (position, unit direction); a small pool of falling items (position, type); a low-resolution glow buffer the size of the playfield; timers for wide-paddle and fire modes; flash level; combo count; paddle position; manual/AI mode and an idle timer; a seeded PRNG.
- Per update:
  - Read the paddle knob; if it moved noticeably, reset the idle timer. Manual mode while the idle timer is short.
  - Decay the glow buffer exponentially (rate set by the glow control: longer trails at high settings). Count down power-up timers; set paddle width accordingly.
  - Move the paddle: in manual mode follow the knob; otherwise step toward the AI target at a capped speed. Clamp to the field.
  - For each live ball: advance by direction × speed × dt; reflect off the three walls; if inside the brick band and the brick under it is alive, kill it, reverse vertical direction unless in fire mode, bump combo and flash, splat glow at the brick centre (bigger in fire mode), and with probability proportional to the luck control spawn a random power-up there. If crossing the paddle line moving down and within the paddle's span, bounce up, add horizontal "english" proportional to where it struck the paddle, renormalise, and enforce a minimum upward steepness; reset combo. Remove balls that fall past the end. Record the earliest-arriving descending ball's predicted crossing x. Splat a small glow at the ball (bigger in fire mode).
  - If no balls remain, serve a new one with a flash. If no bricks remain, rebuild the wall with a big glow burst.
  - Advance items toward the paddle; on catch apply the effect (multiball spawns a mirrored copy of each live ball with a perturbed direction), flash and splat glow; remove missed items. Track the earliest-arriving item.
  - AI target: the ball's predicted x, unless an item will arrive comfortably before the ball and the paddle can reach it in time, then the item; with no descending ball, go for an item or return to centre.
  - Decay the flash.
- Per draw: for every panel pixel, map to the playfield (quarter turn), start from a dim background plus flash, draw walls, bricks (outline darker, row-graded fill, shimmer in fire mode), paddle (pulsing ends when wide, pulsing marker in manual mode), add the glow buffer, and map through the ramp. Then plot items (plus shapes, blinking at a type-specific rate; multiball items are wider) and balls (plus shapes, a fuller square-ish shape in fire mode) on top.

## Knobs

- K1 Paddle — manual paddle position; turning it takes control from the AI.
- K2 Speed — ball speed, from lazy to frantic.
- K3 Luck — power-up drop probability.
- K4 Glow — how long hit glows and ball trails persist.
- Buttons (on the original's knob push-buttons): rebuild the wall, trigger multiball, trigger wide paddle, trigger fire ball.
- Audio: none directly; mapping sound to Speed or Glow makes the game "dance" but loses control.

## Making it better in Luxel

- The library already has `brick-breaker-2d` (formerly `breakout-2d`); merge the best of this into it: power-ups, combo-scaled flash, AI that predicts and detours for items, and manual takeover. In Luxel the takeover should come from external events (tap/drag on the preview, `POST /api/events`, MQTT) rather than a knob — drag the paddle with a finger on the web UI.
- Render with `renderBulk`: `fillRect` per live brick (only bricks that changed need re-drawing if the frame persists), `splat` with additive blend for ball, items and hit bursts, and `fade` on the persistent frame for trails — no glow buffer array at all. That is a fraction of the per-pixel cost and scales to 128x128.
- Lay the playfield out from `gridWidth()`/`gridHeight()` and pick portrait or landscape automatically (bricks along the long axis on a wide panel) instead of hard-coding a quarter turn.
- Colour bricks per row with an OKLCH ramp; use `easeOutBack` for the wide-paddle grow and an `easeOutElastic` shake of the brick wall on fire-ball hits.
- As a scene: a text layer showing score/level and a sprite layer for power-up icons would make it read even more like an arcade cabinet. Strips: no.

## Payoff

4/5 — the most charming preset in this batch; a polished attract-mode game with real juice. Fold its features into the existing brick-breaker pattern.
