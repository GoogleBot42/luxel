# Implementer brief — library overhaul (read BRIEF.md first, then this)

    W=/home/googlebot/workspace/pixler/.claude/worktrees/bridge-cse_014QiyztoBqGP3qwhg36jCbt
    S=/tmp/claude-1000/-home-googlebot-workspace-pixler--claude-worktrees-bridge-cse-014QiyztoBqGP3qwhg36jCbt/478e04dd-960d-5a5c-b1f6-15fd084b8103/scratchpad

You are one of several agents editing ONE shared worktree at the same time,
partitioned by FILE OWNERSHIP. Your task prompt lists exactly the files you own
(edit / create / delete). Touch nothing else under `$W` — if a file you do not
own looks wrong, say so in your report. Do not run `git add`, `git rm`, `git
commit`, `git stash` or anything that touches the git index: delete files with
plain `rm`, and the orchestrator commits per batch. Put scratch files under
`$S/libfix/<batch-id>/`.

## The bar

Jeremy's principle: **capture the idea and make it better**, never a replica.
Every pattern you leave behind must be something a person would be proud to show
on a 64x64 HUB75 panel AND, where the idea suits one, on a 60–300 px strip.
"Works and passes the gate" is the floor, not the bar. Look at your own output.

## Hard rules (the gate and the owner enforce these)

- Never read `/home/googlebot/workspace/pixler/corpus`, any `.epe`, or the
  Patternflow sources in `$S/libfix/patternflow`. Prose specs under
  `$W/docs/pattern-specs/` are the only source material.
- Header: first line `// name: <Display Name>`; then a provenance comment —
  for a rewrite/merge of corpus-derived ideas: `// Clean-room reimplementation
  from the prose spec(s) docs/pattern-specs/<a>.md[, <b>.md] — reimagined, not a
  replica.`; for a Patternflow idea: `// Original work from the prose idea spec
  docs/pattern-specs/patternflow/<slug>.md (Patternflow preset, CC BY-SA 4.0 —
  prose-described idea only, no code).`; for an original: `// Curated original
  (hand-written showcase of the Luxel language/builtins).` Then 2–6 lines on
  what it does and how it is built.
- Controls: real-unit `//#` bounds on the export line or the line directly
  above (`//# min=1 max=10 step=1 default=5`), handler consumes the declared
  units and clamps; the top-level constant equals `default=`. NEVER a
  pattern-level brightness/dim/gamma control. A dial that is honestly unitless
  carries the bare word `unitless-ok` inside its directive. Mode selectors:
  `min=0 max=n-1 step=1`. Read `$W/.claude/rules/library.md`.
- A `renderBulk` pattern turns a grid cell into a frame slot with
  `gridIndex(col, row)` or draws in coordinate space — never `row * w + col`.
- Canvas simulations size from `gridWidth()`/`gridHeight()` (clamped to a
  sensible cap, e.g. 64 wide, 4096 cells) with a fallback when `gridWidth()`
  is 0 — never a hard-coded 16x16. Use `fillCanvas`/`paintCanvas`/`blit` to
  put a canvas on the frame, `blur2D`/`feedback`/`arrayAdd` for the sim.
- The array ledger is 10,236 elements and every `array(pixelCount)` costs
  what the rig says: a strip pattern with three full-length buffers is BLACK
  on a 64x64 panel. Prefer the persistent frame (`renderBulk` + `fade()` +
  `splat`/`setPixel`/`fillRange`) over buffers. `luxel check --grid 64x64`
  must pass for every file you ship.
- Per-pixel noise (`perlin*`, `simplex*`), `atan2`, `hypot`, `pow`, `exp`
  are host-cheap and device-expensive. Compute fields once per frame on a
  small canvas (`fillNoise3D` rows, a polar table) and look them up, or use
  bulk ops. Entity patterns (N balls/legs/sprites tested at every pixel) are
  `renderBulk` + one `splat`/`drawLine`/`blit` per entity.
- Every frame-rate-dependent decay must be delta-based (`pow(k, delta/16)` or
  `exp`-style with delta), or use `setFrameRate` deliberately.
- Toolbox: `$W/docs/lang.md` — read "Builtin reference", "Whole-frame
  rendering", "Control bounds". OKLCH and `mixColors` for even gradients;
  `paint()`/`setPalette` for palettes; 30 easings; `beat`/`beatSin`;
  `hash`/`hash2` for stable per-pixel randomness; `curl2` for flow;
  `setGlow`/`setBlur` for cheap bloom; `timeScale`/`setFrameRate`.

## Merging variants into a survivor

Keep the better display name (your prompt says which). Absorb each variant as
a mode selector, palette preset or control — only where the variant is a real
look; drop looks nobody would pick. Delete the absorbed files with `rm`. Do
NOT delete their `docs/pattern-specs/*.md` files; instead prepend one line to
each absorbed spec: `> Superseded: this idea now lives in library/<survivor>.js
(2026-11).` — the specs are the licensing audit trail.

## Verification (every file you ship, before you report)

    nix develop $W --command bash $S/libfix/verify-pattern.sh $S/libfix/<batch-id>/v library/<a>.js library/<b>.js …

runs the library lints + five CI rigs + the 64x64 budget check and writes
contact sheets: `<v>/<stem>.png` (32x32, 8 frames over 8 s) and
`<v>/64/<stem>.png` (64x64, 4 frames), plus `.strip.png` waterfalls for 1D
patterns and `.json` with host ms/frame. **Open every sheet with the Read tool
and judge it**: dark, static, garish, blocky, upside-down, or dull means you
are not done. Compare against the OLD sheet in `$S/libfix/sheets/<stem>.png`
when you rewrote something — the new one must be clearly better. For perf
work, quote before/after host ms/frame at 64x64 (`<v>/64/<stem>.json` vs the
old figure in `$S/libfix/perf/sweep.md`).

A `sheet.mjs` render does NOT seed `//# default=` values — that is why the
top-level constant must equal the default (the playground seeds defaults on
load). If a control needs a non-default value to look good, the default is
wrong.

Other agents are rebuilding nothing in `$W/target` — but if `luxel` or the
wasm is transiently missing, wait a minute and retry rather than rebuilding.

## Report (≤ 40 lines)

- Files created / edited / deleted (full list).
- Per shipped pattern: one line — what it is now, strip/panel/both, host
  ms/frame at 64x64 (before → after where it existed).
- Specs you marked superseded.
- Anything deferred, uncertain, or that needs an engine feature (the
  orchestrator files tickets) — and any file outside your ownership that
  looks wrong.
- No code in the report.

## Trap found by L07 (2026-11-01)
On a strip with NO map, `gridWidth()` returns the pixel count, not 0 — so a
"both" pattern must branch on `has2DMap()` first, never on `gridWidth() > 0`,
or the strip runs the panel path.
