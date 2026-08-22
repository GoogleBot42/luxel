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
- **`blur2D(arr, w, h, radius)`** [S/M] ★★★ — DONE (separable in-place
  box blur over the first w×h row-major elements, per-axis windows
  `2·radius + 1` with clamped edges matching `blur1D`; undersized array
  is a clean runtime error; Typing Heatmap now uses it).
- **Bulk array math** [S] ★★ — DONE (`arrayAdd`/`arraySub` over the
  shorter length, `arrayScale` = alias of `feedback`, `arrayMix(dst,
  src, t)` unclamped lerp with `t = 1` as exact copy; all in-place,
  return dst; aliased `f(a, a)` calls pinned by test).
- **Canvas deposit/sample helpers** [M] ★★ — DONE (`canvasSet(buf, w,
  x, y, v)` edge-clamped `floor(x·w)` cells — `x = 1` lands in the last
  column, no `* 15.99` fudge; `canvasGet(buf, w, x, y)` **bilinear**
  with texel centers at `(i + 0.5)/w`, so set/get agree on cell centers
  and larger maps upscale smoothly; h = len/w). Follow-up DONE too:
  `canvasAdd(buf, w, x, y, v)` accumulates into the same cell for
  particle deposits, returning the cell's new value.

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

## Soak-v5 follow-ups (2026-07-08 — see bench-report.md for the failing list)

The v0.1.25 heap hardening left 19/195 gallery patterns degraded on-device
(zero panics — every failure below is a clean vmerr/rejection). In-place
bytecode execution is being worked on now; the rest are queued:

- ~~Streaming pattern uploads~~ — DONE (v0.1.27): /api/code + /api/patterns
  stream like OTA/assets; upload cap gone, per-connection buffer 4 KB.
- ~~Byte-accurate array budget~~ — DONE (v0.1.27).
- ~~In-place bytecode execution~~ — DONE (v0.1.26, LXBC v2): the VM
  interprets flat bytes; decoded Programs ≈ 1.2–2.5× blob (was ~5×).
- ~~Oracle probe: `render2D` with no map~~ — DONE 2026-07-08 (see
  04-oracle-findings.md): a PB that ever saved a map can't be made mapless
  via its public interface, so PB-as-experienced always has one; Luxel now
  auto-installs a default ceil(√n) grid for 2D/3D-only patterns. The
  remaining Breakout/Crosstown/Frogger failures at 300 px were the
  patterns' own square-rig assumption — they'd OOB identically on a real
  PB at 300 px (engine test pins both halves). CLOSED 2026-07-19: the
  clean-room library reimplementations dropped that assumption; all four
  run clean on-device (full-library soak 321/322).
- **Flash-mapped library execution** [L] ★ — the very last word in pattern
  RAM: run library patterns straight out of flash-mapped storage (no RAM
  copy of the code at all). Needs contiguous blob placement (the
  sequential-storage KV chunks aren't mappable) — e.g. a dedicated raw
  region like the web-assets partition. With in-place execution + the
  const-array pool shipped, exactly ONE library pattern still exceeds the
  device: "Music Sequencer - for V3 ONLY" (663 lines; 17.8 KB blob,
  ~71 KB total engine footprint per heapstat — it loads at idle heap but
  leaves 19 KB free, 1 KB under the 20 KB floor, and is rejected with the
  friendly capacity error). UPDATE 2026-07-27: v0.1.34 (flash-resident
  read-back copies + envelope dropped pre-floor-check) CLEARED the
  capacity motivation — Music Sequencer V3 now runs at 300 px with
  ~70 KB free (full-library capacity, 322/322 modeled). The raw
  current-pattern slot v0.1.34 added is also exactly the "contiguous,
  mappable region" this item needs, so what remains is purely the MMU
  work — now a perf/endgame item, not a capacity one.
- **WiFi-blob buffer tuning** [M] ★★ — AGREED FOLLOW-UP 2026-07-29.
  esp-radio's RX/TX buffer counts are
  default-generous; the blob's heap draw is the biggest remaining consumer
  (~50 KB measured residual at idle on v0.1.34). VERIFIED 2026-07-29
  where the knobs actually live on our pinned esp-radio: NOT esp-config —
  they're runtime fields on `esp_radio::wifi::ControllerConfig` (already
  constructed in main.rs): `static_rx_buf_num` (default 10 × ~1.6 KB,
  allocated at wifi init and never freed), `dynamic_rx_buf_num` (default
  32, on-demand), tx counts, `ampdu_rx_enable`/`rx_ba_win`. Dropping
  static 10→4 + dynamic 32→16 + AMPDU off should reclaim ~15-25 KB at
  the cost of RX throughput on busy networks; tune conservatively and
  soak — the blob's allocations don't null-check, so an undersized pool
  under load shows up as StoreProhibited, not a clean error. Main use
  now: the small-chip profile (see boards.md tiers), not classic-ESP32
  capacity.
