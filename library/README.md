# Luxel library

The starting content the playground and every device ship with, in three peer
directories: patterns (`*.js`), sprites (`sprites/*.sprite`) and scenes
(`scenes/*.scene`). All of it is Apache-2.0: the Patternflow-inspired patterns were built from prose idea specs only (ideas, not expression — the same describer firewall as the corpus ports), so the implementation is ours; each header credits the CC BY-SA 4.0 preset the idea came from.

## Patterns

155 patterns as of the 2026-11 overhaul (down from 308), beside 35 sprites and
18 scenes. Each one is one of three kinds, and its header says which:

- **Clean-room reimplementations** of ideas from the Pixel Blaze community
  corpus. The scraped corpus has unknown licensing, so none of its code
  appears here: a **describer** read the original and wrote a prose-only spec
  (behavior, algorithm, colors, controls — no code, no identifier names, no
  copied constants), and an **implementer** who never saw the original wrote
  fresh Luxel code from that spec alone.
- **Patternflow ideas**, through the same firewall: prose idea specs under
  `docs/pattern-specs/patternflow/`, never the source.
- **Curated originals** — hand-written showcases of the language and builtins.

**Idea over fidelity.** The 2026-11 overhaul deduplicated the library (variants
became modes, palettes and controls of one survivor), deleted test scaffolding
and weak ports, and rewrote the rest to capture each original's *idea* and do
it better with what Luxel has — OKLCH, simplex/curl noise, easings,
`renderBulk`, canvases, scenes. A pattern identical to its original is not
the goal.

File conventions:

- First line: `// name: <Display Name>` — the pattern browser reads this.
- Second line, the provenance, in one of these forms:
  - `// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/<a>.md[, <b>.md] — reimagined, not a replica.`
  - `// Original work from the prose idea spec docs/pattern-specs/patternflow/<slug>.md (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).`
  - `// Curated original (hand-written showcase of the Luxel language/builtins).`

  Then a few lines on what it does and how it is built.
- Controls follow `.claude/rules/library.md`: real-unit `//#` bounds, the
  top-level constant equal to `default=`, never a pattern-level brightness
  control.
- **Strip and panel in one file:** export `render(index)` beside
  `renderBulk` (that keeps a mapless strip from being handed a default
  square grid) and branch on `has2DMap()` — never on `gridWidth()`, which
  reads the pixel count on such a strip (docs/lang.md). The strip branch uses
  index-space ops (`splatIndex`, `fillRangeAA`, `gradientRange`) instead of
  `array(pixelCount)` buffers.
- **Canvases** size from the grid — `min(gridWidth(), cap)` ×
  `min(gridHeight(), cap)`, with a fallback when there is no grid — never a
  fixed 16×16. Fill with `fillCanvas`/`paintCanvas` filter 1 (bilinear) only
  when the canvas is coarser than the grid; at 1:1 use filter 0.
- **Never black:** an idle viewer should not face a dark panel for more than
  about a second at the defaults. Sound patterns fall back to a built-in demo
  groove when no sensor data arrives.

This is the single source for every pattern the playground shows.
`web/tools/gen-gallery.mjs` builds the playground's pattern browser
(`gallery.json`) from this directory alone, and `luxel serve`'s default
pattern (`rainbow.js`) is `include_str!`'d from here. Test and bench fixtures
live under `tools/patterns/` and `tools/bench-patterns/`, never here.

The prose spec each reimplementation was built from is kept at
`docs/pattern-specs/<slug>.md` — the firewall's audit trail. A spec whose idea
was folded into another file is kept too, with a first line
`> Superseded: this idea now lives in library/<survivor>.js (2026-11).` The
scraped corpus is never read by the build; it survives only as an untracked,
local compile-compatibility test battery.

## Sprites (`sprites/*.sprite`)

