# Crossfading
kind: 1D
sensors: no

## Purpose

This is as much a framework demo as a pattern: it shows how to host several independent sub-patterns inside one pattern and smoothly crossfade between them on a timed rotation. It ships with three example sub-patterns. The header comment explicitly invites users to swap in their own sub-patterns.

## What it looks like

The strip cycles endlessly through three looks, each held for several seconds, with a smooth dissolve between consecutive looks:

1. **Blue–purple shimmer** — hues confined to the blue-through-purple range, arranged in nonlinear rippling bands along the strip (band spacing is compressed toward one region of the strip because the hue is driven by a reciprocal function of position). A slow sinusoidal wobble makes the bands sway, and the whole strip's brightness slowly pulses between medium and full.
2. **Rainbow blocks** — a rainbow gradient that scrolls along the strip over a few seconds per revolution, but displayed only in evenly spaced lit blocks: the strip is divided into a handful of equal segments and only the latter half of each segment is lit, giving a dashed/blocky scrolling rainbow.
3. **Bouncing red pulse** — a single pure-red pulse, a few pixels wide with a linear falloff to black at its edges, bounces end-to-end (triangle-wave motion, constant speed, a couple of seconds per bounce). A "low-budget Larson scanner / KITT" look.

The rotation feels like: hold a look, then spend roughly the last two-fifths of its time slot dissolving into the next look, wrapping around after the third.

## Algorithm

**Architecture.** Sub-patterns are stored in two parallel arrays: one of per-frame setup functions and one of per-pixel renderer functions, indexed by mode number. The key trick: sub-renderers never write pixels directly. Instead they set three shared "current color" globals (red, green, blue components), either directly or via a private HSV-to-RGB helper that writes those same globals. The host decides what to do with them.

**Per frame.** A sawtooth clock spanning (seconds-per-mode × mode-count) is scaled by mode count; its integer part is the current mode index, and the next mode is the following index modulo mode count. From the fractional part, compute a crossfade progress value: zero for the first portion of the slot, then rising linearly from zero to one across the final fraction of the slot (the crossfade fraction is a tunable constant, a bit under half). Run the current mode's per-frame function; if the crossfade progress is nonzero, also run the next mode's per-frame function.

**Per pixel.** Invoke the current mode's renderer (which populates the shared RGB globals). If not crossfading, emit that color. If crossfading, stash the first result, invoke the next mode's renderer, and emit the component-wise weighted average of the two RGB triples, weighted by crossfade progress. Blending is a plain linear mix in RGB space (the author notes true HSV-space blending was rejected as too expensive).

**State between frames:** only the derived mode index and crossfade progress (recomputed each frame from the global clock) plus each sub-pattern's own per-frame time value. No per-pixel state, no randomness anywhere.

**HSV helper.** The pattern includes its own standard sector-based HSV→RGB conversion (wrapping negative hue, clamping saturation/value) because the built-in HSV pixel-setter can't be intercepted. The author warns this loses the engine's extra-precision/HDR color path — a re-implementation on an engine where HSV output can be captured before quantization can avoid that caveat.

**Layout assumptions:** none problematic. Sub-patterns use position normalized by pixel count (the red pulse's width is a fixed few pixels rather than strip-relative, which is a minor scaling quirk on very long strips).

## Colors

- Sub-pattern 1: blues into violets, fully saturated, brightness pulsing.
- Sub-pattern 2: full rainbow, fully saturated, full brightness in the lit blocks, black gaps.
- Sub-pattern 3: pure red on black.
- Crossfades produce whatever the RGB average yields (can pass through desaturated in-between tones — accepted by design).

## Controls

None. Mode duration, mode count, and crossfade fraction are code constants intended to be edited.

## Timing

Each sub-pattern owns the strip for several seconds; the dissolve occupies roughly the last two-fifths of that. Sub-pattern internal cycles (rainbow scroll, pulse bounce, shimmer pulse) each run a few seconds per cycle.