- ~~Web pool 3→2 (make the webui tolerant first)~~ — RESOLVED 2026-08-15
  (UPDATES.md entry has the full story): the web tolerance shipped
  (fetchgate.ts + coldload.mjs acceptance harness, 10/10 clean on the
  Athom), the verification gauntlet surfaced and fixed a pattern-store
  OOM-on-read panic and the picoserve shutdown slot-wedge
  (QuickCloseSocket) — but Chromium needs ~3 sockets at cold NAVIGATION
  (preconnect + nav, pre-page, unfixable client-side), so the default
  pool STAYS 3 and pool 2 shipped as the **`small-chip` cargo feature**
  (pool 2 + esp32 heap 88 KB, ~17 KB reclaimed, occasionally-refused
  first nav accepted). Tiers 3–4 in boards.md get it via that feature.
- ~~Flash-access fairness under playlist churn~~ — RESOLVED 2026-08-15
  (v0.1.36): the per-swap persist (`patterns::store_current`) was taking
  the flash driver OUT of the global for its whole multi-page burst, so
  every `with_flash` user read busy — that one absence explained all
  three symptoms (asset pushes "flash write failed", `/api/ota`'s
  misleading "update already in progress" from `begin()` finding no
  driver, served assets truncating). `write_raw` now borrows the driver
  per erase/write op via `with_flash` (the same soak-proven shape as the
  OTA/assets writers) and skips while an OTA is active. Verified on the
  Athom under a 5 s playlist: asset pushes 6/6 (was 1/6), 20/20
  identical served-asset hashes, OTA accepted + clean reboot, cold loads
  5/5. Deploy tooling no longer needs stop-playlist→push→resume on
  v0.1.36+.
- ~~Per-swap flash WEAR under playlist churn~~ — RESOLVED 2026-08-15
  (v0.1.37), implemented exactly as sketched: `Msg::Code`/`Crossfade`
  carry the library id (stamped by the RENDER task at the swap — fixes
  the sender-side id race too), `SrcLoc::Library`/`BcLoc::Library`
  read-back variants serve transiently from the pattern store, and
  `store_current` runs only for ad-hoc pushes (which now log a serial
  line — if it appears on playlist advances, the fix regressed).
  Playlist churn writes NOTHING to flash (was ~17k erase cycles/day on
  fixed sectors at 5 s items vs ~100k NOR spec). Verified on the Athom:
  0 slot writes across churn, read-back byte-identical to the library
  copy, envelope framing exact, live pixel-count rebuilds mid-playlist,
  resume-across-reboot, 3/3 cold loads, full hw-bench soak.
- **Small-chip profile + more board features** [M] ★★ — follow-ups from
  the 2026-07-29 chip-support assessment (docs/boards.md "Beyond the
  current boards"): (1) DONE (2026-08-22): `board-s3-devkit` and
  `board-c6-devkit` ship as "builds, untested on metal" — no firmware
  logic changed, but build-esp32.sh/stack-check.sh had the classic-ESP32
  chip/target/toolchain hardcoded and now share
  `firmware/board-target.sh`, and the flake gained the `riscv32imac`
  target for the C6. Both are in the release matrix; the C6 image is the
  fleet's tightest OTA margin (60,976 B). Remaining for these two: light
  them up on real hardware and spend their DRAM slack on heap (Gitea
  #56), and decide whether the installer page should list them (#57); (2) a
  `small-chip` cargo feature bundling web pool 3→2 + tuned WiFi buffers
  for the S2/C2 tier, where giants reject cleanly (acceptable — the
  budgeted-engine rejection path is the degradation story) — the feature
  EXISTS as of 2026-08-15 with the pool half (2 slots + esp32 heap
  88 KB); WiFi-buffer tuning still to join it; (3) WROVER
  PSRAM as an esp-alloc second region for pattern arrays (WiFi blob must
  stay on internal RAM) — the big capacity unlock for the classic line
  if ever wanted.

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
  DONE. Both generators are now pinned by test and spelled out in
  docs/lang.md ("Determinism and seeding"): `random()` = splitmix64,
  `prng()` = xorshift32 (13/17/5), state ← the seed's raw 16.16 word,
  scaling `(r · max) >> 32`. New `randomSeed(s)` seeds `random()`'s
  stream the way `prngSeed` seeds `prng`'s (returns the previous seed;
  `prngSeed` keeps returning the previous *state*, which round-trips) —
  so an installation can agree on a sequence without rewriting patterns
  onto `prng`. No sequence changed; PB source is unaffected.
- **Frame-rate / time-scale controls in-pattern** [S] ★ — DONE.
  `timeScale(s)` scales the delta the engine applies to the pattern
  clock (time/beat/beforeRender delta all follow; 0 freezes, negatives
  clamp); `setFrameRate(fps)` holds the last frame until 1000/fps real ms
  have passed and then hands `beforeRender` the whole interval. Both
  enforced in `Engine::frame`, so every host honors them identically; the
  host output cadence (and therefore the reported `fps`) is unchanged —
  documented, not faked.
- **External event injection** [M] ★★★ — DONE (v0.1.38): engine-side
  32-event drop-oldest queue; `eventCount()` + `readEvent(out)` filling
  `[type, x, y, value]`; fed by preview clicks/drags (type 1, normalized
  x/y — playground AND device mode, batched ~50 ms) and `POST
  /api/events` ("EV1\0" frame, shared parser in `netin`, firmware +
  mirror). Typing Heatmap and Crosshair Pulse now consume real events
  (phantom generators pause while input flows). The MQTT-topic → event
  mapping followed in v0.1.39: `luxel/<id>/event`, text lines
  `type [x [y [value]]]`, verified against real mosquitto
  (tools/mqtt-e2e.mjs).

## Peripherals & audio (M5 territory, high wow-factor)

- **I2S mic + on-device FFT** [L] ★★★ — real sound-reactivity. GROUNDWORK
  DONE (v0.1.20): engine sensor bindings live (`Engine::set_sensors`),
  playground browser-mic source, PB sensor-board UART frames parsed on the
  expansion header, POST /api/sensors network injection. Remaining: the PB
  v3's own onboard mic (undocumented closed hardware — needs a bench
  session to find the pins) or any I2S/PDM mic + on-device FFT.
