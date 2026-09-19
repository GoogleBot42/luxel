# Luxel web UI v2 — design proposal

Status: APPROVED 2026-09-18 — all decisions D1–D12 made (see §9). Nothing built yet. Decision points are
numbered **D1…D12** and collected at the end.

Inputs: 42 screenshots of the current UI, a code audit of `web/src` (4052-line App.svelte),
a survey of the engine/firmware constraints, and a comparative study of PatternFlow, OBS,
WLED 2D, Pixel Blaze and LED-matrix fonts. All four live in the session scratchpad and are
summarized where they matter.

---

## 0. The thesis in three sentences

1. **One Layout object owns geometry.** Today geometry lives in five places and the map is
   presented as part of a pattern. v2 has exactly one Layout (kind + dimensions + optional map
   program) owned by the device, or chosen virtually in the playground, and *every* preview,
   tile, thumbnail and playlist row renders through it.
2. **Features are gated by Layout kind, not by hiding buttons.** Scenes (compositing), text
   layers and text builtins exist only when the Layout is a matrix. A strip user never sees them.
3. **A Scene is an ordered stack of Layers, and a playlist plays patterns or scenes.** A scene
   with one pattern layer *is* a pattern; playlist transitions fall out of the compositor.

---

## 1. What is wrong today (evidence, not opinion)

From the screenshots + audit:

