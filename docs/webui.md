# Web UI — redesign backlog

Captured from Jeremy's 2026-07-06 feedback batch. The web app (`web/`) has
grown a single monolithic `App.svelte` with everything crammed into one
header bar; it needs to become a **two-mode, tabbed application**. This doc
is the working backlog — check items off as they land. See PLAN.md §2.3 for
the original Web IDE spec and ideas.md "Playground / DX" for feature-level
ideas.

Legend: **[S/M/L]** effort · 🔧 = needs firmware support · ✅ done.

## The central restructure: two modes

The app has two audiences and must present differently for each:

- **Device mode** — connected to (or served from) real hardware. This is a
  *device console*: edit the running pattern, manage the on-device library,
  configure the device, build playlists. **Not** a "playground."
- **Playground mode** — no device: someone exploring the project, or without
  hardware handy (GitHub Pages, docs embeds). Local WASM engine only. This is
  where Share, zero-hardware preview, and onboarding matter.

Mode is derived from whether a `DeviceSession` is connected. Everything below
keys off it.

### Header is overcrowded [M] ✅ (Phase 2)
Too many options, rarely-used ones too prominent. Fixed: the header is now just
wordmark · tab bar · status (fps/streaming) · connection. File actions moved to
the editor toolbar (save/delete/share visible, import/export in a ⋯ overflow
menu); layout/fps/pause/debug moved to a playback bar under the editor.

### Tabbed structure [L] ✅ (revised — editor-on-demand)
**The Editor is not a tab.** It opens full-screen (own bar: ← back · name ·
save/⋯) when you create a pattern or pick one to inspect; back returns to the
originating library. On load it resumes the last edit (else the Library); on a
device it opens on the running pattern (back → Device Patterns).

- **Home tabs — playground:** Patterns Library (only).
- **Home tabs — device:** Patterns Library · Device Patterns · Settings.
  - *Patterns Library* ✅ — examples + community corpus + your saved patterns
    (chips), with **+ New pattern**. The gallery is lazy-mounted then kept alive.
    Picking a tile opens the editor on it.
  - *Device Patterns* ✅ — the device's stored patterns; each opens/activates in
    the editor; its own **+ New pattern**. (Replaces the old examples dropdown
    for device patterns.)
  - *Playlist* ✅ (firmware v0.1.15) — plays saved patterns in order with
    **per-entry parameters** (the same pattern can appear multiple times with
    different looks) and a flexible duration model: a playlist-level default,
    optional per-item override, and default-0/blank = manual advance. Flash-
    persisted; **resumes across reboots**. Wire = line format (D/I/C, no JSON
    parser); scheduler = an embassy task loading each item's pattern + controls;
    a manual code push stops it. UI: PlaylistRow with inline Controls + reorder/
    remove; "+ playlist" in the editor toolbar captures the current params.
    Single ad-hoc (non-playlist) patterns don't persist across reboot yet.
  - *Settings* ✅ (shell) — device info + pixel-count readout; brightness /
    pixel-count editing / WiFi form are honest Phase-3 placeholders (see below).
- The **examples dropdown was removed entirely**; pattern selection is the
  Library / Device Patterns lists. Share is a prominent editor-bar button in the
  playground (hidden on a device).

## Editor

### Mapper is a first-class editor tab, and debuggable [M] ✅
**Done.** The map is now a **Luxel program** (not JS): it exports
`render(index)` and calls a new `plot(x, y[, z])` builtin once per pixel. It's a
peer **sub-tab of the script editor** (pattern · map), edited in the same
CodeMirror (luxel highlighting/completions/hover), and **debuggable exactly like
a pattern** — gutter breakpoints, step over/into/out, stack + locals + globals —
because it runs on the same VM (a second engine in "map mode": the per-pixel
`drive` loop stores the plotted coordinate instead of a color). Collected coords
install into the pattern engine as a 2D/3D map; a live scatter preview renders.
Implementation spans luxel-core (`plot` builtin + `Engine::enable_map_mode`/
`run_map`/`map`), luxel-wasm (`lx_run_map`/`lx_map_*`), luxel.ts
(`compileMap`/`runMap`), and App.svelte (map sub-tab + shared `Debugger.svelte`).
Playground-only for now — device map upload is a later firmware item.

### 3D mapping [L] ✅
Done (see Phase 4): `normalizeMap` keeps z, and Preview renders a rotating
depth-sorted point cloud whenever the map's z actually varies.

### Debugging must not lie when connected to a live device [M] ✅
**Resolved by the local-preview model (below).** Device mode now runs the same
WASM engine as the playground for its preview, so the step-debugger is genuine
local computation — it works everywhere, no lying. The old "gating it off in
device mode" concern is moot.

