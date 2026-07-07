# Extension ideas: builtins, language, engine, integration

Luxel is source-compatible with Pixel Blaze but deliberately *not* limited
to it. This is a living backlog of ways to go beyond parity, roughly ranked
within each section by value ÷ effort. Every language/builtin addition must
preserve PB-source compatibility: existing PB patterns keep compiling.

Legend: **[S/M/L]** effort · ★ value (1–3) · `compat` = keeps PB patterns valid.

## Builtins

New builtins are the cheapest high-value wins — they slot into the VM table
and the autocomplete/docs pipeline, and can't break existing code.

- **Easing functions** [S] ★★★ — DONE (quad/cubic in/out/inOut +
  `easeOutBack`/`easeOutElastic`/`easeOutBounce`; endpoint + shape tests).
- **Color-space helpers** [M] ★★★ — DONE (`oklch`/`oklab`, value-returning
  `rgb2hsv`/`hsv2rgb`, `mixColors`).
- **Smoothing / filters over arrays** [M] ★★ — DONE (`blur1D(arr, radius)`,
  `feedback(arr, decay)`).
- **Vector helpers** [S] ★★ — DONE (`dist`/`dist3`, `dot`/`dot3`,
  `angleBetween`, `length`/`length3`).
- **`map(x, inLo, inHi, outLo, outHi)`** [S] ★★★ — DONE.
- **`fract`/`step`/`sign`/`saturate`** [S] ★★ — DONE.
- **Simplex noise + curl noise** [M] ★★ — simplex DONE (`simplex2`/
  `simplex3`, hash-based fixed point, tested). Curl noise deferred: a
  finite-difference curl on 16.16 noise is too quantization-noisy to be
  pretty; do it when the noise gets analytic derivatives.
- **`beatSin`/`beat`(bpm, lo, hi)** [S] ★★ — DONE.
- **Deterministic `hash(x)` / `hash2(x,y)`** [S] ★★ — DONE (lowbias32,
  sequence pinned by test).

## Language

- **`const`/`let`** [S] ★★★ — DONE.
- **`**` (exponent) operator** [S] ★ — DONE (right-assoc, tighter than
  `*`; Insn::Pow; unary-lhs divergence from JS documented).
- **Array literals `[1, 2, 3]`** [M] ★★★ — DONE (lexer/parser/compiler
  `NewArray`; semantics tests pin `[1,2,3].length/.sum()`).
- **Ternary chains / already have `?:`** — verify and document.
- **`switch`** [M] ★ — occasionally nice; low priority.
- **Compound member ops on arrays** — `arr[i] += x` already works; audit
  for gaps.
- **Block-scoped `let`** [L] ★ — true TDZ/block scoping; correctness nicety
  over today's function-scoping. Low value for the LED domain.
- **A real string type** [L] ★★ — currently only labels; strings would
  enable text/scrolling-marquee patterns and richer control labels. Big VM
  change (heap strings, ops). Gated behind a use case.
- **Named/default parameters** [M] ★ — `function f(a, b = 1)`. Modest.

## Engine / runtime

- **Multi-pattern blend / transitions** [L] ★★★ — run two patterns and
  crossfade (the corpus "Shimmer Crossfade" fakes this per-pixel). A
  first-class compositor enables playlists with real transitions and
  layered effects. Big but headline-worthy.
- **Global post-process chain** [M] ★★ — STARTED: `setGamma(g)` (LUT-based
  output gamma) is in. Remaining: brightness curve, global blur/glow,
  palette remap, and a settings-page (not in-pattern) way to set them.
- **Per-pixel persistent state buffer** [M] ★★ — a sanctioned scratch array
  the engine double-buffers, for feedback effects without manual bookkeeping.
- **Deterministic seedable `prng` matching a documented algorithm** [S] ★ —
  we diverge from PB (documented); pinning our own good algorithm + docs so
  patterns are reproducible across Luxel devices (for synced installations).
- **Frame-rate / time-scale controls in-pattern** [S] ★ — expose
  `setFrameRate`, `timeScale` for slow-mo/debug.

## Peripherals & audio (M5 territory, high wow-factor)

- **I2S mic + on-device FFT** [L] ★★★ — real sound-reactivity: expose
  `frequencyData[]`, `energyAverage`, `maxFrequency` like PB's sensor board.
  The single most-requested LED-controller feature. The stubs already exist.
- **Generic sensor framework** [L] ★★ — accelerometer/light/analog as
  pluggable providers (already the M5 plan); makes the PB sensor board just
  one driver.
- **UPXL / output-expander support** [M] ★★ — drive thousands of pixels via
  expander boards.

## Integration (M4)

- **MQTT + Home Assistant discovery** [M] ★★★ — the reason this project
  exists for a lot of people; brightness/power/pattern-select as HA entities.
- **DDP / E1.31 input** [M] ★★ — DONE (firmware v0.1.18 + mirror; UDP
  :4048/:5568, shared parser in luxel-core::netin, live override with 2.5 s
  fallback to the pattern, `live` in /api/status + Settings row. Multicast
  sACN joined but only unicast verified from the dev container).
- **Luxel-to-Luxel sync** [L] ★★★ — timebase sync + leader/follower groups
  so multiple controllers render one coherent installation. `nodeId()` is
  already a builtin anticipating this.
- **Web-based .epe import/export in the playground** [S] ★★★ — DONE
  (import button + drag-drop anywhere + export download; e2e-covered).

## Playground / DX

> Web-UI structural redesign (two modes: device console vs. playground;
> tabs; settings page; header declutter) is tracked in
> [docs/webui.md](webui.md). Items below are feature-level.


- **Hover docs from the builtin table** [S] ★★ — DONE (builtins +
  predefined globals show sig + doc on hover; e2e-covered).
- **Pattern browser with animated previews** [M] ★★★ — DONE (192 live
  tiles: examples + compiles-clean corpus; 1D → bar, render2D → 16×16
  rectangle per Jeremy's distinction; viewport-lazy with an engine cap).
  Remaining niceties: render3D projection tiles, search/filter box,
  waterfall option for 1D.
- **Shareable pattern URLs** [S] ★ — DONE (`#p=` deflate+base64url
  fragment; share button copies, load restores; e2e-covered).
- **Multi-pane: map editor + preview** [L] ★★ — mapper v1 DONE (PB-style
  JS map function → normalized map installed in the engine, scatter
  preview, render2D auto-selected). Still open: mapper as a first-class
  **editor tab** (CodeMirror, debuggable), **3D projection preview**, visual
  drag-editing, Fill/Contain toggles, device map upload, map in share links.
  Web-UI redesign tracking these lives in [docs/webui.md](webui.md).

## Top picks if forced to choose 5

1. `map()` + easing + `oklch` color helpers (a day of builtins, huge visual
   and authoring uplift).
2. Array literals (the most-missed language feature).
3. I2S mic + FFT (the headline hardware feature; stubs exist).
4. MQTT/HA integration (the project's raison d'être for many).
5. Multi-pattern blend/transitions (turns playlists into something PB can't
   do well).