- **Generic sensor framework** [L] ★★ — accelerometer/light/analog as
  pluggable providers (already the M5 plan); makes the PB sensor board just
  one driver.
- **UPXL / output-expander support** [M] ★★ — drive thousands of pixels via
  expander boards.

## Integration (M4)

- **MQTT + Home Assistant discovery** [M] ★★★ — DONE (firmware v0.1.19 +
  mirror; light = power/brightness, select = pattern library, MQTT
  discovery, /api/mqtt + Settings form; shared payloads in
  luxel_core::hamqtt; verified against real mosquitto via the mirror).
- **DDP / E1.31 input** [M] ★★ — DONE (firmware v0.1.18 + mirror; UDP
  :4048/:5568, shared parser in luxel-core::netin, live override with 2.5 s
  fallback to the pattern, `live` in /api/status + Settings row. Multicast
  sACN joined but only unicast verified from the dev container).
- **Luxel-to-Luxel sync** [L] ★★★ — v1 DONE (v0.1.21): leader broadcasts
  its engine timebase (+ sensor relay) on UDP :4049; followers hard-jump
  then slew by stretching frame deltas (≤±25%). Role in Settings +
  /api/sync, persisted. Proven with two mirrors (sync-e2e: 2.5 s desync →
  −5 ms). On-device verification blocked (needs ≥2 recovered Luxels).
  Future: pattern/playlist distribution to followers.
- **Web-based .epe import/export in the playground** [S] ★★★ — DONE
  (import button + drag-drop anywhere + export download; e2e-covered).

- **AP-mode provisioning** — DONE (v0.1.22): no-creds boot (or POST
  /api/apmode / the Settings button — a one-shot flag, crash-safe) → open
  AP `luxel-xxxx` @ 192.168.4.1 with DHCP (edge-dhcp) + catch-all DNS +
  captive-portal redirect serving the normal web app; Settings → WiFi
  provisions and reboots to station. Radio path needs a phone test.

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
- **Multi-pane: map editor + preview** [L] ★★ — mapper DONE through v2:
  first-class editor tab (CodeMirror, breakpoint-debuggable), 3D
  auto-rotating projection preview, device map upload, **map in share
  links** (`#pj=` envelope), and **render3D gallery tiles** (cube-lattice
  point-cloud thumbs; the 5 render3D-only corpus patterns are no longer
  skipped). Still open (niceties): visual drag-editing, Fill/Contain
  toggles. Web-UI redesign tracking lives in [docs/webui.md](webui.md).

## Top picks if forced to choose 5

1. `map()` + easing + `oklch` color helpers (a day of builtins, huge visual
   and authoring uplift).
2. Array literals (the most-missed language feature).
3. I2S mic + FFT (the headline hardware feature; stubs exist).
4. MQTT/HA integration (the project's raison d'être for many).
5. Multi-pattern blend/transitions (turns playlists into something PB can't
   do well).
