# Extension ideas: builtins, language, engine, integration

Luxel is source-compatible with Pixel Blaze but deliberately *not* limited
to it. This is a living backlog of ways to go beyond parity, roughly ranked
within each section by value ÷ effort. Every language/builtin addition must
preserve PB-source compatibility: existing PB patterns keep compiling.

Legend: **[S/M/L]** effort · ★ value (1–3) · `compat` = keeps PB patterns valid.

## Builtins

New builtins are the cheapest high-value wins — they slot into the VM table
and the autocomplete/docs pipeline, and can't break existing code.

- **Easing functions** [S] ★★★ — DONE (the standard thirty: sine/quad/cubic/
  quart/quint/expo/circ/back/elastic/bounce, each in/out/inOut; endpoint +
  reference-value + shape tests).
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
- **Ternary chains / already have `?:`** — DONE (verified: right-assoc,
  chains, nesting in either slot, single-branch evaluation, assignment
  in a branch — all already correct; the gap was documentation, now a
  section in docs/lang.md plus compiler/VM tests).
- **`switch`** [M] ★ — DONE. JS semantics (single discriminant eval,
  source-order `==` label tests, fall-through, `break`, `default`
  anywhere). No new opcodes: it lowers to `Dup`/`Ne`/`JmpIfFalse` +
  `Pop`/`Jmp` trampolines. A Luxel extension — PB's compiler rejects
  SwitchStatement outright.
- **Compound member ops on arrays** — DONE. Audit found the whole family
  (`+= -= *= /= %= <<= >>= &= |= ^=`, prefix/postfix `++`/`--`) already
  correct on elements, including single evaluation of the array/index
  sub-expressions and JS result values; the one gap was `**=`, now
  added. Pinned by tests.
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
- ~~WiFi-blob buffer tuning~~ — DONE 2026-08-22, shipped as the missing
  half of the **`small-chip`** feature (UPDATES.md entry has the full
  numbers). The knobs are runtime fields on
  `esp_radio::wifi::ControllerConfig` (NOT esp-config), set in main.rs:
  under `small-chip` it's `static_rx_buf_num` 10→4,
  `dynamic_rx_buf_num` 32→16, `ampdu_rx_enable` off. TX counts stay at
  the defaults on purpose — dynamic TX buffers are on-demand, so
  lowering the cap reclaims nothing at idle and only buys TX starvation.
  **Measured on the Athom (A/B, both small-chip builds, idle
  `heap_free`): 115,548 → 125,460 = +9,912 B (9.7 KB).** Below the
  15–25 KB the 2026-07-29 note guessed, and the reason is instructive:
  essentially ALL of it is `static_rx_buf_num` (6 buffers × ~1.6 KB =
  9.6 KB, allocated in `esp_wifi_init` and never freed). The dynamic
  pools and the AMPDU block-ack buffers are on-demand — capping them
  bounds the worst case but reclaims ~nothing at idle. Full small-chip
  profile vs the then-default build was +27.1 KB (98,352 → 125,460); see
  the paragraph below — the default moved, so it is +20.6 KB today.
  Soaked clean: 321/322 hw-bench (the one failure is the same
  pattern-side OOB the default build has), 44 k DDP frames at 245 pkt/s
  × 300 px alongside 6-way API hammering, a 629 KB streaming asset
  upload, 18/20 cold loads — no panic, no rollback, `heap_free` floor
  99 KB. Going below static 4 is NOT recommended without fresh soak
  evidence: the blob doesn't null-check, so an undersized pool is a
  StoreProhibited crash, not an error. **The DEFAULT build took the mild
  half of the trim on 2026-08-22 too** (Gitea #60): `static_rx_buf_num`
  10→6 alone, AMPDU RX and the dynamic pool left stock, A/B'd at
  **98,352 → 104,832 (+6,480 B)** on the Athom and soaked with serial
  attached (hw-bench 321/322, 44 k DDP frames + 6-way API hammer, cold
  loads at parity, no panic, slot held). So the default build's floor is
  the higher number now, and the small-chip profile is worth +20.6 KB
  over it rather than +27.1 KB.
- ~~Web pool 3→2 (make the webui tolerant first)~~ — RESOLVED 2026-08-15
  (UPDATES.md entry has the full story): the web tolerance shipped
  (fetchgate.ts + coldload.mjs acceptance harness, then 10/10 clean on the
  Athom — briefly regressed to 0/10 when the installer page's second vite
  entry grew the native request burst to 4, re-fixed 2026-08-29 and back
  to 10/10; Gitea #92), the verification
  gauntlet surfaced and fixed a pattern-store
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
  budgeted-engine rejection path is the degradation story) — DONE: the
  pool half landed 2026-08-15 (2 slots + esp32 heap 88 KB) and the
  WiFi-buffer half 2026-08-22, together +27.1 KB of heap on the Athom;
  (3) WROVER PSRAM as an esp-alloc second region for pattern arrays (WiFi blob must
  stay on internal RAM) — the big capacity unlock for the classic line
  if ever wanted.

## Engine / runtime

- **Multi-pattern blend / transitions** [L] ★★★ — run two patterns and
  crossfade (the corpus "Shimmer Crossfade" fakes this per-pixel). A
  first-class compositor enables playlists with real transitions and
  layered effects. Big but headline-worthy.
- **Global post-process chain** [M] ★★ — DONE. Four whole-frame stages the
  engine runs once per frame after the last `render`, in a fixed order:
  `setOutputPalette` (recolor by luma through a stop list) → `setBlur`
  (3-tap blur, 1–8 passes) → `setGlow` (light-bleed bloom that keeps the
  source at full) → `setGamma` (the original LUT gamma). Off by default and one comparison per frame when unset; the
  spatial stages are allocation-free and the two table stages rebuild only
  on change. Settings-page half: blur %, glow %, and a **brightness
  curve** (the dimmer's response, distinct from gamma's per-pixel content
  curve) are persisted device settings on `/api/output`. The spatial stages
  are map-aware (Gitea #140): with a 2D map that reads as a regular W×H
  matrix — row-major or serpentine — blur and glow sweep rows then columns;
  every other layout keeps index-space behaviour. The device-settings half
  is now complete: palette remap is a persisted device setting too, stored
  as a reserved-key blob in the pattern store, with
  `POST`/`DELETE /api/output/palette` and a stop editor in the Output card
  (Gitea #139). Device and pattern palettes
  compose — the device stage runs on the frame the pattern already
  recolored.
- **Per-pixel persistent state buffer** [M] ★★ — DONE. `pixelState(i[, ch])`
  / `setPixelState(i[, ch], v)`: an engine-owned, double-buffered per-pixel
  buffer (up to 4 channels) — reads see last frame's snapshot, writes land
  next frame, unwritten pixels carry over. Allocated on the first write
  only, charged to the array byte budget, half the RAM of the `array(
  pixelCount)` idiom (Fx, not Value) and off the PB element ledger. See
  docs/lang.md "Per-pixel state"; `library/ember-diffusion.js` is the
  neighbour-reading showcase.
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
  (tools/mqtt-e2e.mjs). Follow-up done 2026-08-22: the library's remaining
  fake-trigger controls (Ripples 2D, Slime mold palette, SaberDeploy
  Tutorial) now listen for events too, with the manual controls kept.

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
