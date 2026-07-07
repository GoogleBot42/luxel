# Update log

## 2026-07-06 night — control-picker layout, no device-URL field, no share on device ✅

Three UI fixes from your feedback:

- **Color-picker controls no longer run off-screen.** An `hsvPicker`/`rgbPicker`
  laid its three channels out in one horizontal row, so the 2nd/3rd sliders
  overflowed the narrow right rail and were unreachable — and the channels had
  no numeric field. They now **stack vertically**, each channel with a slider
  **and** an editable number box (like the scalar sliders). Verified: 0 rail
  overflow.
- **No more "device url" field.** The address is always known — a real device
  serves the UI from its own flash (auto-connect to same origin), and reconnect
  just reuses that. So disconnect → **"reconnect" button, no URL to type**. A
  hosted/standalone playground has *no* device support at all (that's what
  playground mode is for), so it shows no connect UI. (Dev/e2e point the built
  UI at a device or the mirror with `?device=<base>`.)
- **Share is gone in device mode.** Those links carry the pattern in the URL —
  on a device that's a LAN address that won't work for anyone else. Share is now
  shown only in the playground (keyed off the same "is this a playground?"
  state, so it's hidden whether you're connected or just served from a device).

The map sub-tab is likewise gated to playground mode (device map upload is a
later firmware item). device-e2e updated (auto-connect via `?device=`, asserts
no URL field / no share button in device mode, and reconnect-without-URL); both
suites green. Pushed to the dev device.

## 2026-07-06 night — clean device connect-on-load (no more waterfall garbage) ✅

Fixed the connect weirdness you flagged: on load the waterfall skipped and kept
stale/old data after the connection stabilized, because the async handshake
(status → source → controls → ws) leaked pre-stabilization frames.

- **Connection phase state** (`idle → connecting → live`). `connectDevice` enters
  **connecting** and holds the preview blank; a `markLive()` helper flips to
  **live** the moment the *real* stream delivers its first datum (ws pixels or
  status, or the HTTP-fallback poll) and clears the preview **once** at that
  transition. So leftover playground content, canvas-resize artifacts, and
  HTTP/WS cadence jitter can never end up in the waterfall.
- **Visible connecting state** — a "connecting…" pill in the header and a
  spinner overlay on the preview, so the async handshake reads as intentional
  instead of showing stale frames.

Verified in the device-e2e (against the native mirror): on connect, the
waterfall is held **blank (0 lit)** all the way through the handshake, then
streams cleanly — plus the existing 19 device checks and 55 playground checks
stay green. Pushed to the dev device.

## 2026-07-06 night — mapper is a debuggable Luxel program in its own editor tab ✅

Re-read the feedback and fixed the thing I'd under-heard: "the mapping function
should be a tab on the script editor **and debuggable as well**." Per your call,
the map is now a **Luxel program** (not JS), which makes it debuggable for free
by reusing the pattern VM + debugger.

- **New `plot(x, y[, z])` builtin** + engine "map mode": a map program exports
  `render(index)` and calls `plot()` once per pixel. A second engine runs it
  through the *same* per-pixel `drive` loop as patterns, but stores the plotted
  coordinate instead of a color. Collected coords install into the pattern
  engine as a 2D/3D map.
- **Editor sub-tab (pattern · map).** The map is edited in the same CodeMirror
  as patterns — luxel highlighting, completions, hover — not a bare textarea.
- **Debuggable exactly like a pattern.** Gutter breakpoints, step over/into/out,
  the call stack, locals, and globals all work on the map program because it's
  real VM code. (The debugger panel is now a shared `Debugger.svelte`.) A live
  scatter preview renders the pattern on the computed map.
- Spans all layers: luxel-core (`plot`, `enable_map_mode`/`run_map`/`map` +
  4 unit tests), luxel-wasm (`lx_run_map`/`lx_map_*`), luxel.ts
  (`compileMap`/`runMap`), App.svelte. Playground-only for now (installing a
  computed map on the device is a later firmware item).

Also, two small fixes you flagged: the page **`<title>` is now just "Luxel"**
(was "Luxel Playground"), and the **Patterns tab shows a loading state** — the
built-in examples render immediately and a spinner reads "loading patterns…"
while the corpus streams in, instead of a bare "0 patterns."

Verified in real chromium: 55 playground checks (incl. map runs, installs,
scatter renders, **breakpoint pauses the map run at pixel 0**, compile errors,
back-to-strip) + 19 device-mode checks, all green; 65 Rust tests pass. Pushed to
the dev device as a hot asset reload (the map tab is playground-only, so it's
hidden in device mode).

## 2026-07-06 night — web UI Phase 2: tabs + decluttered header ✅

