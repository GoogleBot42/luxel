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
  - *Playlist* 🔧 — choose patterns and play on a loop. Needs firmware playlist
    storage + a scheduler. Still deferred.
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

### Debugging must not lie when connected to a live device [M]
The device has no stepping debugger, yet placing a gutter breakpoint currently
arms debug mode even in device mode (via `onBreakpoints → toggleDebug`,
bypassing the disabled button). Two acceptable fixes: **hide the debug
capability entirely while streaming**, or **switch to local WASM computation**
for debugging (compile the same source locally and step there). Start by fully
gating it off in device mode; local-compute debugging is the nicer follow-up.

## Device connection & status

### "ws push" is jargon [S] ✅-able now
The streaming indicator says "ws push" — meaningless to users. Say something
like "streaming" / "live" (and "polling · 40 ms" for the HTTP fallback).

### Remember the device URL across reconnect [S] ✅ (superseded)
Resolved by removing the device-URL field entirely. The address is always known:
a real device serves the UI from its own flash (auto-connect to same origin),
`?device=<base>` is a dev/e2e override, and reconnect reuses the bound base — so
disconnect → a plain **"reconnect"** button, never a URL to type. A hosted
playground has no device support at all (`isPlayground` gates all device UI).

### Connection is async and handled badly on load [M] ✅
**Done.** Added a connection phase state (`idle → connecting → live`).
`connectDevice` enters `connecting` and holds the preview blank; a `markLive()`
helper flips to `live` on the *first real datum from the stream* (ws pixels or
status, or the HTTP-fallback poll) and clears the preview once at that moment —
so no pre-stabilization frames (leftover playground content, canvas-resize
artifacts, HTTP/WS cadence jitter) ever land in the waterfall. A "connecting…"
pill (header) + overlay (preview) shows the async handshake instead of stale
frames. Verified in the device-e2e: on connect the waterfall is held blank
(0 lit) through the handshake, then streams. See `deviceConn`/`markLive` in
App.svelte.

## Settings page 🔧 [L]

A real device needs a settings surface (page/dialog). Fields:
- **Brightness** ✅ — runtime value (`shared::BRIGHTNESS` atomic, 0–31) with
  `GET/POST /api/brightness`, applied every frame in the encode path (SK9822's
  5-bit current field + a software scale for WS2812) and persisted in a `LXDV`
  nvs record (survives reboot). The Settings slider drives it live. Shipped in
  firmware v0.1.12. (Preview is intentionally pre-brightness — it shows the
  pattern's colors; brightness dims the physical strip only.)
- **Strip type & pixel count** 🔧 — **not editable when connected** today
  (layout is locked in device mode). Needs `GET/POST /api/config` (pixel count,
  LED protocol) with persistence + safe re-init of the LED driver.
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

### Spinners while computing/loading [S]
Pattern tiles should show a spinner while their preview engine is compiling /
the pattern is loading, instead of appearing dead/blank.

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
3. **Phase 3 (firmware + settings):** ~~fix the connect-on-load race~~ ✅ done
   (web-only); ~~`/api/brightness` + Settings brightness slider~~ ✅ done
   (firmware v0.1.12, runtime + persisted + live-applied); remaining:
   `/api/config` (pixel count/protocol — needs runtime `PIXEL_COUNT`, buffer
   re-sizing, and a reboot-to-apply flow like WiFi) + Settings WiFi form.
4. **Phase 4 (bigger features):** ~~mapper-as-editor-tab (CodeMirror) +
   debuggable~~ ✅ done (Luxel map program, see above); 3D preview (map already
   emits `[x,y,z]` — Preview needs a projection); Playlist tab + firmware
   playlist storage; MQTT/HA (M4); AP-mode provisioning; device map upload
   (install a computed map on hardware).
</content>
</invoke>
