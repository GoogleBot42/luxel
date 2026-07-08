# RGB Test Pattern
kind: 1D
sensors: no

Trivial diagnostic pattern; short spec on purpose.

## What it looks like
A mostly-dark strip with a few marker pixels lit. The very first and very last pixel are always lit half-brightness white (endpoint markers). Every tenth pixel (by index) is lit in the current test color; everything else is black. The test color steps through a fixed cycle of four modes — half-brightness white, then pure red, then pure green, then pure blue — advancing to the next mode about every couple of seconds, looping forever. Useful for verifying wiring order, color channel order, and strip length.

## Algorithm
- State: an accumulated millisecond timer and a current mode index (exported, so it's visible/settable externally).
- Per frame: add the frame delta to the timer; when it crosses the mode period (about two seconds), subtract the period (no drift) and advance the mode index cyclically through the four modes.
- Per pixel: if it's the first or last pixel, output half-white; else if the pixel index is an exact multiple of ten, output the current mode's color from a small lookup table of RGB triples; else output black.
- No randomness. Uses the actual pixel count for the end marker, so it adapts to any strip length. The every-tenth spacing is a fixed stride (reasonable for a test pattern; could be made a slider if desired).

## Colors
Exactly: half-intensity white markers, and a cycle of half-white → full red → full green → full blue for the stride pixels. (These are channel-primaries by design — it is a channel-order test — so "pure red/green/blue" is the spec, not an aesthetic choice.)

## Controls
None.
