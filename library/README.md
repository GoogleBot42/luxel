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
- Second line: `// tags: <shape>, <theme>[, <theme>…]` — the Patterns page's
  shape control and theme chips filter on it (see "Gallery tags" below).
- Third line, the provenance, in one of these forms:
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

### Gallery tags

A closed vocabulary, lower-case, comma-separated, any spacing:

- **Shape — exactly one.** `strip`: looks right only on a 1-D strip. `panel`:
  needs a 2-D map. `both`: looks right either way (typically a file that
  branches on `has2DMap()`, or a `render(index)` that reads well in index
  order on a panel too).
- **Themes — one or more.** `ambient` (moods, washes, generative fields),
  `nature` (organic or physical phenomena), `sound` (reads `frequencyData` /
  `energyAverage`), `game`, `retro` (pixel-art / 8-bit styled), `clock`,
  `holiday`.

`tools/check-library.sh` fails a missing line, an unknown tag, zero or two
shapes, or no theme, and WARNS when the shape looks unlike the source (`panel`
on an index-only `render(index)`, `strip`/`both` with no `render(index)` and
no `has2DMap()`). `web/tools/gen-gallery.mjs` writes the line into
`gallery.json` as `tags` (shape first). In the console, `Strip` offers strip +
both, `Panel` panel + both; a console opens on its own fixture's shape.

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

Scenes are inherently panel-only — a draw op is a silent no-op without a regular
2D matrix (docs/spec/scenes.md §2), which is why the console's Scenes tab is
gated on one and why `scenes.json` carries a constant `kind: "grid"` the way a
pattern carries its own `kind`. The shipped ones are authored for a 64×64
panel and keep every placed box inside 32×32, so they still read on a smaller
matrix — the exception being a layer that is deliberately the whole layout
(`spark-field`'s tiled sprite), which clips rather than disappears.

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
