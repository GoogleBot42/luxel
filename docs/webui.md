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

### Header is overcrowded [M]
Too many options, rarely-used ones too prominent. Fix by (a) collapsing file
actions (import/export/share/save/delete) into an overflow/⋯ menu, (b) showing
only mode-relevant controls, (c) moving structural navigation to tabs (below).

### Tabbed structure [L]
The app needs top-level tabs, not one wall of header buttons.

- **Device mode tabs:** Editor · Patterns · Playlist · Settings
  - *Editor* — code + live preview + controls + vars + mapper (as a sub-tab).
  - *Patterns* — the on-device library (the current "browse" gallery, promoted
    from a modal to a first-class tab) + save/load/delete against the device.
  - *Playlist* 🔧 — choose patterns and play them on a loop. Needs firmware
    playlist storage + a scheduler (does not exist yet — big).
  - *Settings* — see below.
- **Playground mode tabs:** Editor · Patterns (+ Share). No Settings/Playlist
  (nothing to configure), but Share is meaningful here.

## Editor

### Mapper is a first-class editor tab, and debuggable [M]
Today the map function lives in a `<details>` in the right rail. It should be a
**tab on the script editor** (peer of the pattern source), edited in CodeMirror
(not a bare `<textarea>`), and **debuggable** the same way patterns are — step
through the map function, inspect it. See ideas.md "Multi-pane: map editor +
preview".

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
2. **Phase 2 (web restructure):** tabs (Editor/Patterns) + header declutter
   (overflow menu); promote gallery to a Patterns tab; mode-aware header. Update
   e2e `data-role` hooks.
3. **Phase 3 (firmware + settings):** `/api/brightness`, `/api/config`
   (pixel count/protocol); Settings tab wiring WiFi + brightness + pixel count;
   fix the connect-on-load race.
4. **Phase 4 (bigger features):** mapper-as-editor-tab (CodeMirror) + debuggable;
   3D preview; Playlist tab + firmware playlist storage; MQTT/HA (M4); AP-mode
   provisioning.
</content>
</invoke>