Phase 2 of the web-UI redesign — a real tabbed app instead of one crowded
header. Web-only (no firmware / no reflash); pushed to the dev device as a hot
asset reload, so http://192.168.0.205/ is already running it.

- **Tab bar in both modes.** `Editor · Patterns` in playground; device mode adds
  `Settings`. Panels stay mounted and hide via CSS, so the render loop, editor
  state, and gallery tile-engines all survive a tab switch.
- **Header decluttered** to just wordmark · tabs · status (fps + streaming/
  polling) · connection. Everything else moved out:
  - *Editor toolbar* (above the editor): pattern picker + `save`/`delete`,
    `share` (playground only, prominent), and a `⋯` overflow with import/export.
  - *Playback bar* (below the editor): layout (strip/grid/px — playground only),
    fps, pause, debug.
- **Patterns tab** — the gallery, promoted from a modal overlay to a first-class
  inline tab. Lazy-mounted on first visit then kept alive; picking a pattern
  jumps back to the Editor tab.
- **Settings tab** (device mode) — device address, live pixel-count readout, and
  status; brightness / pixel-count editing / WiFi are honest **Phase-3
  placeholders** (labeled as needing firmware — only `/api/wifi` exists today).
- **Share** hidden in device mode (it's a playground affordance), prominent in
  playground mode — per the feedback.

Both e2e suites were rewired to the new `data-role` hooks (`pattern-picker`,
`tab-*`, `overflow`, `map-badge`, `layout-*`, `cfg-pixels`, `pause`, `fps`) and
are **green in real chromium** — 51 playground checks + 19 device-mode checks
against the native mirror. Verified the served bundle on hardware: index.html
(revalidated) points at the new immutable hashed JS/CSS.

Still ahead (docs/webui.md): Phase 3 (firmware `/api/brightness` + `/api/config`,
wire the Settings fields, fix the connect-on-load race), Phase 4 (mapper as a
CodeMirror tab + debuggable, 3D preview, Playlist tab + firmware storage,
MQTT/HA, AP-mode).

## 2026-07-06 evening — HTTP caching: assets only re-download when changed (v0.1.11) ✅

You asked the device to take advantage of browser caching — no redownload of
unchanged assets. Done and verified live on hardware:

- **Content-hashed bundle files** (`/assets/index-<hash>.js`/`.css`) now serve
  `Cache-Control: public, max-age=31536000, immutable` — the browser reuses
  them with **zero network** until their hash (and thus URL) changes.
- **The unhashed files** (`index.html`, `luxel.wasm`, `gallery.json`) serve a
  **strong ETag** + `Cache-Control: no-cache`, so the browser revalidates with
  `If-None-Match` and gets a **304 Not Modified** (empty body) when unchanged —
  a full download only when the content actually changed.

Implementation: the LUXA archive gained a v2 format (`LUX2`) carrying an
8-byte SHA-256 content hash per file (packed host-side); the firmware serves it
as the ETag and answers `If-None-Match` with a 304. The firmware still reads
legacy `LUXA` archives (those assets just revalidate to a 200). No new stack
cost (stack-check green at 12 KB; clippy clean).

Verified on the device (v0.1.11, OTA'd onto ota_0): every asset returns its own
correct ETag; a matching `If-None-Match` → `304` with no body; a stale one →
`200`; `/assets/*` carries `immutable`, the rest `no-cache`. So a second visit
to http://192.168.0.205/ re-fetches nothing unless a file changed.

## 2026-07-06 evening — web UI feedback sorted + Phase-1 fixes ✅

You gave a big batch of web-UI feedback. First, **sorted into the docs so
nothing is forgotten**: the full redesign backlog now lives in
[docs/webui.md](docs/webui.md) (two modes: device console vs. playground;
tabs; settings page; header declutter — each item tagged with effort and
firmware-dependency), cross-referenced from ideas.md. Notable finding: a real
settings page needs new firmware (brightness/pixel-count/config endpoints,
MQTT) — only `/api/wifi` exists today.

Then **Phase 1** (web-only, no reflash — both e2e suites green in chromium):

- **Not a "playground" on a device.** The wordmark now shows the device
  (name/URL) in device mode, "playground" only when standalone. A `data-mode`
  hook is in place for the Phase-2 restructure.
- **Reconnect remembers the device.** Disconnect → connect no longer needs the
  URL re-typed; the last successful base is remembered (and persisted to
  localStorage), reused automatically.
- **"ws push" → "streaming"** (and "polling · N ms" for the HTTP fallback) —
  the old jargon meant nothing to users.
- **Debugger no longer lies on a live device.** A gutter breakpoint used to arm
  debug mode even when connected (the button was disabled but the gutter path
  bypassed it); it's now fully gated off in device mode.
- **Share hidden in device mode** — it only makes sense for the hosted
  playground.
- **Pattern-browser spinners** while a tile's preview is still computing.

The heavier items (tab restructure + header overflow menu, firmware settings
endpoints, connect-on-load race, mapper-as-editor-tab, 3D preview, playlists)
are phased in docs/webui.md as Phases 2–4.

## 2026-07-06 late — chunked patterns: larger than one flash page (v0.1.10) ✅

You asked for chunking so patterns aren't capped at one 4 KB flash page.
Done and hardware-verified:

- A pattern's source now splits across up to 4 chunk items (~3.8 KB each,
  ~15 KB total — 4x the old limit, and the practical ceiling since the 16 KB
  HTTP buffer bounds a POST there anyway). Each pattern also has a small meta
  item (name + chunk count + generation).
- **Atomic updates via generation flip:** an update writes new chunks to the
  *other* generation, then rewrites the meta (the single-item commit point) —
  a power loss before that leaves the old version fully intact. A **RAM
  index** (built at boot) keeps list/lookup off the flash.
- **Format-version marker:** boot wipes `storage` if the on-flash layout
  isn't the current one, so the incompatible pre-chunking items auto-migrate
  (saw `format 0 != 2, wiping storage` on the upgrade boot).

**Two bugs caught in hardware testing, both fixed:**
- A GET of a >4 KB pattern OOM'd (`allocation of 21880 bytes failed`): the
  old `format!(json_escape(..))` path built an ~11 KB intermediate plus a
  doubling result buffer → a ~22 KB contiguous request that fragmentation
  couldn't satisfy. Now escapes into one pre-sized string; `read_source`
  pre-sizes too.

Verified on hardware: byte-exact round-trip of 2-, 3-, and 4-chunk patterns
(7.7 / 10.8 / 15.1 KB, md5-matched), upsert returns the latest, small +
large patterns coexist, persistence across reboot. Larger-than-15 KB
patterns stay in the browser library (source stays there regardless).

## 2026-07-06 afternoon — pattern library verified on hardware (v0.1.7) ✅

You serial-flashed v0.1.6 (new factory-less table). Boot was textbook:
`Erasing storage (Data(Spiffs))…`, table shows `storage @ 0x210000`,
`booted from: ota_0`, `patterns: 0 stored`, flash creds + assets both
survived. Then full pattern CRUD verified on the real device:

- list/save/get/activate/delete all round-trip through flash; activate
  drives the pixels; missing-delete → "no such pattern".
- **Found + fixed a bug in testing (v0.1.7):** sequential-storage's
  `fetch_all_items` returns superseded item versions after an upsert, so
  the list showed a duplicate id — now deduped by key (`fetch_item` already
  returns the latest for reads).
- **Persistence:** OTA'd both slot directions on the factory-less table
  (ota_0↔ota_1, clean) and confirmed saved patterns survive the reboot —
  serial `patterns: 2 stored`, NEXT_SEQ correctly reseeded from stored ids.

Left two curated patterns in the device library ("simplex aurora", "beat
pulse", both using the new builtins) with aurora running. **Task #9 done.**

## 2026-07-06 midday — firmware pattern storage (v0.1.6); dropped factory; stack guardrails

Working with you awake. Two design calls you made, both shipped:

**Dropped the factory partition → dedicated `storage` partition.** You have
no distinct golden image (serial flash = the same build that ships OTA), so
factory was 1 MB of dead weight. New table (partitions.csv): pure A/B
(ota_0/ota_1, bootloader falls back to ota_0 when both fail) + `storage`
(0x210000, 1 MB) slotted ahead of the unmoved assets region. `ota.rs`
already read the table dynamically, so no logic change. **Applying it needs
one serial flash** — your normal `build-esp32.sh flash` runner already does
`--partition-table=partitions.csv --erase-parts otadata`, so it lays down
the new table, clears otadata (clean boot into ota_0), and preserves
nvs/assets.

**Firmware pattern library — task #9, the CRUD contract's device half
(v0.1.6).** New `patterns.rs`, built on **`sequential-storage`** (your call
over my first hand-rolled blob — it's the PLAN's storage model and matches
your "established crates" preference). Each pattern is one KV item (key =
u32 id, value = name+source) in the `storage` partition: a store/remove is
atomic, so a power loss mid-save loses at most the one in-flight pattern,
never the library — and it's wear-leveled. Routes match the mirror exactly:
`GET/POST /api/patterns`, `GET/DELETE /api/patterns/<id>`,
`POST /api/patterns/<id>/activate` — POST compile-checks before storing.

- **Safety guard:** `patterns::init` resolves the `storage` partition from
  the *live* table and disables the store (writes refuse, reads empty) if
  it is absent — so v0.1.6 is safe even on the current (old) table, where
  0x210000 is the live ota_1 app slot. No corruption; it lights up after
  your reflash.
- **Async/blocking bridge:** sequential-storage is async, esp-storage is
  blocking — a small `AsyncFlash` adapter forwards to the blocking methods,
  driven by `block_on` (it never truly pends). The flash driver is *leased*
  out of the OTA module per transaction (Drop-guarded) so its critical-
  section mutex is never held across the erases.
- **One caveat (FYI):** sequential-storage items must fit one 4 KB flash
  page, so a single pattern's source is capped at ~3.5 KB (clear API error
  above that; larger patterns stay in the browser library). If you want
  unlimited device-side pattern size later, chunking across keys would lift
  it — say the word. A serial reflash clears device patterns (they survive
  OTA updates; `--erase-parts` now includes `storage` for a clean region).

**Stack guardrails (so the original OTA-crash class can't recur):**
- `#![deny(clippy::large_stack_arrays)]` in the firmware (threshold 1 KB,
  `firmware/clippy.toml`) — turns a stray `[0u8; 4096]` on the stack into a
  hard `cargo clippy` error. Caught exactly this while writing patterns.rs.
- `tools/stack-check.{sh,py}` — builds with `-Z emit-stack-sizes` and fails
  if any function's frame exceeds a budget. Unlike clippy it sees *library*
  frames too (esp-storage's `FlashStorage::read` bounce buffer — the actual
  original culprit). Added `python3` to the flake for it.

Requires one serial flash of v0.1.6 (new table). Then I'll verify pattern
CRUD, creds, assets, and an OTA round-trip on hardware.

## 2026-07-06 morning — device back on v0.1.5; full checklist verified on hardware ✅

You reflashed v0.1.5 with creds. Device came up on `factory`, joined WiFi
via **compile-time creds** (no flash record yet), 300 px, 123 fps, no
vmerr. Then I ran the promised morning checklist end-to-end — all four
steps passed on real hardware, zero panics:

1. **WiFi creds → flash** — `POST /api/wifi` stored them in the `nvs`
   partition and rebooted. It came back with
   `wifi: joining "MOMCorp Intranet" (flash-stored creds)`. **Your
   partition ask is done and live**: creds now boot from flash, so future
   OTA images need no baked-in creds. Compile-time creds remain only as a
   last-resort fallback (a flash-wipe can't lock you out).
2. **Assets push** — `POST /api/assets` streamed the 431,755-byte LUXA
   archive (5 files) into flash in ~10 s, hot-reloaded, no reboot. The
   full playground now serves gzip'd at http://192.168.0.205/.
3. **New builtins on hardware** — live-coded a `beatSin`+`simplex2`+
   `setGamma` pattern via `/api/code`; pixels animate, `vmerr:null`. (FPS
   eases to ~87–99 under per-pixel simplex across 300 px — expected.)
4. **OTA round-trip** — pushed the 926 KB app image; device switched
   `factory` → `ota_0`, rebooted, came back clean at 124 fps. This is the
   exact path that used to crash 100% of the time. Serial log: **no panic
   since the reflash** (last crash in the log predates tonight's session).

The beatSin/simplex/setGamma demo is left running on the wall. Remaining
open item: **firmware pattern-library storage** (task #9) — still a
genuine flash-layout decision; notes below.

## 2026-07-06 ~01:30 — second overnight batch: simplex, setGamma, **, mapper, pattern library

All committed, tested (106 core tests, 40 e2e checks in real chromium),
and already in the v0.1.5 image + packed assets waiting for the device:

- **`simplex2`/`simplex3`** (`f1b7abc`) — fixed-point simplex noise,
  smoother than perlin, seeded, deterministic.
- **`setGamma(g)`** (`f95d805`) — output gamma as a cached LUT (zero
  per-pixel cost); `setGamma(2.2)` makes LED fades perceptually even.
- **`**` exponent operator** (`16d6f68`) — right-assoc, tighter than `*`.
- **Mapper (M3)** (`4f157d3`) — write a PB-style JS map function, apply,
  and the playground installs a real 2D map into the engine (new
  `lx_set_map` wasm export) with a live scatter preview. The ring-map
  rainbow screenshot is worth opening: e2e-6-mapper.png.
- **Pattern library + autosave** (`11d43a9`) — edits survive closed tabs;
  named saves live in a picker optgroup. This is the prototype UI for
  device pattern CRUD.
- Corpus re-run with everything new: **288/293 ok, zero regressions**
  (user globals correctly shadow new builtin names — checked explicitly).

**Morning checklist (after your one serial flash of v0.1.5):** I'll
detect the device and, automatically: POST /api/wifi (creds → flash,
lockout-proof forever), push the ~432 KB assets archive (new playground +
gallery + mapper + library UI, /api/assets, no reboot), live-code a
beatSin/simplex/setGamma pattern to verify the new builtins on hardware,
and run one OTA round-trip. Nothing needed from you beyond the flash
command in the note below.

Also shipped since that list (all mirror/playground-verified, waiting on
the device only for the firmware storage half): **mapper**, **pattern
library + autosave**, **device pattern CRUD** (save/load/delete patterns
*on the device* — mirror + playground done, firmware storage queued as
the one remaining piece), and a real **CORS preflight fix** (cross-origin
DELETE from the hosted playground to a device by IP). Everything is in
git; `git log 94e8c52..HEAD` is the full night. Full workspace tests +
both browser e2e suites are green.

## 2026-07-06 ~11:30 — overnight progress: builtins batch 2, .epe UI, pattern browser

All hardware-independent, all committed, all verified locally (tests +
real chromium):

- **Builtins batch 2** (`6745a38`): `beat`/`beatSin` (tempo without
  audio), `hash`/`hash2` (stable per-pixel randomness, pinned lowbias32),
  `blur1D`/`feedback` (the trails/fire idioms as builtins),
  `dot`/`dot3`/`angleBetween`, and value-returning color —
  `hsv2rgb`/`rgb2hsv`/`mixColors(..., out)` with mixColors blending in
  OKLab. 101 core tests green. Also: array literals turned out to already
  work — ideas.md refreshed.
- **.epe import/export in the playground** (`463fa54`): import button +
  drag-drop anywhere + export download (PB-compatible shape). e2e covers
  a real chromium download round-trip.
- **Pattern browser** (`0d87f8f`): your gallery idea — 192 live tiles
  (built-in examples + every corpus pattern that compiles clean), 1D as
  bars / render2D as 16×16 rectangles exactly per your spec, viewport-
  lazy so it stays light. Click a tile to open it. It looks genuinely
  delightful — see e2e-5-gallery.png in the scratchpad shots.

Web bundle: 194 KB gz JS + 491 KB gallery.json (raw; ~150 KB gz over the
wire, lazy-fetched only when the browser opens — device flash region has
plenty of room). The new builtins + wasm + gallery need the next assets
push / firmware flash to reach the device; your morning serial flash
picks up ALL of it in one go.

## 2026-07-06 ~10:00 — ⚠ MY MISTAKE: device is offline on ota_0 (needs one serial touch when you're up)

Right after proving OTA works, I pushed a build of the new builtins from
MY shell — which doesn't have your `LUXEL_SSID`/`LUXEL_PASS` exported —
so the image baked **no WiFi creds** and booted into offline render-only
mode. The device is healthy (rainbow on ota_0, serial confirms "no wifi
credentials; offline mode") but unreachable over the network, so I can't
fix it remotely. This is the exact "hard lesson" from 2026-07-05 that I
had recorded and failed to apply. Sorry.

**Recovery (one step, when you're up):**
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```
That flashes the current build (**v0.1.5**) — which now includes the
batch-2 builtins, auto-baked creds (see below), AND flash-stored WiFi
credentials: the moment it's up I'll `POST /api/wifi` once and the creds
live in the nvs partition forever — after that even a credless image
joins the network, so tonight's failure mode is structurally dead. Alternative if you prefer not to flash:
`espflash erase-region 0xd000 0x2000` clears otadata → boots factory
(v0.1.4 with creds) and I'll OTA the rest myself.

**So it can't happen again (committed):**
- `firmware/creds.env` (git-ignored) now holds the dev creds;
  `build-esp32.sh` auto-sources it — every build bakes creds no matter
  whose shell runs it.
- `tools/ota-push.sh` now **refuses to push any image that doesn't
  contain the SSID string** (grep -a for it in the binary).
- The real cure stays queued: NVS-stored credentials so images never
  carry creds at all.

## 2026-07-06 ~09:40 — v0.1.4 flashed (thanks!) — OTA fully verified, remote work unblocked

After your serial flash, hardware-verified the fix end to end:
- The 190 KB JS + 87 KB wasm (the files that used to stall/crash) served
  repeatedly, cleanly, ~1.5 s / 0.8 s each. Zero panics.
- Full OTA round-trip: factory → ota_0 (904 KB, ok) → then a second OTA
  **while a loop hammered asset + status requests** → ota_1, ok. The
  worst-case combined load that used to be instant death now just works.
- Serial log: zero new panics since the flash.

Device: ota_1, v0.1.4, ~120 fps, heap_free ~72 KB. The A/B loop is
proven again and I can push everything else tonight over OTA. Carrying on
with M3 + the ideas backlog; log entries below as they land.

## 2026-07-06 ~09:00 — OTA crash ROOT CAUSE found + fixed (v0.1.4) — this supersedes the 08:15 note

You were right that something was fundamentally wrong. The serial log had
24 panics and every single one is the same signature: **stack overflow on
the main task** ("write to the stack guard value on ProCpu"), caught during
a flash read in the HTTP serving path.

Three compounding causes:

1. **The main task stack was 15.6 KB, by accident.** esp-hal gives the
   main task "whatever RWDATA is left after .data/.bss" — and our 120 KB
   heap static ate almost all of it. That one stack runs the entire
   embassy executor (every task's poll), picoserve's deep response path,
   AND the WiFi level-6 NMI frames, which land on whatever stack is
   current. Measured in the ELF: 15,596 bytes. The captured panic's SP was
   already 1.7 KB *past* the stack end.
2. **esp-storage's `FlashStorage::read` puts a 4 KiB sector bounce-buffer
   on the caller's stack** — every asset chunk we served pushed 4 KB onto
   that already-deep stack at maximum depth. (This is the "library
   function" trap: the convenient `Storage::read` API is the wrong one;
   `read_nor` with aligned offset/len/buffer reads directly, zero stack.)
3. WiFi NMI on top of 1+2 → guard hit → panic → reboot. It looked like
   "OTA crashes" because OTA sessions are exactly when the page/status/
   asset traffic and flash ops coincide; the erase-on-write fix (v0.1.2)
   was real but treated a different, secondary hazard.

The v0.1.4 fix (firmware only, two small changes): heap 120→96 KB so the
main stack is now **60 KB** (measured in the ELF: 61,712 bytes; heap_free
still ~70 KB), and `assets::read_chunk` now uses `read_nor` through a
word-aligned heap staging buffer — no more stack bounce-buffer in the
serving path.

On your "reinventing the wheel" hunch: partially right, wrong culprit.
ota.rs does hand-roll erase/write at raw offsets where
esp-bootloader-esp-idf's `OtaUpdater::next_partition()` hands you a
bounds-checked `FlashRegion` — worth cleaning up later — but that code
wasn't the crash; the reads/stack were. Queued the cleanup.

**I attempted OTA of v0.1.4 myself** (fails safe: a crash mid-upload just
reboots to the current slot). Check /api/status — if it says 0.1.4, go to
bed, nothing needed. If it still says 0.1.3, one serial flash:
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```

## 2026-07-06 ~08:15 — ⚡ PLEASE FLASH THIS before bed (v0.1.2, task-16 fix)

You asked what I need flashed to unblock M3 remote work. This is it:
**`67f2111` fixes the OTA-vs-flash hazard** (erase-on-write interleaved
with network reads instead of a watchdog-tripping pre-erase burst; plus
a Timer-yielded asset-serving path). Once this is on the device, I can
OTA everything else remotely — settings, AP provisioning refinements,
all the ideas.md work — without needing you.

Flash command (creds already baked, so it just connects):
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```
(the runner erases otadata → boots the flashed image; leave the monitor
running as usual). It's v0.1.2 — `/api/status` will show that once up.

Fails safe as always: if the erase-on-write change has any issue, a bad
OTA just reboots to the current slot; the flashed build itself connects
and live-codes regardless (compile-time creds unchanged). **AP-mode
provisioning still needs your phone to verify** (a device with no creds
leaves WiFi, so I can't reach it) — I'll build it and OTA it, but final
sign-off is yours when you're up.

Correction to my earlier note: the build you flashed a few messages ago
was HEAD *without* a task-16 fix (I'd described it but hadn't written it
yet). THIS commit is the actual fix.

Also shipped tonight while you read this: OKLCH/OKLab perceptual color
builtins (top visual-quality item from ideas.md).

## 2026-07-06 ~07:45 — extension builtins + idea backlog

Acting on your "extend beyond PB" note: wrote `docs/ideas.md` (a ranked
backlog across builtins / language / engine / audio / integration /
playground, with your animated-pattern-browser idea and its
1D-bar-vs-2D-rectangle preview distinction), and shipped the first
cheap-but-high-value batch of builtins: `map()`, `sign`, `step`,
`saturate`, `dist`/`dist3`, and the six `easeIn/Out/InOut` `Quad`/`Cubic`
curves. All pure math, so no PB pattern is affected (new names PB
doesn't have). They're in the VM, autocomplete, and language docs, with
tests. My top-5 picks are in ideas.md — `map`+easing+oklch color helpers
and array literals are the highest authoring-quality-per-effort; I2S
mic/FFT and MQTT/HA are the headline features when we want them.

## 2026-07-06 ~07:15 — const/let landed; caught a real playground bug

`const` and `let` now work (your request). `let` == `var` for now
(function-scoped; block scoping later); `const` needs an initializer and
errors on reassignment, correctly scoped (a local const doesn't lock a
same-named global). Full stack: lexer → AST → parser → compiler, 3 new
tests, editor keyword completion, and docs/lang.md updated with a
"Luxel extensions" section. All 32 semantics tests green.

Writing the e2e for it surfaced a genuine bug I'd introduced with the
device-served playground: the auto-connect probe fetched `/api/status`,
and a dev server answers that with its SPA fallback (200 HTML), so the
local playground tried to "connect to a device", failed, and left a
stuck "device unreachable" banner. Fixed to require real device JSON.
Good catch — it would've bitten the normal local playground, not just
the test.

Queued your other note (extend builtins/features/language — the "not
limited to PB" thesis) as a design task for a docs/ideas.md brainstorm.

The device is back (you re-flashed it) and healthy; I've deliberately
kept my remote testing light after the earlier WiFi-wedge, and the
flash-vs-WiFi fix (task 16) is the thing that unlocks safe remote
iteration again.

## 2026-07-06 ~06:30 — full playground serves from flash (works!) + an OTA regression to fix, and I've STOPPED touching the device

Big win and an honest problem, both real.

**Works, browser-verified against the real PB:** the whole Svelte
playground now lives in the device's flash and serves from it. New
pieces: an "assets" flash partition (0x310000, 960 KB) behind the OTA
slots; a `LUXA` archive format (`web/tools/pack-assets.mjs` packs the
gzipped `dist/` — 279 KB, 4 files); `POST /api/assets` streams it in
(independent of firmware OTA, hot-reloads the TOC, no reboot); the
server serves `/`, `/luxel.wasm`, etc. from flash with
`Content-Encoding: gzip`, embedded minimal page demoted to `/min`; and
the playground auto-enters device mode when same-origin `/api/status`
answers. Chromium loaded `http://192.168.0.205/` end to end: editor up,
device badge, 300-px preview, ~100 fps. Bumped the connection pool to 3
(safe now the stack overflow is fixed) so the preview socket doesn't
starve page/asset loads.

**Two real bugs, same root cause — the ESP32 flash-vs-WiFi hazard:**
1. Serving a *large* asset (JS 190 KB, wasm 87 KB — multi-chunk) stalls:
   interleaving esp-storage flash reads with WiFi TCP writes starves the
   executor, so the second `write_all` never drains. Single-chunk files
   (html, css) serve fine. Tried 8 KiB chunks + `yield_now` between
   writes — not enough.
2. **OTA now trips a hardware watchdog** during the erase phase when
   assets are installed: the device resets (no panic — a watchdog, not a
   Rust panic) ~14 s in, connection reset. It **fails safe every time** —
   always reboots back into the working build — but it means I can no
   longer push updates remotely, and I can't fix a running firmware's OTA
   path *via* OTA (chicken/egg).

**Decision: I stopped experimenting on the device.** It's in a stable,
fully-working state (serves the page, WS, live-code, small assets — all
good). Continuing to hammer it risked leaving it wedged for you, and I
was starting to see transient wedges under my own back-to-back tests.
The right fix for BOTH bugs is the standard one: **memory-map the assets
flash region** and read via the cached data bus instead of esp-storage
flash-controller ops — no cache-off windows, no WiFi starvation. That's
a clean next-session task (needs a serial flash to land, since it
changes the running OTA path). The source for everything above is
committed and sound; only the runtime flash-timing needs that rework.

**To get back to a fully-updatable device:** one serial flash of a build
with the mmap fix. Until then the PB happily runs what's on it.

Everything else tonight (debugger fixes, autocomplete, bidirectional WS,
language docs) is landed, hardware-verified where relevant, and
independent of this.

## 2026-07-06 ~05:15 — WS verdict + bidirectional multiplexing live on hardware (8daf311)

After your last flash the fixed build held: five status hammers, zero
panics, heap 121 KB free. Then the A/B you asked for, on the real PB:

|            | HTTP polling | WS push |
|------------|--------------|---------|
| rate       | 10.0 fps     | 9.7 fps |
| gap p50    | 87 ms        | 96 ms   |
| gap p90    | 143 ms       | 130 ms  |
| gap p99    | **394 ms**   | **185 ms** |

Same average rate, but the tail — the visible stutter — is halved.
Combined with freeing a connection slot, WS stays.

Your bidirectional suggestion then proved *necessary*, not optional:
with the push socket pinning one of the chip's two connections, extra
HTTP callers starved (reproduced: "fetch failed" under mixed load). Now
one socket carries everything — pixel push down, API calls up
(`"<id> call\nbody"` → `{"id":…,"r":…}`), playground multiplexes
transparently with HTTP fallback. The native mirror was rewritten from
tiny_http to a hand-rolled std HTTP layer so its /ws loop is
structurally identical to the firmware's (single-threaded full-duplex).

Hardware-verified end-to-end: 6 live-code pushes + control sets +
pattern fetch over one socket while pixels streamed and a concurrent
plain-HTTP request succeeded; zero panics. 12/12 device e2e, 25/25
local e2e, all tests green. Serial flashing also hardened along the
way: `flash` now erases otadata (the image you flash is the image that
boots — the mismatch that caused the 4 AM round).

## 2026-07-06 ~02:20 — INCIDENT: device down after WS OTA (my fault); recovery staged

The WS-push build OTA'd fine but the device never came back. Doing the
arithmetic I should have done first: bumping the server pool to 3 tasks ×
~32 KB buffers on top of WiFi's ~90 KB almost certainly exhausted the
ESP32's 184 KB heap at boot → allocation panic → and esp-backtrace's
default behavior is to HALT, so one panic = a brick until someone touches
power. You were asleep, serial detached; nothing I can do remotely — the
one failure mode OTA can't recover, found the hard way.

**When you're up: power-cycle the PB once.** A persistent watcher pushes
the staged recovery build (pool back to 2, smaller buffers, and — the
real fix — panics now reboot after 3 s instead of halting forever)
automatically the moment the device answers. If a power cycle alone
doesn't bring it back (crash loop), it'll still recover: each loop
iteration reboots through a WiFi window. Absolute worst case:
`espflash erase-region 0xd000 0x2000` over serial clears otadata → boots
factory.

Meanwhile: docs/lang.md (full language reference) written; continuing
with hardware-independent work (bidirectional WS protocol + remote
debugger against the native mirror, const/let).

## 2026-07-06 ~01:45 — WS pixel push implemented, hardware A/B in progress

Implemented across all four surfaces (firmware /ws, native mirror,
playground, minimal page): binary frames = pixels ~15 Hz, text frames =
typed JSON (status+controls, vars/readouts). 12/12 device e2e green.

Per your note, measuring on the real PB before keeping it. Baseline
captured first, HTTP polling against the previous firmware:
**10.0 fps, gap p50 87 ms / p90 143 ms / p99 394 ms** — that jitter is
the choppiness you saw. WS build OTA'd (861 KB, no size tricks needed
now); push measurement running.

Your bidirectional-WS point is sound and the groundwork exists:
picoserve's next_frame takes a cancel signal that safely unblocks
between frames, so one socket can carry API calls + push concurrently.
If the numbers favor WS, I'll multiplex code/control/var (and later the
remote-debug protocol) onto the same connection.

## 2026-07-06 ~01:00 — editor autocomplete (140a8c6)

Typing pops completions for all 86 builtins with full signatures and
one-line docs (new `web/src/lib/builtins.ts`, kept in sync with the VM
table — it'll also seed the language reference). Predefined constants
(PI, pixelCount, GPIO modes…), keywords, and identifiers you've already
used in the pattern complete too. Verified in chromium; both e2e suites
still green.

Also queued your `const`/`let` language-extensions request as a task —
it needs compiler work (block scoping, assignment-to-const errors), so
it's sequenced after the device-facing items tonight.

## 2026-07-06 ~00:30 — debugger notes resolved (060884b)

**Your `heat` question: the debugger is right.** In Palette Fire 2D, `heat`
is assigned inside `render2D` *without* `var` — in PB's JS-derived scoping
(and ours), undeclared assignment creates a global. `var` inside a function
would make it local. Pinned with a test so it stays true.

**Breakpoint line mismatch: real bug, fixed.** Resolution only matched
exact lines, so a breakpoint on a blank/comment/brace line silently
installed *nothing* (and the dot pointed at a line that could never stop).
Now it snaps forward to the nearest executable line and the gutter dot
moves there — dot and stop-line always agree. While hunting this I
verified the rest of the chain is correct: pause happens *before* the
stopped line executes (so its variables show pre-execution values),
stepping tracks across beforeRender→render→pixels, and the paused line
scrolls into view.

Running log of autonomous work, newest first. Started the night of
2026-07-05 when Jeremy went to sleep ("keep going with less intervention").

## 2026-07-06 — overnight session begins

Queue for tonight, from Jeremy's list + existing plan, in intended order:

1. Debugger correctness pass: `heat`-as-global question + stopped-line
   highlight (his two debugger notes).
2. Editor autocomplete for builtins.
3. WebSocket pixel push (device + native mirror + playground + minimal page),
   then OTA to the PB and measure against HTTP polling.
4. NVS-stored WiFi credentials (kills the creds-baked-image lockout risk).
5. Pattern language docs.
6. Remote debugger design (+ implementation as far as the night allows).

Device state at session start: PB v3 at 192.168.0.205, slot ota_1, v0.1.1,
124 fps, rainbow restored after the live-code green test. OTA loop fully
proven (factory → ota_0 → ota_1 + bootloader-fallback test passed).