| # | Problem | Evidence |
|---|---|---|
| 1 | The pixel map reads as per-pattern | It's a sub-tab of the pattern editor; it is autosaved in the pattern's working copy (`store.ts:58`); it ships inside share links (`App.svelte:1716`); every pattern load re-derives it; "+ New pattern" silently discards it (`:999`). Settings has **no geometry card at all**. |
| 2 | Three sources of truth for geometry, no reconciler | `layout` (preview rig), `devicePixels`, `deviceMap.kind/w/h` — combined in exactly one function (`deriveRig`), once per load, strip→grid only. The engine has a fourth (`preferred_dims`), and the firmware a fifth (the engine's fabricated √n grid, invisible to `/api/map`). |
| 3 | Mode-blind rendering | Every device-pattern row and playlist thumbnail is a 64-px 1D bar on every board (`PatternThumb` never receives `kind`). Gallery tiles are fixed 16×16 / 64-px constants regardless of the connected device. A 1D pattern on a 64×64 panel previews as a bar although the panel shows it row-major. |
| 4 | The editor's three bars mix four concerns | Playback bar = rig config (layout kind, px, W×H) + device mutation (install grid / install map / clear map) + transport (fps, pause) + tooling (sound, debug), separated by one `sep`. Device-mutating buttons look identical to preview-only ones. The only `.primary` button in the app is `share`, playground-only. |
| 5 | Errors are ~1000 px from their cause | Compile error banner top-right of the right rail; the offending line is far left. |
| 6 | Settings is flat, unsorted, board-blind | 9 cards, ~30 fields: brightness is card 3 under a read-only DDP diagnostic; six expert Output knobs at the same weight as Color order; LED protocol / color order / power cap shown on a HUB75 console where they are meaningless; Pixels is a scalar where a panel wants W×H. |
| 7 | Two pattern browsers, two behaviours | Library = animated searchable tiles; Device Patterns = static list rows with `edit ›` and no "which one is running". |
| 8 | Naming and confirming go through `window.prompt`/`confirm` (8 sites) | The only way to name a pattern is a native prompt. |
| 9 | Desktop-only editor | At 390 px the right column is clipped and unreachable. |

What works and is kept as-is (audit §7): `fetchgate`, the local-preview-plus-push model, the
capacity banner idiom (certainty-graded, never blocking), the gallery tile scheduler, the
Luxel-program mapper and its debugger (only its *placement* is wrong), `Controls.svelte`'s
"guess" treatment, `data-role` e2e hooks, the boot cover, the LNA classifier, the 9-token
palette, the playlist's optimistic transport.

---

## 2. Concept model

The one-sentence test every screen must pass (from the comparative study): **a scene is an
ordered list of layers; the top layer draws in front; each layer is a pattern, some text, or an
image, placed in a rectangle and blended.** Five nouns and one ordering rule. PatternFlow —
the reference Jeremy pointed at — needs thirteen nouns (pattern, layer, ramp, knob lane, deck,
module, header, show, cue, edition, composition, feature, variant), three separate web UIs, nine
dockable panels and a compile step before an LED lights; its complexity is concept count and
surface count, not feature count. It also has no text layer at all, so text is differentiation
for Luxel, not catch-up. Two ideas worth stealing from it and from FPP: a per-layer color ramp
(makes a monochrome pattern reusable as a layer — it's our existing output palette, per layer)
and "black is transparent" as a one-checkbox compositing rule that needs no alpha channel.

```
Device / Playground
└── Layout            ← the ONE geometry object (D1 name)
    dims:   1 | 2 | 3                          ← what everything downstream keys off
    source: regular | map                      ← "custom" is a SOURCE, not a dimensionality
    regular 1D (strip):   pixels
    regular 2D (matrix):  w, h, wiring, panels (cols, rows, chain)
    regular 3D (lattice): w, h, d              [preview-only in v2; device = map]
    map:    map program (Luxel, plot(x,y) | plot(x,y,z)) + pixel count → coords; dims from
            the program; detect_grid may promote a 2D map to "regular" for gating
    outputs[]: {pin, protocol, pixel range | panel set}  ← wiring segments of ONE pixel space
            (D11: the Athom has two outputs; one Layout spans them — a strip Layout splits
            its index range across outputs, a matrix Layout assigns panels to outputs in the
            arrangement widget. One playlist, one scene, one HA light. "Independent displays"
            = Layout[] later, not now; the firmware has one output driver today.)

Pattern              ← unchanged: source + controls; stored on device or in the browser
Values               ← a playlist item / scene layer OWNS its control values inline (D6: no named presets)

Scene                ← matrix only. Ordered stack, bottom → top.
└── Layer[]
    type: pattern | text | sprite | color        (image-from-file = future ticket)
    common: name, visible, opacity 0–100, blend (normal|add|lighten|multiply|mask),
            box {x, y, w, h} in pixels, fit (fill|contain|tile), flip, rotate 90°
    pattern: pattern id + control values, key: none | black-transparent | luma-alpha
    text:    text (literal | clock | text slot n), font, color, align, scroll, speed
    sprite:  a sprite-tagged PATTERN in the pattern store (palette ≤16 colours, w×h ≤ 64×64,
             frames ≥1); drawn with the cursor on the scene preview; blitted natively by the
             compositor (no engine, no pattern-layer slot); always black-keyed
    color:   solid color (a wash / a bar)

Playlist             ← items are Pattern (+values) OR Scene; duration; crossfade
```

Two engine facts shape the product (engine survey §1–3, §8):

- Pattern layers are the expensive kind: each is a full engine plus a 12 KB internal-DRAM frame
  buffer at 4096 px. The crossfade already runs **two** engines; a persistent two-pattern scene
  is a small generalization. A third pattern layer at 64×64 does not fit today.
- Text, image and color layers need **no engine** — they are drawn natively into the composite
  (the same blit kernel `bulk.rs` already uses for sprites, ~1 µs per texel). They are effectively
  free, and there can be several.

So the product rule is honest and simple: **a scene has up to 2 pattern layers and any number
of text / image / color layers** (per-board; strips never see scenes). The UI states the
per-layer frame cost live (fps readout) instead of pretending layers are free.

### Feature visibility by Layout kind

|                          | strip | matrix | volume | custom |
|--------------------------|:-----:|:------:|:------:|:------:|
| Patterns page            |   ✓   |   ✓    |   ✓    |   ✓    |
| Playlist                 |   ✓   |   ✓    |   ✓    |   ✓    |
| **Scenes** (compositor)  |   –   |   ✓    |   –    |   –    |
| Text builtins in editor  |   –   |   ✓    |   –    |   –    |

| Renderer picker in editor|   –   |   –    |   –    |   –    |
| Preview / tile shape     |  bar  |  grid  | cloud  | scatter|
| Map program editor       |   –   |   –    |   –    |   ✓    |

"custom" that *is* a regular grid (detected by the existing `detect_grid`) counts as matrix.

---

## 3. Terminology (D1, D2)

| Concept | Recommended | Alternatives considered | Why |
|---|---|---|---|
| Geometry object | **Layout** | Fixture (xLights/LedFx jargon), Display (odd for a strip), Map (PB's word — but "map" is what the *custom* kind uses) | Plain English, already the code's word, reads well in "LED layout: 64×64 matrix". |
| Layout kinds | **Strip · Matrix · 3D · Custom** | 1D/2D/3D (engineer-speak) | Users know what a matrix is; "3D" is fine as a label because it is rare. |
| Compositor unit | **Scene** | Composition, Mix, Stack, Show | OBS/LedFx word; short; a playlist "plays scenes" reads naturally. |
| Scene element | **Layer** | Source (OBS) | "Overlay/underlay" is layer language; Photoshop/Figma users get it instantly. |
| Layer kinds | Pattern · Text · Image · Color | Sprite (implies animation frames) | Image now; animated image later without renaming. |
| Saved control values | **Preset** | Look, Snapshot | Jeremy's own word. |
| Hardware-bound UI | Console · hardware-free UI: Playground | (existing) | unchanged |
| Playground geometry | "Preview as …" | Virtual device | It's a chip in the header: `Preview as 64×64 matrix ▾`. |

---

## 4. Information architecture

### Console (served from / bound to a device)

```
Header:  luxel  ● luxel-f6b0a8 · 64×64 matrix      Patterns  Scenes  Playlist  Settings        27 fps
```
- **Patterns** — one page, one tile grid, two sources via a segmented control:
  `On device (5)` | `Library (307)`. Tiles are the device's shape. The running pattern has a
  ring + "playing" pill. Tile actions: Play, Edit, ⋯ (duplicate, add to playlist, delete).
  Picking a Library tile on the console opens the editor with "Save to device" as the primary.
  (Playground: `Library` | `Mine`.)
- **Scenes** (matrix only) — list of saved scenes with composite thumbnails; "+ New scene".
  Opens the Scene editor.
- **Playlist** — as today, items are patterns or scenes, thumbnails in device shape, values
  collapsed behind a chip.
- **Settings** — basics first, Advanced disclosure below (§5.3).
- **Editor** — on-demand full-screen, as today (this was right).

### Playground

```
Header:  luxel playground        Patterns  Scenes        Preview as  64×64 matrix ▾      60 fps
```
The "Preview as" chip is the playground's Layout. Options: Auto (follow the pattern:
3D › 2D › 1D) · Strip [n] · Matrix [w]×[h] · 3D lattice · Custom map program. **Shape only —
no wiring** (Jeremy, 2026-09-18): serpentine, start corner and outputs are console settings in
LED layout, because they describe hardware. Wiring is invisible to `render2D` and the bulk
grid ops (they see coordinates) but it changes what a 1D pattern looks like under the "By
index" projection (rows alternate direction) — so the console's preview runs the local engine
with the device's real wiring (the geometry store hands it the index→coordinate map, and
`frame_grid` carries `serpentine`), while the playground's "By index" assumes row-major.
Scenes tab: D10. No playlist, no settings (nothing to persist to).

### What the map program becomes
The mapper (a Luxel program with `plot()`, debuggable) is kept whole, but it is reached from the
Layout picker (Settings → LED layout → Custom → "Edit map program"; playground → chip → Custom),
not from inside a pattern. It gets its own full-screen editor identical in chrome to the pattern
editor (code left, scatter preview right, debugger available). "Install on device" is that
screen's single primary action.

---

## 5. Screens

(Mockups: batch 1 = S1–S5 below; batch 2 = Scene editor + fonts.)

### 5.1 Patterns (S1, S1b, S1c)
- Tile shape = Layout shape. On a 64×64 console every tile is square; on a 300 px strip every
  tile is a bar. A 1D pattern on a matrix gets a dim caption `1D · shown row-major` and its tile
  renders exactly what the panel will show (the engine's actual fallback), not a bar.
- Running tile: 2 px green ring + `▶ playing`. Hover strip: `▶ Play · Edit · ⋯`.
- Mobile: 2 columns, tap = play, `Edit` link under the name.

### 5.2 Editor (S2, S2b)
- Header: `← Patterns` · inline-editable name · `saved · on device` · **Save** (primary) · ⋯
  (Add to playlist, Duplicate, Export .epe, Import .epe…, Delete; Share in playground).
- Code pane owns its errors: gutter dot + wavy underline + a one-line status strip pinned to the
  bottom of the code pane (`✗ line 14 · unknown identifier "nosie"`). No banners in the rail
  for compile errors.
- Right rail: **Preview** (header = label + `64×64 · 27 fps on device` + icon transport:
  play/pause · fps ▾ · mic · debug) → capacity line (existing idiom) → **Controls** → **Vars**
  (collapsed). No layout dropdown, no pixel field, no sub-tabs, no install buttons.
- Matrix consoles: the editor's autocomplete/docs include the text builtins; strips don't.
- Mobile: rail stacks above code; code is read-mostly.

### 5.3 Settings (S3, S3b)
Order is by frequency of use, then a labelled **Advanced** divider:
1. **Device** — Name · Brightness (large, first control on the page).
2. **LED layout** — the summary in big type (`128 × 128 matrix`) with a live thumbnail of the
   device shape; then the kind picker and its fields. Strip board: Strip/Matrix/Custom, Pixels,
   LED type, Color order, Data pin. HUB75 board: Matrix (Strip disabled with a hint).
   `Custom map program →`.
   **Panel arrangement (S3c, S3d).** Panels tile in x AND y, so the matrix fields are:
   panel size + scan (HUB75) or pixel wiring (strip-built matrix: start corner, run direction,
   serpentine); `Panels [c] across × [r] down`; chain: start corner, run direction, serpentine
   (snake), rotate alternate rows 180°. A live SVG draws the panel grid with the chain path
   numbered from the `IN` connector, per-panel scan direction, and the resulting total size —
   the graphic is the spec; prose can't describe a snaked 3×2 chain. The same widget, one level
   down, describes pixel wiring inside a WS2812 matrix. Stated constraints: one chain per
   output on this board; every panel in a chain has the same size and scan; geometry applies
   after a reboot. An **estimated refresh** readout (`57 Hz`, amber under 100 Hz) computed from
   panels × planes × clock, with the fix named (fewer planes in Advanced › Panel driver, or
   fewer panels per chain). Firmware side is #401 (runtime geometry) — the arrangement adds
   `cols rows start dir snake rot180` to `/api/layout matrix …`; the firmware builds the
   panel→pixel remap once at boot and the engine sees one W×H grid.
3. **WiFi** — connected network + `Change network…` (form collapsed).
— Advanced — "Settings most installs never touch."
   `▸ Output processing` (gamma, brightness curve, power cap, blur, glow, palette) ·
   `▸ Panel driver` (HUB75 clock, planes, live rescan Hz — board-gated) · `▸ Clock & time zone` ·
   `▸ Multi-device sync` · `▸ MQTT · Home Assistant` · `▸ Network input (DDP / E1.31)` ·
    `▸ Storage` · `▸ Firmware & recovery` (version, update, setup AP).
   Each row carries a one-line status so a collapsed page still answers "is X on?".
Reboot-requiring actions are labelled as such and grouped; nothing reboots from a bare button.

**Capabilities, not board names (S3, S3i).** The device advertises what it can do in a
`caps` block on `/api/status`, computed by the firmware from board features + the current
Layout, and the UI shows a setting only when its capability is advertised — absent, never
disabled. Today's only such signal is the presence of `data_pins` in `/api/config`, which
the UI reads as "this is a panel"; that inference is what rots. Per field, from what the
firmware does now:

| Field | Needs | Strip | HUB75 panel | Regular 2D from strips | 3D / irregular map |
|---|---|:-:|:-:|:-:|:-:|
| Gamma · Brightness curve · Palette | nothing | ✓ | ✓ | ✓ | ✓ |
| Power cap (mA) | a per-pixel current model | ✓ | – (rec: hide; the firmware has a scan-aware panel model, but a panel is a fixed load on a supply sized for it) | ✓ | ✓ |
| Blur · Glow | neighbours: index order on 1D, rows/cols on a regular grid | ✓ "along the strip" | ✓ "across the panel" | ✓ | – (no neighbour table yet) |
| Color order · LED type · Data pin | a strip driver | ✓ (in LED layout) | – | ✓ | ✓ |
| Panel driver (clock, planes, scan) | HUB75 | – | ✓ | – | – |
| Scenes · Text · Fonts | a regular 2D grid | – | ✓ | ✓ | – |

`caps` also carries `layers` (pattern layers this board affords, 2 on the S3 panel) so the
scene editor's "2 of 2 used" note is data, not a UI constant. The mirror advertises what it
implements, which also documents its drift from the firmware.

### 5.4 Playlist (S4, S4b)
- Items are **pattern + its own values** (inline, edited in place; no named presets — D6) **or scene**. A scene
  row has no values chip — its layers own their values — and offers `Edit scene ›` instead.
- Transport group left (Play primary · stop · prev/next · now-playing with progress); defaults
  right (duration, crossfade, ⋯ → clear). `+ Add` at the bottom of the list opens ONE picker
  used everywhere: patterns and scenes.
- Rows: handle · thumbnail in device shape · name + type (`Pattern` / `Scene · 2 layers`) ·
  duration chip (override = accent) · `3 values ▾` chip (expands inline
  sliders) · ✕.
- Mobile: same rows, smaller thumbnails; this is the primary mobile surface.

### 5.4b "Add to…" flows and values (D6 decided: inline values, no named presets)
- Editor ⋯ menu: `Add to playlist` (captures the current slider values as the new item's
  values) · `Add to scene ▸` (matrix only; lists scenes + `New scene…`, adds the pattern as the
  top layer) · Duplicate · Export/Import · Delete. Tile ⋯ on the Patterns page: the same.
- These are shortcuts from the place you just tuned the look. The primary path to build a scene
  is the scene editor's `Add layer → Pattern` picker (OBS-style: you add sources from the scene).
- Values live in exactly three places and are edited in place: the editor (the running
  pattern), a playlist row, a scene layer. Named, shared presets were dropped (Jeremy,
  2026-09-18: "confusion is an issue") — every edit would have raised a detach-or-update
  question. Filed as a future ticket driven by the HA "named look" use case.

### 5.3b Multiple outputs (D11 = A, decided 2026-09-18; S3j, S3k)
- **Rule:** an output drives a **consecutive run of the Layout's index space** — a pixel range
  on a strip, a run of panels along the existing chain order on a matrix. Outputs split a
  sequence the arrangement widget already defines; there is no second wiring model.
- **LED layout gains an Outputs table**, shown when `caps.outputs > 1`. Row = pin · LED type ·
  color order · pixel count (strip) or panel count (matrix) · `reverse` (a run wired
  backwards) · computed range (`pixels 300–599`, `panels 3–4`). A single-output board keeps
  those fields inline as today; a board with a spare output shows `+ Add output`.
- **Graphics:** strip = one line split into output-tinted segments with an IN marker and
  direction arrow per run; matrix = one chain polyline per output from its own IN marker,
  panels tinted by output. Chain start/direction/snake stay global.
- **Everything downstream is unchanged:** one pixel count, one grid, one projection, one
  playlist, one scene, one brightness, one power cap (supplies are usually shared; per-output
  caps can come later), one HA light. "Independent displays" would be `Layout[]` — not now.
- **Wire:** `/api/layout` gains one `out <n> <pin> <proto> <order> <count> [rev]` line per
  output (line-oriented like the playlist). **Firmware:** one output driver instance today →
  ticket A13 (second SPI/RMT instance, frame split by run, per-output encode buffer; the
  power model sums runs).

### 5.4d Projection — how a pattern made for another layout is shown (S3e, S2c)
Jeremy's point: 1D patterns don't render "correctly" on a matrix by index-mapping, and a
"true vs device" toggle on the Patterns page is unnecessary if the mapping itself is
configurable. So:
- **Rule (Jeremy, 2026-09-18):** the Projection block shows ONLY the pattern kinds that are
  not native to the current Layout; each picker offers ONLY the axes that layout has; a
  single-option case is a one-line note, never a disabled control; card art takes the
  layout's shape (bars on a strip, a cube on 3D, the map's outline on a custom map).
- **The full table** (By index = today's fallback everywhere; "along an axis" = render one
  strip and replicate, cheap on any layout):

  | Layout dims | 1D patterns | 2D patterns | 3D patterns |
  |---|---|---|---|
  | 1D (strip) | native | Middle row (y = 0.5) · Middle column (x = 0.5) | Line through the centre along x · y · z |
  | 2D (matrix, or a custom map that plots x,y) | By index · Along x · Along y | native | Slice xy (z = 0.5) · xz · yz |
  | 3D (lattice, or a custom map that plots x,y,z) | By index · Along x · Along y · Along z | Repeat along z (xy image extruded) · y · x | native |

  **"Custom" is a coordinate SOURCE, not a dimensionality** (Jeremy, 2026-09-18): a Layout has
  `dims` (1/2/3) and a `source` (regular fields, or a map program whose `plot(x,y)` vs
  `plot(x,y,z)` fixes the dims). Projection keys off dims alone — a slice or an axis is a
  coordinate substitution and doesn't care whether pixels sit on a grid. Regularity matters
  for exactly two things: what "By index" means (rows vs wiring order), and which native
  features exist (Scenes and text need a real grid, so a custom 2D map gets them only when it
  detects as a regular W×H matrix — the engine's existing `detect_grid`). Bulk `renderFrame`
  patterns follow the 2D row (on a strip they get a w×1 grid).
- **Engine win, not just UI**: `stretch` renders the strip ONCE per frame (w or h calls to
  `render`) and replicates it — a 1D pattern on the 64×64 panel becomes 64× cheaper and
  `pixelCount` reads as 64, which is what its author assumed. Implemented in `luxel-core`, so
  device, mirror and playground agree by construction.
- **Where it lives**: a **device default** in Settings → LED layout → Projection (selectable
  cards with a live strip projected each way), and a **per-item override** — projection is
  "how this pattern is presented on this layout", so it is saved with a playlist item's or a
  scene layer's values like any slider. It never touches the pattern source (the map mistake).
- **Wherever values are edited** (editor Controls, a playlist item's values, a scene layer's
  inspector) the row is (a) **visible only when it can matter** — the pattern's dimensionality
  differs from the layout's native one AND that layout offers more than one option; a 2D pattern
  on a matrix shows nothing, a 3D pattern on a custom 2D map shows nothing; and (b) **advanced
  by weight**: after the pattern's own controls, under a hairline, one quiet line
  `Projection  device default · along x  change` that expands the same cards Settings uses.
  An override reads in accent (`along y · override  reset`) so a playlist shows at a glance
  which items deviate (S2c, S2d).
- Tiles and previews render through the effective projection, so no toggle; the tile caption
  reads `1D · along x`.
- Wire: `/api/layout … proj1d=<index|x|y|z> proj2d=<x|y|z> proj3d=<x|y|z|xy|xz|yz>` for the
  defaults (only the fields meaningful for the layout kind are accepted); a playlist item or scene layer carries an
  optional `proj` field beside its control values.

### 5.4c When the Scenes tab shows (D10)
- Console: strictly by Layout kind. A matrix console always has the tab (empty state:
  "+ New scene"); a strip, 3D or custom console never does.
- Playground: no hardware constraint, so the tab is always present; when the "Preview as"
  chip is not a matrix the page is a one-line empty state with a primary
  `Preview as a 64×64 matrix` button. Hiding it there would make the feature undiscoverable.

### 5.5 Scene editor (S6 — batch 2)
```
← Scenes   [Clock overlay]   saved · on device            Play on device ▶   Save   ⋯
┌───────────────────────────┬──────────────────────────────────┬──────────────────────┐
│ LAYERS (top = front)      │  PREVIEW  64×64 · 24 fps         │ INSPECTOR             │
│ ⠿ 👁 T  Time   "12:48"    │  ┌────────────────────────────┐  │ Text layer            │
│ ⠿ 👁 ▤  Aurora 2D  60 %   │  │                            │  │ Text  [12:48      ]   │
│ ⠿ 👁 ▤  Rainbow  (base)   │  │   (composite, live)        │  │ Source ○ fixed        │
│                           │  │   selected layer's box     │  │        ● clock HH:MM  │
│ + Add layer ▾             │  │   outlined & draggable     │  │        ○ from API/HA  │
│   Pattern · Text · Image  │  └────────────────────────────┘  │ Font  [5×7 ▾]         │
│   · Color                 │  fps cost: 2 pattern layers      │ Color [■]  Align [⋮]  │
│                           │                                  │ Box  x 0 y 28 w 64 h 8│
│                           │                                  │ Scroll [none ▾]       │
│                           │                                  │ Blend [normal ▾] 100 %│
└───────────────────────────┴──────────────────────────────────┴──────────────────────┘
```
- The layer list is the OBS/Photoshop idiom: drag to reorder, eye to hide, click to select.
  "Add layer" is the only primary in the left column; adding a second pattern layer when the
  board can't afford it shows why ("this device fits 2 pattern layers; text and image layers
  are free").
- The preview is the composite, always the device shape; the selected layer's box is outlined
  and draggable/resizable on the canvas (pixel-snapped). That is the one direct-manipulation
  affordance in the whole app, and it earns its place.
- Inspector is per layer type. Pattern layer: pattern picker (device patterns, thumbnails) +
  its controls + color ramp + box/fit + blend/transparency/opacity (S7b); Text layer as
  drawn; Image layer: asset picker (future, not planned); Color.
- **Compositing model (no alpha channel — frames are RGB888, 3 B/px):**
  - *Blend* = how a layer's counted pixels combine with what is beneath. Per pixel, B = what
    is beneath, L = the layer, α = opacity (scaled per pixel by luma under the "by
    brightness" key); keyed-out pixels leave B untouched in every mode:
    **Normal** `B + α(L−B)` (the everyday mode; the only one where the key matters) ·
    **Add** `min(255, B + αL)` (light adds up; black free-transparent; sparkles over a wash) ·
    **Lighten** `max(B, αL)` per channel (keeps the brighter; never clips; two patterns
    sharing the panel; `blit` mode 2 today, cheapest) · **Multiply** `B·L/255` faded to B by
    α (white leaves B, black kills it: tint, shadow, roaming spotlight) · **Mask**
    `B·luma(L)/255` (brightness-only stencil: text set to Mask over a rainbow base =
    rainbow-filled letters). Screen deliberately omitted for now — Lighten covers its use
    with shipped code; add later if missed. All ≤ a few multiply-adds/px, < 1 ms at 4096 px.
  - *Transparent* = which of the layer's pixels count, a **key** rather than alpha — shown
    only while Blend = Normal, because Add/Screen/Multiply carry their own (adding black or
    multiplying by white changes nothing): **nothing** (opaque box — the base layer, or a
    wash at low opacity), **black pixels** (skip exactly-black pixels; `blit` mode 3, FPP's
    rule; hard-edged, cheap, right for sprites/text/comets on black), **by brightness**
    (luminance becomes opacity: black transparent, white opaque, fades blend; one luma + one
    lerp per pixel; right for glows and fire).
  - *Color ramp* = the engine's existing output-palette stage (luma → gradient of stops)
    applied per layer after its own post-chain and before compositing. A white-on-black or
    single-hue pattern becomes any gradient without touching its code (PatternFlow's per-layer
    ramp). Cost: a 768 B LUT per layer + one lookup per pixel. Same stop editor as Settings →
    Output → Palette.
- Mobile: three panes stack (preview sticky on top). Editing scenes on a phone is acceptable but
  not a design driver.

### 5.5b Sprite layer + pixel drawing (S7c; Jeremy, 2026-09-18)
- **Storage = a sprite-tagged pattern** in the existing pattern store: palette-indexed pixel
  array (≤16 colours) + `blit`, exactly what `library/bulk-sprite-scroll-2d.js` hand-writes.
  No new record type (the store's no-migration rule), no upload path, identical in the
  playground, playable on its own, up to 64×64 (source ≈ 8 KB, const pool 16 KB, under
  `MAX_SOURCE`/`MAX_BC`). A `frames` dimension is in the format from day one; the editor's
  frame strip appears at 2+ frames (later).
- **Runtime:** the compositor recognises the sprite tag and blits the const array natively
  (`blit` mode 3, ~1 µs/texel) — no engine, so a sprite does NOT consume one of the two
  pattern-layer slots. Always black-keyed; an erased pixel is transparent.
- **Drawing:** on the scene preview, at the layer's box, pixel-snapped. Selecting a sprite
  layer adds a tool row above the canvas (pencil · eraser · fill · colour + recent swatches);
  click/drag paints. Reuses the preview-click path that already feeds `readEvent`.
- Inspector: name · size · frames · palette · box (w/h mirror the sprite) · blend · opacity.
  Transparency is fixed (black key), shown as a line, not a select.

### 5.6 Fonts (built-in only — no Settings section; the text layer's font picker is the UI)
**Decision 2026-09-18:** user font upload is a possible future feature, filed and not planned.
Consequently there is no Settings › Fonts group (S8 dropped) and no `/api/fonts` beyond the
built-in names; the text-layer inspector's font picker lists the three built-ins with a glyph
preview.
- Built-in fonts ship inside `luxel-core` as `include_bytes!` (identical on device and
  playground, ~1.2 KB total, no attribution plumbing). Bundle from the comparative study, all
  public-domain / CC0 / BSD-2 after primary-source license checks:
  **tiny** = Tom Thumb (3×5 ink in a 4×6 cell, ~190 B, ~16 chars across a 64-wide panel) ·
  **regular** = X11 misc-fixed 5×7 (public domain, 420 B; the exact `.raw` embedded-graphics
  ships) · **large** = Spleen 5×8 or misc-fixed 6×10 (BSD-2 / PD). Add Tiny5 (OFL, 1,749
  glyphs) later if non-ASCII is wanted. Rejected: Minecraftia (personal-use only), Adafruit's
  Picopixel/Org_01/Tiny3x3 (no per-font license), u8g2 fonts (its RLE *loses* at 5–8 px and the
  big faces are 300 KB).
- Font blob format on the device: **PSF2** (32-byte header + `length × charsize` MSB-first
  row-packed glyphs, O(1) lookup, ~20 lines of no_std Rust). Not a bespoke format. Per-glyph
  advance widths as a separate optional table so monospace v1 doesn't paint us in.
- User fonts: the browser is the front door — BDF, GFX `.h`, PNG sheet, or TTF rasterized with a
  live 64×64 threshold preview → PSF2 → device (this is what WLED's Font Factory, ESPHome and
  embedded-graphics all do; nobody parses BDF/TTF on the MCU). **Blocked today** by
  whole-bundle-only asset upload (engine survey §5); a future ticket, not planned.
- Fonts are referenced by name everywhere (text layer font picker, `font("tiny")` in patterns).

---

## 5.7 Conditional-visibility audit (Jeremy, 2026-09-18: "audit to see if there are similar things")

Rule restated: a control is **absent** unless the thing it acts on exists — decided by the
device's `caps`, the Layout's dims/regularity, or the pattern's own exports. The single
deliberate exception is a control that is *disabled with a reason* because the user must learn
a budget (Add layer → Pattern at the layer cap). Firmware facts checked: clock (SNTP), sync,
MQTT and network input are unconditional in every build; the variable axes are the board,
`hub75`, `psram-arena`, `hosted-ui`, `small-chip`; the engine exposes `wants_sensors()` and
`exported_vars()`.

| Where | Control | Visible only when | Was wrong in a mockup? |
|---|---|---|---|
| Shell | Scenes tab | regular 2D (console); D10 (playground) | — |
| Shell | device chip, "on device" fps | console | — |
| Patterns | `On device` segment · `Play` tile action | console (playground: `Mine` · `Open`) | — |
| Patterns | projection caption on a tile | pattern dims ≠ layout dims | — |
| Patterns | PixelBlaze Library segment | corpus JSON present (dev-only, as today) | — |
| Editor | mic / "sound" button | pattern reads sensor bindings (`wants_sensors`) | **S2 showed it always** → fixed |
| Editor | VARS section | pattern exports ≥1 var | **S2 implied always** → absent when none |
| Editor | Pins panel | pattern exports pin controls (as today) | — |
| Editor | capacity line | console with a known `heap_free` (as today) | — |
| Editor ⋯ | Share | playground | — |
| Editor ⋯ | Add to playlist · Delete (device) | console | — |
| Editor ⋯ | Add to scene ▸ | regular 2D | — |
| Editor | text builtins in completions/docs | regular 2D | — |
| Editor | projection row in Controls | dims differ AND >1 option | — (fixed earlier) |
| Settings | `Layout` select | the board offers a choice (strip boards: Strip/Matrix/Custom) | **S3/S3c showed a disabled Strip on HUB75** → select removed; summary line carries it |
| Settings | LED type · color order · data pin | `caps.strip_driver` | — |
| Settings | panel size/scan/chain/arrangement · estimated refresh | `caps.panel` | — |
| Settings | pixel-wiring rows | regular 2D built from strips | — |
| Settings | brightness hint text | per driver (SK9822 current limiter / WS2812 software / HUB75 planes) | — |
| Settings | Reboot into setup AP · Update… | `caps.reboot` / `caps.ota` (real firmware, not the mirror) | — |
| Settings › Advanced | Panel driver | `caps.panel` | — |
| Settings › Advanced | Fonts | regular 2D | — |
| Settings › Advanced › Storage | PSRAM line | `caps.psram` | — |
| Settings › Advanced | Output processing fields | per-field caps (§5.3) | fixed earlier |
| Settings › Advanced | Clock · Sync · MQTT · Network input | always (unconditional in firmware) | — |
| Playlist | scene items in the `+ Add` picker | regular 2D | — |
| Scenes | Add layer → Pattern at the cap | **disabled with reason** (the exception) | — |
| Scenes | Add layer → Sprite | regular 2D (always; drawn, not uploaded) | **S7 lacked it** → added (S7c) |
| Scenes | Add layer → Image from file | `caps.assets` (future, not planned) | removed |
| Scenes | sprite drawing tool row above the preview | a sprite layer is selected | — |
| Scenes | text source → Clock | always compiled; shows "not synced" state rather than hiding | — |
| Scenes | text source → Text slot | `caps.text_slots` (Phase C) | — |
| Scenes | Transparent (key) row on a pattern layer | Blend = Normal | **S7b showed it under Screen** → moved under Blend, Normal only |
| Scenes | Scroll speed row | scroll ≠ none | **S7 showed a disabled slider** → removed |
| Fonts | Upload font… · uploaded rows | `caps.assets` (future, not planned) | **S8 showed upload** → removed |
| Playground | Playlist · Settings tabs | never | — |
| Playground | wiring options in the "Preview as" chip (serpentine, start corner, outputs) | never — wiring describes hardware | **S5 showed a serpentine box** → removed |

## 6. Text in patterns (D5)

The pattern language has no string type. The cheapest honest path (engine survey §8 ii b):
- The compiler accepts string literals **only as arguments to text builtins**, interns them into
  a `strings` section (same machinery as the existing `msgs` table, ≤255 B each, deduplicated,
  survives lean decode), and passes an opaque handle. No heap strings, no new Value semantics
  visible to patterns, every existing blob stays valid.
- Builtins (append-only table, `builtin_cold` tier, bodies in `bulk.rs` beside `blit`), three
  mechanisms that cover disjoint needs and are each small:
  1. literals: `drawText("HELLO", x, y[, align])` · `textWidth("HELLO")`;
  2. host-set **text slots** (8 per device, written by the UI / `POST /api/text` / MQTT / an HA
     text entity): `textSlot(n)` returns a text handle usable wherever a literal is —
     `drawText(textSlot(0), x, y)`, `textWidth(textSlot(0))`. One draw verb, not a
     `drawTextSlot` twin (Jeremy, 2026-09-18). This is how a Scene text layer gets its content
     with zero pattern code (it writes slot 0), and how an HA notification reaches a pattern;
  3. numbers: `drawNumber(value, x, y, digits, decimals)` (fixed-point aware; clocks, counters,
     temperatures — most of what people put on a panel).
  Plus modal state `font("tiny"|"regular"|"large")` and the existing brush color. One origin
  convention: top-left, always. Text draws in grid space and is a no-op without a grid, like
  `blit`. Scrolling is a single `Position` enum (static / left / right / up / down / bounce) at
  pixels-per-second, plus an optional trail — not direction + reverse + mode flags.
- Dynamic text from outside (HA/MQTT/API) therefore needs no string type: `POST /api/text
  <slot> <utf8>` and an HA `text` entity per slot. (D5)

---

## 7. API changes (all flat match arms, line formats ≤ 4 KB, no new body kinds)

| Change | Why |
|---|---|
| `GET /api/status` gains `geom {kind, w, h, source, pattern_dims}` | The UI must know the engine's *effective* geometry, including the fabricated √n grid; today it needs three endpoints and still can't see it. |
| `GET/POST /api/layout` (supersedes `/api/config` pixels + `/api/map grid`) | One object for the one concept: `strip <n>` / `matrix <w> <h> [serp]` / `custom <dims> …` (custom stays `grid`/coords wire as now). Keep old endpoints as aliases for a release. |
| `GET/POST /api/scenes`, `/api/scenes/<id>`, `/api/scenes/<id>/activate` | Line format like the playlist; blob under a new reserved key (≤3840 B ≈ 45 layers). |
| Playlist `I` lines accept `S<id>` for scene items | Playlist plays scenes. |
| `POST /api/text <slot> <text>` · `GET /api/text` | Text slots for HA/MQTT/API-driven text. MQTT/HA: a `text` entity per slot. |
| `POST /api/assets/<name>` (per-file, streaming) | Enables user fonts and images without repacking the web bundle. Later phase. |
| `GET /api/fonts` | Names of built-in + uploaded fonts for pickers. |
| Playground: `lx_outpipe(...)` in luxel-wasm | The playground does not run the device output chain today; previews already diverge by the whole Settings chain. Required before layers, independent of everything else. |

Mirror (`luxel serve`) must gain every one of these so device-e2e can drive them.

---

## 8. Phasing and tickets

Each phase is independently shippable and verifiable in chromium; firmware items are separate
tickets so web work never waits on flashing.

**Phase A — Foundation (web + small firmware)**
A1 web: app shell decomposition (App.svelte → shell + stores + pages) keeping every `data-role`.
A2 web: `stores/geometry.ts` — the one Layout reconciler; every preview/tile/thumb reads it.
A3 fw+mirror: `/api/status.geom`. A4 fw+mirror: `/api/layout`. A5 wasm: `lx_outpipe`.
A6 web: Patterns page (unified browser, device-shaped tiles, running marker).
A7 web: Editor v2 (header/primary, inline errors, preview-header transport, no layout UI).
A8 web: Settings v2 (basics/advanced, LED layout section, board gating).
A9 web: Playlist v2 (values chips edited in place, device-shaped thumbs, + Add picker, mobile).
A10 web: Map program editor as its own screen from the Layout picker.
A11 web: in-app dialogs replacing window.prompt/confirm.

**Phase B — Compositor**
B1 core: persistent 2-layer blend generalizing the crossfade (opacity + blend mode + key).
B2 core: native text/image/color layers drawn into the composite; scene record + `/api/scenes`.
B3 fw: budget accounting for two resident engines (`budget.rs` per-layer, `engine_heap` sum).
B4 web: Scenes page + Scene editor (layer list, inspector, draggable boxes). B5: playlist
scene items. B6: composite thumbnails.

**Phase C — Text**
C1 core: `strings` section + string literal in text-builtin args. C2 core: font blob format +
built-in fonts in `luxel-core`; `text/textNum/textWidth/font` builtins. C3 fw: text slots
`/api/text` + HA text entities. C4 web: editor completions/docs gated by Layout; fonts section.

**Future — filed, not planned (Jeremy, 2026-09-18)**
One ticket: per-file streaming asset upload → user fonts (BDF/PNG/TTF → PSF2 in the browser),
image layers, animated images. Nothing in Phases A–C depends on it; the design leaves the
`image` layer type and the `caps.assets` flag as the hook.

---

## 9. Decisions — all made 2026-09-18 (Jeremy)

- **D1** Geometry object = **Layout**; kinds by dims Strip · Matrix · 3D; "custom map" is a
  coordinate source, not a kind.
- **D2** **Scene + Layer**; layer kinds Pattern · Text · Sprite · Color (image-from-file: future).
- **D3** **One Patterns page** with a segmented source control (On device | Library; Library | Mine).
- **D4** **Two pattern layers per board budget** (`caps.layers`); text/sprite/color unlimited;
  Add layer → Pattern disabled with its reason at the cap (the one deliberate exception).
- **D5** **Dynamic text via device text slots** (`POST /api/text`, HA text entity per slot,
  `textSlot(n)` handle); no string type.
- **D6** **Inline values, no named presets** — a playlist item / scene layer owns its values and
  edits them in place ("confusion is an issue with named presets"). Future ticket for named
  presets driven by the HA use case.
- **D7** Playground "Preview as" defaults to **Auto** (3D › 2D › 1D).
- **D8** **Keep the amber-on-dark** visual language.
- **D9** Mobile = **responsive stacking only**; no separate flows.
- **D10** Scenes tab **always visible in the playground** with a "preview as a matrix" empty
  state; console: regular 2D only.
- **D11** **One Layout spanning all outputs**; each output drives a consecutive run.
- **D12** **Keep device blur/glow, fix #446** (free the scratch when every stage is off), gate
  per board via `caps.blur_glow`.

## 10. Tickets (Gitea, milestone "Web UI v2", epic #461)

| Phase | Tickets |
|---|---|
| A — Foundation | A1 #462 shell · A2 #463 geometry store · A3 #464 status geom+caps · A4 #465 /api/layout · A5 #466 lx_outpipe · A6 #467 Patterns · A7 #468 Editor · A8 #469 Settings · A9 #470 Playlist · A10 #471 map editor · A11 #472 dialogs · A12 #473 projection · A13 #474 outputs · A14 #475 HUB75 arrangement · A15 #476 #446 fix |
| B — Compositor | B1 #477 two-layer blend · B2 #478 native layers + /api/scenes · B3 #479 budget · B4 #480 Scenes UI · B5 #481 sprites · B6 #482 thumbnails |
| C — Text | C1 #483 strings · C2 #484 fonts + builtins · C3 #485 text slots · C4 #486 text-layer UI |
| Future (not planned) | #487 asset upload / user fonts / images · #488 named presets |
