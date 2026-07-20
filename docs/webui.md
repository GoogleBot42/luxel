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

### 3D mapping [L]
`normalizeMap` currently drops the z coordinate; the mapper already accepts
`[x,y,z]` but Preview only renders 2D. Add a 3D projection view (rotatable
point cloud / isometric). Pairs with the mapper-tab work.

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
  heap Vec). Persisted in the `LXDV` nvs record; capped at `MAX_PIXELS` (2048).
  Shipped firmware v0.1.13. The Settings Pixels field is editable and re-anchors
  the local preview.
- **LED protocol** ✅ — `GET/POST /api/protocol` (sk9822/ws2812 + aliases)
  switches the driver **live, no reboot**: `Msg::Protocol` calls
  `spi.apply_config()` to change the clock (8 MHz ↔ 2.4 MHz) and resizes the
  encode buffer. Persisted in `LXDV` (v3). Settings has a protocol dropdown.
  Shipped firmware v0.1.14. Verified on hardware (sk9822↔ws2812↔sk9822, no
  crash, SPI restores full speed).
- **MQTT / Home Assistant** 🔧 — ("HQTT" in the notes) M4 territory, unbuilt.
  Broker host/port/creds + HA discovery toggle.
- **WiFi** — `GET/POST /api/wifi` already exists (stores creds + reboots).
  Surface it here. **AP-mode** provisioning is still to build.

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
   (clear-all, total run-time, deleted-pattern handling). Remaining: MQTT/HA
   (M4); AP-mode provisioning; runtime LED-protocol re-init edge cases.
</content>
</invoke>