## Device mode = local preview + push [L] ✅

Jeremy: the live pixel stream from the device "isn't helpful — remove it," and
the connect/disconnect buttons too ("it's always connected for the API"). So
device mode was re-architected into **"the playground that also drives the
strip":**
- **No pixel streaming.** Removed the ws push socket, HTTP pixel/vars/readouts
  polling, `deviceConn`/`markLive`, the connecting pill + preview overlay, and
  the streaming/polling indicator. The preview always runs on the **local WASM
  engine** (even on a device); `recompile()` is device-independent now.
- **No connect/disconnect/reconnect buttons, no device badge.** The wordmark
  shows the bound device; the API connection is assumed always-up. A failed
  connect just shows an error (reload to retry).
- **Editing pushes to the device.** Typing → local recompile (150 ms) + a
  throttled `devicePush` (500 ms, guarded by the local compile so a broken
  pattern is never sent). New/opened/imported patterns push immediately
  (`applyEdit`). Controls update the local engine AND the device. Layout changes
  are preview-only (never push).
- **The connect handshake on load stays** (Jeremy: "still wait for the device to
  finish connecting so it knows what pattern to open"): `patternLoading` covers
  the editor while `connectDevice` fetches status/pattern/brightness/config/
  protocol; then the local engine builds from the pulled/resumed source.
- device.ts lost `openSocket`/`wsCall`/`pixels`/`vars`/`readouts`/`controls`/
  `setVar` (all unused now). device-e2e asserts no connect chrome + local preview
  renders + controls still drive the device pixels via push.

## Capacity warning: "this pattern may not fit your device" [M] ✅ (Gitea #15)

The local-preview model has one honest gap: the playground's engine is
unbudgeted, so a pattern that renders beautifully in the browser can be
**rejected by the device it is pushed to** — and the rejection is
*asynchronous*. `POST /api/code` answers 200; the render task then fails the
post-load floor check and records a `pattern too large for this device`
vmerr. Nothing in the editor used to say so; the strip just kept showing the
previous pattern.

The editor now predicts that outcome before the push lands, and confirms it
after.

**Prediction — a measurement, not a size heuristic.** `lx_device_model`
(luxel-wasm) replays the firmware's own load sequence under a counting
allocator: the LXP envelope resident across `deserialize_lean`, dropped, then
`Engine::from_program_budgeted` at the device's array budget, then three
frames. The peak live bytes is what the device's floor check would see.
wasm32 is 32-bit like the ESP32, so the structures measure the same width —
this is strictly closer to hardware than the 64-bit `heapstat` test that
originally established the model. `Luxel.deviceModel()` wraps it;
`checkCapacity()` in App.svelte runs it on every successful recompile.

**Inputs.** `/api/status` `heap_free` (now declared in `DeviceStatus`) and the
device's own pixel count — never the preview layout's, since strip length is
hardware truth. `heap_free` is measured with the *current* pattern still
loaded, which is exactly the right baseline: the firmware builds the new
engine before releasing the old one, so that number really is the incoming
pattern's headroom.

**Thresholds.** `luxel_core::budget` is now the single definition of
`RUNTIME_FLOOR` (20 KB), the array-budget arithmetic, and `load_headroom()`;
`firmware/src/main.rs` and the wasm model both import it, so the prediction
cannot drift from the device that enforces it. On top of that the UI reserves
the last **15 %** of headroom as "tight" — the model is exact but the real
device's heap moves underneath it between the status read and the load (WiFi
buffers, HTTP connection buffers, MQTT publishes, jsonview snapshots).

**Rendering.** A banner in the editor's right-hand banner stack, the same
`.banner` idiom as compile/runtime errors, `data-role="capacity-warning"` with
`data-level="tight"|"over"` and the full byte breakdown in the `title`.
Severity follows *certainty*, not size: our local model is advice and renders
amber (`.banner.warn`, with `.capacity-over` for the heavier variant); the
device's own vmerr is a fact and renders red
(`data-role="capacity-rejected"`). It is **non-blocking** throughout — the
pattern still pushes, because the device is the authority on what it can run
and the editor only says what it expects.

**Playground: silent.** No device, no budget to judge against — and per the
mode rules the playground must not sprout a device affordance. A device that
can't report its free heap (`heap_free` 0 or absent: the native mirror,
older firmware) is silent too; an unknown budget is not a small one, and
guessing would cry wolf on every pattern.

**Testing without hardware.** `luxel serve --heap-free BYTES` makes the mirror
impersonate a device with that much free heap (default 0 = "can't tell you").
device-e2e spawns a second mirror claiming 30 KB free — 10 KB of load
headroom with the arena clamped at its 16 KB minimum, which puts all four
verdicts within reach of a one-line pattern — and asserts each band appears,
clears, and doesn't block the push.

