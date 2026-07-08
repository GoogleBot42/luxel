# Nano Orbital
kind: 1D
sensors: no

A simple pattern intended for modular-panel installations (Nanoleaf-style: many identical panels, each with the same number of LEDs). It is close to trivial.

## Visual behavior
Exactly one LED is lit on each panel at any moment; all other LEDs are dark. All panels light the same relative position within themselves, so a single dot appears to orbit around every panel in lockstep. The dot steps through each panel's positions over the course of roughly a minute (one full lap per panel per lap of the master clock). Colors are a full-saturation rainbow spread across the whole installation by pixel position, and the entire rainbow also rotates slowly with the same roughly-one-minute clock. The stepping is discrete (the dot jumps from LED to LED, no interpolation or fading — the author themselves noted it could stand to be smoother).

## Algorithm
State: one working array the size of the strip holding this frame's on/off values.

Per frame:
1. Read a slow sawtooth clock with a period of roughly a minute.
2. Compute a step index: floor of (clock × number of panels), giving an integer that walks from zero up through the panel count once per clock period.
3. Clear the whole working array, then for each panel set exactly one entry to full: the entry at (panel number × pixels-per-panel + step index).

Per pixel: hue = clock value + normalized position along the whole strip; full saturation; brightness = the working array entry (all or nothing).

## Layout notes / hardcoding
Both the panel count and the pixels-per-panel are hardcoded constants (each a dozen in the author's build), meant to be edited to fit the installation. Subtle trap: the code steps the within-panel position by floor(clock × panelCount), which only walks through every LED of a panel because the author's panel count happens to equal the pixels-per-panel. The correct generalization is floor(clock × pixelsPerPanel) for the step, and deriving panel count as pixelCount / pixelsPerPanel. Also guard the case where pixelCount is not an exact multiple of pixels-per-panel (out-of-range writes).

## Controls
None.

## Colors
Rainbow: hue advances gradually panel-to-panel across the installation and the whole wheel drifts slowly over about a minute. Background is fully dark.

## Timing
One dot-orbit per panel takes on the order of a minute; the rainbow rotation has the same period.
