# 1-USFLAG_BLINK
kind: 1D
sensors: no

This is a simple, mostly static pattern: it paints a US-flag color scheme along a 1D strip once at startup, then only the "stars" blink.

## What it looks like
The strip shows, in order: a first quarter of alternating red and white blocks (stripes), then a quarter of solid blue in which every several-th pixel is a white "star", then the remaining half of alternating red and white blocks again. The stripe blocks are a handful of pixels long (about five). The white star pixels inside the blue field blink in unison: they stay bright white for most of a several-second cycle, then wink out briefly, then return. Everything else is static. Intended to wrap a flag layout around a strip (e.g. a loop), hence a built-in positional shift.

## Algorithm
- At startup (first frame), fill a per-pixel classification array with one of four states: red, blue, white, or off; and a parallel boolean array marking which white pixels are blinking "stars".
  - First quarter of the strip: alternate red and white in runs of about five pixels.
  - Second quarter: solid blue; within it, roughly every fifth position is overwritten as a white star and flagged as pulsing.
  - Second half: alternating red/white runs again.
  - All writes go through an index-shift helper: a signed offset (default about a dozen pixels backward) is added to the logical index and wrapped around the strip end, so the whole flag can be rotated to line up with the physical installation. Obvious improvement: expose this offset as a control (it is only an exported variable in the original) or default it to zero.
- Every frame: sample a square wave from a repeating clock a few seconds long with a high duty cycle (on the order of nine-tenths on, one-tenth off). That single value is the shared brightness for all star pixels this frame.
- Per pixel render: look up the classification — blue pixels render pure blue, red pixels pure red, pulsing white pixels render white at the square-wave brightness (so they blink off briefly each cycle), plain white pixels render solid white, anything else black.
- Dead/disabled code in the original: a rotate-the-whole-array-by-one routine exists but is never called (a commented-out scroll effect), and the per-frame logic contains a vestigial branch structure that only matters on the very first frame. A reimplementation just needs: build once, then blink.

## Layout assumptions
Purely index-based 1D; proportions (quarter/quarter/half) scale with pixel count, but the run length of stripes and star spacing are hardcoded small constants, and the default wrap offset assumes a specific physical strip. Fine on typical strips of dozens-to-hundreds of pixels; on very short strips the quarters degenerate.

## Controls
None in the UI. (The wrap offset and the most recent star index are exported as watchable variables only.)

## Timing
One blink cycle takes a few seconds; the off-blip is brief.