## Settings page 🔧 [L]

A real device needs a settings surface (page/dialog). Fields:
- **Brightness** ✅ — runtime value (`shared::BRIGHTNESS` atomic, 0–31) with
  `GET/POST /api/brightness`, applied every frame in the encode path (SK9822's
  5-bit current field + a software scale for WS2812) and persisted in a `LXDV`
  nvs record (survives reboot). The Settings slider drives it live. Shipped in
  firmware v0.1.12. (Preview is intentionally pre-brightness — it shows the
  pattern's colors; brightness dims the physical strip only.)
- **Pixel count** ✅ — `GET/POST /api/config` resizes the strip **live, no
  reboot**: the render task rebuilds the engine + SPI buffer on a `Msg::Config`
  (feasible because the SPI is Blocking, no DMA, and the encode buffer is a plain
  heap Vec). Persisted in the `LXDV` nvs record; capped at the board's
  `MAX_PIXELS` — 2048 on strip boards, 4096 on 64x64 HUB75 panel boards
  (docs/boards.md, "Pixel caps are per board"). The UI reads that cap from
  `/api/status`'s `max_pixels` on every poll, falling back to
  `/api/config`'s `max`, so the Pixels field clamps to the connected board.
  Shipped firmware v0.1.13. The Settings Pixels field is editable and re-anchors
  the local preview.
- **LED protocol** ✅ — `GET/POST /api/protocol` (sk9822/ws2812 + aliases)
  switches the driver **live, no reboot**: `Msg::Protocol` calls
  `spi.apply_config()` to change the clock (8 MHz ↔ 2.4 MHz) and resizes the
  encode buffer. Persisted in `LXDV` (v3). Settings has a protocol dropdown.
  Shipped firmware v0.1.14. Verified on hardware (sk9822↔ws2812↔sk9822, no
  crash, SPI restores full speed).
- **Output pipeline** ✅ — the Output card, `GET/POST /api/output`, applied
  live to every frame and persisted in the `LXDV` record (v7). Fields:
  **Color order** (wire channel remap), **Gamma** (per-pixel content curve,
  ×10; 0 = off), **Power cap** (mA; 0 = off), **Brightness curve**
  (`brightCurve`, the *master dimmer's* response ×10 — distinct from gamma;
  2.2 makes the slider feel linear, 0 = off), **Blur** (`blur`, 0–100 %,
  3-tap softening — 2D over rows and columns when a matrix map is
  installed, along the pixel index otherwise) and **Glow** (`glow`, 0–100 %,
  light-bleed bloom that keeps the source pixel at full). Chain order:
  blur → glow → gamma → color order → power cap. The POST body is positional
  and whitespace-separated; the last three tokens are optional so older
  clients keep working (absent = keep the stored value).
- **MQTT / Home Assistant** ✅ — shipped (firmware v0.1.19 + mirror):
  broker host/port/creds + HA discovery, `/api/mqtt` + Settings form.
- **WiFi** ✅ — Settings form shipped (Phase 3); **AP-mode** provisioning
  shipped in v0.1.22 (open AP + captive portal when there are no creds).

## Sharing

### Share only in playground mode [S] ✅
Done. Share (pattern-in-URL) is shown only when `isPlayground` — hidden whether
you're connected to a device or merely served from one (its link would be a LAN
address). Prominent in the playground toolbar.

## Patterns browser

### Spinners while computing/loading [S] ✅
Done. Pattern tiles (Patterns Library + PixelBlaze Library tabs — the same
Gallery component backs both) overlay a small spinner (`data-role=
"tile-spinner"`) from the moment a tile is in view until its engine has
compiled and drawn its first frame; patterns that don't compile keep the
existing grayed-out "dead" treatment instead. Device-pattern thumbnails
(PatternThumb) do the same (`data-role="thumb-spinner"`), covering the
source fetch as well. e2e covers spinner-appears / spinner-clears via CPU
throttling + scrolling fresh tiles into view.

---

## Asset-load tolerance for 2-socket devices [M] ✅ (2026-08-15)

Follow-up agreed 2026-07-29 — done, with a twist (full story in
UPDATES.md 2026-08-15). `src/lib/fetchgate.ts` gates EVERY app-initiated
fetch (assets + API) at 2 in-flight with backoff-retry on refused, holds
the gate slot until the body is fully received (fetch resolves at
headers — the original hole), bounds each attempt at 30 s; device probe
abort 1500 → 8000 ms. Acceptance harness: `web/tools/coldload.mjs`
(10/10 clean cold loads on the Athom). The twist: Chromium opens ~2
sockets at cold NAVIGATION (preconnect + nav) before any page code
exists, so the firmware default pool stays 3 and pool-2 became the
`small-chip` firmware profile (occasionally-refused first nav accepted
there). The gate still matters at 3 slots — it's what makes loads clean
while a playlist churns flash and after the in-page burst.

