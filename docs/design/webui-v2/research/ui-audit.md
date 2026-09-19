# Luxel web UI — code audit for the rewrite

Read-only audit of `/home/googlebot/workspace/pixler-webui-v2`, 2026-09-18.
Scope: `web/src/App.svelte` (4052 lines), `web/src/components/*`, `web/src/lib/*`,
`web/src/flash/Flash.svelte`, `web/src/app.css`, `docs/webui.md`, `docs/lang.md`.

Terminology (`.claude/rules/web.md:44-46`): the hardware-bound UI is the **device
console**; the hardware-free UI is the **playground**. The playground must not
show device affordances.

Sizes for orientation: `App.svelte` = 2308 lines of `<script>` (1–2308), 1167 of
markup (2310–3476), 577 of `<style>` (3477–4052). 119 top-level `let`s, 20
reactive (`$:`) blocks, ~120 functions, 31 timer call sites, 109 `data-role`
hooks. There is a second, independent app entry: `web/src/flash/Flash.svelte`
(installer page, `flash.html`), sharing only `app.css` and the LNA classifier.

---

## 1. Screen inventory

There is exactly **one route-less shell** (`App.svelte:2312`, `<div class="shell"
data-mode data-tab>`). Everything is a mounted-but-`hidden` sibling; nothing
unmounts, so state survives switching (`App.svelte:3573-3580`).

| # | Surface | Kind | File:line | Visible when |
|---|---|---|---|---|
| 1 | Boot cover (spinner + `bootLabel`) | full-screen overlay | `App.svelte:2320-2327` | `booting` (until wasm loaded + device probe + running pattern pulled) |
| 2 | Header — editor variant: `← {backLabel}` + pattern name | header | `App.svelte:2329-2340` | `editing` |
| 3 | Header — home variant: wordmark + device/playground label | header | `App.svelte:2342-2344` | `!editing` |
| 4 | Tab: **Patterns Library** | tab button | `App.svelte:2346-2353` | `!editing` (always) |
| 5 | Tab: **PixelBlaze Library** | tab button | `App.svelte:2354-2363` | `!editing && hasPixelblazeLibrary` (probe of `pixelblaze-library.json`, `App.svelte:2238-2243`) |
| 6 | Tab: **Device Patterns** | tab button | `App.svelte:2364-2373` | `!editing && !isPlayground` (shown even while `device === null`) |
| 7 | Tab: **Playlist** | tab button | `App.svelte:2374-2385` | `!editing && device` (connected) |
| 8 | Tab: **Settings** | tab button | `App.svelte:2386-2393` | `!editing && device` |
| 9 | fps readout | header status | `App.svelte:2400`, text from `fpsReadout` `:766-782` | always |
| 10 | **Browser-blocked banner** (LNA/mixed content) | full-width alert, spans all tabs | `App.svelte:2409-2428` | `deviceBlocked` (`browserBlocked()`, `lna.ts:114`) |
| 11 | **Editor view** (2-column grid) | main surface | `App.svelte:2431-2812` | `editing` |
| 12 | Pattern-loading cover | overlay inside editor | `App.svelte:2432-2439` | `patternLoading` |
| 13 | Editor toolbar | toolbar | `App.svelte:2444-2510` | always inside editor |
| 14 | ⋯ overflow menu (import/export .epe) | popover menu | `App.svelte:2490-2499` | `menuOpen` (dismissed by window click, `:2310`) |
| 15 | Sub-tabs **pattern · map** | sub-tab bar | `App.svelte:2512-2534` | `layout.kind === "map"` only |
| 16 | Pattern CodeMirror | editor slot | `App.svelte:2537-2546` | mounted always, `hidden` unless `subTab==="pattern"` |
| 17 | Map CodeMirror | editor slot | `App.svelte:2547-2557` | `mapMounted && layout.kind==="map"`, `hidden` unless `subTab==="map"` |
| 18 | **Playback bar** | toolbar under editor | `App.svelte:2560-2691` | always inside editor |
| 19 | Banner stack (8 kinds) | right rail | `App.svelte:2694-2746` | see §4 |
| 20 | Debugger panel (map) | right rail | `App.svelte:2748-2754` | `subTab==="map" && mapDebugMode` |
| 21 | Debugger panel (pattern) | right rail | `App.svelte:2755-2757` | `debugMode && subTab==="pattern"` |
| 22 | Preview canvas | right rail | `App.svelte:2759-2761` → `Preview.svelte:203-251` | always |
| 23 | Controls section | right rail | `App.svelte:2763-2770` | header always; empty-hint if `controls.length===0` |
| 24 | Pins section | right rail | `App.svelte:2772-2793` | `pins.length>0 \|\| analogPins.length>0` |
| 25 | Map explainer section | right rail | `App.svelte:2795-2804` | `layout.kind==="map"` |
| 26 | Vars section | right rail | `App.svelte:2806-2810` | always (empty-hint when none) |
| 27 | **Patterns Library tab** | home tab | `App.svelte:2815-2841` | `!editing && tab==="library"` |
| 27a | "your patterns" chip row | sub-panel | `App.svelte:2824-2833` | `saved.length > 0` (localStorage library) |
| 27b | Gallery (lazy) | panel | `App.svelte:2834-2840`, `Gallery.svelte` | `galleryMounted && luxel` |
| 28 | **PixelBlaze Library tab** | home tab | `App.svelte:2844-2864` | `hasPixelblazeLibrary` + `tab==="pixelblaze"` |
| 29 | **Device Patterns tab** | home tab | `App.svelte:2867-2909` | `!isPlayground`, `tab==="device"` |
| 29a | offline hint | inline | `App.svelte:2882-2885` | `!device` |
| 29b | empty hint | inline | `App.svelte:2886-2887` | `devicePatterns.length===0` |
| 29c | device pattern rows | list | `App.svelte:2889-2906` | otherwise |
| 30 | **Playlist tab** | home tab | `App.svelte:2912-3013` | `!isPlayground`, `tab==="playlist"` (note: the *button* needs `device`, the *panel* only `!isPlayground` — inconsistent) |
| 30a | transport (play / stop+prev+next / clear) | header controls | `App.svelte:2918-2938` | `playlist.playing` splits the two arms |
| 30b | default-duration row | field row | `App.svelte:2941-2961` | always |
| 30c | crossfade row | field row | `App.svelte:2963-2976` | always |
| 30d | empty/offline hints | inline | `App.svelte:2978-2985` | `!device` / `items.length===0` |
| 30e | `PlaylistRow` list | list | `App.svelte:2987-3010`, `PlaylistRow.svelte` | items present |
| 31 | **Settings tab** | home tab | `App.svelte:3016-3474` | `device` truthy, `tab==="settings"` |
| 31a–31i | 9 setting cards | cards | see §5 | see §5 |
| 32 | Native `window.confirm`/`prompt` dialogs (8 sites) | browser modals | `:473, :638, :1354, :1402, :1413, :1736, :1846, :1885` | per action |
| 33 | **Installer page** (`flash.html`) | separate app entry | `web/src/flash/Flash.svelte`, `web/src/flash/main.ts` | separate URL — see §1b |

**Modals**: there are *no* in-app modals. Every confirmation/naming step is a
native `window.confirm`/`window.prompt` (8 sites above) — including the only way
to name a pattern (`saveToLibrary`, `App.svelte:1351-1385`).

### 1b. Installer page (`flash.html`) — separate entry

`web/src/flash/Flash.svelte` (525 lines) is a **second rollup entry**
(`web/vite.config.ts:21`, mounted by `web/src/flash/main.ts:1-7`) — the WLED
takeover installer. It is a single scrolling page of progressively-revealed
sections, not a wizard with back/next, and it has **no WebSerial**: the whole
flow is a network takeover of a WLED device.

