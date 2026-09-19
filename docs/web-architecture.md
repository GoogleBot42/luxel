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
    Patterns.svelte       ONE pattern browser: the source control + tile verbs
    Playlist.svelte       transport, defaults, rows
    Settings.svelte       the ranked sections + the Advanced list
    Editor.svelte         document header, code pane, right-rail inspector
    MapEditor.svelte      the map program's OWN SCREEN: code, scatter, debugger
  settings/         one card per concern, each owning its form and its endpoint
    Section, Disclosure               the page's two chrome primitives
    DeviceCard, LayoutCard, WifiCard  the three sections above the fold
    ArrangementSvg, OutputsTable,     LED layout's pictures and sub-forms
      ProjectionBlock, ProjectionCard
    OutputCard, PanelDriverCard, ClockCard, SyncCard, MqttCard,
      NetworkInputCard, StorageCard, FirmwareCard   the Advanced bodies
    cards.css
  components/       reusable widgets (CodeMirror wrapper, Preview, Controls,
                    PinPanel, VarWatcher, Debugger, Gallery, PatternThumb,
                    PlaylistRow, PatternPicker, ProjectionRow, Dialog,
                    PreviewAsChip) plus
                    editor-frame.css, the chrome BOTH full-screen editors wear
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

`App.svelte` owns exactly five things: `mode` (playground vs device),
`tab` (which home surface is open), `editing` (the full-screen editor sits over
the home tab — it is not a tab), `mapEditing` (the map program's screen, over
everything, A10/#471), and the boot cover. It also renders the header,
the fps readout and the LNA blocked banner, and it wires page events to
`Editor` methods (`newPattern`, `loadSaved`, `loadGalleryPick`,
`openDevicePattern`, `importEpeFile`, `bootDevice`, `bootPlayground`).

The tab set is `Patterns · Playlist · Settings` on a console and `Patterns`
alone in the playground (proposal §4), built from one `tabs` array so Scenes
(Phase B, Gitea #480) is one more entry, not another `{#if}`.

The shell header does **not** carry the open document. Since A7 (#468) the
editor renders its own header — back, name, save state, Save, ⋯ — and the
shell only passes `backLabel` down and takes a `back` event up. What stays in
the shell header is what is true of the *session*: the device chip / "Preview
as" chip and the fps readout (proposal §5.7).

`openMapEditor(from)` is the shell's one route to the map program's screen
(#471) — an exported component method, because every entry point is elsewhere:
the playground's "Preview as" chip (`openmap` event), the console's
Settings → LED layout link, and the pattern editor's "Map program ›". A8 (#469)
calls it from the real LED-layout card. `from` is only what the screen's back
button says; closing it just clears `mapEditing`, and whatever was underneath
is still mounted.

Every page stays **mounted and `hidden`** when it is not the active tab, so its
state (compiled gallery tiles, CodeMirror documents, scroll position) survives
tab switching. Each page takes an `active` prop: it drives `hidden` and gates
that page's poll subscription and lazy mounts.

## The Patterns page (`pages/Patterns.svelte`, Gitea #467)

One browser for every source of patterns (D3), replacing the three tabs —
Patterns Library, PixelBlaze Library, Device Patterns — that looked like three
apps over the same thing. A segmented source control picks the source:

| mode | sources |
|---|---|
| console | `On device (N)` · `Library (N)` · `PixelBlaze Library (N)` when the corpus JSON is present (dev-only) |
| playground | `Library (N)` · `Mine (N)` (this browser's saved patterns) · same corpus source |

There is ONE search box and ONE `components/Gallery.svelte` **per source**, all
mounted with the inactive ones `hidden` — so switching sources does not throw
away compiled tile engines, and a hidden grid intersects nothing, so it costs
no frames either. The page re-defaults the pick when the *mode* changes
(the app boots as a playground and only then discovers a device), so a console
opens on `On device`.

`Gallery` owns geometry and scheduling only: `items` (a `GalleryItem[]` the
page supplies — device patterns, saved patterns) or `src` (a generated JSON it
fetches), plus `search`, `playingKey`, and bindable `count` / `loading` /
`note` for the page's segment chips. A device pattern whose `source` has not
streamed in yet is a spinning tile, and when it arrives only that tile's engine
is rebuilt. The tile's verbs come from the page through two slots, `actions`
(the hover strip) and `meta` (the mobile `Edit` link).

Tile verbs (§5.1, §5.4b): a bare tile click **plays** an on-device pattern on a
console and **opens** everything else in the editor; the hover strip is
`▶ Play · Edit · ⋯`; `⋯` is Add to playlist (on-device only — a playlist item
is a device pattern id, with no control overrides, i.e. the pattern's own
defaults) · Duplicate · Delete (on-device only, through the `confirm` danger
dialog). `Add to scene ▸` is Phase B (#480) and is absent, not disabled.
Play and Edit both go through the shell to `Editor.openDevicePattern(id)` —
Play simply does not set `editing`, so the running marker and the editor's
document never disagree.

`data-role` contract: `patterns-panel` · `patterns-sources` ·
`patterns-source-<device|library|mine|pixelblaze>` · `patterns-grid` (with
`data-source`, and `hidden` on the inactive ones) · `tile` (with `data-kind`,
`data-dims`, `data-key`) · `tile-face` · `tile-play` · `tile-edit` ·
`tile-menu` · `tile-menu-popup` · `tile-menu-{playlist,duplicate,delete}` ·
`tile-playing` · `tile-edit-link` · `tile-caption` · `tile-spinner` ·
`gallery-search` · `gallery-count` · `gallery-loading` · `new-pattern` ·
`device-offline`.

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
| `playlist-progress` | 2 Hz | `pages/Playlist.svelte` while its tab is open | nothing — it only ticks the now-playing clock (no fetch) |
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

A7 (#468) did move naming into the inline-editable editor header, and exactly
one `promptText` call went away with it. The dialog store is unchanged and
still owns every confirmation (both deletes, playlist clear, the reboot
prompts) plus the share-link fallback.

## The playlist (Gitea #470, proposal §5.4)

Three files and one rule: **an item owns its values.**

| file | owns |
|---|---|
| `stores/device.ts` | the `playlist` store, `addToPlaylist()`, `queuePlaylistSave()`, `markTransport()` |
| `pages/Playlist.svelte` | the transport group, the defaults, the ⋯ menu, the list, `+ Add` |
| `components/PlaylistRow.svelte` | one row: the chips, the inline sliders, the Projection line |
| `components/PatternPicker.svelte` | THE picker — what `+ Add` (and later the ⋯ menus) choose from |
| `components/ProjectionRow.svelte` | the quiet Projection line, shared with the editor's Controls rail (#468) |

`addToPlaylist(patternId, values?, proj?)` is the ONE path every "Add to
playlist" affordance takes (the editor's ⋯ entry and the Patterns tile ⋯
menu). It resolves the name from `devicePatterns`, appends optimistically and
debounces the write — callers pass the values they have tuned and nothing
else. From the editor that includes `stores/pattern.ts`'s
`projectionOverride`: the editor holds a projection choice only until
something durable takes it, and a playlist item is that something. There are no named presets (D6): the same pattern can
sit in the playlist twice with two different looks, and each row edits its own.

A value moved on the row that is CURRENTLY PLAYING is also pushed live with
`POST /api/control`, because the saved playlist only reaches the engine at the
next activation and a slider the fixture ignores is a broken slider.

`PatternPicker` takes `patterns` as a prop rather than reading the store, and
emits `pick: { id, kind }`. `kind` is always `"pattern"` today; Phase B
(#478/#481) adds a `"scene"` SECTION to the same component — the search, the
keyboard handling and the event shape are already shaped for it, and
`PlaylistItem.kind` on the wire model is the row side of the same seam.

The per-item **projection override** (§5.4d) rides beside the values, as the
`P <mode>` line of the playlist wire format (docs/api.md). It is rendered by
`components/ProjectionRow.svelte` — the same component the editor's Controls
rail uses, so a projection can never be captioned two ways — which decides for
itself whether to appear; the playlist row repeats the predicate
(`projectionOptions(patternDims, layout.dims).length > 1`) one level up only so
the chip that OPENS the panel is absent when there is nothing inside it. The
row's thumbnail renders through the override, so what the row shows is what the
device will play.

`banners` is the longer-lived list for conditions rather than events
(`setBanner(id, {level, text, role} | null)`, keyed upsert, insertion-ordered).
The editor's compile, runtime and capacity reports are derived state with
bespoke markup and are not banners at all since A7 (#468): the first two are
the code pane's status strip, the third a `capstrip` under the preview it is
about. Only *conditions* — the device is unreachable, the wasm failed to
load — reach the rail's banner list.

## The editor — three owners (`pages/Editor.svelte`, Gitea #468)

Proposal §5.2, mockups S2/S2b/S2c/S2d. The page used to mix four concerns
across three bars (research/ui-audit.md §4); it now has three owners and
nothing crosses between them.

| owner | what it holds | `data-role`s |
|---|---|---|
| the **header** (`editor-header`) | the DOCUMENT: back · inline-editable name · save state · **Save** (the one primary action) · the ⋯ menu of document verbs | `editor-back`, `pattern-name`, `name-input`, `name-error`, `save-state`, `save`, `overflow`, `add-to-playlist`, `duplicate`, `epe-export`, `epe-import`, `share`, `delete` |
| the **code pane** | its own errors: gutter dot + wavy underline on the line + one status strip pinned to the bottom of the pane | `compile-error`, `runtime-error`, `map-compile-error`, `.cm-err-dot`, `.cm-lintRange-error` |
| the **preview header** | the TRANSPORT, next to the thing it controls | `preview-dims`, `pause`, `target-fps`, `mic-toggle`, `debug` |

Rules that come out of the audit and must not drift back:

- **The name is edited in place.** Click it, Enter or blur commits, Escape
  cancels, an empty name is refused inline (`name-error`) — nothing is ever
  disabled (§5.7). Save on an unnamed pattern opens that editor with the
  reason rather than a dialog.
- **No compile-error banner in the rail.** The rail's `banner` list is for
  *conditions* (the device is unreachable, the wasm failed to load); an error
  about line 14 belongs next to line 14.
- **Absent, never disabled.** `mic-toggle` exists only while the compiled
  pattern binds sensor variables (`Engine.wantsSensors()`); the Vars section
  (`vars-section`) only while it exports some; the Pins panel only while it
  touches GPIO; `share` only in the playground; `add-to-playlist` and `delete`
  (device) only on a console. All read off the ENGINE, never the source text.
- **The console preview runs the device output chain.** `applyOutpipe()` feeds
  `Engine.setOutpipe` from `/api/output` + `/api/brightness` + `caps.panel`
  (the per-pixel current model), and the render loop draws `engine.outpipe()`
  instead of the raw frame while a device is connected (#466). The playground
  has no device chain and keeps drawing `frame()`.
- **A `$:` must not derive from a store another reactive statement writes.**
  `matchRunningToLibrary()` sets `patternName`/`devicePatternId` from inside a
  reactive statement; a `$: displayName = $patternName || …` earlier in the
  file then renders one cycle stale *and never catches up* — Svelte folds the
  store's dirty bit into the fragment patch but does not re-run reactive
  statements that already ran. The header's name, save state and delete
  visibility are therefore **functions called from the markup** with every
  dependency passed in. (Found by device-e2e's "running pattern adopts its
  saved name", which is the regression test for it.)

**The editor configures no geometry at all** since A8 (#469). The interim
"LED layout" block at the foot of the rail — the old playback bar's shape
select, pixel/W×H fields, install-grid and the `Map program ›` link — is gone,
along with `led-layout`, `layout-kind`, `layout-px/w/h`, `grid-install` and
`subtab-map`. The device's Layout is Settings → LED layout's; the virtual one
is the "Preview as" chip's; the map is its own screen's.

`Add to scene ▸` (proposal §5.4b) is deliberately not rendered at all until
scenes exist in Phase B (#480).

## The map program — a screen, not a sub-tab (`pages/MapEditor.svelte`, #471)

Proposal §4 "What the map program becomes", §5.7. The mapper is kept whole —
it is a real Luxel program that `plot()`s one point per pixel on the VM, edited
in the same CodeMirror and stepped with the same `Debugger.svelte`
(research/ui-audit.md §7.6) — but it is **geometry**, so it is reached from the
Layout picker and never from inside a pattern:

| where | control | opens |
|---|---|---|
| playground | "Preview as" chip → `Custom map program →` (`preview-as-map`) | the screen, and the chip then reads `N px custom map` |
| console | Settings → LED layout → `Custom map program →` (`layout-map-link`) | the screen |

It wears the pattern editor's chrome, from the same stylesheet
(`components/editor-frame.css` — a `.editor-frame`-prefixed plain CSS import,
like `settings/cards.css`; slotted markup is compiled in the parent's scope, so
a wrapper component could not have styled it anyway):

| owner | what it holds | `data-role`s |
|---|---|---|
| the **header** | back · "Map program" · the installed / in-use state · ONE primary action · ⋯ | `map-editor-header`, `map-editor-back`, `map-state`, `map-installed`, `map-note`, `map-install` (console) / `map-use` (playground), `map-overflow`, `map-export`, `map-import`, `map-reset`, `map-clear` |
| the **code pane** | its own errors: gutter dot, squiggle, one status strip | `map-editor`, `map-compile-error` |
| the **rail** | the plotted points + the transport that produces them, then the debugger | `map-badge`, `map-run`, `map-debug`, `map-error`, `map-3d` |

Rules:

- **The primary action is the only thing that publishes.** A run fills the
  screen's own scatter; `Use in preview` (playground) sets `mapCoords` +
  `previewAs`, `Install on device` (console) uploads the coordinates and the
  console's Layout follows the device as usual. Once a map IS in use, every
  subsequent run re-publishes, so editing the program stays live.
- **The scatter is the program's, not the app's.** It draws `cloudLayout(pts)`
  — the points at their own dimensionality, 2D for `plot(x, y)` and a rotating
  cloud for `plot(x, y, z)` — coloured by index so the picture shows the wiring
  ORDER as well as the shape. Drag-editing the points and the Fill/Contain
  framing (Gitea #355) attach here.
- **Nothing derived from a run may be a `$:`.** Opening the screen compiles and
  runs from a reactive block, and a `$:` whose input is assigned inside a
  function another reactive block calls renders one cycle stale and never
  catches up (the trap in `.claude/rules/web.md`). `adopt()` assigns the
  points, the Layout, the colour ramp and starts the draw loop together.
- **The map is not the pattern's.** The working copy never carried it
  (`lib/store.ts`), share links stopped carrying it at #463, and the program
  text is persisted on its own key — `luxel.mapSrc`. A console restores it from
  the same place: `GET /api/map` reports a count and dims, never the program
  (Gitea #517). A pre-#463 share link that carries one is run headlessly by the
  shell through `runMapProgram()` — no screen has to open for it.

### Projection (`components/ProjectionRow.svelte`, proposal §5.4d)

One quiet row under a hairline, after the pattern's own controls, visible
**only** when `Luxel.projectionOptions(patternDims, layoutDims)` returns more
than one option — i.e. the pattern's dimensionality differs from the Layout's
AND that Layout offers a choice. Labels come from the engine so every surface
captions a projection identically. Inherited reads as plain text
(`device default · along x`, `change`); an override reads in accent
(`along y · override`, `reset`).

The chosen mode lives in `stores/pattern.ts`'s `projectionOverride` — a value
of the working copy, cleared by every pattern load exactly like
`controlValues`, and never written into the pattern source (that was the map's
mistake). Durable per-item storage belongs to whatever *used* the pattern: a
playlist item's values (A9, #470) or a scene layer's (Phase B). The editor
applies it by re-installing the Layout's projection triple with this pattern's
axis substituted, then recompiling — `pixelCount` changes under an along-axis
projection, and the engine reads it at init.

## The Settings page (`pages/Settings.svelte`, Gitea #469)

Proposal §5.3/§5.3b/§5.4d/§5.7, mockups S3, S3b–S3k. Nine equal-weight cards
became a ranked page: the three things people open Settings for are full
sections at the top, and everything else collapses into one `Advanced` list.

```
Device      name (read-only) · Brightness — the page's FIRST control
LED layout  the summary + thumbnail, the kind picker, the fields, the
            arrangement, the Outputs table, Projection, the map link
WiFi        the connected network + a collapsed `Change network…`
— Advanced —
  Output processing · Panel driver · Clock & time zone · Multi-device sync ·
  MQTT · Home Assistant · Network input · Storage · Firmware & recovery
```

Chrome is two primitives. `Section.svelte` is a small uppercase label, a
hairline rule and a `.form` panel — only FORMS get a background, which is what
kills the card-in-card look. `Disclosure.svelte` is one Advanced row: chevron,
title, and a **one-line status** so a collapsed page still answers "is X on?"
without being opened. A collapsed body is not mounted at all, so the eight
forms behind those rows cost nothing until someone looks.

### LED layout owns geometry, and speaks one endpoint

Everything geometric is here — it used to be split between the editor's
playback bar (shape select, pixel field, install-grid, install-map) and three
Settings cards (Pixels, LED protocol, Data pin), with no card for the shape
itself. The rule now:

> **One `POST /api/layout` per user action, and the reply IS the new state.**

Nothing re-GETs (`applyLayout()` in `stores/device.ts` adopts the reply: the
Layout, the pixel count, the embedded map and the projection defaults all
land together), so the page never shows a value the device has not confirmed,
and a rejected body changes nothing on either side. `reboot_required` in that
reply is what the "applies after a reboot" note reads.

Three fields are deliberately NOT on that endpoint: on a **single-output**
board, LED type and colour order keep `/api/protocol` and `/api/output`, and
the data pin keeps `/api/datapin`. An `out` line is built once at BOOT
(#474/#475, docs/api.md "Live vs reboot"), so routing a colour-order change
through it would make a live setting need a reboot. Whether the firmware
should apply those two live and the split go away is Gitea #524, named in the
code comment.

The section's own pieces:

| piece | what it is |
|---|---|
| the summary | `layoutLabel()` in big type + a live `PatternThumb` of the FIXTURE, so the shape comes from the geometry store and nothing here re-derives it |
| the kind picker | Strip / Matrix / Custom map — **only where the board offers a choice**; a HUB75 board has none and the summary line carries the kind |
| `ArrangementSvg` | the panel chain: tiles, the path numbered from the `IN` connector, per-tile scan direction, the 180° markers, the total size, and output tinting. The same widget one level down (`mode="pixels"`) draws the pixel run through a strip-built matrix |
| the refresh readout | the device's own `matrix.est_hz` (#475) when it reports one, else `estimatedRefreshHz()` — the same formula over the same inputs, for a host that does not. Amber under 100 Hz with the fix named, and the panel's live `rescan_hz` beside it. The browser model's clock and plane count are the firmware's build-time constants until Gitea #525 puts them on the wire |
| the dark-tile note | `matrix.drive` is how many leading tiles this board's framebuffer can shift out; past it the picture dashes them and the page says how many stay dark (#475/#401) |
| `Reboot to apply` | appears next to the "applies after a reboot" note when a stored chain arrangement or output table is outstanding AND `caps.reboot` — `POST /api/reboot` behind `confirm({reboot: true})`, because those are built once at boot and nothing else applies them |
| `OutputsTable` | one row per output when `caps.outputs > 1`, each computing the run it owns (`pixels 300–599`), plus the strip split graphic. `out` lines are all-or-nothing, so a row edit POSTs the whole table |
| `ProjectionBlock` | one row per pattern kind that is NOT native here, with a live card per option. Labels and options come from the ENGINE (`Luxel.projectionOptions`), a single option is a one-line note, and there is no row at all for a native kind |

`data-role` contract: `settings-panel` · `sect-{device,layout,wifi}` ·
`advanced` · `adv-<row>-{row,toggle,status,body}` · `brightness` ·
`device-name` · `layout-{summary,headline,subhead,kind,pixels,pw,ph,scan,
cols,rows,start,dir,snake,rot180,proto,order,datapin,datapin-apply,notes,note,
dark,reboot,map-link}` · `arrangement` (with `data-mode`) · `refresh`, `refresh-hz`,
`refresh-measured` · `outputs`, `output-{row,pin,proto,order,count,rev,range,
add,remove}` · `projection-block`, `projection-kind` (with `data-dims`),
`projection-card` (with `data-mode`), `projection-only` · `wifi-change` ·
`panel-{clock,planes,rescan}` · `storage-{patterns,bytes,heap,psram}` ·
`fw-{version,update,file,note}`.

### `data-reason` marks the one legal disabled control (Gitea #529)

"Absent, never disabled" has exactly one exception in the proposal: a control
gating a **budget the user has to learn** (today only `out-palette-add` at its
32-stop cap). Such a control stays `disabled` AND carries `data-reason` with
the budget it is enforcing, in the same words a sibling element puts on screen
— the attribute is the machine-readable half of an explanation the user can
already read. Nothing else in the app may be `disabled` or
`aria-disabled="true"`: a control whose object does not exist is removed from
the DOM, and a control whose *action* can fail submits anyway and reports the
reason inline (the Save buttons, `promptText`'s `validate`). That makes the
rule mechanical, so the harnesses assert it mechanically:
`disabledSweep(page)` in `web/tools/e2e-common.mjs` returns every
`[disabled], [aria-disabled="true"]` element WITHOUT a `data-reason`, and both
`e2e.mjs` and `device-e2e.mjs` assert the list is empty with every Advanced
body mounted and the playlist, patterns and editor in both their empty and
populated states. Adding a `data-reason` to silence it is a design decision,
not a test fix.

### Visibility is a pure module (`lib/settingsCaps.ts`)

A control is **absent** unless the device advertises the thing it acts on —
never disabled, never inferred from a board name (§5.7). The decision is one
pure function so it can be tested over fixtures instead of one board at a
time in a browser:

```ts
settingsVisibility(caps: DeviceCaps | null, layout: LayoutFacts): SettingsVisibility
```

`caps` is `/api/status`'s block (#464); `layout` is `{kind, dims, regular,
panels}` off `/api/layout`. The result is one flat record of booleans the
markup reads with `{#if}` — `kindPicker`, `stripFields`, `panelScan`,
`arrangement`, `estimatedRefresh`, `outputsTable`, `powerCap`, `blurGlow` (+
`blurGlowScope`, which words it "along the strip" or "across the grid"),
`panelDriver`, `psram`, `ota`, `reboot`, and the rest. `caps === null` (firmware
older than #464) falls back to `FALLBACK_CAPS` — what every build has always
had — rather than to a guess.

The module also owns the pure arithmetic the section draws with:
`estimatedRefreshHz()`, `chainOrder()` (the tile order the SVG numbers —
tiles line by line, a line being a tile row under `dir: "row"` and a column
under `"col"`, `snake` reversing the odd lines and `rot180` marking their
tiles as mounted upside-down; the same walk #475's boot-time remap does),
`outputRanges()` and
`squarish()` (picking Matrix factors the pixel count — 120 px is 12×10, not
11×11 rounded up, so Strip → Matrix → Strip round-trips). All of it is tested
in `web/tests/settingsCaps.test.mjs` against the four `caps` fixtures of the
§5.3 table: a strip board, a HUB75 panel, a regular 2D matrix built from
strips, and a 3D/irregular map. The refresh model is checked against the bench
measurements in `firmware/src/hub75.rs` (77/115/154 Hz at 20/30/40 MHz).

Driving all of it without hardware is what `luxel serve`'s `--board panel`,
`--outputs N` and `--max-pixels N` are for; device-e2e runs the page on all
three shapes (docs/tools.md).

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

- **Console**: the device owns the geometry. Since A8 (#469) the adapter
  `deviceLayout` in `stores/device.ts` reads **`/api/layout` wholesale**
  (#465) — kind, dims, w/h, the chain's wiring and the embedded map in one
  fetch — and not a single consumer changed when it did, which is what that
  adapter exists for. Two fallbacks stay, in order: `/api/status`'s `geom`
  (#464), which answers during the connect handshake AND is the only reporter
  of the engine's fabricated ceil(√n) grid (`source:"default"`, so it WINS
  over the Layout in that one case — a `render2D`-only pattern on a strip
  board really is rendering through a grid the Layout does not describe); then
  `deviceMap`, for firmware older than either field. A "Preview as" choice
  only re-shapes; the pixel count stays hardware truth.
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
| `withProjectionOverride(l, dims, mode)` | the same Layout with ONE projection slot replaced — a playlist item's per-item override (§5.4d) |
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

The device's real wiring arrived with that switch: `/api/layout`'s
`matrix.snake` fills `serpentine`, so a console previews a snaked matrix the
way the fixture shows it rather than row-major. Per-item projection overrides are #470/#473's.

## Store reference

`stores/device.ts` — `device`, `deviceBase`, `isPlayground`, `mode`,
`deviceError`, `deviceBlocked`, `devicePixels`, `pixelMax`, `deviceHeapFree`,
`deviceEngineHeap`, `deviceVmerr`, `deviceFps`, `deviceOutFps`,
`deviceRescanHz`, `deviceMap`, `deviceLayoutWire`, `deviceCaps`,
`deviceVersion`, `deviceSlot`, `deviceStore`, `devicePatterns`, `brightness`,
`brightnessMax`, `deviceProtocol`, `protocolOptions`, `dataPin*`, `wifi*`,
`mqtt*`, `outputStatus`, `palette*`, `clockStatus`, `syncStatus`, `netLive`,
`playlist`; functions `connectDevice`, `detectDeviceBase`, `refresh*`,
`addToPlaylist`, `queuePlaylistSave`, `markTransport`, `applyLayout`,
`installDeviceMapCoords`, `installDeviceGridMap`, `clearDeviceMap`,
`pollSubscribe`, `pollStopAll`,
`startSessionPoll`.

`stores/geometry.ts` — `layout`, `layoutFor()`, `layoutSignature`,
`layoutName`, `shape`, `pixelTotal`, `pixelCount()`, `previewAs`,
`setPreviewAs()`, `patternDims`, `mapCoords`, `setMapCoords()`, `projection`,
`configureEngine()`, `compileForLayout()`, `captionFor()`, `effectiveFor()`,
`tileShape()`, `thumbLayout()`, `projectionCaption()`, `layoutLabel()`, `cloudLayout()`, `runMapProgram()`,
`layoutKey()`, `TILE_MAX_CELLS`, `THUMB_MAX_CELLS`. See **Geometry** below.

`stores/pattern.ts` — `luxel`, `loadLuxel()`, `source`, `dirty`,
`patternName`, `exampleName`, `devicePatternId`, `controlValues`,
`projectionOverride`, `hints`,
`mapSrc`, `MAP_PROGRAM_TEMPLATE`, `NEW_PATTERN`, `newPatternSource()`, `previewFps`, `runtimeError`, `saved`,
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
  maxpixels-e2e,sync-e2e,flash-e2e,coldload,lna-e2e}.mjs` drive them. Moving
  markup between components is fine; renaming or dropping a role is not,
  without updating the harness in the same commit. The last two are the ones
  that get forgotten, because they run against a REAL device rather than a
  mirror and so never ran in the phase that broke them: `coldload.mjs` spent
  A7–A10 reporting `boot FAILED` on healthy loads because it read a label the
  editor's back button no longer carries. A harness's own assertion can be the
  only thing failing — read its zero-failed-request line before believing it.
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
- **One phone breakpoint: `@media (max-width: 600px)`.** Every responsive rule
  in `web/src` hangs off it — the Patterns grid, the Gallery tiles, the
  Playlist page and its rows, the pattern picker. Mobile is a soft requirement
  met by RESTACKING, never by a second flow (CLAUDE.md), so a new surface
  reuses this number rather than inventing one. `Dialog.svelte`'s 420 px is
  not a second breakpoint: it is the width at which two side-by-side buttons
  stop fitting. Targets under it are thumb-sized (≥ 32 px) and no page may
  scroll sideways at 390 px — `device-e2e.mjs` asserts both on the Playlist.
- **No native dialogs.** `window.prompt` / `window.confirm` / `alert` do not
  appear anywhere under `web/src` — naming and confirmation go through
  `stores/dialog.ts` (#472). A native dialog also hangs the e2e harnesses,
  which deliberately install no `page.on("dialog")` handler.