---

## Rough phasing

1. **Phase 1 (web-only, no firmware):** mode concept + naming; remember device
   URL; "ws push" → human label; gate debugger off in device mode; hide Share in
   device mode; gallery spinners. *(No firmware, no reflash — fully verifiable
   in chromium.)*
2. **Phase 2 (web restructure):** ✅ shipped — tab bar in both modes (Editor ·
   Patterns [· Settings on device]); header decluttered to wordmark/tabs/status/
   connection; file actions → editor toolbar + ⋯ overflow; layout/fps/pause/debug
   → playback bar; gallery promoted from modal to inline Patterns tab; Settings
   shell (device info + pixel readout, Phase-3 placeholders). Both e2e suites
   updated to the new `data-role` hooks and green.
3. **Phase 3 (firmware + settings):** ~~connect-on-load race~~ ✅ (web-only);
   ~~`/api/brightness` + slider~~ ✅ (v0.1.12); ~~`/api/config` pixel count~~ ✅
   (v0.1.13); ~~`/api/protocol` LED protocol switch~~ ✅ (v0.1.14) — all **live,
   no reboot**; ~~Settings **WiFi form**~~ ✅ (shows current network + change
   creds, reboots to apply; mirror /api/wifi for e2e).
4. **Phase 4 (bigger features):** ~~mapper-as-editor-tab + debuggable~~ ✅;
   ~~3D preview~~ ✅ (rotating depth-sorted point cloud when the map's z varies);
   ~~Playlist tab + firmware playlist storage~~ ✅ (v0.1.15, resumes across
   reboots); ~~device map upload~~ ✅ (v0.1.16, `/api/map` install a computed
   2D/3D map on hardware so device patterns render2D; persisted). Also shipped:
   **gallery search** (filter 190+ patterns by name), **playlist polish**
   (clear-all, total run-time, deleted-pattern handling); **single-pattern
   reboot resume** (v0.1.29 — an activated saved pattern + its slider values
   persist debounced under a reserved storage key and resume at boot when no
   playlist was playing; see firmware/src/resume.rs);
   ~~runtime LED-protocol re-init edge cases~~ ✅ (v0.1.29 — hardened
   off-hardware; see the checklist below); ~~MQTT/HA~~ ✅ (v0.1.19);
   ~~AP-mode provisioning~~ ✅ (v0.1.22). Nothing left open in this phase.

## v0.1.29 hardware-verification checklist (protocol re-init + resume)

Code-level fixes verified only by compile/review — they need an
eyes-on-LEDs pass:

1. **SPI-first commit ordering**: `Msg::Protocol` now applies the SPI clock
   *before* committing the protocol (atomic + encode buffer); a failed
   `apply_config` keeps the old protocol entirely, so encode format and wire
   clock can't disagree. Verify the happy path didn't regress:
   sk9822↔ws2812↔sk9822 live switches still render correctly at full rate.
2. **Encode-buffer realloc discipline**: the old buffer is freed *before*
   the new one is allocated (ws2812 at 2048 px ≈ 18 KB), the reserve is
   fallible, and on failure the engines are frozen and the alloc retried;
   both encode paths length-check the buffer (lazily re-allocating once heap
   frees up, else skipping SPI output) instead of indexing out of bounds
   (= panic + reboot). Verify: at 2048 px with a
   heap-heavy pattern, switch sk9822→ws2812 — expect a clean switch or an
   "output paused" serial line, never a reboot.
3. **Requested-vs-applied persistence** (`WANT_PIXEL_COUNT`/`WANT_PROTOCOL`):
   back-to-back `POST /api/config` + `POST /api/protocol` (either order, no
   delay) must persist BOTH new values across a reboot — previously the
   second write could persist the first setting's stale value.
4. **Switch under DDP/E1.31 and mid-crossfade**: switching protocol while a
   stream is live (or a playlist crossfade is blending) resizes the buffer
   between frames; frames go out whole (blocking SPI), so no tearing is
   expected. Verify visually in both situations.
5. **Single-pattern reboot resume**: activate a saved pattern, move sliders,
   wait >3 s (debounce), power-cycle → the same pattern + slider values come
   back; a playlist that was playing still wins over the record; an ad-hoc
   editor push must NOT change what resumes (the last *saved* state
   returns); stopping a playing playlist then rebooting resumes the item
   that was showing.
</content>
</invoke>
