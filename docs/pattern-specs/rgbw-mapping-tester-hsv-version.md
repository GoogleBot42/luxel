# RGBW Mapping Tester - HSV Version
kind: 1D
sensors: no

This is a trivial diagnostic pattern, not a visual effect.

## What it does
The entire strip (every pixel identical) cycles through four solid colors in a fixed order: pure red, pure green, pure blue, then white. Each color holds for a bit over a second; the full cycle takes several seconds and repeats forever. Its purpose is to verify RGBW strip configuration: if the colors appear in the wrong order or the strip isn't uniform, the color-order / LED-type settings are wrong.

## Algorithm
A single sawtooth time value cycling over several seconds is quartered; each quarter selects one of the four colors via a simple if/else chain, and every pixel is set to that color through the HSV path. No per-frame state beyond the time phase; no randomness; no layout assumptions (any pixel count, no map needed).

## Colors
- First quarter: fully saturated red.
- Second quarter: fully saturated green.
- Third quarter: fully saturated blue.
- Fourth quarter: white at roughly half brightness (the reduced brightness keeps power draw comparable to the single-channel colors).

The one deliberate trick: white is produced by setting saturation to zero through the HSV API, which on RGBW strips activates the dedicated white LED — that is the whole point of this "HSV version" of the tester.

## UI controls
None.