| # | Section / element | Line | Visible when |
|---|---|---|---|
| — | `<main>` + `<h1>` + intro | 171-177 | always |
| 1 | `data-role="fw-source"` "1 · Firmware" | 180-209 | always |
| 1a | "Looking up the latest release…" | 182-183 | `sourceState === "loading"` |
| 1b | "Couldn't reach a firmware source" | 184-190 | `sourceState === "none"` |
| 1c | version + bundled/GitHub tag + notes link | 191-200 | `source` |
| 1d | "can't download them for you" note | 201-207 | `!source.canFetchBinaries` |
| 2 | `data-role="device"` "2 · Your WLED device" | 212-273 | always |
| 2a | address input + `probe-btn` | 217-230 | always; disabled unless `sourceState === "ready"` |
| 2b | `probe-result` — 5 exclusive branches (`wled` 234, already-`luxel` 237, CORS-opaque `reachable` 242, `blocked` 248, nothing 253) | 232-257 | `probe !== null` |
| 2c | `arch-stop` (ESP8266 dead end / unsupported chip) | 258-271 | `archUnsupported` |
| 3 | `data-role="flash"` "3 · Flash it" | 276-337 | `deviceReady && source` (`:62-65`) |
| 3a | `board-select` | 279-287 | always in §3 |
| 3b | manual download link + `bin-file` input | 289-295 | `board && !source.canFetchBinaries` |
| 3c | `flash-btn` (3 label states) + inline error | 296-306 | `board` |
| 3d | "browser wouldn't send the upload" | 307-312 | `flashState === "blocked"` |
| 3e | `manual-steps` `<details>` + `watch-btn` | 313-334 | `board`; auto-open when blocked |
| 4 | `data-role="progress"` "4 · The takeover" | 340-384 | `waitState !== "idle"` |
| 4a | `wait-status` + `<progress>` | 343-350 | `waitState === "waiting"` |
| 4b | `takeover-ok` | 351-355 | `waitState === "up" && luxelUp` |
| 4c | `timeout-help` + `rewatch-btn` | 356-381 | `waitState === "timeout"` |
| 5 | `data-role="assets"` "5 · The web app" | 387-426 | `source && (waitState === "up" \|\| probe?.kind === "luxel")` |
| 5a | `.luxa` download + file input | 395-400 | `!source.canFetchBinaries` |
| 5b | `assets-btn` + curl fallback | 401-416 | always in §5 |
| 5c | `done` + `done-link` | 417-424 | `assetsState === "done"` |
| — | footer | 428-433 | always |

**State:** 11 component-local `let`s + 5 `$:` derivations; four overlapping
status enums (`flashState` `:68`, `waitState` `:126`, `assetsState` `:139`,
`probe.kind` `flash/lib/device.ts:44-48`) with ordering encoded in `{#if}`
conditions rather than a machine. **Persists nothing** — no localStorage, no URL
state — during a 1–3 minute operation (`waitForLuxel` polls 2 s × 180 s,
`flash/lib/device.ts:103-118`); a reload drops the user back to step 1 with the
device mid-takeover.

**Endpoints:** `GET /api/status` (Luxel signature), `GET /json/info` (WLED
identity, then a `no-cors` liveness ping), `POST /update` (multipart,
`mode:"no-cors"`, 120 s), `POST /api/assets` (LUXA bundle), plus
`firmware/manifest.json` or the GitHub releases API
(`flash/lib/releases.ts:87+`).

**Geometry:** none at all. It never asks about strip vs grid, pixel count, data
pin, color order or LED type. "Board" exists only to pick a firmware *image* by
chip (`flash/lib/releases.ts:18-27, 45-50`), filtered by `arch` from WLED's
`/json/info` (`:53`). The user is handed off at `:420` to a device with default
geometry — so the main app is where setup must happen. Worth noting for the
rewrite: WLED's LED config (count, pin, order) is available at `/json/cfg` and
`/json/info` is already fetched, so carrying it over is a *new* feature, not a
port.

