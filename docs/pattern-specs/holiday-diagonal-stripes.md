# Holiday_Diagonal_Stripes
kind: 2D
sensors: no

This is a simple pattern; the spec is short on purpose.

## What it looks like

Diagonal stripes in solid red, white, and green scrolling slowly and steadily
across a 2D display. A slider adjusts the stripe slope from horizontal-ish to
steeply diagonal.

## Algorithm

No state beyond a phase read from a repeating clock each frame (one full
scroll cycle takes several seconds).

Per pixel: form a scalar "stripe coordinate" as x plus y times twice the
slope setting, plus the scroll phase. Feed that, multiplied by a small
constant (around six) to set stripe density, through a sinusoidal
zero-to-one wave, then split the wave's output into three equal thirds:
bottom third renders solid red, middle third solid white, top third solid
green. Colors are hard-coded fully saturated primaries plus white — no
blending, no gradients.

Two consequences of using a sinusoidal wave rather than a sawtooth, which a
reimplementer should preserve for fidelity:
- the stripe sequence mirrors (…red, white, green, white, red…) instead of
  strictly repeating red/white/green;
- band widths are unequal — the wave lingers near its extremes, so the red
  and green bands are wider than the white bands between them.

## Controls

- **Slope** (slider): zero gives near-vertical stripe boundaries driven mostly
  by x; increasing it mixes in more y, tilting the stripes toward diagonal.

## Layout / notes

Requires 2D coordinates; no pixel-count assumptions. Trivial to port.
