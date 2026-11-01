# HUD Ticker Rows

Idea spec written from Patternflow preset preset_0511.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

The panel is sliced into horizontal lanes a handful of pixels tall, stacked top to bottom. Each lane scrolls sideways like a stock ticker; neighbouring lanes go in opposite directions and at one of a small set of speeds, so the whole screen shears in a kinetic, machine-like way. Each lane is chopped into equal-width segments separated by a thin vertical divider tick, and every segment shows one little sci-fi interface motif chosen pseudo-randomly: a horizontal loading/progress bar, a run of diagonal caution chevrons, a sparse grid of data dots, a tiny sine-wave oscilloscope trace, or nothing at all. Each motif type has its own hue offset from the base hue, the dividers yet another. The look is a spaceship HUD / data-centre wall / glitch-art "UI soup": dense, readable at a glance, constantly moving, all on black. The motifs are fixed per segment, so as segments scroll the same "widgets" pass by again and again.

## The idea

Build an endless animated HUD by tiling a few tiny vector glyphs into scrolling lanes, with a hash per segment choosing which glyph. It works because the eye reads the glyphs as meaningful interface elements and the counter-scrolling lanes give energy without any complex simulation.

## Algorithm in words

- State: a time accumulator advanced by the speed knob.
- Lane index = row divided by lane height. Each lane gets a scroll direction from its parity and a speed multiplier from its index taken over a short repeating cycle (a few tiers). Its horizontal offset is time × a base scroll rate × that multiplier.
- For each pixel, add the lane offset to x, find which segment it lands in and its local position inside the segment.
- Hash (lane, segment) to a value in 0..1 and pick one of five motifs by splitting that range into equal fifths:
  - progress bar: a thin horizontal bar a few pixels tall, centred vertically, spanning most of the segment from its left edge;
  - chevrons: diagonal stripes from the sum of the local coordinates;
  - blank;
  - data dots: lit where both local coordinates hit a small lattice;
  - waveform: a sine of local x scaled to nearly the lane's half-height, drawn as a line about a pixel and a half thick.
- The first column of every segment is a divider, always lit in its own hue.
- Colour is the base hue plus the motif's offset, high saturation, full value.

## Knobs

- K1 — base hue (all motif hues rotate together).
- K2 — scroll speed.
- K3 — lane height: thin lanes give many tiny lanes, tall lanes give fewer, chunkier widgets with taller waveforms.
- K4 — segment width: narrow segments are a confetti of glyphs, wide ones give long progress bars and multi-cycle waveforms.
- No audio.

## Making it better in Luxel

- This is naturally a **scene**: each motif as a small sprite (or text glyph) and each lane as a scrolling layer with its own speed, instead of evaluating motif geometry per pixel. In a `renderBulk` pattern the same thing is a handful of `fillRect`/`drawLine`/`blit` calls per visible segment — tens of ops per frame instead of thousands of per-pixel branches.
- Make the widgets live: progress bars that fill and reset with `easeInOutCubic`, waveforms that animate their phase, dots that blink with `hash2(segment, floor(t))`, occasional segments that re-roll their motif. That turns a static tiling into a believable control panel.
- Add a text-layer lane of scrolling numbers or short words (the Text extension) for an unmistakable ticker; optionally drive one lane's waveform and bars from `frequencyData` so the HUD reacts to music.
- Palette: a restrained two-hue OKLCH scheme (amber + cyan, or phosphor green) looks more "real HUD" than full hue offsets; keep the dividers dim.
- Strips: a 1D variant works well — one lane of scrolling segment glyphs (bars, dashes, blips) on a 60–300 px strip.

## Payoff

Score: 4/5 — the strongest idea in this batch: instantly readable, energetic, and a perfect showcase for Luxel scenes/sprites.
