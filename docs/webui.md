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

### Tabbed structure [L] ✅ (Phase 2, minus Playlist)
The app now has top-level tabs shown in both modes (device adds Settings).

- **Device mode tabs:** Editor · Patterns · Settings _(Playlist deferred — 🔧)_
  - *Editor* — code + live preview + controls + vars + mapper (still a right-rail
    `<details>`; promoting it to a CodeMirror sub-tab is Phase 4).
  - *Patterns* ✅ — the gallery, promoted from a modal overlay to a first-class
    inline tab (lazy-mounted on first visit, then kept alive so tile engines
    persist). Picking a pattern jumps back to the Editor tab.
  - *Playlist* 🔧 — choose patterns and play them on a loop. Needs firmware
    playlist storage + a scheduler (does not exist yet — big). Still Phase 4.
  - *Settings* ✅ (shell) — device info + pixel-count readout now; brightness /
    pixel-count editing / WiFi form are honest Phase-3 placeholders (see below).
- **Playground mode tabs:** Editor · Patterns. Share is a prominent toolbar
  button here (hidden in device mode).

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

### Remember the device URL across reconnect [S]
Disconnect then connect should not require re-typing the URL. Persist the last
device URL (localStorage) and pre-fill / reuse it.

### Connection is async and handled badly on load [M]
Connecting to a device on page load is racy: the waterfall skips and keeps
stale/old data after things stabilize. The connect sequence (status → source →
controls → ws handshake) needs proper sequencing and a clean "connecting"
state that clears/holds the preview until the stream is actually live, instead
of leaking pre-stabilization frames into the waterfall.

## Settings page 🔧 [L]

A real device needs a settings surface (page/dialog). Fields:
- **Brightness** 🔧 — currently a compile-time const (`APA_BRIGHTNESS`, 0–31,
  SK9822 only). Needs a runtime value + `GET/POST /api/brightness` + apply in
  the render/encode path (and a WS2812-side global scale).
- **Strip type & pixel count** 🔧 — **not editable when connected** today
  (layout is locked in device mode). Needs `GET/POST /api/config` (pixel count,
  LED protocol) with persistence + safe re-init of the LED driver.
- **MQTT / Home Assistant** 🔧 — ("HQTT" in the notes) M4 territory, unbuilt.
  Broker host/port/creds + HA discovery toggle.
- **WiFi** — `GET/POST /api/wifi` already exists (stores creds + reboots).
  Surface it here. **AP-mode** provisioning is still to build.

## Sharing

### Share only in playground mode [S]
Share (pattern-in-URL) makes sense for the hosted/GitHub-Pages playground, not
on a device console. Hide it in device mode; keep it prominent in playground
mode.

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
3. **Phase 3 (firmware + settings):** `/api/brightness`, `/api/config`
   (pixel count/protocol); Settings tab wiring WiFi + brightness + pixel count;
   fix the connect-on-load race.
4. **Phase 4 (bigger features):** ~~mapper-as-editor-tab (CodeMirror) +
   debuggable~~ ✅ done (Luxel map program, see above); 3D preview (map already
   emits `[x,y,z]` — Preview needs a projection); Playlist tab + firmware
   playlist storage; MQTT/HA (M4); AP-mode provisioning; device map upload
   (install a computed map on hardware).
</content>
</invoke>
