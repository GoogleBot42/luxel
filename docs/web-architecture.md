# Web app architecture (`web/src`)

How the playground / device console is put together. Written for the web UI v2
work (Gitea epic #461); `docs/webui.md` is the historical record of how the
pre-v2 UI got there and is not a spec.

Terminology (`.claude/rules/web.md`): the hardware-bound UI is the **device
console**, the hardware-free one the **playground**. Same bundle, same
components — `stores/device.ts` decides which one you are looking at.

## Layout

```
web/src/
  App.svelte        the shell: mode, tab, editing, boot cover, header, blocked banner
  main.ts           mounts App
  app.css           global tokens + element resets (dark only)
  stores/           plain-TS Svelte stores — all app state lives here
    device.ts       session lifecycle, hardware facts, settings state, ONE poll scheduler
    geometry.ts     THE Layout reconciler — every preview, tile and thumbnail reads it
    pattern.ts      the pattern document, the wasm host, local library, .epe + share codecs
    notify.ts       transient notes + the banner list
    dialog.ts       the modal primitive: promise-returning confirm/promptText
  pages/            one component per surface
    Library.svelte        Patterns Library and PixelBlaze Library (two variants, one file)
    DevicePatterns.svelte the device's stored library
    Playlist.svelte       transport, defaults, rows
    Settings.svelte       the card list + the visible-tab refresh
    Editor.svelte         toolbar, code pane, playback bar, right-rail inspector
    MapEditor.svelte      the map program: its engine, debugger and code pane
  settings/         one card per concern, each owning its form and its endpoint
    DeviceCard, NetworkInputCard, BrightnessCard, WifiCard,
    OutputCard, ClockCard, SyncCard, MqttCard, cards.css
  components/       reusable widgets (CodeMirror wrapper, Preview, Controls,
                    PinPanel, VarWatcher, Debugger, Gallery, PatternThumb,
                    PlaylistRow, Dialog)
  lib/              non-UI logic: device HTTP client, fetchgate, LNA classifier,
                    wasm bindings, control hints, playlist transport, audio, builtins
  flash/            a SECOND rollup entry (flash.html) — the WLED takeover installer;
                    it shares only app.css and lib/lna.ts
```

The dependency direction between stores is one-way and enforced by hand:

```
device.ts  ←  geometry.ts  ←  pattern.ts        (notify.ts depends on nothing)
```

`device.ts` therefore never writes the pattern document. `connectDevice()`
*returns* the running pattern it pulled and the caller (`pages/Editor.svelte`)
installs it. Keep it that way: `geometry.ts` reads wire state out of
`device.ts` and nothing reads back.

## The shell

`App.svelte` owns exactly four things: `mode` (playground vs device),
`tab` (which home surface is open), `editing` (the full-screen editor sits over
the home tab — it is not a tab), and the boot cover. It also renders the header,
the fps readout and the LNA blocked banner, and it wires page events to
`Editor` methods (`newPattern`, `loadSaved`, `loadGalleryPick`,
`openDevicePattern`, `importEpeFile`, `bootDevice`, `bootPlayground`).

Every page stays **mounted and `hidden`** when it is not the active tab, so its
state (compiled gallery tiles, CodeMirror documents, scroll position) survives
tab switching. Each page takes an `active` prop: it drives `hidden` and gates
that page's poll subscription and lazy mounts.

## The poll scheduler (`stores/device.ts`)

One `setInterval` for the whole app, ticking at 500 ms. It replaces three
ad-hoc reactive blocks that each cleared and re-created their own interval with
their gating conditions duplicated inline.

```ts
const stop = pollSubscribe(id, everyMs, fn);  // re-registering the same id replaces it
stop();                                       // unsubscribe
```

Contract:

- A subscriber's `fn` runs when at least `everyMs` has elapsed since its last
  run. Cadences need not divide the tick; they are lower bounds.
- The ticker runs **only while a device session is live**. It starts when the
  first subscriber registers with `device !== null` and stops when the last one
  leaves or the session drops, so a playground tab has no timers at all.
- `pollStopAll()` tears everything down (app teardown).

Registered cadences today:

| id | cadence | who subscribes | what it refreshes |
|---|---|---|---|
| `status` | 1 Hz | `startSessionPoll()`, once from the shell | `/api/status` → fps readout, free heap, vmerr, per-board pixel cap |
| `playlist` | 1 Hz | `pages/Playlist.svelte` while its tab is open | `/api/playlist`, reconciled against the optimistic transport intent (#431) |
| `settings` | 0.5 Hz | `pages/Settings.svelte` while its tab is open | `/api/status` live field, `/api/mqtt`, `/api/sync`, `/api/clock` |

`/api/output` is deliberately **not** polled: it is a form, and re-reading it
under the user's fingers would fight their edits. It is read once when the
Settings tab becomes visible, and re-read after every write.

Do not add a bare `setInterval` to a component. Subscribe here instead — the
device serves from a tiny connection pool and every extra poll competes with
the UI's own fetches (docs/tools.md, `panel-load-bench`).

## The notify primitive (`stores/notify.ts`)

Nine independent `*Note` strings, each with its own `setTimeout`, became one
thing:

```ts
note("save", "saved to device", 3000);  // ttl 0 = sticky until something replaces it
clearNote("mic");
$notes.save                              // read in markup
```

Channels: `save`, `share`, `map`, `mic`, `wifi`, `mqtt`, `datapin`, `ap`,
`palette`. A channel is a *surface*, not a message — the component that renders
`$notes.<channel>` owns where it appears and what `data-role` it carries.

## The dialog primitive (`stores/dialog.ts`)

Naming and every confirmation used to be `window.prompt` / `window.confirm` at
eight call sites (Gitea #472): unstyleable, wrong on a phone, and drivable
from a test only through a puppeteer `page.on("dialog")` handler that matched
on the message *text*. They are now one store plus one renderer.

```ts
if (!(await confirm({ title: "Delete pattern from the device?",
                      body: '"Aurora" is removed…', confirmLabel: "Delete",
                      danger: true }))) return;

const name = await promptText({ title: "Save pattern", label: "Name",
                                initial: suggestion, confirmLabel: "Save" });
if (name === null) return;          // cancelled; "" is never returned
```

- `confirm()` resolves **false** on Cancel, Escape or a backdrop click.
- `promptText()` resolves **null** the same way, else the trimmed text. Its
  `validate` (default: non-empty) runs on submit and keeps the dialog open
  with the reason shown — nothing is ever disabled (proposal §5.7).
- `reboot: true` renders the standing "the device reboots to apply this" line
  (proposal §5.3: reboot-requiring actions are labelled as such). WiFi save,
  setup-AP and the strip data pin use it.
- `danger: true` makes the primary button destructive-red. Both delete sites,
  playlist clear and the installer's wrong-image guard use it.
- Requests never stack: a second `confirm`/`promptText` while one is on screen
  resolves as cancelled rather than replacing what the user is reading.

`components/Dialog.svelte` is the only renderer — Escape cancels, Enter
confirms, Tab is trapped inside the panel, focus returns to whatever had it,
and the buttons stack full-width under 420 px (D9). Exactly one instance is
mounted per app entry: the shell, and `flash/Flash.svelte` for the installer.

Its `data-role` contract (the e2e hooks): `dialog` on the panel,
`dialog-backdrop`, `dialog-title`, `dialog-body`, `dialog-input`,
`dialog-reboot`, `dialog-error`, `dialog-confirm`, `dialog-cancel`. Harnesses
drive it through `acceptDialog`/`cancelDialog` in `web/tools/e2e-common.mjs`
and must never install a `page.on("dialog")` handler.

A7 (#468) moves naming into an inline-editable editor header; when it does,
only the `promptText` call in `saveToLibrary()` goes away — the save path is
already independent of where the name came from.

`banners` is the longer-lived list for conditions rather than events
(`setBanner(id, {level, text, role} | null)`, keyed upsert, insertion-ordered).
The editor's compile/runtime/capacity banners are still derived state with
bespoke markup and stay where they are.

## Geometry — the one Layout (`stores/geometry.ts`, Gitea #463)

There is exactly ONE geometry object in the UI, and every preview, gallery
tile, row thumbnail and playlist row renders through it. It is *reconciled*,
never set:

```
device Layout (console)      stores/device.ts `deviceLayout`   ─┐
"Preview as" choice          `previewAs` (persisted)            ├─→  layout
the compiled pattern's dims  `patternDims` (preferredDims())    │
projection defaults          `projection` (the device's)       ─┘
```

`reconcileLayout()` and everything derived from it are **pure** and live in
`lib/geometry.ts`, tested in `web/tests/geometry.test.mjs` (one case per cell
of the console-shape × pattern-dims × choice table, plus a parity check of the
projection tables against the engine's own through the built wasm). The store
is only the wiring.

```ts
interface Layout {
  dims: 1 | 2 | 3;          // strip · matrix or 2D map · lattice or 3D map
  regular: boolean;         // addressable as w×h(×d); false = coordinate cloud
  source: "device" | "user" | "pattern" | "default";
  w: number; h: number; d: number;
  pixels: number;           // what an engine is compiled at
  coords?: number[][];      // positions, when the renderer needs them
  serpentine?: boolean;     // the device's real wiring; undefined = row-major
  projection: Projection;   // proj1d/proj2d/proj3d (docs/spec/projection.md)
}
```

Rules the reconciler encodes:

- **Console**: the device owns the geometry (`/api/status`'s `geom`, #464 —
  and `/api/layout` when #465 lands: `deviceLayout` in `stores/device.ts` is
  the ONE adapter to swap, nothing downstream changes). A "Preview as" choice
  there only re-shapes; the pixel count stays hardware truth.
- **Playground**: `Auto` (the default, D7) follows the compiled pattern
  3D › 2D › 1D; anything else is the user's and outlives pattern loads.
- `devicePixels` / `deviceMap` are **raw wire state** in `stores/device.ts`.
  Only the adapter reads them; no surface may treat them as geometry.

Derived helpers every consumer uses instead of re-deriving anything:

| helper | what it gives |
|---|---|
| `tileShape(l)` | `bar` · `grid` · `cloud` · `scatter` — the shape to draw |
| `layoutLabel(l)` | `64×64 matrix`, `300 px strip` — the header chip |
| `effectiveFor(dims, l)` | what the pattern sees (pixelCount, w/h, projection) |
| `captionFor(dims, l)` | `1D · along x`, or null when native |
| `thumbLayout(l, n)` | the same shape at tile/thumbnail size |
| `layoutKey(l)` | cheap identity: changed ⇒ rebuild your engines |
| `configureEngine(e, l)` | the ONE place an engine is given a map + projection |
| `compileForLayout(lx, src, max)` | compile a pattern onto the Layout it will be shown on |

**The invariant: every consumer reads geometry from here.** A component must
not compile an engine at a pixel count of its own, install a map of its own,
or decide a shape from a pattern's source text. Gallery tiles take their
dimensionality from the ENGINE (`preferredDims()`); `gen-gallery.mjs`'s `kind`
is an advisory hint that saves a second compile and is allowed to be wrong.

Painting lives in `lib/draw.ts` (`paintBar` / `paintGrid` / `paintPoints`), so
the editor preview, the gallery tiles and the row thumbnails cannot drift.

What is NOT here yet: the device's real wiring. `serpentine` is wired through
`wiringCoords()` (and unit-tested) but nothing sets it until `/api/layout`
(#465) reports it — until then the console previews row-major, like the
playground. Per-item projection overrides are #470/#473's.

## Store reference

`stores/device.ts` — `device`, `deviceBase`, `isPlayground`, `mode`,
`deviceError`, `deviceBlocked`, `devicePixels`, `pixelMax`, `deviceHeapFree`,
`deviceEngineHeap`, `deviceVmerr`, `deviceFps`, `deviceOutFps`,
`deviceRescanHz`, `deviceMap`, `devicePatterns`, `brightness`,
`brightnessMax`, `deviceProtocol`, `protocolOptions`, `dataPin*`, `wifi*`,
`mqtt*`, `outputStatus`, `palette*`, `clockStatus`, `syncStatus`, `netLive`,
`playlist`; functions `connectDevice`, `detectDeviceBase`, `refresh*`,
`queuePlaylistSave`, `markTransport`, `installDeviceMapCoords`,
`installDeviceGridMap`, `clearDeviceMap`, `pollSubscribe`, `pollStopAll`,
`startSessionPoll`.

`stores/geometry.ts` — `layout`, `layoutFor()`, `layoutSignature`,
`layoutName`, `shape`, `pixelTotal`, `pixelCount()`, `previewAs`,
`setPreviewAs()`, `patternDims`, `mapCoords`, `setMapCoords()`, `projection`,
`configureEngine()`, `compileForLayout()`, `captionFor()`, `effectiveFor()`,
`tileShape()`, `thumbLayout()`, `projectionCaption()`, `layoutLabel()`,
`layoutKey()`, `TILE_MAX_CELLS`, `THUMB_MAX_CELLS`. See **Geometry** below.

`stores/pattern.ts` — `luxel`, `loadLuxel()`, `source`, `dirty`,
`patternName`, `exampleName`, `devicePatternId`, `controlValues`, `hints`,
`mapSrc`, `NEW_PATTERN`, `newPatternSource()`, `previewFps`, `runtimeError`, `saved`,
`saveToLocalLibrary`, `deleteFromLocalLibrary`, `findSaved`, `startAutosave`,
`stopAutosave`, `loadWorkingCopy`, `compileToBytecode`, `parseEpe`,
`exportEpe`, `encodeShare`, `decodeShare`.

`stores/notify.ts` — `notes`, `note()`, `clearNote()`, `noteStore()`,
`banners`, `setBanner()`, `clearBanners()`.

## Invariants worth not breaking

- **The preview is local; the device is a sink.** The editor compiles with the
  local wasm engine and pushes source + LXBC to `/api/code`. There is no pixel
  stream back (removed at Jeremy's request); only the connect handshake reads
  the device's running pattern.
- **`data-role` attributes are the e2e contract.** `web/tools/{e2e,device-e2e,
  maxpixels-e2e,sync-e2e,flash-e2e}.mjs` drive them. Moving markup between
  components is fine; renaming or dropping a role is not, without updating the
  harness in the same commit.
- **Bundle shape is load-bearing** (Gitea #92): `dist/index.html` must emit one
  `<script>`, one stylesheet and no `modulepreload`. `cssCodeSplit: false` is
  what lets a page component or a plain `.css` import (e.g.
  `settings/cards.css`) land in that single stylesheet.
- **`$:` only tracks what appears in its own syntax.** Name every dependency in
  the block, not just inside the function it calls. And a `$:` whose input is
  assigned *inside a function another reactive block calls* can render one
  cycle stale — assign the derived values in that function too
  (`components/PatternThumb.svelte`'s `adopt()`), or it will draw the previous
  Layout's shape while holding the new Layout.
- **Geometry comes from `stores/geometry.ts`.** One `layout`, reconciled from
  the device / the "Preview as" choice / the compiled pattern's dims. No
  component compiles at a pixel count of its own or installs a map of its own —
  see the Geometry section above.
- **No native dialogs.** `window.prompt` / `window.confirm` / `alert` do not
  appear anywhere under `web/src` — naming and confirmation go through
  `stores/dialog.ts` (#472). A native dialog also hangs the e2e harnesses,
  which deliberately install no `page.on("dialog")` handler.
