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
  app.css           design tokens, element resets, and THE shared primitives
                    (.btn/.inp/.slabel/.menu/.pop) — dark only
  stores/           plain-TS Svelte stores — all app state lives here
    device.ts       session lifecycle, hardware facts, settings state, ONE poll scheduler
    geometry.ts     THE Layout reconciler — every preview, tile and thumbnail reads it
    pattern.ts      the pattern document, the wasm host, local library, .epe + share codecs
    notify.ts       transient notes + the banner list
    dialog.ts       the modal primitive: promise-returning confirm/promptText
  lib/router.ts     one fragment per screen; the shell is its only caller
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
      PanelModuleCard, ProjectionBlock, ProjectionCard
    OutputCard, ClockCard, SyncCard, MqttCard,
      NetworkInputCard, StorageCard, FirmwareCard   the Advanced bodies
    cards.css
  components/       reusable widgets (CodeMirror wrapper, Preview, Controls,
                    PinPanel, VarWatcher, Debugger, Gallery, PatternThumb,
                    PlaylistRow, PatternPicker, ProjectionRow, Dialog,
                    PreviewAsChip, Popover, DeviceChip, HeaderBrightness) plus
                    editor-frame.css, the chrome BOTH full-screen editors wear
  lib/              non-UI logic: device HTTP client, fetchgate, LNA classifier,
                    wasm bindings, control hints, playlist transport, audio,
                    builtins, the boot resume rule (resume.ts)
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

