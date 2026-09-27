# Luxel library

The starting content the playground and every device ship with, in three peer
directories: patterns (`*.js`), sprites (`sprites/*.sprite`) and scenes
(`scenes/*.scene`). All of it is Apache-2.0.

## Patterns

Clean-room reimplementations of the Pixel Blaze community corpus. The
scraped corpus has unknown licensing, so none of its code appears here.
Instead, every pattern in this directory was produced by a two-party
firewall:

1. A **describer** read the original and wrote a prose-only functional
   specification — behavior, algorithm, colors, controls, timing — with
   no code, no identifier names, and no copied numeric constants.
2. An **implementer** who never saw the original wrote fresh Luxel code
   from that specification alone.

File conventions:

- First line: `// name: <Display Name>` — the pattern browser reads this.
- A provenance comment noting the clean-room origin.
- Patterns using a 2D buffer simulate on a 16×16 virtual canvas sampled
  by normalized coordinates in `render2D`, so any map works.
- Sound/motion patterns bind sensors the PB way (`export var
  frequencyData`, etc.); the engine stubs them with zeros until real
  peripherals land, so they run dark rather than erroring.

This is the single source for every pattern the playground shows — the
curated hand-written showcase examples and the reimplemented community
corpus together. `web/tools/gen-gallery.mjs` builds the playground's
pattern browser (`gallery.json`) from this directory alone; there is no
separate inlined example set. The firmware/CLI/test default seed
(`rainbow.js`, `blink-fade.js`) is `include_str!`'d from here too.

The prose specification each corpus reimplementation was built from is
preserved at
`docs/pattern-specs/<slug>.md` — the clean-room firewall's audit trail.
The original scraped corpus is never read by the build; it survives only
as an untracked, local compile-compatibility test battery.

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