Original art, authored as **text** pixel art rather than as images. `library/`
is a reviewable tree — every entry in it is source a person can read in a diff,
which is the property the clean-room firewall is built on — and a committed PNG
is a blob a reviewer cannot diff, on top of putting an image decoder in the web
build for art that is authored here anyway. A *user's* image still becomes a
sprite: that is `luxel sprite import` and the console's `Import image…`
(docs/webui.md).

The grammar, which `parseSpriteSource` in `web/src/lib/librarySource.ts`
defines and unit-tests:

```
# a line starting with # is a comment, anywhere in the file
name: Heart                  required, 1..=64 UTF-8 bytes
fps: 12                      optional, 0..=30, default 0 (a still sprite)
palette:
  r ff3355                   one <char> <rrggbb> per line, in record order
  h ff8fa3

frame                        one `frame` line per frame, then its pixel rows
.rr...rr.                    one character per texel; `.` is TRANSPARENT
rhrrrrrrr
```

`.` is always index 0 and is never declared. Blank lines and comments are
skipped everywhere, so a fully transparent row has to be written as dots rather
than left empty. Every frame must be the same size, and a character that is not
`.` must be in the palette — the two mistakes that are easy to make by hand and
impossible to see in a rendered thumbnail. A declared colour nothing paints with
is an error too: it would ship three bytes and a misleading colour count, and it
is always an authoring slip. The encoded record must fit the 16 KiB cap the
device's request buffer imposes, so a shipped sprite can always be POSTed.

## Scenes (`scenes/*.scene`)

A scene source **is** the `/api/scenes` wire record (docs/spec/scenes.md §1),
with two liberties taken for reviewability and undone by the generator:

- `#` comment lines, which the wire has no room for.
- `I @pat/<slug>` / `I @spr/<slug>` in place of a store id. A raw store id on
  an `I` line is an error: eight hex digits minted by somebody else's flash mean
  nothing here, and a shipped scene must not carry one.

A scene may carry keyframe tracks (`M` lines, docs/spec/scenes.md §1
"Keyframes") — `sunrise` does — and clip layers (`X clip`, §2) — `fire-clock`
does. `M` lines are free to align their columns with extra spaces and to sit
under a `#` header row: the generator ships the console's canonical
serialization (single spaces), and the gate checks that it kept every `M`
line and that `luxel_core` still takes it.

Each reference is rewritten to a **derived** 8-hex id — FNV-1a over
`<kind>/<slug>`, `libRefId` — before the record is serialized. What ships is
therefore an ordinary scene record: `luxel_core::scene::parse` validates it and
the real compositor previews it, and no parser anywhere grows a special case.
Swapping those placeholders for whatever the target store assigns happens at
clone time, in `web/src/lib/sceneRefs.ts`.

The **slug is the file stem**, and it is the stable key on the library side: a
scene reference and a clone both key on it, because the display name is the one
thing a library entry is allowed to change.

