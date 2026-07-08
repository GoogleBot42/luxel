# Doom Fire
kind: 2D
sensors: no

## What it looks like

A classic rising-fire simulation on a 2D matrix, in the style of the famously low-fi fire from the PlayStation port of DOOM (the header credits that technique and notes it uses no Perlin/gradient/fractal noise — just neighbor propagation with random cooling). A bed of intense heat sits along one edge; flames lick upward from it, flickering organically, occasionally leaning sideways as if gusts of wind pass through. Flame tips break up into ragged tongues that fade out partway up the display. In the optional "dragon's breath" mode, the whole fire periodically dies back to embers and then erupts violently in a repeating surge, like a beast exhaling flame every few seconds.

## Algorithm

**State.** Two 2D heat buffers (previous and current), swapped every simulation step. Each buffer is sized to the display plus one extra pad column on each side (so horizontal sampling never needs clipping/wrap checks) and one extra row beyond the display which permanently holds the fire's "source" heat. All heat values live in the zero-to-one range (the source can be pushed slightly above one in dragon mode). Additional state: a wind value, a simulation-time accumulator, and a breath-cycle phase.

**Simulation is decoupled from rendering.** Each frame, elapsed milliseconds accumulate; a simulation step runs only when the accumulator passes a minimum interval (user slider, zero to a few hundred milliseconds), then the accumulator resets. Rendering happens every frame from the current buffer, so slow simulation looks like chunky retro fire rather than a slow frame rate.

**Per simulation step:**

1. Swap the buffers.
2. Update wind. The intent (per the author's comment): wind occasionally changes, and every change passes through a calm (zero-wind) interlude before a new random direction is chosen, giving the feel of periodic gusts rather than constant lean. Concretely, with modest probability each step: if wind is currently nonzero it resets to zero, otherwise it picks a new random value spanning roughly one column-width in either direction. (Caution for the reimplementer: as written, the direction-picking helper only returns a value when the random check passes, so exact behavior leans on the source language's implicit-return semantics — implement the stated *intent*: mostly calm, brief random sideways gusts.)
3. For every interior cell (skipping pad columns; the topmost row is never written, so it stays dark): new heat = heat of the cell directly *below* it in the previous buffer, sampled with a horizontal offset for wind, minus a random cooling amount, floored at zero. The wind offset is weighted by horizontal position — strongest near the left/right edges, zero at the center — so gusts bend the flanks of the fire more than its core. The cooling amount is a uniform random value scaled by the maximum-cooling setting *and by proximity to the source row*: cells near the source cool the most per step, and cooling shrinks as heat rises.
4. Re-perturb the source row (see below).

**Why the height-dependent cooling is clever (and backwards from naive expectation):** heat runs the harshest random gauntlet in its first few rows; parcels that survive that with heat to spare then rise through ever-gentler cooling, so they get "carried" high before dying. This produces long, distinct flame tongues instead of a uniform fade — the author notes it simply looks better.

**Source-row perturbation, two modes:**

- *Normal fire:* every source cell is set to a high base heat plus a small spatial wave across the columns whose phase slowly slides back and forth (driven by a triangle wave of a slow clock, cycling over many seconds), so the base intensity gently shimmers across the width.
- *Dragon's breath:* every source cell is set to a breath level plus a fixed-phase spatial wave across the columns. The breath level is a wave over a repeating cycle of several seconds, so the base sweeps from near-dead up past full heat and back — periodic die-back and eruption. Switching back to normal mode re-initializes the source row to full.

**Per pixel (render):** map the pixel's normalized 2D coordinates to a buffer cell (shifting by the pad column). Brightness = the heat value *cubed*, which steepens contrast so only genuinely hot cells glow. Hue = the user's base hue plus a small shift that grows with brightness (hotter cells slide a little way along the color wheel — red toward orange at defaults). Saturation starts a little over full and is reduced as brightness rises, so the hottest cells wash toward white. Final value is scaled by the user's chosen overall brightness.

## Colors

With the default base color: black, through deep ember red, to orange, to a near-white hot core — the whole ramp emerges from the hue-shift-with-heat and desaturate-with-heat rules rather than a stored palette. The color picker re-bases the entire fire onto any hue (green fire, blue fire, etc.), keeping the same hotter-equals-brighter-and-whiter structure.

## Controls

- **Color picker (HSV):** sets the fire's base hue and overall brightness (the picker's saturation is ignored).
- **Slider — flame height:** inversely controls the maximum random cooling within a bounded range; higher means flames survive taller.
- **Slider — dragon mode:** behaves as a toggle (past halfway = on) switching between normal fire and the breathing/erupting mode.
- **Slider — speed:** sets the minimum time between simulation steps, from as-fast-as-possible up to a few hundred milliseconds per step (slower = chunkier, more retro).

## Layout assumptions

Display width and height are hardcoded constants (a modest square matrix by default) that the user is told to edit, and a 2D mapping is required. Obvious fix for a reimplementation: derive the dimensions from the pixel map's bounds (or expose them as controls) instead of hardcoding, keeping the buffer one row taller and two columns wider than the display.

## Timing

At default speed, many simulation steps per second — lively flicker. Wind gusts come and go every second or two. Dragon-breath eruption cycle: several seconds per exhale.
