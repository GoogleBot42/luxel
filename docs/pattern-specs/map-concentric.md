# Map - Concentric
kind: 1D+2D
sensors: no

## Overview
A very simple demo pattern, clearly written to illustrate the difference between the
1D and 2D renderers and to show off a mapped installation. It is nearly trivial;
this spec is short on purpose.

## Visual behavior
- **2D:** The panel is split into two regions: a filled circle centered in the middle
  of the mapped area, and everything outside it. The circle is one hue, dimmer;
  the background is a different hue (offset a large fraction of the way around the
  color wheel, so it reads as a strongly contrasting color) and noticeably brighter.
  Both hues drift together around the color wheel continuously, so the two-tone
  circle-on-background slowly cycles through all color pairings. Nothing else moves;
  it is a static shape with slowly rotating colors. One full trip around the hue
  wheel takes several seconds.
- **1D:** A classic scrolling rainbow: hue is the pixel's fractional position along
  the strip plus the same global time phase, at full saturation and full brightness.
  The whole rainbow spans the strip once and scrolls smoothly, one full cycle in
  several seconds.

## Algorithm
- Per frame: advance a single global phase value that ramps from 0 to 1 and wraps,
  taking several seconds per cycle. This phase is the base hue.
- 1D per pixel: hue = phase + (index / pixel count); full saturation and brightness.
- 2D per pixel: compute squared distance of the pixel's mapped (x, y) from the
  center of the unit square. If it is inside a circle of the chosen radius, render
  hue = phase at a low-ish brightness (roughly a third of full); otherwise render
  hue = phase plus a large constant offset (a bit less than half the wheel) at a
  brighter level (well over half of full). Comparing squared distance against
  squared radius avoids a square root.
- No state other than the phase and the radius. No randomness. No layout
  hardcoding beyond assuming the 2D map is normalized to a unit square.

## Colors
Full-saturation spectral hues only. At any instant: circle in one pure hue, background
in a contrasting pure hue; over time both sweep the whole rainbow.

## Controls
- One slider, "radius" concept: sets the circle's radius. Scaled so that even at
  maximum the circle stays comfortably inside the unit square (max radius is a bit
  under half the panel width). At zero the circle disappears and the panel is all
  background color.

## Notes / gotchas
- The radius is only assigned inside the slider handler. On platforms that do not
  replay saved slider values at startup, the circle would have zero/undefined radius
  until the slider is touched; a reimplementation should give the radius a sensible
  default (mid-range) at init.