**Shared with `App.svelte`:** `data-role` hooks (22 of them, driven by
`web/tools/flash-e2e.mjs:107-208`), the `app.css` custom properties, and — the
one genuinely shared module — `lib/lna.ts` (`flash/lib/device.ts:23` imports
`browserBlocked`/`lnaHint` explicitly so the two can't drift).
**Duplicated:** it does **not** use `fetchgate` (bare `fetch()` at
`Flash.svelte:85, 149` and throughout `flash/lib/device.ts`), it has a **second
device client** (`web/src/flash/lib/device.ts` vs `web/src/lib/device.ts`), it
re-authors the blocked-device copy inline (`:248-252`) instead of App's
`.blocked-bar` (`App.svelte:2410-2427`), and it invents its own
`.err`/`.ok`/`.bad` classes (`:473-479`, with a hard-coded `#6fc36f`) instead of
App's `.banner error`/`.banner warn`.

**Debt:** no persistence (above); four un-unified enums; a reactive block that
mutates its own dependency (`:54-59` silently changes the user's board pick on
re-probe); a native `confirm()` at `:104`; section numbering hard-coded in
`<h2>` text and already out of sync with the code's own step comments
(`:21,30,50,67,125,138` count six steps, the page shows five); `canFetchBinaries`
branching hand-written in four places (`:201, 289, 317, 395`); ~90 lines of
troubleshooting prose interleaved with logic (`:242-247, 260-268, 307-312,
358-377`); the `reachable` branch asks the user to read JSON in another tab to
identify their chip (`:246`); no abort for the 3-minute poll (re-entrant from
`:331` and `:379`); no `aria-live`/`role="status"` on the live wait line
(`:344`); and the already-converted path (`:237-241`) tells the user to "use
step 5" while steps 3 and 4 stay hidden, so the page reads 1, 2, 5.

---

## 2. State model

### 2.1 The top-level state that matters

All in `App.svelte`'s single `<script>`; 119 `let`s. The load-bearing ones:

| Variable | Line | Role |
|---|---|---|
| `source` | 44 | the pattern text in the editor (single working copy, app-wide) |
| `layout: Layout` | 45 | **the preview rig** — `{kind:"strip",pixels} \| {kind:"grid",w,h} \| {kind:"map",coords}` (`examples.ts:205-210`) |
| `exampleName` / `patternName` | 46 / 1283 | name context for the header + export filename |
| `dirty` | 84 | source edited away from what it was loaded/saved as; drives device-resume |
| `engine` / `luxel` | 39-40 | the local WASM engine for the preview; `Engine` rebuilt on every recompile |
| `tab: Tab` | 936 | `"library" \| "pixelblaze" \| "device" \| "playlist" \| "settings"` — the *home* to return to |
| `editing` | 945 | the editor is a full-screen overlay, **not** a tab (`:930-934`) |
| `subTab` | 1426 | `"pattern" \| "map"` — only meaningful while `layout.kind==="map"` (`:1036`) |
| `device: DeviceSession \| null` | 102 | live session |
| `deviceBase: string \| null` | 919 | `""` = served from device, URL = `?device=` override, `null` = playground |
| `isPlayground` / `mode` | 924 / 928 | derived from `deviceBase`, not from `device` — so a *disconnected* device console is still "device" mode |
| `devicePixels` | 121 | hardware pixel count; the truth for capacity + layout math |
| `deviceMap` | 518-525 | `{installed, dims, count, kind?, w?, h?}` from `GET /api/map` |
| `devicePatterns` | 115 | device library `{id,name,source?}`, source lazily filled |
| `devicePatternId` | 117 | set while the editor holds a device-stored pattern |
| `playlist` + `playlistIntent` + `playlistSaving` | 577-584 | device playlist with optimistic transport (`playlist.ts`) |
| capacity block | 144-254 | `deviceHeapFree`, `deviceEngineHeap`, `deviceVmerr`, `capacity` |
| settings forms | 256-525 | `brightness/brightnessMax`, `pixelMax`, `deviceProtocol/protocolOptions`, `dataPin*`, `wifiSsid/wifiForm/wifiNote`, `mqttStatus/mqttForm/mqttNote`, `outputStatus`, `paletteStops/paletteAmount/paletteSupported/paletteNote`, `clockStatus`, `syncStatus`, `apNote`, `netLive` |
| mapper block | 1426-1447 | `mapSrc`, `mapEngine`, `mapCompileError`, `mapError`, `mapDebugMode`, `mapBreakpoints`, `mapDbg` |
| `rigChosen` / `rigDerivePending` | 1620 / 1625 | the #372 rig-derivation latch |

### 2.2 Persistence

**localStorage (`lib/store.ts`) — two keys only:**
- `luxel.patterns` (`store.ts:14`) — the playground's named library: `{name, source, savedAt}[]`. **No layout, no controls, no map.**
- `luxel.current` (`store.ts:15`) — the autosaved working copy: `{source, layout, patternName, exampleName, dirty}` (`store.ts:54-64`), written debounced 800 ms by `queueAutosave` (`App.svelte:1335-1349`).

**The device (see §6 for the full endpoint list)** holds: pattern library
(`/api/patterns`), the *installed pixel map* (`/api/map`), playlist
(`/api/playlist`), brightness, pixel count, protocol, data pin, WiFi, MQTT,
output pipeline + palette, clock tz, sync role.

**Not persisted anywhere:** `controlValues` (`App.svelte:52`) — slider positions
are lost on reload in the playground; on a device they are only persisted when
captured into a playlist item (`addToPlaylist`, `:617-629`) or by the firmware's
own resume record. `targetFps`, `debugMode`, `micOn`, `breakpoints`, `mapSrc`
(except inside a share link) are all session-only.

**URL fragment:** `#p=` / `#ps=` (source only) and `#pj=` / `#pjs=` (JSON
`{s: source, m: mapSrc}`) — `sharePattern` `:1716-1740`, `loadFromHash`
`:1747-1774`. **Query:** `?device=<base>` override (`detectDeviceBase` `:2201-2224`).

### 2.3 How `layout` is chosen, changed, and (not) pushed

Seven code paths write `layout`:

1. **Boot default** — `DEFAULT_PATTERN.layout = {kind:"strip", pixels:60}` (`examples.ts:215-223`), i.e. a 60-pixel strip before anything is known.
2. **Working-copy restore** — `layout = wc.layout` (`App.svelte:2256`), the *previous pattern's* rig.
3. **Device connect** — `layout = {kind:"strip", pixels: st.pixels}` (`App.svelte:1074`). The device's installed map is fetched *after* this (`refreshDeviceMap`, `:1140`) and never applied directly.
4. **Gallery pick** — `p.kind` from the build-time manifest, and **only when `!device`** (`App.svelte:1603-1610`): `grid → 16×16`, `cloud → cubeLattice(5)`, else `strip 60`.
5. **`deriveRig(engine)`** — the #372 path (`App.svelte:1651-1681`): runs once per *load* (gated by `rigDerivePending`, set by `markPatternLoaded` `:1630`), reads `Engine.preferredDims()` (`luxel.ts:345`), and **only ever upgrades a strip**. `dims===2` → grid, using `deviceMap.w/h` when the device map is a procedural grid, else `√devicePixels` square, else 16×16. `dims===3` → cube lattice, **playground only**.
6. **Manual playback-bar picks** — `setLayoutKind` (`:1797-1820`) / `setLayoutNum` (`:1822-1830`), both setting `rigChosen = true`.
7. **`installMap`** (`:1492-1496`) — the map program's output; and `onPixelCountChange` (`:1905-1920`) resets to a strip.

**The push direction is one-way and manual.** Three comments say it outright:
`installMap` "local preview only — a layout change never pushes to the device"
(`:1495`), `setLayoutKind` `:1819`, `setLayoutNum` `:1829`. The device's map is
only ever changed by two explicit buttons in the *editor's playback bar* —
`install grid on device` (`:2606-2612` → `installDeviceGrid` `:554-565` →
`POST /api/map "grid W H"`) and `install on device` (`:2625-2631` →
`installDeviceMap` `:538-550` → `POST /api/map <dims> <coords…>`), plus
`clear device map` (`:2633-2639`).

### 2.4 Why the pixel-map UI reads as per-pattern (it is device state)

Jeremy is right, and it is structural, not cosmetic. Six independent signals all
say "this belongs to the pattern":

1. **It lives inside the pattern editor.** The only map UI is the `pattern · map`
   sub-tab pair (`App.svelte:2512-2534`) — visually a *second document of this
   pattern*, peer to its source — plus the install buttons in the playback bar
   (`:2605-2644`). Settings has **no geometry/map card at all** (§5): the one
   genuinely device-level geometry control is not on the device page.

2. **It is persisted with the pattern's working copy.** `WorkingCopy.layout`
   (`store.ts:58-63`) is written next to `source`/`patternName` by
   `queueAutosave` (`App.svelte:1338`). Reload restores the rig *from the
   pattern record*.

3. **It travels in share links, attached to the pattern.** `sharePattern`
   (`:1716-1721`): "a custom map is part of the look — carry its PROGRAM in the
   link", serialized as `{s: source, m: mapSrc}`. A map is literally shipped as
   part of a pattern document.

4. **It is re-derived per pattern load.** `markPatternLoaded` (`:1630-1633`)
   clears `rigChosen` and arms `rigDerivePending` on *every* pattern arrival
   (gallery pick, library open, device-pattern open, `.epe` import, share link,
   device connect, paste `:1637`). `deriveRig` (`:1651`) then sets the rig from
   the *compiled pattern*. So switching patterns visibly changes "the map" —
   the classic per-document behaviour.

5. **The gallery encodes geometry per pattern.** `gallery.json` carries
   `kind: "strip"|"grid"|"cloud"` per entry, computed at build time from the
   source text by regex (`web/tools/gen-gallery.mjs:34-57` — `render2D` or one of
   8 bulk-2D builtin names → grid; `render3D` only → cloud). `onGalleryPick`
   (`App.svelte:1592-1615`) installs that as `layout`. Tiles are *shaped* by it
   (`Gallery.svelte:277-282`). The user reads this as "this pattern is a grid
   pattern", and the editor confirms it.

6. **`newPattern` throws the map away.** `App.svelte:999`:
   `if (layout.kind === "map") layout = {kind:"strip", pixels: pixelCount()}` —
   creating a new pattern *silently uninstalls the preview map*. Nothing about a
   device-level setting should behave that way.

Meanwhile the actual device-level facts are read but barely used:
`deviceMap.kind/w/h` (`device.ts:274-290`) is consumed in exactly one place —
`deriveRig` `:1657-1663` — and only when `kind === "grid"`. A device with a
**coords** map installed contributes nothing to the preview. The installed map is
surfaced only as a mono badge `"{count}px {dims}D on device"` (`:2614`, `:2640`),
which does not even show `w×h`. And the right-rail explainer tells the user the
opposite of the truth for the grid case: *"It only arranges this preview — it
isn't uploaded to the device"* (`:2801`), which is correct for `kind==="map"`
(where the block lives) but is the sentence a user will generalise from.

**Net effect:** the device has exactly one pixel map, changeable by two buttons
buried in a per-pattern toolbar; the UI presents up to three different
"layouts" — the device's installed map, the editor's preview rig, and the
gallery tile's shape — with no single place that says "this device is a 64×64
grid."

---

## 3. 1D-only / mode-blind assumptions

Every place the UI assumes a strip or ignores known device geometry:

| # | Site | File:line | Assumption |
|---|---|---|---|
| 1 | Boot default layout | `examples.ts:215-223` | `strip, 60 px` before anything is known |
| 2 | Device connect | `App.svelte:1074` | resets to `strip` at `st.pixels`, discarding the installed map; a 4096-px panel becomes a 4096-px *bar* until `deriveRig` upgrades it, and only for `render2D` patterns |
| 3 | `deriveRig` only upgrades | `App.svelte:1652` (`if (rigChosen \|\| layout.kind !== "strip") return false`) | a 1D `render` pattern on a device with `grid 64 64` installed previews as a strip, though the panel will show it row-major across 64×64 |
| 4 | `deriveRig` 3D suppressed on device | `App.svelte:1674` | `render3D` never gets a cloud preview on a device, even one with a 3D coords map installed |
| 5 | `setLayoutKind` grid default | `App.svelte:1816-1817` | builds `√devicePixels` square, **ignoring `deviceMap.w/h`** that `deriveRig` already knows — two different answers to "what shape is this device" in the same file |
| 6 | Layout dropdown always offered | `App.svelte:2567-2571` | strip / grid / 2D map offered unconditionally, even when the device's geometry is fixed and known (Jeremy's complaint); "3D" is not an option at all though maps can be 3D (`Preview.svelte:89`, `map3D`) |
| 7 | Grid W/H editable on a device | `App.svelte:2586-2604` | the `px` field is `disabled={!isPlayground}` (`:2580`) but W and H are not — a user can set 3×3 on a 64×64 panel and it silently only moves the preview |
| 8 | Gallery tile geometry is fixed constants | `Gallery.svelte:45-55` (`STRIP_PX=64`, `GRID=16`, `CUBE=5`) and `:87-98` | tiles never reflect the connected device: on a 64×64 HUB75 console every 2D tile is a 16×16 thumbnail and every 1D tile a 64-px bar |
| 9 | Gallery `kind` is a build-time regex | `web/tools/gen-gallery.mjs:34-57` | a *second, weaker* classifier competing with the compiled-program `preferredDims()` (`luxel.ts:339-348`); a `render2D` mentioned in a comment counts here but not there |
| 10 | `PatternThumb.kind` defaults `"strip"` and is **never passed** | `PatternThumb.svelte:13`; call sites `App.svelte:2899`, `PlaylistRow.svelte:102` | *every* device-pattern row and *every* playlist row thumbnail is a 64-px 1D bar, on every board. `PatternThumb` also has no `cloud` mode at all |
| 11 | `PlaylistRow` compiles at 64 px | `PlaylistRow.svelte:43` | control metadata read at a fake pixel count; `array(pixelCount)` patterns can behave differently (cf. the #420 element-budget class of failure) |
| 12 | `newPattern` template is 1D | `App.svelte:953-955` (`render(index) { hsv(index / pixelCount,1,1) }`) | "+ New pattern" on a 64×64 panel device starts you in 1D; `:999` also drops any map |
| 13 | Preview 1D injection | `Preview.svelte:20` | `y` forced to 0 for `strip` — correct, but means a strip-previewed 2D device loses the y axis for `readEvent` patterns |
| 14 | Share links carry no rig except a map program | `App.svelte:1719-1721`, `:1747-1774` | a shared 16×16 grid pattern reopens at whatever `deriveRig` guesses; grid *dimensions* are never transmitted |
| 15 | Settings has no geometry concept | `App.svelte:3021-3087` | "Pixels" is a scalar count; nothing says "64×64 panel", and the installed map is not shown or editable here |
| 16 | Settings "Status" shows the **browser's** fps | `App.svelte:3079-3086` | `{fps.toFixed(0)} fps (local preview)` on the *device* settings page (deliberate per `docs/webui.md:265-266`, but it is the only fps on that page) |
| 17 | `recompileMap` pixel count split | `App.svelte:1455` | map program runs at `devicePixels` on a device, `pixelCount()` in the playground — a third notion of "how many pixels" |
| 18 | `installDeviceGrid` has no inverse read-back | `App.svelte:554-565` | sets `deviceMap` optimistically without `kind/w/h`, so the very fields `deriveRig` depends on are blanked right after an install |
| 19 | Capacity model is the one geometry-correct path | `App.svelte:176-193` (`devicePixels`, "never the preview layout's") | worth preserving as the model for the rest |

**The general shape of the bug:** the app knows `devicePixels`, `deviceMap.kind/w/h`
and `Engine.preferredDims()`, but only one consumer (`deriveRig`) ever combines
them, once per pattern load, and only in the strip→grid direction. Everything
else — tiles, thumbnails, the new-pattern template, the layout dropdown, share
links — hardcodes a 1D or 16×16 assumption.

---

## 4. Editor toolbar & playback bar

### 4.1 Editor header (`App.svelte:2329-2340`, `:2398-2400`)

| Order | Control | Line | Condition |
|---|---|---|---|
| 1 | `← {backLabel}` back button | 2330-2337 | `editing`; label from `backLabel` `:1021-1026` (Device Patterns / PixelBlaze Library / Patterns Library) |
| 2 | pattern name (plain text) | 2338-2340 | always; `patternName \|\| exampleName \|\| "untitled pattern"` — **not editable here** |
| 3 | spacer | 2398 | |
| 4 | fps readout | 2400 | always; device fps when connected (`:766-782`) |

### 4.2 Editor toolbar (`App.svelte:2444-2510`), DOM order

| Order | Control | Line | Condition |
|---|---|---|---|
| 1 | **save** | 2445-2451 | always; opens a native `prompt` to name it (`:1351-1385`); routes to device or localStorage by `device` |
| 2 | **delete** | 2452-2460 | `devicePatternId !== "" \|\| saved.some(name match)` |
| 3 | **share** (`.primary`) | 2461-2470 | `isPlayground` only |
| 4 | **+ playlist** | 2471-2479 | `device && devicePatternId` |
| 5 | **⋯ overflow** | 2480-2500 | always → menu: `import .epe…` (2492), `export .epe` (2495) |
| 6 | `saveNote` | 2501 | transient (2–3 s) |
| 7 | `shareNote` | 2502 | transient |
| 8 | hidden file input | 2503-2509 | — |

### 4.3 Sub-tabs (`App.svelte:2512-2534`)

`pattern` / `map`, rendered **only when `layout.kind === "map"`**. So the map
editor is reachable only by first selecting "2D map" in the playback-bar
dropdown (`setLayoutKind` `:1800-1808` sets `subTab="map"` and runs the map
program), and it disappears the moment the layout changes (`:1036`).

### 4.4 Playback bar (`App.svelte:2560-2691`), DOM order

| Order | Control | Line | Condition |
|---|---|---|---|
| 1 | layout kind `<select>` (strip/grid/2D map) | 2567-2571 | always |
| 2 | pixel-count `<input>` + "px" | 2572-2584 | `layout.kind==="strip"`; **disabled on a device** |
| 3 | grid W `×` H inputs | 2585-2604 | `layout.kind==="grid"`; never disabled |
| 4 | **install grid on device** | 2606-2612 | `grid && device` |
| 5 | `{count}px {dims}D on device` badge | 2613-2615 | `grid && device && deviceMap.installed` |
| 6 | `{n} px mapped` badge | 2618 | `layout.kind==="map"` |
| 7 | **run map** | 2621-2623 | `subTab==="map"` |
| 8 | **install on device** | 2625-2631 | `subTab==="map" && device && layout.kind==="map"` |
| 9 | **clear device map** | 2633-2639 | + `deviceMap.installed` |
| 10 | `{count}px {dims}D on device` badge (again) | 2640-2642 | same |
| 11 | map error text | 2645 | `mapError` |
| 12 | separator | 2647 | |
| 13 | target-fps `<select>` (max/60/30/15/5) | 2648-2654 | always |
| 14 | **pause / play** | 2655-2657 | always |
| 15 | **debug** (map) | 2659-2667 | `subTab==="map"` |
| 16 | **sound** (mic toggle) | 2671-2679 | `subTab!=="map"` |
| 17 | mic error text | 2680 | `micError` |
| 18 | **debug** (pattern) | 2681-2689 | `subTab!=="map"` |

### 4.5 Banner stack (`App.svelte:2694-2746`), top to bottom

| Order | Banner | Line | Condition |
|---|---|---|---|
| 1 | wasm load failure (error) | 2694-2696 | `loadFailure` |
| 2 | device error (error) | 2697-2699 | `deviceError` |
| 3 | capacity **rejected** (error, device's own vmerr) | 2704-2708 | `deviceRejectedForSize` (`:165-168`) |
| 4 | capacity **prediction** (warn, `data-level`) | 2708-2719 | `capacity` (local model) |
| 5 | `.epe` import error (error, dismissible) | 2720-2725 | `importError` |
| 6 | compile error (clickable → `jumpToError`) | 2726-2730 | `compileError && subTab==="pattern"` |
| 7 | map compile error (clickable) | 2731-2740 | `mapCompileError && subTab==="map"` |
| 8 | runtime error (warn, dismissible) | 2741-2746 | `runtimeError && !compileError && subTab==="pattern"` |

### 4.6 Right-hand panels, top to bottom

Debugger (`:2748-2757`) → Preview (`:2759-2761`) → **Controls** (`:2763-2770`) →
**Pins** (`:2772-2793`) → **Map** explainer (`:2795-2804`) → **Vars**
(`:2806-2810`). All in one scrolling rail `minmax(320px, 420px)` wide
(`:3542`).

### 4.7 Assessment — grouping and hierarchy

Jeremy's "the buttons are kind of a mess and placed a bit randomly" is
justified by the DOM order above. Concretely:

**Three unrelated axes are interleaved in one flat playback bar.** The bar
mixes (a) *what the pixels are* (layout kind, px, W×H — a rig/hardware concern),
(b) *device geometry installation* (install grid / install map / clear map —
device state mutation), (c) *preview transport* (target fps, pause), and
(d) *tooling modes* (sound, debug). Items 1–11 and 13–18 have nothing to do
with each other, and there is exactly one `<span class="sep">` between them
(`:2647`).

**Device-mutating buttons sit next to preview-only ones with no visual
distinction.** `install grid on device` / `install on device` / `clear device
map` (`:2606`, `:2625`, `:2633`) permanently change device state; `strip`/`grid`
and the W/H inputs next to them change nothing but this browser tab. Nothing in
the styling separates them, and the only cue is a `title` attribute.

**Genuinely primary actions are demoted.** Save (`:2445`) is a plain button
styled identically to delete; the only `.primary`-styled toolbar button is
*share*, which exists only in the playground (`:2461-2470`). In device mode the
toolbar has no visual primary at all. `+ playlist` (`:2471`) — a niche action
requiring the pattern to already be saved on the device — has the same weight as
save.

**The ⋯ overflow is under-used.** It contains only import/export `.epe`
(`:2492-2496`), while at least as rare things (install map on device, clear
device map, mic, target-fps, `.epe`) sit in the open.

**Duplicated affordance.** The `{count}px {dims}D on device` badge is emitted
twice from different branches (`:2614` and `:2640`) with the same `data-role`,
so a DOM query for it is ambiguous.

**Two "debug" buttons with identical labels** (`:2659` map, `:2681` pattern)
occupy the same slot depending on `subTab` — the user cannot tell from the
button which engine it arms.

**Rarely used, currently prominent:** target-fps selector, mic/"sound",
`run map`, `install on device`, `clear device map`, `+ playlist`, the grid W/H
spinners on a device.
**Frequently used, currently ambient:** save, the compile-error banner (it is in
the right rail, not near the code), the pattern name (header text, not an
editable field).
**Misplaced:** anything with "on device" in its label belongs to a device page,
not to a per-pattern toolbar (§2.4); `pause` and `target fps` are preview
settings, not pattern actions; the pattern name should be editable where it is
displayed rather than via `window.prompt`.

---

## 5. Settings page

`App.svelte:3016-3474`, visible only when `device` is non-null. Cards in DOM
order. "Typical" = a user setting up an LED strip once; "power" = someone
tuning, debugging or integrating.

| # | Card / field | Line | API | Live? | Audience | Board-dependent |
|---|---|---|---|---|---|---|
| 1 | **Device** | 3021 | | | | |
| 1a | Address (read-only) | 3023-3026 | none (`device.base`) | — | typical (orientation) | no |
| 1b | Pixels | 3027-3039 | `POST /api/config` (`device.ts:198`), max from `/api/status.max_pixels` | live, no reboot | **typical** | yes — `pixelMax` 2048 strip / 4096 panel (`device.ts:24-29`); meaningless as a *count* on a panel where the real question is W×H |
| 1c | LED protocol (sk9822/ws2812) | 3040-3048 | `GET/POST /api/protocol` | live | **typical** (strip) | **yes — strip-only concept**; shown on HUB75 boards too |
| 1d | Data pin + apply&reboot | 3049-3078 | `GET /api/config.data_pin*`, `POST /api/datapin` | **reboot**, two-step + confirm | power | yes — hidden when `data_pins` empty (`:3049`), i.e. panel boards/old firmware |
| 1e | Status (local preview fps + vmerr) | 3079-3086 | none — browser value | — | power (and misleading: it is the *browser's* fps on the device page) | no |
| 2 | **Network input** | 3089 | | | | |
| 2a | Status (DDP/E1.31/idle) | 3091-3095 | `GET /api/status.live` (`:788-795`, 2 s poll) | read-only | power | no |
| 2b | Explainer (ports, universes) | 3097-3102 | — | — | power | no |
| 3 | **Brightness** | 3105 | | | | |
| 3a | Slider 0..`brightnessMax` | 3107-3119 | `GET/POST /api/brightness` | live + persisted | **typical — arguably the #1 field** | no (max 31 from device) |
| 4 | **WiFi** | 3127 | | | | |
| 4a | Current SSID + source | 3129-3141 | `GET /api/wifi` | read-only | typical | no |
| 4b | Network (SSID) | 3142-3145 | — | — | typical | no |
| 4c | Password | 3146-3155 | — | — | typical | no |
| 4d | save & reboot | 3156-3166 | `POST /api/wifi` | **reboot**, confirm (`:1846`) | typical (once) | no |
| 4e | reboot into setup AP | 3173-3179 | `POST /api/apmode` | **reboot**, confirm (`:473`) | power / recovery | no |
| 5 | **Output** | 3182 | (whole card `{:else}` "not available on this firmware" `:3348-3350`) | | | |
| 5a | Color order (RGB…BGR) | 3185-3197 | `GET/POST /api/output` | live | **typical** (strip wiring) | **strip-oriented**; a HUB75 panel's channel order is not a user concern |
| 5b | Gamma (×10 on the wire) | 3198-3215 | same | live | power | no |
| 5c | Power cap (mA) | 3216-3229 | same | live | power (but safety-relevant) | strip-oriented (per-LED current model) |
| 5d | Brightness curve | 3230-3247 | same | live | power | no |
| 5e | Blur % | 3248-3261 | same | live | power | **yes** — 2D over rows/cols when a matrix map is installed, else along the index (`docs/webui.md:310-313`) |
| 5f | Glow % | 3262-3275 | same | live | power | no |
| 5g | Palette editor (preview, per-stop color+pos, add/remove/clear, amount %) | 3276-3347 | `POST/DELETE /api/output/palette` | live + persisted | power (deep) | no; gated on `paletteSupported` (`:3276`) |
| 6 | **Clock** | 3353 | | | | |
| 6a | Device time | 3355-3360 | `GET /api/clock` (2 s poll) | read-only | typical (diagnostic) | no |
| 6b | UTC offset (hours) | 3361-3374 | `POST /api/clock` | live | power (needed only for `clockHour()` patterns) | no |
| 7 | **Multi-device sync** | 3377 | | | | |
| 7a | Role (off/leader/follower) + status | 3379-3399 | `GET/POST /api/sync` | live | power | no |
| 8 | **MQTT / Home Assistant** | 3407 | | | | |
| 8a | Status | 3409-3418 | `GET /api/mqtt` (2 s poll) | read-only | power | no |
| 8b | Broker host + port | 3419-3435 | — | — | power | no |
| 8c | User | 3436-3439 | — | — | power | no |
| 8d | Password | 3440-3449 | — | — | power | no |
| 8e | save | 3450-3455 | `POST /api/mqtt` | live | power | no |
| 9 | **Pattern library** (count + link to Device Patterns) | 3464-3471 | `GET /api/patterns` (count only) | — | typical | no |

**Assessment.** Nine cards, ~30 fields, flat, in an order that is neither
frequency- nor audience-sorted: the single most-used control (Brightness) is
card 3, below a read-only DDP diagnostic card; six of seven Output fields are
expert tuning knobs presented at the same weight as Color order; MQTT (4 fields
+ password) is a full card for an integration most users never enable; and two
of the nine cards (Network input, Pattern library) are pure read-outs with no
settings in them. Three actions reboot the device (4d, 4e, 1d) and are mixed in
with live ones with no grouping. There is **no geometry/map card** at all
(§2.4), and nothing on the page is board-aware: `LED protocol`, `Color order`
and `Power cap` are strip concepts shown unconditionally on a HUB75 console,
while `Pixels` is a scalar where a panel wants W×H.

A "typical user" set would be: Brightness, WiFi, Pixels/geometry, LED protocol,
Color order — five fields. Everything else (gamma, brightness curve, blur, glow,
palette, power cap, clock tz, sync, MQTT, DDP status, data pin, AP mode) is
power-user material and is exactly what Jeremy suggests moving "further down, in
a spoiler."

---

## 6. Device API surface used by the UI

All device traffic goes through `DeviceSession` (`web/src/lib/device.ts`) →
`gatedFetch` (`fetchgate.ts:104`), which caps in-flight requests at 2
(`fetchgate.ts:92`), retries connection failures 6× with backoff, and buffers
whole bodies inside the gate slot (`fetchgate.ts:118-135`).

| Endpoint | Method | `device.ts` | Request → response as used | UI feature |
|---|---|---|---|---|
| `/api/status` | GET | 154 | → `{fps, out_fps?, rescan_hz?, pixels, max_pixels?, vmerr, live?, heap_free?, engine_heap?, …}` | device detection (`App.svelte:2213-2219`), connect handshake (`:1056`), 1 Hz fps + heap + vmerr poll (`:752-757`, `:1181-1196`), DDP status (`:788-795`), capacity model |
| `/api/pattern` | GET | 159 | → source text | pull the running pattern on connect (`:1077`) |
| `/api/brightness` | GET / POST | 164 / 172 | → `{brightness,max}`; body = int 0..31 | Settings brightness slider (`:3107`) |
| `/api/config` | GET / POST | 185 / 198 | → `{pixels,max,protocol,data_pin?,data_pin_default?,data_pin_next?,data_pins?}`; body = int | Settings Pixels + Data pin picker (`:3027`, `:3049`) |
| `/api/datapin` | POST | 192 | body = pin \| `"default"` → `{ok,data_pin?,error?}` | Settings apply&reboot (`:3061`) |
| `/api/protocol` | GET / POST | 207 / 215 | → `{protocol,options[]}`; body = name | Settings protocol select (`:3042`) |
| `/api/code` | POST | 220 | body = LXP1 envelope (`device.ts:75-101`) → `RunResult` | **every keystroke** (500 ms debounce, `:1269-1272`) and every pattern open (`applyEdit` `:1200`) |
| `/api/control` | POST | 228 | body = `"name v0 v1 v2"` (raw 16.16) | Controls panel (`onControlSet` `:1927-1930`) |
| `/api/patterns` | GET / POST | 235 / 252 | → `{patterns:[{id,name}]}`; POST body = LXP1 envelope → `{ok,id?}` | Device Patterns list; save-to-device (`:1364`) |
| `/api/patterns/:id` | GET / DELETE | 242 / 264 | → `{id,name,source}` | row thumbnails (`loadDevicePreviewSources` `:849`), open-in-editor, delete (`:1404`) |
| `/api/patterns/:id/activate` | POST | 481 | → `RunResult` (`code:"bc-version"` special-cased) | opening a device pattern (`:883`) |
| `/api/map` | GET / POST | 274 / 294,305,312 | → `{installed,dims,count,kind?,w?,h?}`; POST body = `"<dims> <raw…>"`, `"grid W H"`, or `""` | `deviceMap`; install grid / install map / clear map buttons (`:2606`,`:2625`,`:2633`) |
| `/api/wifi` | GET / POST | 317 / 325 | → `{ssid,source}`; body = `"ssid\npassword"` | Settings WiFi card |
| `/api/mqtt` | GET / POST | 334 / 340 | → `MqttStatus`; body = 4 newline-separated fields | Settings MQTT card |
| `/api/output` | GET / POST | 359 / 382 | → `{order,gamma,capMa,brightCurve?,blur?,glow?,palette?,paletteAmount?}`; body = 6 positional tokens | Settings Output card |
| `/api/output/palette` | POST / DELETE | 405 / 417 | body = `"<amount> <pos r g b…>"` | Settings palette editor |
| `/api/clock` | GET / POST | 423 / 432 | → `{synced,local,tzMinutes}`; body = minutes | Settings Clock card |
| `/api/apmode` | POST | 441 | body `""` → `{ok,note?}` | Settings "reboot into setup AP" |
| `/api/sync` | GET / POST | 447 / 452 | → `SyncStatus`; body = mode | Settings Multi-device sync |
| `/api/sensors` | POST | 459 | body = 98-byte SB1.0 frame | mic → device streaming, ≤20 Hz, one in flight (`:2162-2169`) |
| `/api/events` | POST | 468 | body = `EV1` frame, ≤32 events | preview click/drag injection, batched 50 ms (`:1990-2003`) |
| `/api/playlist` | GET / POST | 489 / 495 | → `Playlist`; POST body = `D/X/I/C` line format | Playlist tab (1 Hz poll `:736-741`, 400 ms debounced save `:599-612`) |
| `/api/playlist/{play,stop,next,prev}` | POST | 509-520 | body = index (play) | transport buttons (`:2918-2932`) |

**Endpoints defined but never called from the UI:** none — every method on
`DeviceSession` has a caller. **Fields read but unused:** `DeviceStatus.heap_largest`
(`device.ts:48`), `frame_us/vm_us/pipe_us/out_us` (`:62-65`), and
`deviceMap.kind/w/h` outside `deriveRig`.

### Multi-call compositions (one user action → several requests)

- **Connect** (`connectDevice` `:1050-1154`): `status` → `pattern` → `brightness`
  → `config` → `protocol` → `wifi` → `mqtt` → `output` → `patterns` → `playlist`
  → `map` — **up to 11 sequential round-trips through a 2-slot gate** before the
  boot cover lifts. Each is individually `try`/`catch`-ed for old firmware
  (`:1084-1136`), which is why it cannot be batched as written.
- **Open a device pattern** (`loadDevicePattern` `:879-910`):
  `GET /api/patterns/:id` → `POST …/activate`; on `code:"bc-version"` it
  *locally recompiles*, `POST /api/patterns` (re-save) and activates again —
  four requests plus a wasm compile for one click.
- **Device Patterns list**: `GET /api/patterns` then **N sequential**
  `GET /api/patterns/:id` (`loadDevicePreviewSources` `:849-863`, deliberately
  serial for the 2-socket pool) just to render thumbnails.
- **Push a pattern** (`devicePush` `:1160-1176`): `POST /api/code` then
  `GET /api/status` — the second call exists purely because the device's
  capacity rejection is asynchronous and only surfaces as `vmerr`.
- **Save to device** (`saveToLibrary` `:1356-1377`): local compile →
  `POST /api/patterns` → `refreshDevicePatterns()` → N source fetches.
- **Add to playlist** (`addToPlaylist` `:617-629`): requires the pattern to
  *already* be saved on the device (`devicePatternId`), then rewrites the
  **whole** playlist (`POST /api/playlist` with every item) 400 ms later.
- **Transport** (`playlistPlay` `:707-711`): `POST …/play` → `GET /api/playlist`,
  with `reconcileTransport` (`playlist.ts:194-204`) masking the read for up to
  3 s because the device applies the change in its render loop.
- **Pixel-count change** (`onPixelCountChange` `:1905-1920`): `POST /api/config`,
  then a purely local reset of `layout`, `subTab` and the preview.
- **Install geometry**: `POST /api/map` only — no follow-up `GET`, so
  `deviceMap.kind/w/h` are blanked locally (`:545`, `:560`).

---

## 7. What works well (keep it)

1. **`fetchgate.ts` (74 lines)** — one global gate for *every* app-initiated
   fetch, assets included. It caps in-flight at 2 (`:92`), retries refused
   connections with exponential backoff (`:139-141`), bounds each attempt at
   30 s (`:99`), and — the subtle part — **holds the slot until the body is
   fully received** (`:118-135`), because `fetch()` resolves at headers and a
   300 KB gallery body keeps a device socket busy for seconds. This is the
   single thing that makes the console usable when served from a 2-socket ESP32.
   Keep it verbatim; it has no UI coupling.

2. **The local-preview-plus-push model** (`docs/webui.md:94-118`). The preview
   always runs on the local WASM engine, even on a device (`recompile` `:1207`
   is device-independent); the device is a sink that receives bytecode
   (`devicePush` `:1160`) and control values (`:1929`). Consequences worth
   keeping: the step debugger is genuine local computation and works everywhere;
   there is no pixel-stream socket to go stale; typing stays responsive
   (150 ms local recompile vs 500 ms throttled push, `:1266-1272`); and a
   pattern that fails to compile is never sent (`:1162`).

3. **`data-role` hooks (109 in `App.svelte`, 23 across components, 22 in the
   installer).** Two e2e
   suites (`web/tools/e2e.mjs` 800 lines, `web/tools/device-e2e.mjs` 1401 lines)
   drive real chromium against ~80 of them, including *negative* assertions that
   removed UI stays removed (`e2e.mjs:109` "the examples dropdown is gone",
   `device-e2e.mjs:166` "no reconnect button"). A rewrite that keeps the same
   role names keeps most of that harness.

4. **The capacity banner (#15/#287/#420)** — `checkCapacity` `:176-254` +
   `deviceRejectedForSize` `:165-168` + the banners at `:2704-2719`. It is the
   only part of the UI that models the *device* rather than the browser: it
   uses `devicePixels`, never the preview layout (`:184-186`), distinguishes the
   live-push path from the stored path and says "saving it to the device's
   library would fit" when that is the actual fix (`:210-212`), and grades
   severity by **certainty** — amber for the local prediction, red for the
   device's own `vmerr`. It is non-blocking throughout. That certainty-graded,
   never-blocking banner idiom is the right template for the whole rewrite.

5. **Gallery lazy tiles (`Gallery.svelte`).** `IntersectionObserver` +
   `STEP_BUDGET=6` engine frames per rAF + `TILE_FPS_MS=90` + `ENGINE_CAP=40`
   with LRU-by-`seen` eviction (`:160-196`) keeps ~190 live pattern engines down
   to a couple of dozen. Compilation is spread at ≤2 per frame (`:171-173`).
   Non-compiling patterns degrade to a grayed "dead" tile rather than
   disappearing (`:94-97`). Spinner per tile from in-view to first frame
   (`:283-290`).

6. **The Luxel-program mapper.** The map is a real Luxel program on the VM
   (`plot(x,y[,z])` per pixel), compiled into its own engine
   (`luxel.ts:205-209`), edited in the same CodeMirror and **debugged with the
   same `Debugger.svelte`** — breakpoints, step over/into/out, stack, locals,
   globals (`App.svelte:1451-1557`). That is a genuinely good idea and should
   survive; only its *placement* (inside the pattern editor) is wrong.

7. **`Debugger.svelte` (158 lines)** — a pure view over a `DebugSnapshot` that
   dispatches `step`/`break` and is reused by two engines with only a
   `runningHint` prop difference (`App.svelte:2748-2757`). Exactly the component
   shape the rest of the app lacks.

8. **`Controls.svelte`'s "guess" treatment** (`:30-43`, `:74-79`, `:207-220`).
   An untouched control with no `//# default=` cannot have its live value read
   back from the engine, so the slider position shown is a *placeholder* — and
   the UI says so: dimmed widget, `?` flag with an explanatory tooltip, and a
   native `indeterminate` checkbox for toggles (`:139`). Honest-by-default UI;
   keep the principle.

9. **`PinPanel.svelte`'s idle-level model** (`:37-46`). "Press" drives a pin to
   the *opposite of its idle level*, so a pulled-up pin goes LOW — the
   button-to-ground wiring people actually have. Momentary press and latch are
   independent holds. Keyboard press/release is handled explicitly (`:76-86`).

10. **Optimistic transport reconciliation** (`lib/playlist.ts`). `POST play/stop`
    doesn't return state and the device applies it in its render loop, so
    `reconcileTransport` (`:194-204`) shows what the user asked for until the
    device agrees or a 3 s window expires. A small, tested (`web/tests/playlist.test.mjs`),
    reusable pattern for every fire-and-poll control in the app.

11. **The boot cover** (`:2320-2327`, `bootLabel` `:2269`). The app does not
    render until it knows playground-vs-device *and* has pulled the running
    pattern, so the playground never flashes before the console appears. Same
    idea at pattern-load granularity (`patternLoading` `:2432-2439`).

12. **The LNA classifier (`lib/lna.ts`)** and its dedicated blocked-banner
    (`:2409-2428`) — a browser refusal is diagnosed by *shape* (https page,
    http target) rather than by error text, and the banner names the two ways
    around it instead of saying "cannot reach device". Shared with the installer
    page so the two can't drift (`lna.ts:22-23`).

13. **Component-local CSS + 9 CSS custom properties** (`app.css:1-12`). The
    whole app themes off `--bg/--bg-panel/--bg-inset/--border/--text/--text-dim/--accent/--error/--warn`;
    no framework, no build-time CSS dependency. Worth keeping (a rewrite could
    add a light theme by touching only that block).

---

## 8. Tech-debt hotspots and a decomposition sketch

### 8.1 What makes `App.svelte` hard to restructure

- **Size and shape.** 4052 lines in one component: 2308 of script, 1167 of
  markup, 577 of style. 119 top-level `let` bindings in one scope, 20 `$:`
  blocks, ~120 functions, 31 timer call sites.
- **Every concern is in the same scope.** The pattern editor, the map editor,
  the device session, the capacity model, the playlist, nine settings forms, the
  microphone, the event injector, the pin poller, the share-link codec, the
  `.epe` importer, the render loop and the debugger are all peers in one
  closure. Nothing can be moved without untangling the closure — e.g.
  `installDeviceMap` (`:538`) reads `layout`, writes `deviceMap` *and* writes
  `saveNote`, which is a toolbar-local transient.
- **Four polling loops with ad-hoc lifecycles**, each a reactive block that
  clears and re-creates an interval: playlist 1 Hz (`:736-741`), status 1 Hz
  (`:752-757`), settings 2 Hz (`:797-812`), plus the rAF render loop
  (`:2149-2193`). Their gating conditions (`device && tab === … && !editing`)
  are duplicated inline.
- **Timer-based transients as state.** `saveNote`, `shareNote`, `mapError`,
  `micError`, `wifiNote`, `mqttNote`, `dataPinNote`, `apNote`, `paletteNote` are
  nine separate strings each cleared by its own `setTimeout`. There is no
  notification primitive.
- **Reactive-statement fragility.** `.claude/rules/web.md:10-14` documents the
  trap (a `$:` block only tracks what appears in its own syntax), and the code
  works around it by hand — `matchRunningToLibrary(devicePatterns, source,
  dirty, devicePatternId, editing, device)` (`:830`) passes six dependencies as
  arguments purely to make tracking work; `Controls.svelte:30-37` does the same.
  Every future edit has to re-derive that rule.
- **Recursive `recompile()`.** `:1207-1261` calls `deriveRig` which mutates
  `layout` and then re-enters `recompile()` (`:1215-1224`), guarded by a
  one-shot flag. Correct, but it makes the compile path the place where geometry
  is decided — the opposite of where a reader looks for it.
- **Three sources of truth for geometry** (`layout`, `devicePixels`,
  `deviceMap`) with no reconciler (§2.4, §3).
- **`window.prompt`/`confirm` × 8** (`:473, 638, 1354, 1402, 1413, 1736, 1846,
  1885`) — unstyleable, untestable without dialog handlers, and the only naming
  affordance in the app.
- **One-way device writes.** Most settings POST and then re-`GET`
  (`onOutputChange` `:398-405`), but `installDeviceGrid`/`installDeviceMap`
  (`:554`, `:538`) write an *optimistic* `deviceMap` missing `kind/w/h` —
  the very fields `deriveRig` needs.
- **Duplicate/ambiguous `data-role`s** (`map-installed` at `:2614` and `:2640`).
- **A second app entry that shares almost nothing.** `flash/Flash.svelte` and
  `App.svelte` share only `app.css` and `lib/lna.ts`; the installer has its own
  `flash/lib/device.ts`. `.claude/rules/web.md:15-30` explains why the emitted
  `<script>`/`<link>` set of each entry HTML is load-bearing (Gitea #92) — so
  the rewrite's bundle shape must be re-checked with `web/tools/coldload.mjs`.
- **Doc rot:** `docs/webui.md` is marked complete/closed (`:9-15`) and ends with
  a stray tool-call artifact (`</content>` / `</invoke>` on lines 478-479) —
  it is a historical record, not a spec, and should not be the rewrite's brief.

### 8.2 Candidate decomposition (page-level sketch)

Not a spec — the seams the current code already suggests.

**Shell**
- `App.svelte` → routing + mode only: `mode` (playground/device), `tab`,
  `editing`, boot cover, header, the blocked banner. Target: < 200 lines.

**Stores (plain `.ts`, no Svelte component)**
- `stores/device.ts` — `DeviceSession` lifecycle, the connect handshake, and
  **one** poll scheduler with per-tab subscriptions (replaces the three
  intervals). Owns `devicePixels`, `deviceMap`, `deviceHeapFree`, `vmerr`, fps.
- `stores/geometry.ts` — **the missing piece.** The single reconciler of
  `deviceMap` (device truth) × `Engine.preferredDims()` (what the pattern wants)
  × a user override → the rig every consumer reads: preview, gallery tiles,
  thumbnails, the new-pattern template, share links. Answers "what will this
  actually look like" once, for everyone.
- `stores/pattern.ts` — `source`, `dirty`, name/id, control values, the working
  copy + local library (today's `lib/store.ts`), `.epe` import/export, share
  codec.
- `stores/notify.ts` — one transient-notification primitive replacing the nine
  `*Note` strings, plus a promoted banner list replacing the 8-deep inline stack.
- keep `lib/{fetchgate,lna,hints,playlist,luxel,audio,builtins}.ts` as they are.

**Pages**
- `pages/Library.svelte` (Patterns Library + PixelBlaze Library — one component,
  two data sources, as `Gallery.svelte` already is).
- `pages/DevicePatterns.svelte`.
- `pages/Playlist.svelte` (already nearly self-contained; `PlaylistRow` stays).
- `pages/Settings.svelte` → split into `settings/` cards, each owning its own
  form + endpoint, grouped by audience: **Basics** (brightness, geometry, WiFi,
  protocol/color order) always open; **Advanced** (output tuning, palette,
  clock, sync, MQTT, data pin, AP mode, DDP status) behind disclosure. Add the
  missing **Geometry** card (pixel count *or* W×H, the installed map, install /
  clear) — this is where the map belongs, not in the editor.
- `pages/Editor.svelte` → header (editable name), `EditorToolbar.svelte`
  (file actions only), `PreviewPanel.svelte` (preview + transport: pause,
  target fps, sound), `InspectorPanel.svelte` (banners, debugger, controls,
  pins, vars), and `MapEditor.svelte` promoted out of the pattern sub-tab into
  a device-geometry surface (kept debuggable — see §7.6).

**Components to keep essentially as-is:** `Editor.svelte` (CodeMirror wrapper),
`Debugger.svelte`, `Controls.svelte`, `PinPanel.svelte`, `VarWatcher.svelte`,
`Gallery.svelte` (parameterise its tile geometry from `stores/geometry.ts`),
`PatternThumb.svelte` (same — and actually pass `kind`), `Preview.svelte`
(add a 3D/`cloud` path everywhere, not just the map branch).

---

## Appendix A — `docs/lang.md`: what the language says about geometry and controls

This is the ground truth the UI is supposed to reflect, and it contradicts the
current UI in two places.

### Entry points — dimension is implicit, and never an error

- `lang.md:22-26` — `beforeRender(delta)` once per frame, then `render(index)`
  once per pixel.
- `:27-30` — "For mapped fixtures, export `render2D(index, x, y)` or
  `render3D(index, x, y, z)` instead — coordinates arrive normalized to 0..1
  from the **installed** pixel map. **The most specific exported renderer wins
  for the installed map; plain `render` is the 1D fallback.**"
- `:32-35`, restated `:781-784` — `renderFrame()` (a Luxel extension) paints the
  whole frame and **wins over all three per-pixel renderers, on any map or
  none**.
- `:785-787` — `export var renderFrame` can be **assigned at runtime** and
  dispatches from the next frame, so the set of live entry points is not
  statically knowable from source text.

**There is no dimension declaration** — no manifest field, no directive. A
pattern's dimensionality is implied entirely by which function it exports.

**A 2D pattern on a mapless device is not an error**, it silently acquires
geometry: `:462-464` — a 2D-only pattern "falls back to the engine's default √n
grid map"; `:827-832` — a `renderFrame`-only pattern naming any
coordinate/grid-space op (`gridWidth`, `gridHeight`, `fillRect`, `fillCircle`,
`splat`, `drawLine`, `fillCanvas`, `blit`) gets "the same default `ceil(√n)`
grid map a `render2D`-only pattern gets", while an index-space-only pattern is
"left mapless, so it never acquires a geometry it did not ask for". `:816-817` —
coordinate-space ops with no map at all use `render2D`'s 1D fallback (x from
index, y = 0.5). `:820-826` — the `ceil(√n)` default over-provisions (60 px →
8×8) and unused trailing cells clip; grid ops with no grid are **a no-op, not an
error**, with `gridWidth()` returning 0 so a pattern can branch.

*Implication for the rewrite:* the honest preview question is never "is this
pattern 1D or 2D" but "what will the engine do with this pattern **on this
device's installed map**" — exactly the reconciliation `stores/geometry.ts`
(§8.2) would own, and exactly what `Engine.preferredDims()` (`luxel.ts:339-348`)
already computes but only `deriveRig` consumes.

### Controls

`:207-214` — the prefix is stripped and the remainder becomes the label
(`sliderSpeed` → a slider labelled "Speed"). Full set:

| Prefix | Direction | Widget |
|---|---|---|
| `slider` | input | slider, 0..1 default (`:207-209`, `:1096`) |
| `toggle` | input | on/off (`:209`); indeterminate without `default=` (`:1127`) |
| `trigger` | input | momentary (`:209`) |
| `inputNumber` | input | numeric entry (`:209`, example `:1107-1108`) |
| `hsvPicker` | input | colour picker (`:209`) |
| `rgbPicker` | input | colour picker (`:209`) |
| `showNumber` | readout | pattern *returns* the value (`:210-211`) |
| `gauge` | readout | pattern *returns* the value (`:210-211`) |

Note there is **no `hue*` prefix** — the pickers are `hsvPicker`/`rgbPicker`.
This matches `ControlKind` in `luxel.ts:22-30` and the widget switch in
`Controls.svelte:80-151` exactly; the control layer is the one part of the UI
that is already faithful to the language.

`//#` hint directives (`:1087-1129`): keys are `min`, `max`, `step`, `default`,
all optional plain numbers, unknown keys ignored; without a directive a slider
is 0..1. Both placements (trailing on the export line, or own-line immediately
above it) mean the same thing and **merge, own-line winning per key**
(`:1111-1114`); a blank line or intervening comment breaks the association.
Bounds are **UI-only** — `default` is where the *widget* starts, not an
initializer for the pattern's variable (`:1116-1120`). `:1122-1129` documents
exactly the placeholder/`?`-badge behaviour `Controls.svelte:30-43` implements.
PB ignores the comment, so the source stays valid Pixelblaze (`:985-986`).
The parser is `lib/hints.ts:257-278`, with a plain-JS twin in
`tools/verify/hints.mjs` kept in sync by `web/tests/hints.test.mjs`.

### Maps are device-level, per the docs

- `:29-30` — coordinates come from the **installed** map; the renderer is chosen
  against the installed map.
- `:453-458` — blur/glow spread in 2D when "a 2D map installed … reads as a
  regular W×H matrix", and "the **device-level** blur/glow settings
  (`/api/output`) follow the same rule".
- `:445-451` — the device-vs-pattern model is spelled out explicitly for
  palettes ("There is a *device-level* palette too — Settings → Output,
  persisted in flash … The two compose"). **That is the exact mental model the
  map UI should copy and currently does not.**
- `:627-632` — patterns only *query* geometry (`pixelMapDimensions()`,
  `has2DMap()`, `has3DMap()`, `mapPixels()`); a pattern cannot define
  coordinates.
- `:460-465`, `:827-832` — the only map a pattern can cause is the implicit
  `ceil(√n)` default.

So the language already says what Jeremy says: the map belongs to the device.
The UI is the only layer that suggests otherwise (§2.4).