`App.svelte` owns exactly five things: `mode` (playground vs device — decided
once at boot by `lib/probe.ts`, a classified, retrying probe of this origin's
`/api/status`: a 4xx or an HTML answer is a playground, anything else is a
device that may be busy, and an inconclusive same-origin probe still boots
device mode with the unreachable bar and a scheduled handshake retry, #777),
`tab` (which home surface is open), `editing` (the full-screen editor sits over
the home tab — it is not a tab), `mapEditing` (the map program's screen, over
everything, A10/#471), and the boot cover. It also renders the header,
the fps readout and the LNA blocked banner, and it wires page events to
`Editor` methods (`newPattern`, `loadSaved`, `loadGalleryPick`,
`openDevicePattern`, `importEpeFile`, `bootDevice`, `bootPlayground`).

The tab set is `Patterns · Scenes · Playlist · Settings` on a console and
`Patterns · Scenes` in the playground (proposal §4 and D10), built from one
`tabs` array — which is how Scenes (#480) became one more entry rather than
another `{#if}`.

The shell header does **not** carry the open document. Since A7 (#468) the
editor renders its own header — back, name, save state, Save, ⋯ — and the
shell only passes `backLabel` down and takes a `back` event up. What stays in
the shell header is what is true of the *session*: the device chip / "Preview
as" chip, the brightness slider on a console, and the fps readout
(proposal §5.7).

Since #538 the shell header is **not rendered at all** while `editing ||
mapEditing` (Jeremy: "when editing a file, 'luxel' and other things in the
header bar hide"; mockup S2). The session facts travel with it: an editor
screen ends its header with a rail segment (`.edhdr-rail`) holding
`components/DeviceChip.svelte` on a console, and the "Preview as" chip in the
playground. Anything that needs to be reachable from an editor screen has to
live in the editor's own header — there is no bar above it.

Header layout (mockup S1): 44px, `padding:0 16px`, and one row of
`wordmark · device chip · tabs · spacer · control · fps`. At ≤600px it becomes
the mock's two rows (S1c) — a 40px identity row and a 38px scrolling tab strip
— by un-`display:contents`-ing the `.hdrtop` wrapper. The active tab is
`var(--text)` with a 2px amber underline flush to the header border; amber
TEXT is wrong (Jeremy, 2026-09-19).

DOM order in that header is the VISUAL order, and deliberately so: the one
focusable control right of the tabs (the brightness slider on a console, the
"Preview as" chip in the playground) is authored AFTER `<nav class="tabs">`,
outside `.hdrtop`, so Tab visits wordmark → chip → tabs → brightness → fps.
`order:` is only ever put on things nobody can focus (the spacer, the fps
readout). Before #538's closure the whole `.hdrtop` preceded the tabs and
`hdr-brightness` was the FIRST stop on every console screen. The readout
itself is bare — `27 fps`, the mock's own copy — and what the number is
(panel `out_fps` vs render `fps`, the rescan ceiling, the local preview rate)
lives in its `title`.

The device is named by `stores/device.ts`'s `deviceLabel`: `/api/status`'s
`name` when the firmware reports one, else the host it answers on. Never the
word "device" — a console that says "device" tells nobody which board is on
the bench.

`components/HeaderBrightness.svelte` is beyond the mocks (Jeremy asked for it):
a 96px range left of the fps readout, console only. It writes on `change` —
pointer release, or a settled run of arrow keys — debounced 150ms, so a drag
is ONE `POST /api/brightness`, not one per step. Settings keeps its own
control; both drive the `brightness` store.

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

## The URL (`lib/router.ts`, Gitea #538)

One fragment per screen, so a refresh reopens what was open (Jeremy asked for
it by name for Settings):

| route | screen |
|---|---|
| `#/` | Patterns |
| `#/scenes` | Scenes |
| `#/scenes/<id>` | the scene editor, on that scene |
| `#/playlist` | Playlist |
| `#/settings` | Settings |
| `#/editor` | the pattern editor |
| `#/map` | the map program screen |

**Why the hash and not a path.** The device serves this console from flash and
its GET router is a flat match over asset paths (`firmware/src/server.rs`): an
unknown path falls through to 404, there is no SPA fallback, so
`http://luxel-f6b0a8/settings` would be a dead link on the only machine most
people ever load this app from. A fragment never reaches the server, so the
same URL works on the device, on the native mirror and on the hosted copy.
Real paths need a firmware fallback route first.

The share link is a fragment too (`#p=…`, `stores/pattern.ts`); the two are
told apart by the leading slash — a route always starts `#/`. A share link
wins at boot and its fragment is left alone.

There is deliberately **no `?pattern=<id>`**. A route names a SCREEN; which
document the editor holds is the working copy's business (restored from the
autosave, or pulled from the device's running pattern). Putting the id in the
URL means a refresh re-activates that pattern on the hardware, and a page
refresh must never change what the LEDs are doing.

`#/scenes/<id>` is the ONE route with an argument, and the reason above is
exactly why it is allowed: opening a scene does not run it. Restoring that
route changes nothing on the fixture, which is what lets a playlist row link
straight at a scene (`App.openScene(id)`, Gitea #480).

`App.svelte` is the only caller: `$: if (routing) syncUrl(currentPage)` pushes
on a page change, `popstate` applies one, and boot applies the fragment AFTER
the device handshake so the route wins over the boot default.

**Testing note.** `page.goto(sameUrlWithHash)` is a same-document navigation —
the app never re-boots. A harness that means "reload" must use `page.reload()`
(see `reloadInto()` in `tools/device-e2e.mjs`).

## Shared primitives (`app.css`, Gitea #538)

The mockups (`docs/design/webui-v2/mockups.html`) are the visual spec; the
numbers in `app.css` are quoted from it rather than approximated. Tokens:
`--bg/--bg-panel/--bg-inset/--border/--text/--text-dim/--accent/--accent-soft/
--error/--warn/--ok/--mono/--sans`. `--ok` is THE green (playing ring, device
dot, playlist progress) — never a literal in a component.

| class | what it is |
|---|---|
| `.btn` | 32px, 6px radius, `--bg-inset` |
| `.btn.primary` | filled amber, `#16110a`, 600 — the ONE primary action of a screen |
| `.btn.quiet` | transparent, `--text-dim` |
| `.btn.icon` / `.btn.sm` / `.btn.sm.icon` | 32×32 / 26px / 26×26 |
| `.inp`, `.inp.mono`, `.inp.num` (72px), `.inp.xs` | the field scale |
| `.slabel` | the small-caps section label, `.08em` |
| `.menu` / `.menu .mi` / `.mi.del` | the 214px verb list and its rows |
| `.pop` / `.pop .pr` / `.popfoot` | the 296px chooser (mockup S5) |

Modifiers only bite in combination (`.btn.primary`, `.inp.num`), so a page's
own `.icon` or `.num` class can never be captured by the global sheet. There
were four different `.primary` blocks and three menu stylesheets before this;
one `.btn` and one `.menu` is the point.

### `components/Popover.svelte`

THE popover: the editor ⋯, the map ⋯, the tile ⋯, the playlist ⋯ and the
"Preview as" chooser all mount through it. It owns geometry and dismissal
only — the LOOKS are the global `.menu` / `.pop` rules, because Svelte
compiles slotted markup in the CALLER's scope and a wrapper component cannot
style what it was handed.

```svelte
<Popover open={menuOpen} anchor={moreBtn} kind="menu" align="end"
         dataRole="editor-menu" on:close={() => (menuOpen = false)}>
  <button class="mi" …>Duplicate</button>
  <div class="sepr"></div>
  <button class="mi del" …>Delete</button>
</Popover>
```

- `anchor` is the element it hangs off (`bind:this` on the trigger).
- `kind`: `menu` (214px verb list) or `pop` (296px chooser). A `menu` closes
  when one of its items is clicked; a `pop` does not — you set several fields
  in it before leaving.
- Positioned `fixed` off the anchor's viewport rect, so a menu opened from a
  tile inside the scrolling grid is not clipped by it — and therefore it
  **dodges all four viewport edges**: clamp left/right/top, flip above when
  the bottom would overflow. (Jeremy: "dropdowns don't dodge sides of screen".)
- It keeps the last non-degenerate anchor rect: a hover affordance like the
  tile's ⋯ stops being hovered the instant the popover covers the pointer, and
  a zero-sized rect would otherwise fling the menu into the corner.
- Escape and an outside click both dispatch `close`; the OWNER holds `open`.
- **A SECOND opener must `stopPropagation`.** The outside-click listener is on
  `window`, so a click anywhere that is neither inside the popover nor inside
  `anchor` dispatches `close` — including a click on a button whose own
  handler just set `open = true`, which then flips straight back and the
  popover never appears. The trigger you `bind:this` as `anchor` is exempt;
  every other one (the projection row's value text, a swatch that is not the
  anchor) needs `on:click|stopPropagation`. Costs a debug cycle every time,
  because the handler visibly runs and the state visibly ends up false.

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
fetches), plus `search`, `playingKey`, `only` / `autoStyle` (the layout split
below), and bindable `count` / `loading` / `note` for the page's segment chips.
A device pattern whose `source` has not streamed in yet is a spinning tile, and
when it arrives only that tile's engine is rebuilt. The tile's verbs come from
the page through two slots, `actions` (the hover strip) and `meta` (the mobile
`Edit` link).

The tile is the mock's card (`docs/design/webui-v2/mockups.html` `.tile`,
frames S1/S1b/S1c): a `--bg-panel` box with a **full-bleed** canvas whose
aspect ratio carries the fixture's shape — `6/1` on a strip, the lattice's own
`w:h` on a matrix, square for a point cloud — over a left-aligned meta block
(13 px name, 11 px mono caption). The grid uses the mock's fixed column counts
(6 squares, 3 bars, 2 on a phone) rather than `auto-fill`, because what fits is
decided by the tile's SHAPE, not its pixel width. The playing tile wears the
`--ok` ring and the `▶ playing` pill, and its hover strip drops exactly one
verb: **`▶ Play` is absent, `Edit` and `⋯` stay** (#555 — mock S1 draws the
playing tile without a strip at all, but that is the frame showing one state
per tile, not a rule; dropping the whole strip made the running pattern the
one thing on the page you could not open or delete). The slot is handed
`playing` alongside `item` and `dead` so the PAGE decides which verb goes.
The hover strip's gradient is `pointer-events:none`,
so only the verbs take the mouse and a click anywhere else on the thumb still
plays/opens the pattern.

The tile's `⋯` is mockup S2's `.menu`: `Add to playlist` (console) ·
`Add to scene ▸` · rule · `Duplicate` · `Export .epe` · `Import .epe…` ·
rule · `Delete` (console).
`Add to scene ▸` (#480/#478) is a submenu of the same group wherever scenes
can exist — the playground, or a console whose Layout is a matrix. Anywhere
else it is absent, not disabled: the shell's one `scenesReady` predicate is
passed down and `components/AddToSceneMenu.svelte` is simply not rendered.

These are LIBRARY verbs acting on a tile, not the editor's document
verbs under another name: **`Import .epe…` parses, compiles and stores the
file in the library the tile came from** — `On device` via
`POST /api/patterns`, `Mine` via `saveToLocalLibrary` — taking the same
`copyName()` path as `Duplicate` on a name collision, reporting failures
through `stores/notify`, and never opening the editor or touching what the
device is playing (#563, #572). The editor's own `Import .epe…` still means
"replace the open document"; the two are deliberately different verbs with
the same name. Its `<input type="file">` (`tile-menu-import-file`) lives
outside the popover — the menu closes before the file dialog opens, and an
input inside an unmounted popover never fires `change`.

### A Layout never offers a pattern it cannot show (#538)

Jeremy's projection rule — a fixture renders its own dimensionality and
anything lower — reaches the Patterns page as a filter. "Incompatible" is
`projectionCompatible(patternDims, layoutDims)` from `lib/geometry.ts`, i.e.
*the dims differ and no projection exists for the pair*; nothing here has its
own idea of the rule.

* **Library · Mine · PixelBlaze** simply drop them, and the segment chip counts
  what is on screen. The search still searches what is left.
* **The playlist's `+ Add` picker** (`components/PatternPicker.svelte`) drops
  them from BOTH of its sections, for the same reason and by the same rule
  (#562): picking one would write it to the device's store and queue an item
  the fixture cannot play. It has no tiles to compile, so the advisory dims are
  all it uses — `gallery.json`'s `kind` for a library row, `guessPatternDims()`
  for a device one, a device row whose source has not streamed in yet counted
  as 1D. When the filter is what emptied the list, `picker-empty` says
  `Nothing here can play on this layout.` rather than blaming the search.
* **On device** does not: those patterns are the user's own, stored on their
  own hardware. They go into a second `<Gallery only="incompatible" autoStyle>`
  under the grid, behind a collapsed-by-default disclosure reading
  `Not for this layout (N)`. `autoStyle` compiles each tile through
  `autoLayoutFor` — the playground's "Auto", i.e. the pattern's own shape —
  because the device's Layout is precisely the one that cannot show it. Those
  tiles carry Edit and ⋯ but no Play. There is no explanatory prose beyond the
  heading, by request.

The filter only applies when the Layout is a real FIXTURE (`layout.source` is
`device` or `user`). Under playground Auto the Layout follows whichever pattern
the editor holds, so filtering by it would empty the library depending on what
was last opened.

A tile must be classified before it compiles, or the grid would resolve one
tile at a time as they scroll into view. `gallery.json` ships an advisory
`kind`; for the sources that arrive as bare source (device patterns, `Mine`)
`guessPatternDims()` applies the same regex rule `tools/gen-gallery.mjs` and
`engine.rs` use. `Engine.patternDims()` replaces the guess as soon as the
tile compiles, and the tile moves if it was wrong.

`patternDims()`, not `preferredDims()`: the latter answers "does this pattern
want a map installed" and folds `render` and `renderFrame` into one `0`, while a
caption, a filter and a projection picker have to tell a **1D** pattern (a
strip drawn on this Layout, projectable along an axis) from a **dimensionless**
one (`0` — a field over `pixelCount`, native everywhere, `gallery.json` kind
`"any"`). They pick the same Auto rig, so only the projection surfaces notice
— which is why the collapse survived until `library/fairies.js` was offered an
"along x" that changed nothing (#629).

Two things in `Gallery` are load-bearing and easy to undo by accident:

* `$: shown = tiles.filter(…)` must stay **after** `$: if (items !== null)
  syncItems(items)`. Svelte orders reactive statements by the assignments it
  can see, and `tiles` is assigned inside `syncItems`, so source order decides
  — the other way round, a grid fed by `items` renders empty until something
  else invalidates it.
* the split grids **render** only their half (`{#each shown}`) instead of
  hiding the other one: both halves are mounted over the same item list, so a
  merely-hidden tile would still answer `.tile` queries in the neighbouring
  grid.

Tile verbs (§5.1, §5.4b): a bare tile click **plays** an on-device pattern on a
console and **opens** everything else in the editor; the hover strip is
`▶ Play · Edit · ⋯`, less `▶ Play` on the tile already playing (#555); `⋯`
is Add to playlist (on-device only — a playlist item
is a device pattern id, with no control overrides, i.e. the pattern's own
defaults) · Duplicate · Delete (on-device only, through the `confirm` danger
dialog) · `Add to scene ▸` (only where `scenesReady` — on a strip console it
is absent, not disabled).
Play and Edit both go through the shell to `Editor.openDevicePattern(id)` —
Play simply does not set `editing`, so the running marker and the editor's
document never disagree.

`data-role` contract: `patterns-panel` · `patterns-sources` ·
`patterns-source-<device|library|mine|pixelblaze>` · `patterns-grid` (with
`data-source`, and `hidden` on the inactive ones) ·
`patterns-incompatible` (the collapsed group, `hidden` when empty) ·
`patterns-incompatible-toggle` (its disclosure, with `aria-expanded`) ·
`tile` (with `data-kind`, `data-dims`, `data-key`) · `tile-face` ·
`tile-name` · `tile-play` (absent on a dead tile and on the playing one) ·
`tile-edit` ·
`tile-menu` · `tile-menu-popup` ·
`tile-menu-{playlist,duplicate,export,delete}` (mockup S2's `.menu`: three
groups, two `.sepr`s, the destructive verb last) ·
`tile-playing` · `tile-edit-link` · `tile-caption` · `tile-dead` ·
`tile-spinner` · `gallery-search` · `gallery-loading` · `gallery-note` ·
`new-pattern` · `device-offline`.

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
| `settings` | 0.5 Hz | `pages/Settings.svelte` while its tab is open | `/api/mqtt`, `/api/sync`, `/api/clock` — awaited ONE AT A TIME (#540) |

`/api/output` is deliberately **not** polled: it is a form, and re-reading it
under the user's fingers would fight their edits. It is read once when the
Settings tab becomes visible, and re-read after every write. Nor is
`/api/status` read anywhere but the `status` subscriber: DDP/E1.31 liveness is
a field of that same body, and the Settings tab used to re-GET the whole thing
at 0.5 Hz just to read it (#540).

Do not add a bare `setInterval` to a component. Subscribe here instead — the
device serves from a tiny connection pool and every extra poll competes with
the UI's own fetches (docs/tools.md, `panel-load-bench`).

**Count the CONNECTIONS the open tab needs, not just the requests.** A device
runs three web tasks (two on a small chip) and a closing connection holds its
slot for up to 2 s, so a page that keeps three sockets busy leaves nothing for
anyone else — its own next poll included. Measured on the Athom (#540): the
Settings tab firing its reads in parallel took `web[]` from `[0,1,1]` to all
three busy, and a second client was refused on 28 of 31 samples. Serialised,
the tab costs one connection beyond the status poll.

**A cadence is a ceiling, and a subscriber never runs twice at once.** The tick
skips any subscriber whose previous run has not resolved. It has to: under
congestion `gatedFetch` retries with ~10 s of backoff, so a run can span many
ticks, and firing anyway piles requests onto a device that is already refusing
connections — the latch behind "it suddenly went slow and never came back".
`last` is still stamped at the START of a run, so healthy cadences are exactly
the table above.

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
- `danger: true` marks the request destructive — both delete sites, playlist
  clear and the installer's wrong-image guard use it. It is `data-danger` on
  the panel, NOT a second button colour: the confirm button is the same
  `.btn.primary` every screen wears, because the mockups define one primary
  fill and their only red is `.mi.del`'s text (#538). The VERB says what the
  action does.
- Requests never stack: a second `confirm`/`promptText` while one is on screen
  resolves as cancelled rather than replacing what the user is reading.

`components/Dialog.svelte` is the only renderer — Escape cancels, Enter
confirms, Tab is trapped inside the panel, focus returns to whatever had it,
and the buttons stack full-width under 420 px (D9). Exactly one instance is
mounted per app entry: the shell, and `flash/Flash.svelte` for the installer.

Its `data-role` contract (the e2e hooks): `dialog` on the panel (with
`data-danger` when the request is destructive),
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
| `stores/device.ts` | the `playlist` store, `addToPlaylist()`, `saveAndAddToPlaylist()`, `queuePlaylistSave()`, `savePlaylistNow()`, `markTransport()`, `playlistPause()`/`playlistResume()`/`playlistStop()`/`playlistStep()` |
| `pages/Playlist.svelte` | the transport group, the defaults, the ⋯ menu, the list, `+ Add` |
| `components/PlaylistRow.svelte` | one row — the handle, the chips, the ✕ — AND, as a SIBLING `<li>` below it, the `.plvals` band the chips open (the inline sliders, the Projection line). Mockup S4 draws the row as one flex line and the band as its own block under it, so the component emits two top-level elements rather than nesting the band inside the row |
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

### The transport (mockup S4, Gitea #538 §F)

Four PERSISTENT controls, in one group, in the same places in every state:
a primary that toggles `‖ Pause` / `▶ Play`, then stop, prev, next as 32px
`.btn.icon` with inline SVGs. They go `disabled` (with `data-reason`) on an
empty queue rather than disappearing — the one place §5.7's
absent-never-disabled rule is deliberately set aside, because a transport that
reshapes itself is what made Clear read as a flicker. The now-playing block
(`min-width:230px`) is mounted for the same reason and merely dims when stopped.

**There is no pause on the wire.** `POST /api/playlist/stop` halts the
auto-advance and leaves the current item loaded and rendering; `index` keeps its
value; `play <index>` re-enters an item and restarts ITS clock at 0. So:

| UI verb | what it does | what it costs |
|---|---|---|
| `‖ Pause` | remember `index`, then `stop` | resuming replays the item from its start, not from where it stopped |
| `▶ Play` | `play <remembered index>` | — |
| stop | forget the index, then `stop` | the next Play starts the queue from the top — this is the difference between the two buttons |
| seek (drag the progress bar) | `play <index>` + a local clock offset | the device restarts the item, so its own advance still arrives a full duration later and the bar parks at the end for the seconds you skipped |

The bar itself is mockup S4's `.prog`: a 3px track with a real `--ok` fill
element inside it (`[data-role="pl-progress"] i`), whose width is the fraction.
The grab area for the seek is an overlay on its wrapper, not padding on the
track — the transport has to be exactly as tall as S4 draws it.

The progress readout is timed LOCALLY from the last `playing:index` change,
because the wire carries no elapsed field. Gitea #509 adds one; when it lands,
feed it into `itemStart` in `pages/Playlist.svelte` and the seek's remaining
inaccuracy goes with it.

`queuePlaylistSave()` debounces 400 ms because edits STREAM (a slider being
dragged). A whole-list verb does not, so **Clear** calls `savePlaylistNow()`:
the rows go in one DOM flush and the POST leaves immediately. The
device-e2e harness asserts both — one MutationObserver batch, one POST.

### The picker's sections

`PatternPicker` is the mock's `.addwrap > .menu.full.pick` (S4c/S4d): a
dropdown anchored under `+ Add`, inside the list it adds to, 360px wide on a
console and the full width of the list on a phone. The search field is FIRST,
then one `.slabel` per section. It emits
`pick: { id, kind, name, source?, layers? }` and renders one section per
source:

* **Patterns** (`kind: "pattern"`) — `devicePatterns`, passed in as a prop
  rather than read from the store. A pick queues the id directly.
* **Scenes** (`kind: "scene"`, Gitea #478) — the scene library
  (`stores/scenes.ts`), refreshed when the picker opens. A pick appends a SCENE
  item, which carries no values of its own (`layers` rides along so the new row
  can say `Scene ▤ · N layers` before the device's next read comes back). Empty
  off a regular 2D console, where scenes cannot exist.
* **Library** (`kind: "library"`) — the generated `gallery.json`, fetched once
  on the first open. A library pattern is source the device has never seen and
  a playlist item is a reference to a STORED pattern, so the owner saves it
  first (`saveAndAddToPlaylist()` → `POST /api/patterns`, then `addToPlaylist`)
  and the row is appended only on success. The picker shows a saving line and,
  on failure, says what happened and adds nothing. Names already on the device
  are dropped from this section so a pick is never a silent overwrite.

Every row is the SAME row (S4c): a device-shaped 26px thumbnail, the name, and
ONE dim fact — a pattern's dimensionality and its projection (`2D`,
`1D · along x`, from `captionFor`), a scene's layer count, and for a library
row what picking it costs (`saves to device`). A scene's thumbnail is its
composite, so what you pick looks like what you will get.

Each section renders at most 40 rows (every row is a live wasm engine); the
search is how you reach the rest.

### The row (mockup S4)

`⠿ handle · 44px device-shaped thumbnail · 13px name + mono `Pattern` ·
`8 s` chip · `N values ▾` chip · ✕ (`.btn.icon.quiet`)`, on a `--bg-panel`
card. The playing row carries a 3px `--ok` left border and a `--ok` `▶` in the
handle's place.

**The handle is the only reorder affordance.** S4 has no ↑/↓ movers, so they
are gone; the handle is focusable and `↑`/`↓` on it move the item, which is
the keyboard and screen-reader path those buttons used to carry. At 390px
(S4b) the duration chip folds onto the subtitle line (`Pattern · 8 s`, accent
when overridden) and the values chip keeps only its count.

`data-role` contract: `playlist-panel` · `pl-{play,pause,stop,prev,next}` ·
`pl-now` · `pl-now-name` · `pl-progress` · `pl-default-sec` · `pl-crossfade` ·
`pl-more` · `pl-menu` · `pl-clear` · `pl-add` · `pl-total` · `pl-empty` ·
`playlist-item` · `pl-grip` · `pl-name` · `pl-duration` ·
`pl-duration-inline` · `pl-duration-edit` · `pl-override` · `pl-sec` ·
`pl-values-toggle` · `pl-values` · `pl-invalid` · `pl-remove` ·
`pl-preempted` (why a direct play stopped the queue) ·
`pattern-picker` · `picker-{search,item,empty,busy,error,loading}` ·
`picker-section-{pattern,scene,library}` ·
`picker-more-{pattern,scene,library}` · `pl-edit-scene` · `scene-thumb`.

### Scene items (Gitea #478, mockups S4c/S4d)

A playlist item is a pattern or a **scene** — `PlaylistItem.kind`, and
`I S<sceneId> <sec>` on the wire (`lib/playlist.ts` owns both halves:
`playlistWire()` serializes, `normalizePlaylist()` fills in the `kind` a
pre-#478 device omits and the `controls` a scene item does not have; unit tests
in `web/tests/playlist.test.mjs`). A scene item NEVER carries `C` or `P` lines:
its layers own their values.

The row is a playlist row like any other — same handle, same card, same
duration chip — with three differences (S4c): its type line reads
`Scene ▤ · N layers`, it has **no values chip**, and `Edit scene ›`
(`pl-edit-scene` → `#/scenes/<id>`) stands where the chip would have been. At
390px the duration folds into the type line and the link shortens to `Edit ›`,
exactly as a pattern row's chip does.

### Composite thumbnails (`components/SceneThumb.svelte`, Gitea #482)

A scene's picture is the same compositing the device does, run locally:
`lib/sceneRender.ts`'s `SceneRenderer` — `Luxel.compositor(w, h)` over a
tile-sized copy of the device's grid (`luxel_core::compose` in the wasm, #477),
one `Engine` per pattern layer, one per sprite layer (built and then only READ),
and text resolved by the host because the compositor reads no clock.
`SceneThumb` paints its frames through the ordinary `lib/draw.ts` painter, so
the composite is finished BEFORE paint (the painters are single-buffer and
opaque), and wears the same `.thumb > canvas.sq` markup `PatternThumb` does —
every surface that already sizes a thumbnail sizes this one, and a row cannot
tell the two apart.

Cost is the design: a composite is N engines on a grid, not one 400-cell
engine, so every scene thumbnail on the page shares ONE ticker in the
component's module scope, with a step budget (2 composites per animation frame,
~8 fps each) and an admission cap (8 resident). The Scenes page's tile grid
keeps its own of the same shape (`components/scene/SceneGrid.svelte`), and both
are `Gallery.svelte`'s discipline: a page full of composites refreshes slower
rather than starving the editor's preview. A surface that wants ONE still frame
instead calls `lib/sceneThumb.ts`'s `sceneThumb(lx, scene, lookup, w, h)`.

### `Add to scene ▸` (proposal §5.4b, mockup S2e)

`components/AddToSceneMenu.svelte` is one row of a ⋯ menu — in the editor's
header menu and in a Patterns tile's menu, identically — with a submenu of the
scene library (`stores/scenes.ts`) and `New scene…`. Picking a scene is
read-modify-write through the ONE store: `withPatternOnTop(scene, id, values,
proj)` (`lib/scene.ts`, pure and tested) → `saveScene`. The pattern lands as
the scene's TOP layer — full box, normal blend, 100 %, unkeyed — carrying the
values it is being shown at, and a refusal reaches the ONE error strip because
that is what `saveScene` does with one. `New scene…` saves a one-layer scene
named after the pattern and asks the shell to open it (`openscene` →
`App.openScene`, which leaves the pattern editor); the playlist row's `Edit
scene ›` takes the same path.

**It exists only where a scene can**: the shell's `scenesReady` (a regular 2D
console, or the playground) passed down as a prop, AND a pattern the device
actually holds. On a strip, a lattice or a custom-map console the menu is
simply one item shorter — ABSENT, never disabled (§5.7). The parent row is a
`<div role="menuitem">` rather than a button because `Popover` closes a menu on
any button click inside it, and the submenu stays mounted (hidden) so a harness
and a screen reader can find it.
Roles: `add-to-scene` · `add-to-scene-menu` · `add-to-scene-item` ·
`add-to-scene-new`.

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
| the **header** (`editor-header`) | the DOCUMENT: back · the name field · save state · [**▶ Play on device**] · **Save** (the one primary action) · the ⋯ menu of document verbs | `editor-back`, `pattern-name`, `name-input`, `name-error`, `save-state`, `editor-play-device`, `save`, `overflow`, `add-to-playlist`, `duplicate`, `epe-export`, `epe-import`, `share`, `delete` |
| the **code pane** | its own errors AND its advisory lints: gutter dot + wavy underline on the line + one status strip pinned to the bottom of the pane | `compile-error`, `runtime-error`, `boxed-lint`, `map-compile-error`, `.cm-err-dot`, `.cm-lintRange-error`, `.cm-lintRange-warning` |
| the **preview header** | the preview's own knobs, next to the thing they control | `preview-dims`, `debug`, `mic-toggle`, `target-fps` |

Since #739 (2026-09-24) the **transport is in the header bar**, not the preview
header — `pause` is a `.btn.transport` beside the save state, on *both* editors.
Jeremy: "the pause button doesn't look like a pause button at all (same on the
script page)… They are also very poorly placed and coloured. I didn't even know
that was an option… Maybe the header bar." It was the text glyph `‖`, which a
headless chromium without a symbol font draws as tofu — the reason the Debug and
mic buttons beside it were already inline SVG. It is now an SVG mark plus the
word `Pause`/`Resume`, accent-outlined while paused; at 390 px it keeps its
place and drops the word.

**The frame is the mock's, element for element** (mockup S2/S2b, Gitea #538):
`.editor-frame` is a two-row grid (header, then `.edbody`), `.edbody` a flex
row of `.left` (the code column, `flex:1`, `--bg-inset`, hairlined on its
right) and `.right` (the rail, a fixed **360px** on `--bg`). The rail scrolls
in an inner `.railscroll`, not on `.right` itself, because `overflow-y` on the
rail would force `overflow-x` with it. The frame's single column track is
`minmax(0,1fr)`: without a zero minimum a long code line or a `nowrap` header
widens the whole document instead of scrolling inside its own pane.

**The header is one flex row**, exactly as the mock draws it: `padding:0 16px`,
`gap:12px`, the document's controls, a spacer, `Save`, `⋯`, and then
`.edhdr-rail` — `width:344px` with `padding-left`/`margin-left:16px` and a
`border-left`. 16px of page padding + 344 + 16 puts that rule exactly on the
360px rail boundary, so `Save` ends at the code column's right edge rather
than the page's (audit E4). `.edhdr-rail` holds the device chip on a console
and the "Preview as" rig chip in the playground.

At ≤600px the frame stacks (rail first, code second — S2b) and the header
becomes S2b's single 44px line (`[← icon] [name] [Save] [⋯]`, the back
button's label dropped). `.edhdr-rail` either disappears — `.statusonly`, the
console's chip is a readout a phone can spare — or wraps to a second unruled
row, which is what keeps the playground's rig chooser reachable; only that
second case sets `flex-wrap`, via `:has()`.

Element-for-element fidelity to that mock frame is checked by
`web/tools/mockdiff.mjs --frames S2,S2b,S2c,S2d,S2err,S2cpop,S2menu,S2dialog`
(docs/tools.md), which must report **zero** deltas outside the map's `allow`
list.

### Lints: boxed variables and interpreter mode (Gitea #627)

After every SUCCESSFUL compile the editor asks the engine one more question
— `lx_kinds` → `luxel_core::jitlint` — and renders the two answers as advice.
Neither blocks anything: the pattern previews, saves and pushes exactly as
before, and a pattern with nothing to say renders no extra element at all.

- **Boxed variables.** `kinds::explain` says which slots the inference had to
  leave `Dyn` (docs/jit-design.md §2); `jitlint::dyn_lints` adds the variable
  NAME (`GlobalDef.name` / `FnDef.local_names`) and a source position — the
  first store that widened the slot, so `heat = array(8)` and not the
  `var heat = 0` above it. Each becomes a WARNING-severity CodeMirror
  diagnostic (amber `.cm-lintRange-warning`, never the red squiggle or the
  gutter dot — those stay the compile error's), plus one amber
  `.codestatus.warn` line (`boxed-lint`) counting them and quoting the first
  reason. It is the same strip the runtime error uses, and the compile error
  still outranks both.
- **Interpreter mode.** `jitlint::jit_eligibility` answers whether a JIT
  board will refuse the whole program for a reason visible at compile time
  (today: a callback-taking builtin — detected through `builtin_sig`'s
  `callback` field, so Gitea #626's prelude removes the refusal without an
  edit here). A refusal shows as a persistent amber `capstrip` under the
  preview (`jit-warning`) saying "Runs in the interpreter on JIT boards: …",
  clickable to jump to the call site, which is also squiggled. The
  device-only reasons are not knowable here and surface from `/api/status`
  instead — which since Gitea #658 they do, in the SAME row:
  `lints.ts`'s `deviceJitReason` turns `/api/status`'s `jit` object into one
  sentence in the same amber `capstrip` (`jit-device`), and the two cannot
  both fire because the local prediction wins when it has one (it can point
  at a line; `too-large` and `no-buffer` cannot). The reason ids are one
  vocabulary across `luxel_jit::Refusal::id`, `jitlint::JitRefusal::id` and
  `firmware/src/jit.rs`, so the wording lives once, in `lints.ts`.
  The happy path is not a strip at all: a device that COMPILED the pattern
  gets a quiet `native` marker beside the frame rate in the shell header
  (`jit-native`), because it explains the number rather than being news.
  `luxel serve --jit native|interp:REASON|off` drives all three without an
  S3 on the bench (docs/api.md).

`lx_kinds`'s JSON is `{jit:{eligible,reason?},dyn:[…],stats:{typed_slots,
total_slots}}`; `web/src/lib/lints.ts` is the whole mapping from it to what
the two surfaces render (unit-tested in `web/tests/lints.test.mjs`), and
`components/Editor.svelte` merges the error and the lints into CodeMirror's
one diagnostic set (`setDiagnostics` replaces it wholesale, so both go
through `applyDiagnostics`). The report is dropped the moment a compile
fails: lints from the last good program would point at lines that have
moved.

### Opening a pattern is not a device action (the push rule, Gitea #563)

The v1 editor pushed on every recompile, unconditionally: the app *was* an
editor and live-push-on-open was the feature. In v2 the Patterns page is a
browser with explicit verbs, and the device may be running a playlist the user
cares about — so one click on a browsing page must not take the installation
over. It did: `Edit` on a Library tile pushed `POST /api/code`, which the
firmware treats as a manual takeover (`playlist::stop()`), stopping the
playlist and leaving the device on an unsaved ad-hoc program with no row in
`On device` to get back from, and nothing in the UI saying so.

**The rule.** The editor writes to the device only while its document IS the
device's running program — `livePush` in `stores/pattern.ts`. Everything that
crosses the wire from the editor is behind it: `/api/code` (the recompile
push), `/api/control` (slider moves), `/api/events` (preview clicks) and
`/api/sensors` (the mic standing in for a sensor board).

| you did this | editor state |
|---|---|
| connect with no unsaved work (the handshake pulls the running pattern) | **live push** |
| connect holding an unsaved edit OF the running pattern | **live push** |
| connect holding any other unsaved working copy | local preview |
| `Play` on an On-device tile, `▶ Play on device` in the header | **live push** |
| `Edit` on the On-device tile that IS running | **live push** |
| `Edit` on an On-device tile that is not running | local preview |
| a Library / Mine / PixelBlaze tile, `+ New pattern`, `Duplicate`, an `.epe` import | local preview |

### Boot obeys it too (Gitea #585)

The last path that still pushed unasked was the console's own boot. The browser
autosaves the working copy (`luxel.current`, `lib/store.ts`), and a console used
to *resume and push* it whenever it was dirty — so opening the app replaced the
running program and, because `/api/code` is a takeover on the firmware
(`playlist::stop()`), stopped a playing playlist, before anything had been
clicked. It is also how a 300 px strip came to be running a `render2D` program
in #573.

The handshake now always pulls the running program, and what it is running is
the INPUT to the decision rather than a thing to overwrite. The rule is one
pure function, `bootResume` in **`lib/resume.ts`** (`tests/resume.test.mjs`),
over two ids — the device pattern the copy is an edit of (persisted in the
working copy since #585) and the one the device is running:

| the browser was holding | boot does |
|---|---|
| a clean copy | opens the RUNNING pattern, live push (unchanged) |
| a dirty edit of the pattern the device is running | keeps the copy, live push resumes, it is pushed |
| a dirty edit of anything else | keeps the copy in **local preview**; the device is not touched |

So a dirty copy is never thrown away — it is the editor's document either way,
with `unsaved · preview only` in the header and `▶ Play on device` as the one
click that changes that. Nothing about the fabricated-grid caption of #573/#586
can trigger from a boot any more, because the boot no longer hands a fixture a
program it cannot show.

**Two empty ids are not a match.** When the working copy was the running
*ad-hoc* program of an earlier session, both ids are `""` — and so are they when
the device is on some unrelated ad-hoc program. There is no way to tell those
apart by id, so the device's current program wins and the copy resumes in local
preview: whatever is on the LEDs now was chosen after that copy was last
touched. The one exception is not a special case in the rule but in how the
running id is *found*: if the device is running exactly the source the working
copy holds, the copy's own `devicePatternId` is still the honest name for what
is playing (a live-push session that reloaded), so `Editor.svelte`'s
`confirmRunning` names it and the normal id match then applies.

`deviceRunningId` is also filled at boot by matching the pulled source against
the stored patterns as their sources stream in (`nameRunningPattern`), so the
Patterns page rings the right tile even when the editor is holding something
else entirely. That is the RUNNING program's identity; `matchRunningToLibrary`
below names the editor's DOCUMENT, and the two coincide only in live push.

In **local preview** the rail preview runs the local engine exactly as always
(through the device output chain, #466) — the difference is only that nothing
is sent. The header says so in the save state, and grows the one verb that
changes it:

* `save-state` (`data-role="save-state"`) carries a contract the harnesses
  assert — but since #738 (2026-09-24) it lives on the element's
  **`data-save-state` attribute**, not in its text. The strings are unchanged.
  Playground or live push: `unsaved` · `saved · on device` ·
  `saved · in browser` · `not saved yet`. Console in local preview:
  `unsaved · preview only` · `saved · on device · preview only` ·
  `preview only · not on device`. Read it with `el.dataset.saveState`
  (`saveState()` in `web/tools/e2e-common.mjs` does).

  What the span **renders** is only the part the Save button cannot say. The
  button now carries the save half itself — `Save` while there is something to
  store, a spinner in flight, `Saved` for a second after, then a dirty-aware
  label — so printing `saved · on device` an inch to its left was the same
  fact twice, which is what Jeremy asked to delete. The other half is not a
  property of the document at all: it says the editor is not driving the LEDs.
  So the pattern editor's span renders `preview only` or nothing, and the
  scene editor's renders `in browser` (a playground scene, saved and clean) or
  nothing. The only wording that left the screen is `on device` vs
  `in browser` on the pattern editor, and the MODE already fixes that — a
  console can only store on the device, a playground only in this browser.
* `Save` stores the pattern and does **not** activate it — on a console that
  is `POST /api/patterns`, which gives a Library pattern its row in
  `On device` without touching the LEDs.
* `editor-play-device` (`▶ Play on device`) is the explicit activation, and
  the only thing in the editor that changes what is playing. A pattern the
  device already stores is activated by id; anything else (a Library pick, an
  unsaved edit) is saved first, because the device can only run what it holds.
  It is absent once the document IS the running one — there is then nothing to
  play, the same reasoning that removed `Play` from the playing tile (#555) —
  and absent in the playground.

**Two ids, not one.** `devicePatternId` (`stores/pattern.ts`) is the id of the
document the EDITOR holds; `deviceRunningId` (`stores/device.ts`) is the id the
DEVICE is running, which is what the Patterns page rings and pills. They were
one value until #563, which is exactly why merely opening a pattern used to run
it: there was no way to hold one without claiming the device was playing it.
`deviceRunningId` is written by `activateDevicePattern()` (THE activation verb,
used by both Play paths), by the connect handshake's source match, and by
`refreshPlaylist()` while the playlist is playing — auto-advance moves the
device off whatever was activated last.

`livePush` is deliberately its own flag rather than `devicePatternId ===
deviceRunningId`: both are `""` for an ad-hoc program, so the pattern pulled
off the device at connect (live) and a Library pattern just opened (preview)
are indistinguishable by id.

Rules that come out of the audit and must not drift back:

- **The name is a persistent bordered field** (`.nameedit`, mockup S2) edited
  in place. Click it, Enter or blur commits, Escape cancels, an empty name is
  refused inline (`name-error`) — nothing is ever disabled (§5.7). Save on an
  unnamed pattern opens that editor with the reason rather than a dialog. It
  was a hover-only affordance until #538; a title you cannot see is a title
  nobody knows they can change.
- **The primary action reads `Save` in both modes.** WHERE it lands is the
  save state's job (`saved · on device` / `saved · in browser`), not the
  button's — "Save to device" put the destination on the verb and said
  nothing the line beside it did not (#538). A library→device save must END
  holding a `devicePatternId`, because that id is what the on-device verbs
  (`add-to-playlist`, `delete`) and the save state key off; when the save
  reply carries no `id`, it is re-read from `refreshDevicePatterns()` by name.
- **The preview header states both frame rates** on a console —
  `<layout> · <n> fps local · <n> fps on device` (`out_fps` on a pipelined
  HUB75 board, else `fps`). The device's figure is already in the shell
  header, so showing only that here told you nothing new; the local one is
  what says whether this browser tab is the bottleneck (#538). The playground
  has one loop and states one number. The transport is pause · **Debug** ·
  [mic] · rate: the debugger sits beside the thing you reach for with it, and
  is labelled because the bug glyph alone did not read as "debugger".
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

The ⋯ menu's order is the mock's: `Add to playlist` · [`Add to scene ▸`] ·
rule · `Duplicate` · `Export .epe` · `Import .epe…` [· `Share…`] · rule ·
`Delete` (error-tinted, last). The first group is the on-device verbs, so the
rule above `Duplicate` only appears when it does. `Add to scene ▸` (proposal
§5.4b) is rendered only where `scenesReady` — the playground, or a matrix
console; on a strip console it is absent, not disabled. `Share…` is the
playground's and joins the document group, which is
where the mock would have put it had a playground frame existed.

### `components/ColorPicker.svelte` (Gitea #538)

`hsvPicker`/`rgbPicker` controls used to be three raw 0..1 channels with a
slider and a number box each — "should not be asking the user to input raw hsv
numbers" (Jeremy, 2026-09-19). They are now the mock's swatch (S2
`.swatches i`, 26×22) opening a `.pop` with a saturation/value field, a hue
strip and direct numeric entry in BOTH spaces plus hex, because "put in values
directly" was the other half of the request. Not `<input type="color">`: the
browser picker is a different app, cannot be themed, and on some platforms
cannot be driven from the keyboard at all.

- **The wire is unchanged.** `kind` says which space the caller's triple is in,
  and that is the space it gets back — unrounded. `hsv` in, `hsv` out. The
  device push is byte-identical to what the raw channels produced; only the
  display converts (`lib/color.ts`, a pure unit-float module with its own
  tests).
- **Hue is remembered across greys.** `rgbToHsv` reports hue 0 for any grey,
  so a saturation drag to the left edge would snap the wheel to red and come
  back somewhere else. The component keeps the last hue the user aimed at.
- **Keyboard-reachable**: the field takes arrows (shift = coarse) and the hue
  strip is a real `<input type=range>` wearing a gradient — a native range
  paints its own track over the element's background, so the gradient lives on
  a wrapper and the track is made transparent.
- It is deliberately generic (kind + a triple in, a triple out, no control
  awareness) because the palette editor (#537) needs exactly this widget per
  stop.

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

It therefore inherits the split header and the 26px transport boxes with it
(#538) — the whole point of the shared stylesheet is that the two screens
cannot drift apart.

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
**only** when `Luxel.projectionOptions(patternDims, layoutDims)` returns
anything at all. That table is also where the rule "a Layout never shows a
pattern of higher dimensionality" lives (#545), so an empty list means "native,
or impossible" and the row is simply absent — the UI never restates the table
and never explains it. Labels come from the engine so every surface captions a
projection identically. Inherited reads as plain text
(`device default · along x`, `change`); an override reads in accent
(`along y · override`, `reset`).

**`change` opens the Settings widget in a popup** (`Popover kind="pop"`,
`data-role="projection-options"`): the same `settings/ProjectionCard.svelte`,
the same engine-supplied option list, each card a live preview of THIS pattern
on THIS fixture, plus a `Use device default` reset under a rule. It used to be
a wrapped row of bare 12px text buttons — "the projection option to override
projection uses very different UI [from] the device-wide projection … the
collapsed minimal row is perfect. I'm only talking about when the user decides
to change the value" (Jeremy, 2026-09-19). The cards only compile and animate
while the popup is open (`active={open}`), so a rail full of engines never runs
behind a shut chooser. Two columns at the `.pop`'s 296px: three would leave each
card a ~90px preview, which is not a picture of anything.

The chosen mode lives in `stores/pattern.ts`'s `projectionOverride` — a value
of the working copy, cleared by every pattern load exactly like
`controlValues`, and never written into the pattern source (that was the map's
mistake). Durable per-item storage belongs to whatever *used* the pattern: a
playlist item's values (A9, #470) or a scene layer's (Phase B). The editor
applies it by re-installing the Layout's projection triple with this pattern's
axis substituted, then recompiling — `pixelCount` changes under an along-axis
projection, and the engine reads it at init.

## Scenes (`pages/Scenes.svelte` + `pages/SceneEditor.svelte`, Gitea #480)

A scene is an ordered stack of layers drawn onto one frame. The RECORD, the
blend kernels and the sprite format live in `luxel-core`
(`docs/spec/scenes.md`); the web's job is to build that record, show what it
will look like, and push it.

### Where the tab comes from (D10, proposal §5.4c)

`Scenes` is the one tab NOT gated on "is there a device" — it needs a regular
2D grid to draw on and nothing else:

* **console** — strictly the DEVICE's Layout kind
  (`uiLayoutKind($deviceLayoutWire.kind, $layout.dims) === "matrix"`). A strip,
  3D or custom-map board never shows it; an empty matrix always does, and the
  page says what a scene is instead of showing an empty grid (mockup S6c).
* **playground** — always. Hiding it there would make the feature
  undiscoverable, so the page carries the S6b empty state whose one action —
  `Preview as a 64×64 matrix` — sets the fixture.

### Three files, three jobs

| file | owns |
|---|---|
| `lib/scene.ts` | the WIRE codec: parse/serialize with the same defaults omitted and the same `scene: line N: …` refusals as `luxel_core::scene`, the `/api/scenes` JSON, and the sprite tag. Pure; pinned against the Rust fixtures by `web/tests/scene.test.mjs` |
| `stores/scenes.ts` | the library — `/api/scenes` on a console, `localStorage` (`luxel.scenes`, the same wire blob) in the playground — plus the verbs and the live-push gate |
| `lib/sceneRender.ts` | the wasm `Compositor` plus one engine per pattern/sprite layer and the host half of a text layer. `lib/sceneThumb.ts` is the one-shot form other screens import |

### The live-push rule is the editor's, applied to a scene (#563/#585)

An edit reaches the device **only while the scene being edited is the one the
device is showing** (`shouldLivePush(id)`). Opening a scene, or editing any
other one, touches nothing until Save. Pushes are coalesced to 10 Hz, because
dragging the marquee is a pointermove storm and each push is a whole-record
POST.

### The editor's three columns (mockups S7 · S7b · S7d · S7e · S7f · S7g)

`pages/SceneEditor.svelte` is the THIRD full-screen screen, a peer of the
pattern editor and the map editor; its `<main>` carries
`data-role="scene-editor-view"`, because all three are mounted at once and an
unscoped harness selector would silently resolve to the pattern editor's.

* **LAYERS** (`components/scene/LayerList.svelte`) — top = front, so the list
  is the reverse of the wire and translates indices at its own edge. Drag to
  reorder (the destination is a 2px accent rule between rows; the list never
  reflows under the pointer), the eye to hide, click to select. Visibility
  lives HERE and never as a checkbox in the inspector. **That ordering rule is
  DRAWN, not written** (#736 item 17, 2026-09-24): `scene-stack-order` is the
  stack seen edge-on at the end of the column header and `scene-stack-base` a
  hatched ground rule under the bottom row, in place of the words `top = front`
  and `base` — "this is a sign that visual UI indicators are needed. Not text
  descriptions which are easily confused." The words survive as the tooltips
  and the accessible names.
* **PREVIEW** (`components/scene/SceneStage.svelte`) — the composite in the
  device's shape and the selected layer's box as a dashed marquee you can drag
  and resize pixel-snapped. Its header carries the dims line and the
  preview-rate chooser (`scene-target-fps`, the pattern editor's own option
  set). It does NOT mount `components/Preview.svelte`: that one is the pattern
  editor's and already appears twice in the DOM. **There is no frame-cost line
  and no budget meter** — both were deleted 2026-09-24 (#736 item 24): "that
  loading bar below the pattern preview needs to completely go away. That and
  this text 'Frame cost: 1 pattern layer · text, sprite and color layers are
  free'". The meter read as progress, and the budget it stated was one nobody
  was near. The sentence has one home now, in the `+ Add layer` menu at the
  cap, where it explains a row that will not click.
* **TRANSPORT** — in the **header bar** (`scene-pause`), not the preview
  column, and the same `.transport` element and words the pattern editor's
  uses (#739). A labelled SVG button, `Pause`/`Resume` and never `Play`: a
  scene's play verb is `▶ Play on device` in the ⋯ menu, and two play marks in
  one bar is how a bar stops meaning anything.
* **INSPECTOR** — one component per type (`PatternInspector`, `TextInspector`,
  `SpriteInspector`, `ColorInspector`, all ending in the shared `StyleTail`).
  *Transparent* is a key on which pixels count and is shown only under
  `Blend = Normal`; under any other blend the row is GONE, not greyed (S7g).
  *Blend* and *Transparent* are `RichSelect` pickers — icon, name and one line
  of plain English per option, from `lib/blendMeta.ts` — not bare `<select>`s
  of jargon (#735). **A pattern layer has no Fit control**: `style.fit` is read
  in exactly one place in the codebase, `compose::blit_sprite`, so on a pattern
  layer the box merely clips a full-layout render and `fit` does nothing
  (`docs/spec/scenes.md` already said so). `scene-fit` is a static line saying
  what the box really is; the chooser is `scene-fit` on the sprite
  inspector — **Stretch** (`fill`), **Fit** (`contain`) and **Tile**
  (`tile`), shown only once the box has a size; with `w`/`h` at 0 the sprite
  draws at its natural size and the row reads `12×12 · natural size` with a
  `natural` button to get back there (#741, docs/spec/scenes.md §4).

### Sprites: a record, a tab and an editor of their own (`pages/Sprites.svelte`, `pages/SpriteEditor.svelte`, #740 #741)

A sprite is a FIRST-CLASS record — the `LXSP` byte layout of
`docs/spec/scenes.md` §4 — not a pattern (Jeremy reversed the sprite-tagged
pattern design of #481 on 2026-09-24: "confusing and may allow cheating").
`lib/sprite.ts` is the codec: `encodeSprite`/`decodeSprite` mirror
`luxel_core::sprite::check` sentence for sentence, and `web/tests/sprite.test.mjs`
builds the same 35-byte "Heart" record by hand that the Rust test builds, so
the two codecs are pinned to the same bytes. `stores/sprites.ts` backs it
with `/api/sprites` on a console and `localStorage` (`luxel.sprites`, base64
records) in the playground, caches decoded records by id, and runs
`migrateTaggedSprites()` once per session: a stored pattern whose source
starts `// @sprite` is decoded by `spriteFromTaggedPattern` (the ONE
remaining reader of the old tag, kept for one release), saved as a sprite,
every scene that named the pattern id is re-pointed, and the pattern is
deleted — so sprites leave the Patterns library. `lib/store.ts`'s
`listPatterns()` hides a tagged pattern in the meantime.

**The Sprites tab** sits after Scenes and is gated exactly like it (matrix
Layout on a console; always present in the playground with the same
"Preview as a 64×64 matrix" empty state). Same skeleton as Scenes: one lede
sentence, `+ New sprite`, a tile grid (`SpriteGrid`/`SpriteThumb`: the sprite
centred at an integer zoom on a texel-sized checker, animated at its fps,
`12×12 · 2 frames`, Edit / Duplicate / Delete behind the tile menu).

**The editor** (`#/sprites/<id>`) is the #741 redesign, and it is where
drawing happens — never on the scene stage. Header: `← Sprites` (or `← Scene`
when a scene's inspector opened it, and it returns there), the name, save
state, `Save`, `⋯` (Duplicate · Delete). Left: four LARGE labelled tools
(Pencil · Eraser · Fill · Pick, keys 1–4) and Colour — ONE picker plus the
colours **in use**, read off the record (`usedColors`), so a colour enters
the strip only when it is painted; there is no recents row, because the old
one filled with every intermediate value a slider drag emitted. Centre: the
canvas (`SpriteCanvas`) at an integer zoom that fits, checker ground, grid
lines from 8 px/texel, pointer paint with capture (one undo step per stroke),
Shift = temporary Pick, right button = erase; under it the frame strip
(`FrameStrip`: thumbnails, `[`/`]`, `+` copies the current frame, `← Move` ·
`Duplicate` · `Delete` · `Move →` as a word row, `Play` at fps, `Onion` ghosts
the previous frame at 35 %). Right: name echo, Size (resize keeps what fits),
Frames (a count — the strip manages them), FPS (`0 = still`), the byte line
against the 16 KiB record (`Save` is the one disabled control when over, with
its reason), a 1:1 preview at the fixture's scale, and `Used in` (scene
names from `usedBy`). Undo/redo: ctrl-z / ctrl-shift-z / ctrl-y, 100 steps.
The colour cap is the FORMAT's 255, enforced in exactly one function
(`colorIndex`, `sprite: 255 colours max`); unused palette entries are
compacted on save.

**In the scene editor** a sprite layer is moved with the marquee like every
other layer — the stage has no paint mode and no pointer handlers of its own
any more, and the marquee wraps the sprite's natural size rather than the
grid so it is a visible grab handle. `SpriteInspector` is Name · Sprite (a
`RichSelect` over the store, `Edit ↗`, `New…` — which makes a blank `Sprite N`,
binds it and opens the editor) · Box (w/h editable) · Size (`natural`) · Fit
(above) · the blend tail. The stage and every thumbnail composite sprite
layers through `Compositor.setSprite(layer, bytes)` (`lx_comp_sprite`); no
engine is compiled for them (`lib/sceneRender.ts`'s `SpriteLookup`).

### Text layers (`components/scene/TextInspector.svelte` + `FontPicker.svelte`, #486)

One inspector, three sources, and the rows that come WITH them (S7h):

* **fixed** — the literal, in the row's own field.
* **clock** — a format select over the wire's six (`CLOCK_FMTS`), plus, when
  `/api/clock` says the device has no time yet, one dim state line saying
  exactly what will be drawn instead and a link to Settings › Clock. The row
  above stays usable. The playground draws from the browser's clock, which is
  never unsynced, so the line is a console thing.
* **text slot** — the 0–7 picker, one line saying who writes it, and the slot's
  current value echoed under `Now` "so the layer is not a black box when the
  panel is in another room". The source is ABSENT on a host advertising
  `caps.text_slots = 0` (firmware older than the endpoint). The playground has
  no API to be written from, so there `Now` is an INPUT — the same trade the
  `Preview as` chip makes for a fixture; on a console it is the readout S7h
  draws.

`stores/textSlots.ts` is the browser's copy of the table: polled from
`GET /api/text` while the scene editor is up (on the one scheduler), written
with `POST /api/text`, and mirrored into the wasm through `Luxel.setTextSlot`
so a pattern drawing `textSlot(n)` previews what the device has.

`Speed` does not EXIST at `Scroll = none` and appears directly under Scroll the
moment a direction is chosen, in px/s — the unit the firmware takes.

The **font picker** is the only font UI in the app (§5.6: there is no
Settings › Fonts group), so its list has to answer "which one" by itself: each
of the three built-ins shows the same sample (`12:48`) rendered in that face at
1:1 device pixels, drawn by running `luxel_core::text::draw` through the wasm
compositor on a grid the size of the sample. They are not bitmaps in the
repo — a hand-drawn sample would go stale the first time a font blob changed.
There is no `Upload…` row and no empty "custom fonts" section: user fonts are
filed and not planned (#487).

### Layout-gated text completions (`lib/builtins.ts`, `components/Editor.svelte`, #486)

`BuiltinDoc.requires: "matrix"` marks a builtin that needs a regular 2D grid;
`visibleBuiltins(matrix)` is the filter, and `matrixLayout` (a `stores/geometry`
derived store over `isMatrixLayout`) is the gate — the SAME rule that hides the
Scenes tab and `Add to scene`, so a console reads its device Layout through it
and the playground its `Preview as` choice. The five text builtins carry the
flag, so on a strip, 3D or custom-map Layout they are absent from the completion
list AND from the docs hover: "the editor never offers a builtin that would
silently do nothing on the device you are connected to" (S2f).

The completion popup and its docs card are CodeMirror's own DOM themed to the
mock's `.acpop` / `.acrow` / `.acdoc` (S2f). The card is built from the
builtin's `sig`, one `<p>` per blank-line-separated chunk of `doc`, and the
optional `example` under a rule.

### Two Svelte traps this screen found

* **`bind:this` into an `{#each}` item is a flush loop.** Writing back through
  the array invalidates it, which re-runs the keyed block, which re-fires the
  binding. It froze the scene grid solid. `components/scene/SceneGrid.svelte`
  captures its canvases with an ACTION into a non-reactive `Map` instead.
* **A `$:` that both reads and assigns the same variable is its own
  dependency** (`stopPoll = stopPoll ?? startScenePoll()`,
  `rafId = rafId || requestAnimationFrame(tick)`). Both are `onMount` now.

### Its `data-role`s

Page: `scenes-panel`, `scenes-lede`, `new-scene`, `scenes-empty`,
`scenes-empty-fixture`, `scenes-preview-as-matrix`, `scenes-grid`,
`scene-tile` (`data-scene`), `scene-tile-play`, `scene-tile-edit`,
`scene-tile-edit-link`, `scene-tile-menu`, `scene-tile-menu-popup`,
`scene-menu-play`/`-edit`/`-duplicate`/`-delete`, `scene-tile-name`,
`scene-tile-layers`, `scene-playing`.

Editor: `scene-editor-view`, `scene-editor-header`, `scene-editor-back`,
`scene-name`, `scene-name-input`, `scene-save-state`, `scene-save`,
`scene-overflow`, `scene-menu`, `scene-play-device`, `scene-duplicate`,
`scene-delete`; `scene-layers`, `scene-layer` (`data-layer`),
`scene-layer-grip`, `scene-layer-eye`, `scene-layer-pick`, `scene-drop`,
`scene-stack-order`, `scene-stack-base`, `scene-layer-badge`,
`scene-add-layer`, `scene-add-menu`,
`scene-add-pat`/`-text`/`-sprite`/`-color`; `scene-preview`,
`scene-preview-dims`, `scene-target-fps`, `scene-pause` (in the HEADER since
#739), `scene-stage`, `scene-marquee`,
`scene-handle-nw`/`-ne`/`-sw`/`-se`; `scene-inspector`,
`scene-layer-name`, `scene-pattern`, `scene-pattern-name`,
`scene-pattern-shape`, `scene-pattern-change`, `scene-controls`,
`scene-controls-label`, `scene-ramp`, `scene-ramp-stops`, `scene-ramp-stop`,
`scene-ramp-edit`, `scene-ramp-editor`,
`scene-ramp-color`/`-pos`/`-add`/`-remove`/`-amount`/`-clear`, `scene-box`
(+`-x`/`-y`/`-w`/`-h`), `scene-fit`, `scene-blend`, `scene-key`,
`scene-key-fixed`, `scene-opacity`, `scene-opacity-value`,
`scene-delete-layer`,
`scene-text-fixed`/`-clock`/`-slot`/`-lit`/`-fmt`/`-font`/`-color`/`-align`/`-scroll`/`-speed`,
`scene-align-l`/`-c`/`-r`, `scene-wash-color`, `scene-sprite-pick`,
`scene-sprite-menu`, `scene-sprite-edit`, `scene-sprite-new`,
`scene-sprite-state`, `scene-sprite-natural`, `scene-sprite-natural-btn`;
`scene-text-slot-n`, `scene-text-slot-hint`, `scene-text-slot-how`,
`scene-text-slot-value`, `scene-text-speed-value`, `scene-clock-state`,
`scene-clock-settings`, `scene-scroll-window`, `scene-font-menu`,
`scene-font-tiny`/`-regular`/`-large`.

The legal disabled control on this screen is `scene-add-pat` at
`caps.layers`, carrying `data-reason` with the same words shown beside it
(D4).

Sprites tab: `sprites-panel`, `sprites-empty-fixture`,
`sprites-preview-as-matrix`, `sprites-empty`, `sprites-lede`, `new-sprite`,
`sprites-grid`, `sprite-tile` (+`-open`/`-thumb`/`-edit`/`-edit-link`/`-menu`/
`-menu-popup`/`-name`/`-meta`), `sprite-menu-edit`/`-duplicate`/`-delete`,
`sprite-thumb`. Sprite editor: `sprite-editor-view`, `sprite-editor-header`,
`sprite-editor-back`, `sprite-name`, `sprite-name-input`, `sprite-name-error`,
`sprite-save-state`, `sprite-save`, `sprite-overflow`, `sprite-menu`,
`sprite-duplicate`, `sprite-delete`; `sprite-tools`,
`sprite-tool-pencil`/`-eraser`/`-fill`/`-pick`, `sprite-color`,
`sprite-inuse`, `sprite-inuse-swatch`, `sprite-inuse-count`,
`sprite-inuse-empty`; `sprite-stage`, `sprite-stage-dims`,
`sprite-canvas-wrap`, `sprite-canvas` (`data-zoom`); `sprite-frames`,
`sprite-frame-count`, `sprite-play`, `sprite-onion`, `sprite-frame`,
`sprite-frame-pick`, `sprite-frame-thumb`, `sprite-frame-add`,
`sprite-frame-ops`, `sprite-frame-left`/`-duplicate`/`-delete`/`-right`;
`sprite-inspector`, `sprite-name-echo`, `sprite-size`, `sprite-w`, `sprite-h`,
`sprite-frames-count`, `sprite-fps`, `sprite-bytes`, `sprite-preview-scale`,
`sprite-preview`, `sprite-usedby`, `sprite-saving`. The legal disabled control
there is `sprite-save` over the 16 KiB record cap, with its `data-reason`.

## The Settings page (`pages/Settings.svelte`, Gitea #469)

Proposal §5.3/§5.3b/§5.4d/§5.7, mockups S3, S3b–S3k. Nine equal-weight cards
became a ranked page: the three things people open Settings for are full
sections at the top, and everything else collapses into one `Advanced` list.

```
Settings    <device name> · vX.Y.Z        (never the base URL)
Device      Name (a text field) · Brightness — the page's FIRST control
LED layout  the summary + thumbnail, the kind picker, the fields, the
            arrangement, a collapsed `Panel module` row (HUB75 only), the
            Outputs table, the map link
Projection  its own section — present only where the Layout offers a choice
WiFi        the connected network + a collapsed `Change network…`
— Advanced —
  Output processing · Clock & time zone · Multi-device sync ·
  MQTT · Home Assistant · Network input · Storage · Firmware & recovery
```

Section rhythm is the mockups' (`web/src/settings/cards.css`): a 760px wrap
with 28px of top padding, a 22px/600 `h1`, 32px between sections, a 14px gap
under each `.secthead`, `.form` on the panel background at 16px padding, and a
132px label column. `Section.svelte` takes an optional `note`, the mono dim
line at the right end of the header row that names the fixture a section is
about (`64×64 matrix`).

Chrome is two primitives. `Section.svelte` is a small uppercase label, a
hairline rule and a `.form` panel — only FORMS get a background, which is what
kills the card-in-card look. `Disclosure.svelte` is one collapsible row:
chevron, title, and a **one-line status** so a collapsed page still answers
"is X on?" without being opened. A collapsed body is not mounted at all, so the
forms behind those rows cost nothing until someone looks. Seven of them are the
Advanced list; the eighth is `Panel module`, which sits inside the LED layout
form (below) wrapped in `.disclist.inline` — so it wears the same chrome
without pretending to be an Advanced setting.

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
the data pin keeps `/api/datapin`. That split was there because an `out` line
used to be reported as boot-built in its entirety; since Gitea #550 only what
a boot really builds says so (docs/api.md "Live vs reboot"), and output 0's
protocol and colour order on `/api/layout` answer `reboot_required:false`
like the aliases do. Folding them onto `out 0` is Gitea #524, named in the
code comment.

The section's own pieces:

| piece | what it is |
|---|---|
| the summary | `layoutLabel()` in big type + a live `PatternThumb` of the FIXTURE, so the shape comes from the geometry store and nothing here re-derives it |
| the kind picker | Strip / Matrix / **3D** / Custom map — **only where the board offers a choice**; a HUB75 board has none and the summary line carries the kind. `3D` is the one kind `/api/layout` does not name (see below) |
| the lattice fields | `w × h × d` with a live cloud thumbnail of what is about to be installed, and an `Install` button — picking `3D` changes nothing until it is pressed |
| `ArrangementSvg` | the panel chain: tiles, the path numbered from the `IN` connector, per-tile scan direction, the 180° markers, the total size, and output tinting. The same widget one level down (`mode="pixels"`) draws the pixel run through a strip-built matrix |
| the refresh readout | `panelRefreshHz()` (`lib/panelDriver.ts`): computed here from the CONFIGURED clock and bit depth wherever `/api/layout` reports a `driver` block (#401/#525) — the same formula the firmware runs, over inputs the form can change, so the number cannot lag the field just edited — and the device's own `matrix.est_hz` (#475) on firmware that reports no driver. Amber under 100 Hz with the fix named, the panel's live `rescan_hz` beside it, and the line under it names the driver it was spent on (`2 panels × 6 planes at 40 MHz`) |
| `Panel module` | a collapsed `Disclosure` under the arrangement, on a HUB75 board only (`vis.panelModule`): the scan rate and the four `panel` fields, i.e. everything printed on the back of a module. See its own section below (#778) |
| the dark-tile note | `matrix.drive` is how many leading tiles this board's framebuffer can shift out; past it the picture dashes them and the page says how many stay dark (#475/#401) |
| the reboot bar | see "Stored, but not running yet" below — the per-field note this row used to carry is gone |
| `OutputsTable` | one row per output when `caps.outputs > 1`, each computing the run it owns (`pixels 300–599`), plus the strip split graphic. `out` lines are all-or-nothing, so a row edit POSTs the whole table |
| `ProjectionBlock` | mounted by the PAGE now, in its own `Projection` section (below) |

### LED layout › Panel module is a form, not a plaque (#401/#525/#778)

**Where it is.** Inside the LED layout card, as one collapsed `Panel module`
row under the arrangement — not in Advanced. Jeremy, 2026-09-26: *"The panel
driver section doesn't belong in Advanced. That dropdown belongs closer or in
the LED layout section. It's not something people can optionally configure, but
once it is configured they probably won't touch it again, so being collapsed
still makes sense."* So it holds everything printed on the back of a module: the
**scan rate** (which rides the `matrix` line, so `LayoutCard` still owns the
write and `PanelModuleCard` dispatches a `scan` event to it) and the four
`panel` fields. `settingsVisibility().panelModule` gates the row AND decides
whether the size row reads `Panel` or `Size`. Roles: `panel-module-row`,
`-toggle`, `-status`, `-body`, then `layout-scan`, `panel-chip`, `panel-clock`,
`panel-planes`, `panel-blank`, `panel-state`, `panel-error`. The card's own
`Rescan … measured · … estimated` strip (`panel-state-row` / `panel-rescan` /
`panel-est`) is GONE with the move: the LED layout card's `Estimated refresh`
row sits four lines below the disclosure and is the same two numbers over the
same inputs, and printing them twice on one screen is what the move made
obvious. The verdict paragraph stays, because nothing else says it.

**The scan field names the ratios, never "board default"** (#778). It used to
offer `board default` for `scan 0`; Jeremy: *"Is that even possible for luxel to
know?"* — it is not. HUB75 is a write-only bus, and `0` does not mean "ask the
board", it means the usual ratio for this height, `ph / 2`. So the options are
the ratios a module prints on its back — `1/32 (usual for 64 rows)`, `1/16`,
`1/8`, `1/4`, those that divide `ph / 2` — the usual one is marked inline (a
tried-and-rejected alternative was `Standard (1/32 for a 64-tall panel)`, which
he called confusing), and picking it sends `0` so the stored value still follows
a later height change. A 32-row module is not offered `1/32` at all: 32 does not
divide 16. `scanOptions()` / `scanWire()` / `scanShown()` in
`lib/panelDriver.ts`, unit-tested.

**Latch blanking applies live** (#778): *"Let's make latch blanking dynamic. I
see value in that one."* It is control bits in the framebuffer words, so the
firmware re-formats its buffers between frames and the device answers
`reboot_required:false`. Two web consequences, both in `lib/panelDriver.ts`:
nothing calls `noteRebootPending` for it (the DEVICE's reply decides, as
always), and `panelDriverState()` leaves `blank` OUT of the configured-vs-live
comparison — `live.blank` legitimately lags the POST reply by one frame, and
comparing it would print "reboot to apply" under the one control that does not
need one. A blanking wide enough to swallow the OE window is refused by the
device with the numbers, and `apiErrors.ts` gives that its own sentence on
`panel-blank`.

The HUB75 driver's bit depth, pixel clock, chip-init sequence and latch
blanking used to be this firmware build's constants, and the card said so.
They are settings now: `/api/layout` reports a `driver` block and takes one
wire line back,

```
panel <planes> <clock_mhz> <chip> <blank>
```

which the firmware merges into the stored Layout and applies at the next boot —
except `blank`, which it applies on its next frame (above). Every field POSTs
that one line through the same `applyLayout()` path the LED layout form uses,
adopts the reply, and sends a `reboot_required` reply to the sticky reboot bar —
the panel's framebuffer and its LCD_CAM clock are built at boot, so on a panel
board the `matrix` line's pw/ph/chain/scan are reboot-required too, and the UI
takes that from the REPLY rather than assuming. That is what makes one live
field cost no special case here.

The block carries two readings of the same four values, and the card's job is
to say which one is on the panel:

```json
"driver": { "planes": 7, "clock_mhz": 30, "chip": "shiftreg", "blank": 1,
            "chips": ["shiftreg","fm6126a","icn2038s","dp3246"],
            "clocks": [8,10,12,15,20,24,30],
            "live": { …, "w": 64, "h": 64, "scan": 32, "fb_bytes": 28672, "fallback": false } }
```

`lib/panelDriver.ts` is the pure module that decides, and
`web/tests/panelDriver.test.mjs` tests it over fixtures — the interesting
states are exactly the ones a healthy bench panel never shows:

| `panelDriverState()` | when | what the card says |
|---|---|---|
| `live` | configured == `live`, geometry included | running exactly what is set here |
| `pending` | `planes` / `clock_mhz` / `chip`, or pw/ph/chain/scan, differ — **never `blank`** (#778) | **Reboot to apply**, naming what is still running and what waits |
| `fallback` | `live.fallback` | the configured driver did not fit in internal RAM; the board default is running — lower the bit planes or the panel size, a reboot alone will not fix it |
| `disabled` | `live` is `null` | panel output is off: no framebuffer came up at all |
| `unknown` | no `driver` block | the row is caps-gated to panel boards, so this is a **console newer than the firmware**: the card says exactly that and offers nothing (#771). `PANEL_DRIVER_DEFAULT` survives only as the refresh ESTIMATE's fallback |

The collapsed row states the whole module and carries the same verdict —
`1/32 scan · plain shift register · 20 MHz · 7 planes · blanking 1`, plus
` · reboot to apply` / ` · not applied` / ` · output off`, and
`1/32 scan · firmware too old` in the `unknown` state (never this build's
constants dressed up as the device's) — because a row nobody opens is the only
place that fact would otherwise not appear. It carries no refresh rate: the
estimated/measured readout sits immediately beneath the row since #778, and
printing the same number twice on one screen is worse than not printing it.
`panelModuleLine()` builds it. The chip `<select>` is built from
`driver.chips` and the pixel-clock `<select>` from `driver.clocks`, never from
a list in the browser, so a firmware that learns a new chip or offers a new
clock needs no web change.

**The pixel clock is a dropdown, and `unknown` is a stated mismatch** — both
Gitea #771, and both the same incident. 40 MHz was reachable as a typed number
with an amber warning beside it; Jeremy typed it and the panel mis-sampled, so
the clock is now a fixed list (`clockChoices()` / `snapClock()` in
`lib/panelDriver.ts`, the device's `driver.clocks` with this build's
8/10/12/15/20/24/30 only as a fallback). A stored value the device no longer
offers is still shown, marked `(not supported)`, so a rollback cannot read as a
different setting. And the `unknown` branch used to draw a read-only plaque of
`PANEL_DRIVER_DEFAULT` — which is how that rollback looked "fixed": plausible
numbers, no controls, no explanation. It now reads *"This console is newer than
the firmware on the device (it reports no panel driver). Push matching firmware
— Settings › Firmware & recovery."*

A refused `panel` line surfaces twice: in `ErrorBar` as always (Jeremy's rule —
an `/api/layout` reply is a banner, never a line at the foot of a form) and, in
this card only, inline beside the fields as `[data-role="panel-error"]`, because
the whole line is about one control. What it must NEVER be is the table's
"this is a bug in the app — please report it" copy: a firmware with no `panel`
verb answers `unknown line (want strip|matrix|map|out|…)`, and
`lib/apiErrors.ts` now reads that want-list and says the firmware is older than
the console instead.

### A 3D lattice, and why it takes two POSTs

`/api/layout`'s `kind` is `strip`/`matrix`/`map`; a 3D lattice is a
coordinate map, so the wire reports it as `map` with `dims: 3`.
`uiLayoutKind(wireKind, dims)` in `lib/settingsCaps.ts` is the ONE place that
turns that back into the picker's `lattice`, and `latticeDimsOf(coords)` in
`lib/geometry.ts` is what recovers the `w×h×d` — a cloud that IS a lattice is
reported as a **regular** 3D Layout, so it reads `8×8×8 lattice · 512 pixels`
rather than `512 px custom map`, and `thumbLayout` can subsample it.

Installing one (`installLattice()` in `stores/device.ts`) is two POSTs:

1. `strip <w·h·d>` — sizes the pixel space. A COORDINATE map does not resize
   anything (only the procedural `map grid W H` form does,
   `crates/luxel-core/src/layout.rs`), and a lattice installed into a smaller
   pixel space is silently truncated by the engine.
2. `map 3 <coords…>` — the coordinates. The grammar takes at most one
   `strip`/`matrix`/`map` line per body, which is why this cannot be one
   request.

The reply to (2) can still carry the OLD pixel count — a resize lands on the
host's render loop — so this re-reads `/api/layout` instead of adopting it.

**The size ceiling is the request buffer.** A device serves each connection
out of one 4 KiB buffer holding the request line, the headers and the body
together (`firmware/src/server.rs`), and a coordinate map that does not
arrive intact is treated as "clear the map" (docs/api.md). So
`latticeMapLine()` writes the lattice INDICES (`0 0 0`, `1 0 0`, …) rather
than 16.16 fractions of 1.0: the engine normalizes a map per axis before
anything reads it (`Engine::set_map_vec`), so the two install the identical
lattice, and indices are what decides whether it fits — 8×8×8 is **3,077
bytes** as indices and 8,257 as fractions. `LAYOUT_BODY_BUDGET` is 3,500 B
and `maxLatticeSide()` derives the cap from it: **8 per side, 512 pixels**,
today. Gitea #548 is the procedural `map lattice W H D` form that would lift
it. The `w × h × d` fields cap at that side, and a lattice over it (or over
the board's pixel ceiling) disables `Install` with a `data-reason` — the one
legal disabled control, a budget the user has to learn.

The browser remembers the lattice it installed (`luxel.layout.lattice` in
localStorage) and believes it again only when the device's own pixel count
still matches: `GET /api/map` reports a COUNT, so without that a reload turns
`8×8×8 lattice` back into `512 px custom map`.

### Projection is its own section (mockups S3e–S3h)

`ProjectionBlock` used to hang off the bottom of the LED layout form, with no
rule and no heading of its own. It is a sibling `Section` now — it is about
PATTERNS, not about wiring — with the fixture in its header note and an 18px
intro line above the rows.

One row per pattern kind that is not native here, and a live `ProjectionCard`
per option; labels and options come from the ENGINE
(`Luxel.projectionOptions`), which mirrors
`crates/luxel-core/src/projection.rs`. Since #538 **a Layout never shows a
pattern bigger than itself**, so the whole table is:

| Layout | rows the section shows |
|---|---|
| 1D (strip) | none — **the section is absent**, with no copy explaining why |
| 2D (matrix, 2D map) | `1D patterns`: By index · Along x · Along y |
| 3D (lattice, 3D map) | `1D patterns`: By index · Along x · y · z, and `2D patterns`: Repeat along z · y · x |

`settingsVisibility().projection` is that rule (`layout.dims > 1`).

`ProjectionCard` has two forms, chosen by the fixture's shape: the 120px tile
(`.pcard`, S3e/S3g/S3h) and, on a 1D fixture, the 200×24 row (`.barcard`,
S3f). A 40×40 SVG glyph appears where the picture alone cannot say which cut
it is — the cube badge on a slice, the grid/cube diagram beside a bar. Its
props are a contract (`luxel`, `layout`, `patternDims`, `mode`, `label`,
`selected`, `active`, and the `select` event): the editor's per-item
projection popup mounts the same component.

### Stored, but not running yet — the reboot bar (`settings/RebootBar.svelte`)

Some settings are built once at boot: the chain arrangement (#475), an
output's driver INSTANCE — whether it exists, its data pad, and a further
output's wire format (#474) — and the device name (its DHCP hostname). The
device says so itself — `reboot_required` on `/api/layout` and `/api/name` —
and `noteRebootPending(field)` in `stores/device.ts` records the FIELD, never
the UI's guess about what is live. Protocol and colour order on output 0, and
every output's run (`count`, `rev`), are live on both hosts and never appear
(#550).

`<RebootBar/>` is mounted by `App.svelte`, not by Settings: it is pinned to
the bottom of the viewport in the `--warn` palette on **every** screen, the
editor included, until the device reboots — the user changes a data pin and
walks off to Patterns, and the device is still running the old wiring
wherever they are. `Reboot now` is `POST /api/reboot` behind
`confirm({reboot: true})` and, like everything else, is absent rather than
disabled where `caps.reboot` is false (the mirror), which leaves the power
cycle to the human. The list is cleared by a reboot and by nothing else.

### Clock & time zone

A zone is a place, not a number. The select is built from
`Intl.supportedValuesOf("timeZone")`, grouped into `<optgroup>`s by region;
picking one computes its CURRENT offset (DST included) with
`Intl.DateTimeFormat(…, {timeZoneName: "longOffset"})` and POSTs
`tzMinutes` exactly as before — **no firmware change**, because a board with
2 KB of NVS is never going to carry the IANA database. The zone NAME is the
browser's memory of which place that offset came from
(`luxel.clock.zone` in localStorage) and is believed again only when its
offset still matches what the device reports.

Device time is `toLocaleString(undefined, {timeZone, dateStyle, timeStyle})`
of the UTC instant (`local − tzMinutes`), so it is a date AND a time in the
looking user's locale. `Sync now` is `POST /api/clock/sync`, which is
**asynchronous** on firmware — the reply is the clock as it stands, so the
button re-reads `/api/clock` after it.

`data-role` contract: `settings-panel` · `settings-subtitle` ·
`sect-{device,layout,wifi,projection}` (+ `sect-<x>-section`,
`sect-<x>-note`) ·
`advanced` · `adv-<row>-{row,toggle,status,body}` · `brightness` ·
`device-name`, `device-name-note` · `layout-{summary,headline,subhead,kind,lat-w,lat-h,lat-d,lat-count,lat-install,lat-note,pixels,pw,ph,scan,
cols,rows,start,dir,snake,rot180,proto,order,datapin,datapin-apply,notes,note,
dark,map-link}` · `reboot-bar`, `reboot-bar-text`, `reboot-now` ·
`clock-{status,sync,tz,offset,note}` · `wifi-address` · `arrangement` (with `data-mode`) · `refresh`, `refresh-hz`,
`refresh-measured` · `outputs`, `output-{row,pin,proto,order,count,rev,range,
add,remove}` · `projection-block`, `projection-kind` (with `data-dims`),
`projection-card` (with `data-mode`) · `wifi-change` ·
`panel-{clock,clock-warn,planes,chip,blank,rescan,est,state,state-row}` ·
`storage-{patterns,bytes,heap,psram}` ·
`fw-{version,update,file,note}` · `api-error-{bar,text,details,dismiss}` ·
`device-down-bar`.

### `data-reason` marks the one legal disabled control (Gitea #529)

"Absent, never disabled" has exactly one exception in the proposal: a control
gating a **budget the user has to learn** — `out-palette-add` at its 32-stop
cap, and `layout-lat-install` at the lattice a single request can carry
(#538). Such a control stays `disabled` AND carries `data-reason` with
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
`arrangement`, `estimatedRefresh`, `outputsTable`, `projection`,
`latticeFields`, `powerCap`, `blurGlow` (+
`blurGlowScope`, which words it "along the strip" or "across the grid"),
`panelDriver`, `psram`, `ota`, `reboot`, and the rest. `caps === null` (firmware
older than #464) falls back to `FALLBACK_CAPS` — what every build has always
had — rather than to a guess.

`lib/panelDriver.ts` is its sibling for the HUB75 driver block — the
`panel` wire line, the chip labels, the live-vs-configured verdict and
`panelRefreshHz()` (above).

The module also owns the pure arithmetic the section draws with:
`estimatedRefreshHz()` (whose `PANEL_DRIVER_DEFAULT` parameter is now only the
fallback for firmware that reports no driver), `chainOrder()` (the tile order the SVG numbers —
tiles line by line, a line being a tile row under `dir: "row"` and a column
under `"col"`, `snake` reversing the odd lines and `rot180` marking their
tiles as mounted upside-down; the same walk #475's boot-time remap does),
`outputRanges()`, the time-zone helpers (`zoneOffsetMinutes`, `zoneLabel`,
`zonesByRegion`, `offsetLabel`) and
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
the compiled pattern's dims  `patternDims` (patternDims())      │
projection defaults          `projection` (the device's)       ─┘
```

On a console only the FIRST row is read — see `deviceGeometry()` below.

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

- **Console**: the device owns the geometry, and **`deviceGeometry()` in
  `lib/geometry.ts` is the ONE function that decides it** (#539, #573). It
  takes nothing but DEVICE readings — `/api/layout`, `/api/status`'s `geom`,
  the pixel count, `/api/map` — so nothing the browser holds is even a
  parameter. Precedence: `/api/layout` (#465, since A8/#469 read **wholesale**
  — kind, dims, w/h, the chain's wiring and the embedded map in one fetch)
  always wins; then `/api/status`'s `geom` (#464), which answers during the
  connect handshake and is all there is on older firmware, but **only its
  fixture readings** (`source` `user`/`board`); then `deviceMap`; then the
  pixel count, i.e. a strip. `stores/device.ts`'s `deviceLayout` is wire
  parsing over that function and decides nothing itself.

  Two inputs used to leak past that and re-shape a console:
  - the persisted "Preview as" choice (#539) — the playground's control, but
    one left behind by a playground session on the same origin (or by the
    pre-v2 editor's layout select, which wrote the same key) re-shaped the
    console for good: a stored `map` choice with no coordinates reconciled a
    64×64 panel down to a 4096 px strip. `reconcileLayout` now returns inside
    its `connected` branch, so not one playground input is reachable on a
    console — before the handshake answers it is the starter strip, never the
    pattern's shape.
  - `geom.source:"default"` (#573) — the grid the ENGINE fabricated for the
    program it was handed (ceil(√n) × ceil(n/w) on a board with no map). The
    boot used to resume a DIRTY working copy and live-push it (`bootDevice`),
    so a browser holding a `render2D` edit from an earlier session made
    a 300 px strip report itself an `18×17 matrix` — chip, tile shapes,
    Settings projections and the #538 filter all followed. The Layout fix is
    here; the push that caused it is gone too (#585, the push rule above), so
    a boot can no longer create the state at all. A running program
    never reshapes the fixture; it may only caption ITSELF (`captionFor`,
    `effectiveFor`), and on the Patterns page it may appear in the
    `Not for this layout` group in the playground's Auto style.
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
dimensionality from the ENGINE (`patternDims()`); `gen-gallery.mjs`'s `kind`
is an advisory hint that saves a second compile and is allowed to be wrong.

Painting lives in `lib/draw.ts` (`paintBar` / `paintGrid` / `paintPoints`), so
the editor preview, the gallery tiles and the row thumbnails cannot drift.

A scatter (a custom map or a 3D lattice) paints **every unlit point first**,
then the lit ones, each group still back-to-front (`paintOrder`, unit-tested in
`web/tests/draw.test.mjs`). Depth alone is the right rule for opaque dots of a
solid object, but a pixel that is OFF is not part of the object: sorted by
depth it legitimately lands in front of a lit neighbour and paints a black
disc over it, which is the "black scatter plot dots draw over other dots"
Jeremy reported on the map preview (#538). The flat 2D case had the same bug in
a simpler form — no depth at all, so index order was paint order.

The device's real wiring arrived with that switch: `/api/layout`'s
`matrix.snake` fills `serpentine`, so a console previews a snaked matrix the
way the fixture shows it rather than row-major. Per-item projection overrides are #470/#473's.

## Errors and liveness — one strip at the top (Gitea #538 round 2)

Two questions have the same answer — *did that work?* — so they share one
surface: `components/ErrorBar.svelte`, mounted by `App.svelte` above every
screen (the editor included, which is why it is not in a page). It is the
`RebootBar` family in the `--error` palette; `RebootBar` is at the BOTTOM in
`--warn` because it reports something that IS stored, this one reports
something that did not happen.

| condition | store | behaviour |
|---|---|---|
| the device is not answering | `deviceDown` / `deviceLastSeen` | `Device unreachable — retrying… last seen 12 s ago`, clocked live, no ✕ — it clears itself, and a `reconnected` note on the `device` channel marks the return |
| a request was refused | `apiError` (`reportApiError`) | the translated sentence, the device's own words under it, a ✕, and the field it is about marked `data-field-error` |

The connectivity banner wins when both are true: a refusal explained by a dead
board is one fact, not two. Neither exists at rest, so nothing the mockups
measure changes — `[data-field-error]` is one rule in `app.css` and is never
present on a resting page.

**Translation is a table, not prose at the call site.** `lib/apiErrors.ts`
(`explainApiError(raw, ctx)`, tested in `web/tests/apiErrors.test.mjs`) maps
each `luxel_core::layout` grammar error — the vocabulary the firmware AND the
mirror share — onto a sentence built from what the FORM knows: the board's
ceiling, the chain the user typed, the field. `pw*ph*cols*rows out of range
for this board` becomes *"8,192 px — this board tops out at 4,096. The panel's
two bitplane DMA frame buffers (28 KB each) must live in internal SRAM, and
the DMA cannot read them from PSRAM fast enough (it needs 60 MB/s); a wider
chain needs new firmware (#599), not a setting. At this size you can arrange
two 32×64 tiles (`matrix 32 64 2 1 tr row 0 0`)."* The raw text is never
dropped — it is the `details` line. `LayoutCard` pre-checks the chain against
`max_pixels` before POSTing (#600), the way `latOverMax` already does for the
lattice, so the explanation arrives with the numbers and nothing is sent that
cannot land.

**Liveness is measured in the gate.** `lib/fetchgate.ts` sees every request
the app makes, so it counts consecutive transport failures (an HTTP error
status is the device *answering* and resets the count) and publishes
`subscribeGate(fn)`; `stores/device.ts` turns that into `deviceDown`. Two
knobs come with it:

- `gatedFetch(url, init, { fastFail })` drops the retry ladder and uses a 4 s
  deadline. `/api/status` uses it (with `force`, so the probe that would clear
  the latch is never the request the latch blocks): the 1 Hz poll is the
  heartbeat, and it has to report a dead board in about a second rather than
  spending ~30 s of patience in silence.
- while the gate says the device is down, a **write** (anything but GET/HEAD)
  fails immediately instead of retrying into a corpse. Reads keep the full
  ladder — that patience is what keeps a cold load alive on a two-socket board
  (#92/#592).

Every optimistic write then has to roll back: a failed playlist POST re-reads
the device's own list and says `playlist not saved` (`playlistWriteFailed` in
`stores/device.ts`), and a refused activation reports through the banner.

## Version skew is a shell banner, and the repair is automatic (#643)

Two numbers meet at a device: the LXBC format this bundle's compiler EMITS
(`Luxel.bcFormat()`, from `lx_bc_format()` in luxel.wasm) and the one the
firmware READS (`/api/status`'s `bc_format`). They ship together and normally
agree. A firmware OTA installed without the matching web assets pulls them
apart, and on the Athom that meant a dark strip and a console whose every save
was refused.

`components/BcBanner.svelte` is mounted once in the shell, directly under
`ErrorBar`, and renders **nothing** when the two agree and no stored blob is
behind. It is under ErrorBar for the same reason ErrorBar is in the shell at
all: the editor is full-screen, and "every save is being refused" has to be
explainable from inside it.

The rules are pure modules, so they are stated once and tested without a
browser:

| module | what it decides | test |
|---|---|---|
| `lib/bcskew.ts` | `bcSkew(bundle, device)` → `match` / `bundle-older` / `bundle-newer` / `unknown` (a missing number on either side is NEVER a match), the banner wording, and `stalePatternIds()` — the `stale` row flags when the firmware reports them, else the `vmerr` / playlist-`invalid` text | `tests/bcskew.test.mjs` |
| `lib/heal.ts` | the repair: fetch each stale pattern's source, compile it here, save it back **by name** so the id and every playlist reference survive. Idempotent, resumable, leaves a pattern that no longer compiles exactly as it was and lists it | `tests/install.test.mjs` |
| `lib/install.ts` | the install sequence (app → reboot wait → assets, never the other order), the board check, and `rebootSettled()` | `tests/install.test.mjs` |
| `lib/luxr.ts` | the `.luxr` container codec, shared with `tools/pack-luxr.mjs` and the release workflow | `tests/luxr.test.mjs` |

The one rule that matters most: **the console never recompiles a store while
its own compiler is older than the device.** That case is a banner carrying
the web-asset upload, not a repair — recompiling with the wrong compiler
would replace unreadable blobs with equally unreadable ones.

Those four modules are loaded directly by `npm test` through node's type
stripping, which does no extension guessing — so they spell their sibling
imports `./luxr.ts`, and `tsconfig.json` carries `allowImportingTsExtensions`
(with its prerequisite `noEmit`; vite/esbuild does the emitting anyway) to
let tsc agree. Everything else in `src/` keeps the extensionless form.

`device-e2e.mjs` drives all of it against `luxel serve`'s impersonation flags
— `--bc-format` for either direction of skew, `--stale-store` for a store the
bump aged, `--accept-ota` for the Update… flow (docs/api.md).

## Activation parks the playlist (Gitea #538 round 2)

`activateDevicePattern()` is THE activation verb — `Play` on a tile, the
Patterns page's play path and the editor's `▶ Play on device` all reach it —
so the park lives in it and every caller gets it for free. A direct play is a
takeover of the fixture, and a playlist that keeps auto-advancing replaces the
chosen pattern a few seconds later with nothing on screen saying why. So the
verb stops the auto-advance the way `‖ Pause` does (`POST /api/playlist/stop`
with the index remembered, #549's park) and records the pattern that took over
in `playlistPreemptedBy`. The transport then reads
`stopped — playing <name> directly` (`pl-preempted`) and `▶ Play` resumes the
parked item; any transport verb clears the explanation, because pressing one
is the user taking the playlist back.

This is a CLIENT fix. `POST /api/patterns/<id>/activate` from Home Assistant,
MQTT or curl still leaves the firmware's playlist running — **Gitea #602**,
named in docs/api.md.

## Reordering the playlist is live (Gitea #538 round 2)

The rows reorder VISUALLY while the pointer is down; the store and the POST
change only on release. HTML5 drag-and-drop could not do that — its `drop` is
the first moment anything is known, the drag image is the browser's, and it
does not exist on touch — so it is pointer events plus one FLIP-ish rule, with
`pages/Playlist.svelte` as the only thing that knows the geometry:

- the grabbed row gets `lifted` (shadow, 1.012 scale, `position:relative`) and
  `translateY(pointer travel)`;
- every row between its origin and the slot it is over gets
  `translateY(∓ the grabbed row's outer height)` — the hole;
- `transform` only, with a 120 ms transition on the rows that are not being
  dragged, so nothing reflows and nothing is measured twice;
- geometry is measured ONCE at grab time, off `PlaylistRow.els()` — a row is
  TWO elements whenever its values band is open, and `outer[i]` is the distance
  to the next row's top, so the 8 px card margin is in the number;
- Escape (or a cancelled pointer) snaps the target back to the origin and
  animates the lifted row home before the drag state is dropped;
- `touch-action: none` is on the **handle only**, so a drag from anywhere else
  still scrolls the list;
- the 1 Hz follow poll is suspended for the length of a drag: it would replace
  `items` under a layout that was measured before it.

`PlaylistRow`'s resting styles are untouched — `lifted`/`anim` are classes
that only exist mid-drag, and `shift` is removed from the style attribute at 0
— so mockdiff S4/S4b stay at zero. The handle's ↑/↓ keyboard reorder is
unchanged. New `data-role`s: none; the drag is asserted through
`data-lifted` on the row and computed `transform`.

## A refresh of the device library keeps the sources it has (Gitea #538 round 2)

`GET /api/patterns` answers ids and names only; each source is fetched
separately and streams in. `refreshDevicePatterns()` used to replace
`devicePatterns` with that bare list, which told `Gallery.syncItems` that every
pattern's source had just changed — and it did the only correct thing with
that: freed every tile engine and recompiled. Deleting ONE of N patterns
re-fetched and re-compiled the other N−1.

It now MERGES by id, keeping the source each surviving row already had. A row
whose content changed under the same id — a save that overwrote a name — is
not something the wire can tell us (no hash, no mtime), so the writer says so:
`refreshDevicePatterns([id, …])` drops those cached sources and nothing else's.
Both save paths (`pages/Editor.svelte`, `saveAndAddToPlaylist`) look the
overwritten id up by name before saving and pass it.

`Gallery` stamps `data-compiled` (a per-tile compile counter) on every tile, so
the harness can prove a survivor's engine was not rebuilt rather than
inferring it from a screenshot.

## Store reference

`stores/device.ts` — `device`, `deviceBase`, `isPlayground`, `mode`,
`deviceError`, `deviceBlocked`, `devicePixels`, `pixelMax`, `deviceHeapFree`,
`deviceEngineHeap`, `deviceVmerr`, `deviceFps`, `deviceOutFps`,
`deviceRescanHz`, `devicePsramFree`, `devicePsramTotal`, `deviceDown`,
`deviceLastSeen`, `deviceMap`, `deviceLayoutWire`, `deviceCaps`,
`deviceVersion`, `deviceSlot`, `deviceStore`, `deviceName`, `deviceLabel`,
`devicePatterns`, `deviceRunningId`, `brightness`,
`brightnessMax`, `deviceProtocol`, `protocolOptions`, `dataPin*`, `wifi*`,
`mqtt*`, `outputStatus`, `palette*`, `clockStatus`, `syncStatus`, `netLive`,
`playlist`, `playlistParked`, `playlistPreemptedBy`; functions `connectDevice`, `detectDeviceBase`, `refresh*`,
`activateDevicePattern`,
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
`patternName`, `exampleName`, `devicePatternId`, `livePush`, `controlValues`,
`projectionOverride`, `hints`,
`mapSrc`, `MAP_PROGRAM_TEMPLATE`, `NEW_PATTERN`, `newPatternSource()`, `previewFps`, `runtimeError`, `saved`,
`saveToLocalLibrary`, `deleteFromLocalLibrary`, `findSaved`, `startAutosave`,
`stopAutosave`, `loadWorkingCopy`, `compileToBytecode`, `parseEpe`,
`exportEpe`, `encodeShare`, `decodeShare`.

`stores/scenes.ts` — `scenes`, `activeSceneId`, `sceneBytes`, `layerCap`,
`sceneSaving`, `refreshScenes()`, `sceneById()`, `saveScene()`,
`deleteScene()`, `duplicateScene()`, `activateScene()`, `livePushScene()`,
`shouldLivePush()`, `cancelLivePush()`, `startScenePoll()`, `newScene()`,
`randomSceneId()`, `nextCopyName()`, `isSceneId()`, `FALLBACK_LAYER_CAP`.
See **Scenes** above.

`stores/notify.ts` — `notes`, `note()`, `clearNote()`, `noteStore()`,
`banners`, `setBanner()`, `clearBanners()`, `apiError`, `reportApiError()`,
`clearApiError()`.

## Invariants worth not breaking

- **The preview is local; the device is a sink.** The editor compiles with the
  local wasm engine and pushes source + LXBC to `/api/code`. There is no pixel
  stream back (removed at Jeremy's request); only the connect handshake reads
  the device's running pattern.
- **Nothing reaches the device unless the user asked for it** (#563). Browsing
  a pattern, opening one in the editor, switching the Layout and reloading the
  page all leave the LEDs alone; `Play`, `▶ Play on device`, `Save`, the
  playlist transport and typing into a pattern the device is already running
  are the writes. `livePush` is the gate — see the push rule above.
- **`data-role` attributes are the e2e contract.** `web/tools/{e2e,device-e2e,
  maxpixels-e2e,sync-e2e,flash-e2e,coldload,lna-e2e}.mjs` drive them. Moving
  markup between components is fine; renaming or dropping a role is not,
  without updating the harness in the same commit. The last two are the ones
  that get forgotten, because they run against a REAL device rather than a
  mirror and so never ran in the phase that broke them: `coldload.mjs` spent
  A7–A10 reporting `boot FAILED` on healthy loads because it read a label the
  editor's back button no longer carries. A harness's own assertion can be the
  only thing failing — read its zero-failed-request line before believing it.
- **Bundle shape is load-bearing** (Gitea #92, #592): `dist/*.html` must ask
  the browser for NOTHING — no `<script src>` tag, no stylesheet link, no
  `modulepreload`. A browser-native request cannot go through `fetchgate`, so
  nothing retries it, and it needs a socket of its own while the document is
  still arriving. Both halves bit: a refused stylesheet rendered the console
  as unstyled HTML, silently; and with the panel's pool at `"web":[1,1,1]`
  even a single remaining `<script src>` tag was refused on every cold load.
  `cssCodeSplit: false` is what lets a page component or a plain `.css`
  import (e.g. `settings/cards.css`) land in a single stylesheet; the
  `inlineBoot()` plugin in `vite.config.ts` then inlines that stylesheet into
  both entry HTMLs as a `<style>`, drops the asset, and replaces the module
  tag with a loader that appends the script after `DOMContentLoaded` — so it
  reuses the document's keep-alive socket — and re-appends it up to three
  times (2/4/6 s) if it is refused. A whole cold load, bundle and wasm and
  every `/api/*` included, then fits in one socket. Cost: the CSS ships twice
  (once per entry) and loses its own immutable cache entry — about +10 KB
  gzipped on the packed asset archive. `web/tests/bundleShape.test.mjs`
  guards the shape; `tools/coldload.mjs` (styled + booted) and
  `tools/bootretry-check.mjs` (the retry) guard the behaviour.
- **The bundle has a hard ceiling: 983,040 B packed**, the `assets` partition
  on every board (docs/boards.md), gated by `tools/ci.sh` — a build over it
  FAILS, and `POST /api/assets` refuses the install. So the build is tuned for
  size in three places, all of them now spent (Gitea #683): `npm run wasm` is
  `web/tools/build-wasm.sh` (`--profile wasm-release` + `wasm-opt -Oz`),
  `build.minify` is `"terser"` with two compress passes rather than esbuild,
  and `tools/pack-assets.mjs` gzips with zopfli. **Not `drop_console`**: the
  e2e harness fails a run on any page-level `console.error`, so dropping those
  would disable an assertion rather than save much. Two thirds of what is left
  is content — `gallery.json` (307 pattern sources, shipped verbatim on
  purpose) and the CodeMirror editor — so the cost of a new surface is
  measured, not estimated: `cd web && npm run build && node
  tools/pack-assets.mjs /tmp/x.luxa`.
- **`$:` only tracks what appears in its own syntax.** Name every dependency in
  the block, not just inside the function it calls. And a `$:` whose input is
  assigned *inside a function another reactive block calls* can render one
  cycle stale — assign the derived values in that function too
  (`components/PatternThumb.svelte`'s `adopt()`), or it will draw the previous
  Layout's shape while holding the new Layout.
- **Geometry comes from `stores/geometry.ts`.** One `layout`: the device's on a
  console, the "Preview as" choice × the compiled pattern's dims in the
  playground. No component compiles at a pixel count of its own or installs a
  map of its own — see the Geometry section above.
- **One phone breakpoint: `@media (max-width: 600px)`.** Every responsive rule
  in `web/src` hangs off it — the Patterns grid, the Gallery tiles, the
  Playlist page and its rows, the pattern picker. Mobile is a soft requirement
  met by RESTACKING, never by a second flow (CLAUDE.md), so a new surface
  reuses this number rather than inventing one. `Dialog.svelte`'s 420 px is
  not a second breakpoint: it is the width at which two side-by-side buttons
  stop fitting. Targets under it are thumb-sized (≥ 32 px) and no page may
  scroll sideways at 390 px — `device-e2e.mjs` asserts both on the Playlist.
- **One `.btn`, one `.menu`, one Popover.** A new button wears `.btn` (+ a
  modifier) and a new dropdown mounts `components/Popover.svelte`; a local
  copy of either is how the four different `.primary` blocks and the three
  divergent menu stylesheets happened (#538). The mockups are the spec —
  quote a number from `docs/design/webui-v2/mockups.html` rather than
  eyeballing one.
- **The route names a screen, never a device action.** A refresh reopens the
  page you were on and changes nothing on the hardware. See **The URL** above.
- **No native dialogs.** `window.prompt` / `window.confirm` / `alert` do not
  appear anywhere under `web/src` — naming and confirmation go through
  `stores/dialog.ts` (#472). A native dialog also hangs the e2e harnesses,
  which deliberately install no `page.on("dialog")` handler.
