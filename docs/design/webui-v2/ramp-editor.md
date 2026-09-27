# The color ramp editor — design (#787)

Status: **approved and IMPLEMENTED** (2026-09-27). Frames S8a–S8i in
[`mockups.html`](mockups.html) are the visual spec; the implementation is
`web/src/components/ColorRamp.svelte` — one component, both mounts — plus the model in
`web/src/lib/gradient.ts`, and `web/tools/mockdiff.mjs` holds it to the frames.

One control, mounted twice: Settings › Advanced › Output processing › **Color ramp**
(the device's own output palette) and the scene editor's per-layer **Color ramp**. It
replaced `components/GradientEditor.svelte` and its two adapters rather than polishing
them — Jeremy, 2026-09-26: *"a complete UI redesign of color ramp (it is a
horrible/confusing interface, and very buggy)"*. The 21-defect inventory that this is
measured against is the first comment on #787.

## The frames

| Frame | Width | What it specifies |
|---|---|---|
| **S8a** | 760 | The Settings mount with the editor open and a stop selected — the whole control in one picture. |
| **S8b** | 760 | The selected stop's swatch open into the app's own `ColorPicker`, and the mid-gesture write contract. |
| **S8c** | 390 | *Outside the stops*, option **A**: clamp both ends — the one Jeremy chose, and what ships. |
| **S8d** | 390 | *Outside the stops*, option **B**: black above the last stop — what shipped until #787; the option not taken. |
| **S8e** | 320 | The scene layer's collapsed ramp row — replaces the row drawn in S7b/S7g. |
| **S8f** | 320 | The same editor expanded inside the 320 px inspector: one column, save-based mode. |
| **S8g** | 390 | The phone variant — every hit target ≥ 24 px (§5.7, #703). |
| **S8h** | 760 | Three states the old control got wrong: no ramp · nothing running (two-stop minimum) · at the 32-stop cap with stacked stops. |
| **S8i** | 760 | The three #563 modes, the device-changed-underneath strip, and the two confirms. |

S7b and S7g keep their own frames (mockdiff pins them); S8e is the amended row, and the
implementation follows S8e where the two disagree.

## The design in one page

- **The bar is the model.** 44 px tall, full width. Click empty bar = add a stop there ·
  drag = move · drag off = remove · click = select. No permanent per-stop row list.
- **The bar is the engine's LUT**, painted as a `256×1` canvas stretched (#748), never a
  CSS `linear-gradient`: a gradient cannot express the clamp, the truncating 16.16
  interpolation, or a hard cut to black, and that mismatch is why the old preview
  disagreed with the device.
- **One selected stop, one row.** Its swatch opens `components/ColorPicker.svelte` — the
  app's own picker, never `<input type="color">` — plus a numeric position and *Remove
  stop*. No `Stop N` header: a header that renumbers itself mid-drag is unreadable, so the
  stop is named by its position bubble on the bar and, to a screen reader, as
  `ramp stop at 173, #c23a6b`.
- **Handles are 12 px painted inside a 24×24 hit box**, sitting on the bar's bottom edge.
  There is no gutter under the bar to miss into.
- **The stage is stated in body text**, never a hover title: *Brightness → color. Each
  pixel's brightness picks a color from this ramp: dark pixels take the left end, bright
  pixels the right.* plus one sentence on what happens outside the stops.
- **The preview is a pair** — what the pattern renders, and what the LEDs do with the ramp
  at the current amount — with the **amount slider beside it** and both its ends spelled
  out. With nothing running, the left cell falls back to a brightness wedge labelled as
  one. One honest picture each, replacing two unlabelled bars where neither said which one
  the device would show.
- **Presets**: Mono · Sunset · Ice · Fire · Spectrum · To black. A preset replaces the
  stops, is undoable, and never touches the amount.
- **Undo** (`Ctrl-Z`) covers every edit since the editor opened; **Reset** returns to the
  ramp as it opened; **Clear ramp** (Settings) / **Remove ramp** (scene) is the one
  destructive action, separated, confirmed, and it does not change the amount.
- **`+ Add stop` lands in the widest gap**, never past the last stop, and is disabled with
  its reason at the 32-stop cap (§5.7's one deliberate "disabled with a reason" exception,
  `data-reason` per #529).
- **Stops within 3 px collapse to one handle with a count badge** and a *next of N* button
  that cycles the selection.
- **One legality rule, printed**: a ramp is **0 stops (off) or at least 2 — never 1**.
- **The mode is on screen** (#563): a live green dot and *"Changes go to the LEDs as you
  make them"* for the device's own ramp and the playing scene; a dim dot and *"the LEDs
  follow when you save"* otherwise; *"Preview only"* with nothing connected.
- **The editor re-reads while open.** If the device's ramp changes underneath an editor
  with local edits, a strip offers *Keep mine* / *Load theirs* and **nothing is sent until
  the user chooses**.
- **Selection is an identity, not an index**, and every commit is computed from the stop
  list at that moment — never from a snapshot captured by a timer.
- **Editing the ramp must not restart the layer's pattern clock.** The ramp is a
  post-stage; re-applying it does not need a scene rebuild.

## The two questions, answered

Jeremy, 2026-09-27:

**Q1 — outside the stops: clamp both ends (A), or keep black above the last stop (B)?**
**A. The ramp clamps at both ends** (frame S8c). Above the last stop the end colour
continues; the cut to black survives only as the *To black* preset, i.e. as something you
say rather than something you discover. B was what shipped until then
(`outpipe::fill_palette_lut` clamped below the first stop and wrote black above the last),
it was stated nowhere in the UI, and it was the root of inventory item 10 — clicking the
bar above the last stop added a *black* stop and moved 88 of 256 LUT entries, against the
component's own promise that adding a stop never changes the gradient.

**Q2 — does it apply to the per-layer scene ramp as well as the device palette?**
**Yes.** One semantic, one component. Both mounts run the same engine stage
(`fill_palette_lut` → `palette_remap_frame`), and one editor with two semantics is how the
old control ended up with three different minimum-stop rules (item 21).

### What A cost, and what it did to stored data

One line, in one place: `outpipe::fill_palette_lut` clamps its *sample position* to the
last stop and then runs `vm::sample_palette` unchanged. Every caller inherits it — the
device palette (`engine.rs`), a pattern's `setOutputPalette` (the firmware's `OutPipe`),
and the per-layer scene ramp (`compose.rs::ensure_lut`).

**`vm::sample_palette` itself is untouched**, deliberately. Its black-above-the-last-stop
edge is bug-for-bug Pixel Blaze for the `paint()` / `setPalette()` / `paintCanvas()`
builtins — established against the oracle on 2026-08-22 (docs/research/04-oracle-findings.md)
and pinned by `palette_edges_match_pixelblaze`. `setOutputPalette` is a documented *Luxel
extension* (docs/lang.md) with no PB behaviour to match, so the ramp is free to clamp where
`paint()` is not.

**Stored data is left exactly as it is, and there is no migration.** A stored palette or
scene ramp whose last stop is below 255 now renders the last stop's colour above it instead
of black — brighter, not darker, and only in the region the user never asked about. The two
alternatives were both worse: appending a black stop at 255 on first read would silently
rewrite the user's data, spend one of the 32 stop slots, and be wrong for the common case
(a ramp that already spans 0..255 is unaffected either way); and a wire-format bump has
real deploy consequences (#643 — an OTA across a format bump leaves every stored blob
unreadable and the device's own console cannot fix it). The behaviour change is written
down in docs/api.md, docs/lang.md, docs/spec/scenes.md and UPDATES.md, and anyone who
wanted the cut can have it back with one black stop — or the *To black* preset.

## Inventory item → how it is gone

The numbering is the #787 "Bug inventory (step 1)" comment.

| # | Defect | Removed by |
|---|---|---|
| 1 | never re-reads; next edit clobbers the device | editor re-reads while open; conflict strip; nothing sent until chosen (**S8i**) |
| 2 | a colour edit's 250 ms trailing commit reverts later gestures | no trailing snapshot commit — every write is computed from the current list; one write when the picker closes (**S8b**) |
| 3 | removing one of two scene stops destroys the whole ramp | *Remove stop* disabled at the minimum with its reason; destroying the ramp is a separate named, confirmed action (**S8h**, **S8i**) |
| 4 | empty position field → 0; empty amount → palette off | fields are two-way bound and repaint after clamping; an empty field is "no change" |
| 5 | `add stop` stacks past the fifth at 255, invisibly | *+ Add stop* lands in the widest gap; stacked stops carry a count badge (**S8h**) |
| 6 | first stop on an empty palette is black, amount 0 | no "add one stop into emptiness" path — the empty state offers presets and states the amount (**S8h**) |
| 7 | `clear` zeroes the amount | *Clear ramp* confirms and says the amount stays (**S8i**) |
| 8 | selection index outlives the stop; edits vanish | selection is the stop's identity; the row disappears with the stop |
| 9 | fields keep showing refused values | two-way binding; the field shows the value in effect |
| 10 | clicking above the last stop adds a black stop and changes the gradient | option A: it cannot happen — the ramp clamps, and a stop added in a clamped zone takes that end stop's own hex (**S8c**) |
| 11 | every arrow key is a full device write, no-ops included | a key that cannot move anything does nothing and writes nothing |
| 12 | clicking the bar at the cap does nothing, silently | the bar is a `not-allowed` zone and the affordance line states the reason (**S8h**) |
| 13 | a click in the 12 px gutter below the bar adds a stop | the handles are on the bar; there is no gutter |
| 14 | 13×13 px handles at phone width | 12 px painted in a **24×24** hit box (**S8g**) |
| 15 | two stops at one position are unreachable | collapse to one handle + count badge + *next of N* (**S8h**) |
| 16 | the main bar is not what the device does below 100 % | the bar is the ramp; the **LEDs** preview cell is the ramp at the current amount, labelled (**S8a**) |
| 17 | nothing states what the stage does | the statement line, in body text, always visible (every frame) |
| 18 | no undo, no presets, no reset | Undo (`Ctrl-Z`), six presets, Reset (**S8a**) |
| 19 | scene `clear` destroys the ramp with no confirmation | *Remove ramp*, confirmed, its consequence named (**S8f**, **S8i**) |
| 20 | the scene mount discards the commit-vs-live split | the mode is a visible property of the editor (**S8f**, **S8i**) |
| 21 | three different rules for a legal ramp | one printed rule: 0 (off) or ≥ 2, never 1 (**S8h**) |
| 22 | amount is a number input in the button row | a slider beside the preview (**S8a**) |

### "Confusing by design" → what the mock does

| # | Decision | What the design does |
|---|---|---|
| 1 | black above the last stop | **answered**: option A, the ramp clamps at both ends (**S8c**); the cut to black is the *To black* preset |
| 2 | two unlabelled bars | one bar (the ramp) + a labelled preview pair (pattern → LEDs at N %) |
| 3 | "the same editor as Settings › Output › Palette" printed in the scene mount | line deleted; it is literally one component now, and the summary states the stop count and amount instead (**S8e**) |
| 4 | `add stop` means "past the last one" | it means "into the widest gap"; clicking the bar means "here" |
| 5 | the detail panel renumbers as you drag | no `Stop N` header; the stop is named by position, in the UI and in `aria-label` |
| 6 | a permanent per-stop row list | one row, for the selected stop, present only while one is selected |
| 7 | four levels deep (Settings › Advanced › Output processing › Palette) | **not fixed** — restructuring Settings is a different ticket. Mitigated: the collapsed row shows the real LUT, and the editor expands *in place*, not into a dialog or a fifth level |
| 8 | editing a scene ramp restarts the layer's pattern clock | fixed: `SceneRenderer.setRamps()` re-installs the scene and rebinds the engines it already holds, so no clock restarts |

## What the implementation did with it

- **One component in both mounts** — `components/ColorRamp.svelte`, with
  `settings/OutputCard.svelte` and `components/scene/RampEditor.svelte` as thin adapters.
  `GradientEditor.svelte` is deleted. Closes #697 and #537.
- **`lx_palette_lut` from wasm is the single source** for the bar and both preview cells
  (#748 §1): `outpipe::fill_palette_lut` cooks it and `outpipe::palette_remap_frame` blends
  it, both the engine's own functions. `lib/gradient.ts`'s `rampLut` is the synchronous
  fallback for the first paint before the module is up, and
  `web/tests/paletteLut.test.mjs` pins the two against each other over every shape that
  hurts — a single stop, stops inside 0..255, a zero-width span, adjacent positions,
  falling channels, the 32-stop cap, amounts 0/1/33/50/99/100, and 50 seeded-random shapes.
  Closes #748.
- **The model is in `src/lib/gradient.ts`**, no DOM and no Svelte, with
  `web/tests/gradient.test.mjs` on it: where a new stop goes, the one legality rule, the
  clamped-zone colour, clustering and `next of N`, identity-based selection, presets,
  undo, the trailing commit and the emptied-field rule.
- **`mockdiff` element maps for S8a, S8b, S8e, S8f, S8g, S8h and S8i**, plus `--sweep`
  clean at 390. **S8c/S8d are deliberately not mapped**: they are a decision drawing — the
  two candidate semantics side by side, with a card header reading `A · clamp both ends`
  rather than `Color ramp` — and nothing in the app is or should be in that state. The
  semantic they decided is pinned by `outpipe.rs`'s
  `palette_lut_clamps_above_the_last_stop_and_below_the_first`, by
  `web/tests/paletteLut.test.mjs`, and by `web/tests/gradient.test.mjs`'s "the ramp CLAMPS
  at both ends".
- **A ramp edit no longer restarts the layer's clock** (confusing-by-design item 8):
  `lib/sceneRender.ts` grew `rampOnlyChange()` and `SceneRenderer.setRamps()`, which
  re-installs the scene and rebinds the engines it already holds instead of recompiling
  them. `pages/SceneEditor.svelte` routes a ramp-only wire change through it, immediately,
  instead of through the 1 s coalesced rebuild.