A scene composites onto the device's Layout: a regular 2D matrix, or — since
Gitea #985 — a strip, as one row in index order (docs/spec/scenes.md §2 "The
canvas"). An irregular 2D map or any 3D map has no canvas, and a scene there
draws nothing. Most shipped ones are authored for a 64×64 panel
(`scenes.json` `kind: "grid"`, the way a pattern carries its own `kind`) and
keep every placed box inside 32×32, so they still read on a smaller matrix —
the exception being a layer that is deliberately the whole layout
(`spark-field`'s tiled sprite), which clips rather than disappears. The
`strip-*` scenes are written for a strip (`kind: "strip"`, below).

### Strips

On a strip of `n` pixels the canvas is `n`×1, so a scene is written in
**segments**:

- **A box is a segment.** `L <type> <x> 0 <w> 0 …` covers pixels `x` ..
  `x+w-1`; `h` 0 and `h` 1 both mean the one row, and `w` 0 is the rest of
  the strip from `x`. A pattern layer's segment is a WINDOW onto that
  pattern's full-strip render, not a squeezed copy: `75 0 75 0` on a 150 px
  strip shows the pattern's own pixels 75..149.
- **A zero `w` is the LAYOUT's width, not "what is left".** `x 75 w 0`
  resolves to a box 75 .. 75+n that overhangs the end and clips, which reads
  as "the rest of the strip" — until the layer is flipped: `flipx` mirrors
  the whole overhanging box, so the visible part samples past the end and
  draws BLACK (Gitea #1026). Give a flipped segment an explicit `w`, or flip
  a whole-strip base instead and lay the other segments over it, as
  `strip-zones` does.
- **`flipx` (flags 3) reverses a segment in place** — the way to run a
  pattern outward from a seam, or mirror two halves. `flipy` does nothing on
  one row.
- **Tiled 1-row sprites repeat along the strip.** Author the sprite 1 px
  tall (an 8×1 bulb, a 12×1 dash), put it on a `tile` layer and give the
  box `w` 0: under `tile` a zero `w` (or `h`) is the whole layout, so
  `L sprite 0 0 0 0 normal 100 none tile 1` covers a strip of any length
  without its pixel count typed in — and the same line wallpapers a panel.
  A taller sprite shows only its top row on a strip.
- **Ramps, masks, clip layers, keyframes and every blend mode work
  unchanged.** An `M` line's `x` slides a segment along the strip; its `y`
  should stay 0.
- **Text draws nothing on a strip** — no glyph fits one row. It is not an
  error: a scene with a clock caption still shows its other layers there.
  Do not build a strip scene around a text layer.
- Keep strip scenes to ≤ 2 `pat` layers, like every scene (`caps.layers`).
- **Boxes are pixels, so a split does not scale.** A seam at 75 is the
  middle of a 150 px strip, a quarter of a 300 and off the end of a 60. Only
  whole-layout boxes (`0 0 0 0`) and tiles read the same at every length;
  say in the scene's header what it does at 60 and 300 px.
- **Declare the shape: `# layout: strip`** as a comment line in the scene
  source makes its `scenes.json` entry `kind: "strip"` (no line, or
  `# layout: grid`, is `"grid"`), which is what lets the console offer it
  only on a strip. It is declared rather than inferred because a scene of
  whole-layout boxes is valid on both shapes and only its author knows which
  it was drawn for. The gate (`check-sprite-scene-library.mjs`) refuses any
  other `# layout:` word, a second `# layout:` line, and — on a strip scene —
  a box that is not a segment (`y` 0, `h` 0 or 1), a keyframe that moves `y`
  off 0, or a text layer.
- **Preview one**: `tools/verify/scene-sheet.mjs <out> --size 150x1
  library/scenes/strip-zones.scene` draws a strip as a waterfall (one row
  per frame, time running down).
- **The shipped strip scenes**: `strip-zones` (fire and sea on two
  segments, a breathing seam glow), `strip-bulbs` (the `bulb-string-8x1`
  sprite tiled with a zero box, any length), `strip-colour-fade` (the "fade
  between N colours" family: the `grey-fade` greyscale source under a colour
  ramp per layer, two ramps crossfading on a keyframe track). A ramp turns
  grey `pos` 0..255 into colour, so any N-colour fade is one `R` line of N
  stops over `grey-fade`.

## The generator and the gate

`web/tools/gen-sprite-scene-gallery.mjs` builds `web/public/sprites.json` (the
metadata plus the base64 `LXSP` record per sprite) and `web/public/scenes.json`
(the metadata, the canonical wire block and the reference table) the way
`gen-gallery.mjs` builds `gallery.json`, from the same npm scripts.
`web/tools/check-sprite-scene-library.mjs` is the gate — `npm run check-library`
from `web/`, and a step in `tools/ci.sh` beside `tools/check-library.sh` — and
it runs the **Rust** codecs over the bytes that ship, so a sprite that draws in
the console is one a device will accept, and a reference that stops resolving
fails the build instead of quietly costing a scene one layer. Both are indexed
in docs/tools.md.
