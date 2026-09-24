<script lang="ts">
  // The shell: mode (playground / device console), which home tab is open,
  // whether the full-screen editor is over it, the boot cover, the header and
  // the browser-blocked banner. Everything else lives in ./stores (state) and
  // ./pages (surfaces) — see docs/web-architecture.md.
  import { onDestroy, onMount } from "svelte";
  import { get } from "svelte/store";
  import DeviceChip from "./components/DeviceChip.svelte";
  import Dialog from "./components/Dialog.svelte";
  import HeaderBrightness from "./components/HeaderBrightness.svelte";
  import PreviewAsChip from "./components/PreviewAsChip.svelte";
  import type { Luxel } from "./lib/luxel";
  import { parseRoute, pushRoute, replaceRoute, type Page, type Route } from "./lib/router";
  import Editor from "./pages/Editor.svelte";
  import MapEditor from "./pages/MapEditor.svelte";
  import Patterns from "./pages/Patterns.svelte";
  import Playlist from "./pages/Playlist.svelte";
  import Scenes from "./pages/Scenes.svelte";
  import SceneEditor from "./pages/SceneEditor.svelte";
  import Settings from "./pages/Settings.svelte";
  import { uiLayoutKind } from "./lib/settingsCaps";
  import { refreshScenes } from "./stores/scenes";
  import BcBanner from "./components/BcBanner.svelte";
  import ErrorBar from "./components/ErrorBar.svelte";
  import RebootBar from "./settings/RebootBar.svelte";
  import {
    connectDevice,
    detectDeviceBase,
    device,
    deviceBase,
    deviceBlocked,
    deviceLayoutWire,
    deviceFps,
    deviceJit,
    deviceOutFps,
    deviceRescanHz,
    isPlayground,
    mode,
    pollStopAll,
    refreshPlaylist,
    startSessionPoll,
  } from "./stores/device";
  import { layout, pixelTotal, runMapProgram, setPreviewAs } from "./stores/geometry";
  import { setBanner } from "./stores/notify";
  import {
    decodeShare,
    dirty,
    exampleName,
    loadLuxel,
    loadWorkingCopy,
    mapSrc,
    patternName,
    previewFps,
    source,
    startAutosave,
    stopAutosave,
  } from "./stores/pattern";

  // ---- navigation ----
  // Home tabs (proposal §4): `Patterns` always, `Scenes` by Layout kind (D10,
  // §5.4c — see `tabs` below), `Playlist` + `Settings` on a console. The
  // editors are NOT tabs: each opens full-screen over the home tab, with a
  // back button. `tab` is the home you return to.
  type Tab = "patterns" | "scenes" | "playlist" | "settings";
  let tab: Tab = "patterns";
  /** Full-screen editor open (over the home tab). */
  let editing = false;
  /** The map program's screen, open over everything else (A10, Gitea #471).
   *  It is not a tab either: it is reached from the Layout picker — the
   *  playground's "Preview as" chip, or Settings → LED layout on a console. */
  let mapEditing = false;
  /** What the map screen's back button says; the opener sets it. */
  let mapBackLabel = "back";
  /** The scene editor's screen (#480) — the third full-screen one. It IS
   *  reached from a tab, so `tab` stays `scenes` while it is open. */
  let sceneEditing = false;
  /** Which scene it holds; the route carries it (`#/scenes/<id>`). */
  let sceneId = "";
  /** First-load cover: hides the app until we've decided playground vs device
   *  (and, on a device, loaded its running pattern) so nothing flashes first. */
  let booting = true;
  let bootLabel = "loading…";
  /** The "PixelBlaze Library" tab browses the scraped corpus (original
   *  Pixelblaze community patterns). It's a local-only convenience: the tab
   *  only exists when tools/gen-gallery.mjs found a populated corpus/ and
   *  wrote public/pixelblaze-library.json — which is a BUILD-time fact
   *  (`__HAS_PIXELBLAZE_LIBRARY__`, vite.config.ts), not something to probe
   *  for over the wire. It is also a playground affordance: a device bundle
   *  never carries the file, so a console does not ask for it at all
   *  (Gitea #564). Decided once the device probe has answered, below. */
  let hasPixelblazeLibrary = false;

  let editor: Editor;

  /**
   * The tab strip, data-driven so Scenes (#480) is one more entry.
   *
   * SCENES (D10, proposal §5.4c) is the one tab that is NOT gated on "is
   * there a device": it needs a regular 2D grid to draw on, nothing else. On
   * a console that is strictly the DEVICE's Layout kind — a strip, 3D or
   * custom-map board never shows it, and an empty matrix always does (S6c).
   * In the playground there is no hardware constraint, so the tab is always
   * present and the page carries the S6b empty state that sets the fixture;
   * hiding it there would make the feature undiscoverable.
   */
  $: scenesReady =
    $device === null ||
    ($deviceLayoutWire !== null && uiLayoutKind($deviceLayoutWire.kind, $layout.dims) === "matrix");
  $: tabs = [
    { id: "patterns" as const, label: "Patterns", show: true },
    { id: "scenes" as const, label: "Scenes", show: scenesReady },
    { id: "playlist" as const, label: "Playlist", show: $device !== null },
    { id: "settings" as const, label: "Settings", show: $device !== null },
  ].filter((t) => t.show);

  /** The label/target the editor's back button returns to. */
  $: backLabel = tabs.find((t) => t.id === tab)?.label ?? "Patterns";

  // ---- the URL ----
  // One fragment per screen (lib/router.ts) so a reload reopens what was
  // open — Jeremy's ask for Settings, 2026-09-19. The shell owns it because
  // the shell owns `tab`/`editing`/`mapEditing`; nothing else writes it
  // except the share link, which is also a fragment and is left alone.

  /** Which page the shell's state IS, in route terms. The scene editor is
   *  the one screen with an argument — `#/scenes/<id>` — because opening a
   *  scene, unlike opening a pattern, does not run it (lib/router.ts). */
  $: currentRoute = (
    mapEditing
      ? { page: "map" }
      : editing
        ? { page: "editor" }
        : sceneEditing
          ? { page: "scenes", id: sceneId }
          : { page: tab }
  ) as Route;

  /** The route the URL last named, so nothing is pushed twice. */
  let urlHref = "";
  /** Nothing writes the URL until boot has settled on a screen. */
  let routing = false;

  $: if (routing) syncUrl(currentRoute);
  function syncUrl(r: Route): void {
    const href = r.page + (r.id ?? "");
    if (href === urlHref) return;
    urlHref = href;
    pushRoute(r);
  }

  /** Put the shell on the screen a route names. Tabs that need hardware are
   *  ignored without it (a `#/settings` link opened in the playground lands
   *  on Patterns rather than on a page that cannot exist). */
  function applyRoute(r: Route): void {
    urlHref = r.page + (r.id ?? "");
    if (r.page === "map") {
      mapEditing = true;
      return;
    }
    mapEditing = false;
    if (r.page === "editor") {
      editing = true;
      return;
    }
    editing = false;
    if (r.page === "scenes") {
      // …but only where the tab exists: a `#/scenes` link opened against a
      // strip console lands on Patterns, the way `#/settings` does in the
      // playground, rather than on a screen that has no fixture to draw on.
      if (!scenesReady) {
        sceneEditing = false;
        tab = "patterns";
        return;
      }
      tab = "scenes";
      sceneId = r.id ?? "";
      sceneEditing = sceneId !== "";
      return;
    }
    sceneEditing = false;
    tab = r.page === "patterns" || $device !== null ? r.page : "patterns";
    if (tab === "playlist") void refreshPlaylist();
  }

  /** Open a scene in the full-screen scene editor. Exported for the same
   *  reason `openMapEditor` is: the playlist row and the Patterns tile's
   *  `Add to scene ▸` (#478) reach it from somewhere else entirely. */
  export function openScene(id: string): void {
    tab = "scenes";
    sceneId = id;
    sceneEditing = true;
  }

  function onPopState(): void {
    const r = parseRoute(location.hash);
    if (r) applyRoute(r);
  }

  /** What the status-bar counter says, and what it is allowed to claim.
   *  Connected: the DEVICE's own rate — `out_fps` on a pipelined HUB75 board
   *  (frames the panel displayed), else `fps` (a strip renders and writes in
   *  the same loop, so there render == wire). `out_fps` has been bounded by
   *  `rescan_hz` since #394, so it is shown as measured rather than clamped
   *  to the ceiling; the ceiling goes in the tooltip. Not connected: the
   *  browser preview loop, which is the only frame rate a playground has.
   *  The READOUT is bare — `27 fps`, exactly the mock's header (S1/S5); what
   *  the number is and where it came from is the tooltip's job, not four
   *  extra words in 44px of chrome. */
  $: fpsReadout = !$device
    ? {
        text: `${$previewFps.toFixed(0)} fps`,
        title: "local preview loop in this browser tab",
      }
    : $deviceOutFps > 0
      ? {
          text: `${$deviceOutFps} fps`,
          title:
            `${$deviceOutFps} fps displayed by the panel (out_fps)` +
            ($deviceRescanHz ? `, panel rescan ${$deviceRescanHz} Hz` : "") +
            ` — device render loop ${$deviceFps} fps, local preview ${$previewFps.toFixed(0)} fps`,
        }
      : {
          text: `${$deviceFps} fps`,
          title: `${$deviceFps} fps rendered by the device — local preview ${$previewFps.toFixed(0)} fps`,
        };

  /** Open the editor full-screen over the Patterns page (where every pattern
   *  is opened from since #467). */
  function openEditor(): void {
    tab = "patterns";
    editing = true;
  }

  /**
   * THE route to the map program's screen (A10, Gitea #471) — the shell owns
   * it because every entry point is somewhere else: the playground's "Preview
   * as" chip picking "Custom map program", the console's Settings → LED layout
   * link, and the pattern editor's "Map program ›" while the Layout is custom.
   * A8 (#469) calls this from the real LED-layout card.
   */
  export function openMapEditor(from = backLabel): void {
    mapBackLabel = from;
    mapEditing = true;
  }

  function onDrop(e: DragEvent): void {
    e.preventDefault();
    const f = e.dataTransfer?.files?.[0];
    if (f && /\.(epe|json)$/i.test(f.name)) void editor.importEpeFile(f);
  }

  onMount(async () => {
    // Probe for a device in parallel with the wasm load, so we can go straight
    // into device mode without ever flashing the playground first.
    const deviceProbe = detectDeviceBase();
    startSessionPoll(); // 1 Hz while a session is live; the scheduler idles otherwise
    let lx: Luxel;
    try {
      lx = await loadLuxel();
    } catch (e) {
      setBanner("load-failure", { level: "error", text: `failed to load luxel.wasm: ${String(e)}` });
      booting = false;
      return;
    }
    // In-progress work wins: a share link's pattern, else the autosaved
    // working copy. The editor opens on it (resume — never lose edits). A
    // device, though, only resumes it when it has *unsaved changes*; a clean
    // copy defers to whatever pattern is actually running on the device.
    const shared = await decodeShare(location.hash);
    let sharedMap = false;
    if (shared) {
      source.set(shared.source);
      if (shared.mapSrc) {
        // A pre-#463 link that carried a map program. Links no longer ship
        // one (a map is the Layout's, not the pattern's), but the old ones
        // still work: the map becomes this playground's Layout choice. The
        // program is run headlessly here — since A10 (#471) the map editor is
        // a screen, and a share link must not have to open one.
        mapSrc.set(shared.mapSrc);
        setPreviewAs({ mode: "map", pixels: get(pixelTotal) });
        sharedMap = true;
      }
      exampleName.set("");
      patternName.set("shared pattern");
    }
    let hadWip = shared !== null;
    // What a console has to decide about (#585): a shared link is itself an
    // unsaved edit to resume, and it is an edit of no device pattern.
    let wip = { dirty: shared !== null, devicePatternId: "" };
    if (!shared) {
      const wc = loadWorkingCopy();
      if (wc) {
        source.set(wc.source);
        patternName.set(wc.patternName);
        exampleName.set(wc.exampleName);
        dirty.set(wc.dirty);
        hadWip = true; // the playground always resumes the last working copy
        wip = { dirty: wc.dirty, devicePatternId: wc.devicePatternId };
      }
    }
    startAutosave();
    // a share link never auto-connects to a device
    const base = shared ? null : await deviceProbe;
    // The corpus tab, decided without a request (#564). Gated on the mode as
    // well as on the build: an asset bundle packed from a dev tree that HAD a
    // corpus would otherwise still ask the device for a file it never ships.
    hasPixelblazeLibrary = __HAS_PIXELBLAZE_LIBRARY__ && base === null;

    if (base !== null) {
      // Device mode: keep the boot cover up (with device-aware text) through
      // the whole handshake so the running pattern is loaded before anything
      // shows.
      bootLabel = "opening the pattern running on the device…";
      deviceBase.set(base);
      // The console OPENS ON PATTERNS → On device, with the running tile lit
      // (Jeremy, 2026-09-19 — it used to drop straight into the editor). The
      // handshake below still pulls the running pattern into the working
      // copy, so opening the editor afterwards is instant and already on it.
      tab = "patterns";
      editing = false;
      await editor.bootDevice(() => connectDevice(base), wip);
    } else {
      if (sharedMap) runMapProgram(lx, get(mapSrc), get(pixelTotal));
      editor.bootPlayground();
      editing = hadWip; // resume in the editor if there was work in progress
    }
    // The URL wins over the boot default: a reload reopens the screen that
    // was open. A share link is a fragment too, and it is NOT a route — it
    // has already been decoded above, so leave its fragment in the bar.
    const route = shared ? null : parseRoute(location.hash);
    if (route) {
      applyRoute(route);
    } else if (!shared) {
      // `currentRoute` is reactive and has not been recomputed yet inside
      // this handler — read the state directly.
      const bootPage: Page = mapEditing ? "map" : editing ? "editor" : tab;
      replaceRoute({ page: bootPage });
      urlHref = bootPage;
    }
    // The scene library, once, at boot — like the playlist. A `#/scenes/<id>`
    // deep link opens the scene EDITOR without the Scenes page ever mounting
    // active, so the library cannot be the page's to fetch (#480).
    void refreshScenes();
    routing = true;
    booting = false;
  });

  onDestroy(() => {
    pollStopAll();
    stopAutosave();
  });
</script>

<!-- back/forward between screens (lib/router.ts) -->
<svelte:window on:popstate={onPopState} />

<div
  class="shell"
  data-mode={$mode}
  data-tab={tab}
  role="application"
  on:drop={onDrop}
  on:dragover|preventDefault
>
  {#if booting}
    <!-- first-load cover: nothing renders behind it, so the playground never
         flashes before we switch into device mode + load the running pattern -->
    <div class="boot" data-role="boot">
      <span class="spinner"></span>
      <span class="boot-label" data-role="boot-label">{bootLabel}</span>
    </div>
  {/if}
  <!-- The shell header exists only over a HOME tab. An editor screen carries
       its own header (mockup S2) — Jeremy, 2026-09-19: "when editing a file,
       'luxel' and other things in the header bar hide". The device chip that
       used to live here moves into the editor's header with it
       (components/DeviceChip.svelte). -->
  {#if !editing && !mapEditing && !sceneEditing}
    <!-- DOM order IS the visual order here, so Tab walks the header the way
         the eye does: wordmark · chip · tabs · brightness · fps. The `.hdrtop`
         wrapper carries no focusable control past the chip, and the one
         header control that IS focusable (the brightness slider / the
         playground's Layout chip) is authored AFTER the tab strip — which is
         where it renders. `order:` is left to place the non-focusable spacer
         and readout (#538 sweep: "tab order follows visual order"). -->
    <header class="hdr">
      <div class="hdrtop">
        <!-- the word `luxel`, and on a console nothing after it: WHICH device
             is the chip's job (mockup S1), not a URL in the wordmark -->
        <span class="wordmark"
          >luxel{#if $isPlayground}&nbsp;<span>playground</span>{/if}</span
        >
        {#if !$isPlayground}
          <span class="slot chip"><DeviceChip /></span>
        {/if}

        <span class="spacer"></span>

        <!-- The device compiled the live pattern (#658, docs/jit-design.md
             §4a: "next to the frame rate"). A quiet marker and nothing
             more — it says why the number to its left is what it is. Its
             own element, because the fps readout is asserted BARE
             (`/^\d+ fps$/`, web/tools/device-e2e.mjs) and because a
             refusal is not reported here at all: that one has a reason
             worth reading, and it gets the amber strip under the preview. -->
        {#if $deviceJit?.state === "native"}
          <span
            class="jitmark mono"
            data-role="jit-native"
            title="This device compiled the pattern to native code: {$deviceJit
              .code_bytes} bytes in {$deviceJit.compile_us} µs.">native</span
          >
        {/if}
        <span
          class="fps mono"
          class:pg={$isPlayground}
          data-role="fps"
          title={fpsReadout.title}>{fpsReadout.text}</span
        >
      </div>

      <nav class="tabs" data-role="tabs">
        {#each tabs as t (t.id)}
          <button
            data-role={`tab-${t.id}`}
            class="tab"
            class:active={tab === t.id}
            on:click={() => {
              tab = t.id;
              // clicking the Scenes tab leaves the scene editor, exactly as
              // clicking Patterns leaves the pattern editor
              sceneEditing = false;
              if (t.id === "playlist") void refreshPlaylist();
            }}
          >
            {t.label}
          </button>
        {/each}
      </nav>

      <!-- What this app is rendering through (#463). The console states the
           device's own Layout in the chip above; the playground offers the
           chip that chooses one. -->
      {#if $isPlayground}
        <span class="slot ctl">
          <PreviewAsChip on:openmap={() => openMapEditor(backLabel)} />
        </span>
      {:else}
        <span class="slot ctl bri"><HeaderBrightness /></span>
      {/if}
    </header>
  {/if}

  <!-- THE error surface (#538 round 2): a refused request, or a device that
       has stopped answering. At the TOP of every screen — the editor
       included, which is why it is here and not in a page. -->
  <ErrorBar />

  <!-- Bytecode-format skew between this bundle and the device, and the
       stored-pattern repair it implies (#643). Renders nothing when the two
       agree, which is every normal session; it sits under ErrorBar for the
       same reason ErrorBar is here — the editor is full-screen, and "every
       save is being refused" has to be explainable from inside it. -->
  <BcBanner />

  <!-- Browser-blocked device connection (#162). Not an error the app can
       retry: this page is https, the device is plain http, and Chromium's
       Local Network Access policy has to allow that combination. Say so, and
       name the routes around it — the device serves this same console over
       http itself. Spans every tab, so it's visible whether the user landed
       in the editor or on a device tab. -->
  {#if $deviceBlocked}
    <div class="blocked-bar" data-role="device-blocked" role="alert">
      <strong>Your browser blocked this page from reaching the device.</strong>
      This copy of the console is served over <code>https</code> and
      {#if $deviceBase}<code>{$deviceBase}</code>{:else}the device{/if} speaks plain
      <code>http</code>, so Chromium has to grant Local Network Access first. Allow it if you get
      the prompt and reload — otherwise:
      <ul>
        <li>
          {#if $deviceBase}
            <a href={$deviceBase} data-role="device-blocked-link">open the console from the device</a>
          {:else}
            open the console from the device
          {/if}
          — it serves this same UI over plain http, with no permission involved.
        </li>
        <li>or host this UI on a plain-http LAN address, which has none of these restrictions.</li>
      </ul>
    </div>
  {/if}

  <!-- the one modal host: confirmations and the share-link fallback
       (stores/dialog.ts; naming is inline in the editor header since #468) -->
  <Dialog />

  <!-- The editor owns its own header (A7, #468): back, the inline-editable
       name, the save state, Save and the ⋯ menu are the DOCUMENT's, and the
       document is the editor's. The shell only tells it where back goes. -->
  <Editor
    bind:this={editor}
    active={editing && !mapEditing}
    {backLabel}
    on:open={() => (editing = true)}
    on:back={() => (editing = false)}
    on:openmap={() => openMapEditor("Editor")}
  />

  <!-- The map program's screen (A10, #471): geometry, not a pattern, so it is
       a peer of the editor rather than something inside it. Mounted once and
       kept — its document, breakpoints and computed points survive leaving. -->
  <MapEditor
    active={mapEditing}
    backLabel={mapBackLabel}
    on:back={() => (mapEditing = false)}
  />

  <Patterns
    active={!editing && !mapEditing && !sceneEditing && tab === "patterns"}
    {hasPixelblazeLibrary}
    on:new={() => {
      openEditor();
      editor.newPattern();
    }}
    on:openSaved={(e) => {
      openEditor();
      editor.loadSaved(e.detail);
    }}
    on:pick={(e) => {
      openEditor();
      editor.loadGalleryPick(e.detail);
    }}
    on:openDevice={(e) => {
      openEditor();
      void editor.openDevicePattern(e.detail);
    }}
    on:playDevice={(e) => {
      // Play WITHOUT opening the editor: it activates the pattern on the
      // device and adopts it as the editor's document, so the running marker
      // and the editor agree. `openDevice` above is the other half of the
      // #563 split — it opens the same pattern and touches nothing.
      void editor.playDevicePattern(e.detail);
    }}
  />

  <!-- The scene editor (#480): the THIRD full-screen screen, a peer of the
       pattern and map editors. It is reached from the Scenes tab, so the tab
       stays `scenes` underneath it. -->
  <SceneEditor
    active={sceneEditing && !editing && !mapEditing}
    {sceneId}
    on:back={() => (sceneEditing = false)}
    on:open={(e) => openScene(e.detail)}
  />

  <!-- Scenes is NOT wrapped in `{#if !$isPlayground}`: the playground has the
       tab too (D10), with the empty state that sets the fixture. -->
  <Scenes
    active={!editing && !mapEditing && !sceneEditing && tab === "scenes"}
    on:open={(e) => openScene(e.detail)}
  />

  {#if !$isPlayground}
    <Playlist active={!editing && !mapEditing && tab === "playlist" && !sceneEditing} />
  {/if}

  <!-- "stored, but not running yet" — pinned to the bottom of the VIEWPORT on
       every screen until the device reboots (#538). Here rather than inside
       Settings: the user changes a data pin and walks off to Patterns. -->
  <RebootBar />

  {#if $device}
    <Settings
      active={!editing && !mapEditing && tab === "settings" && !sceneEditing}
      on:navigate={(e) => (tab = e.detail)}
      on:openmap={() => openMapEditor("Settings")}
      on:pixelchange={() => editor.clearPreview()}
    />
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* mockup S1: 44px, `padding:0 16px`, one row —
     wordmark · device chip · tabs · spacer · controls · fps */
  .hdr {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 44px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }

  /* On a wide screen the header is ONE row, so this wrapper dissolves and its
     children become the header's own flex items (ordered below). At phone
     width it becomes the first of the two rows (mockup S1c) — which is the
     only reason it exists. It holds nothing focusable but the device chip,
     so dissolving it cannot disturb the tab order. */
  .hdrtop {
    display: contents;
  }

  .wordmark {
    order: 1;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--accent);
    white-space: nowrap;
  }

  /* "playground" after the wordmark (mockup S5); a console says which DEVICE
     in the chip instead, and puts nothing here */
  .wordmark span {
    color: var(--text);
  }

  .slot {
    display: inline-flex;
    align-items: center;
  }

  .chip {
    order: 2;
    min-width: 0;
  }

  .tabs {
    order: 3;
    display: flex;
    align-self: stretch;
    margin-left: 8px;
  }

  .spacer {
    order: 4;
    flex: 1;
  }

  .ctl {
    order: 5;
  }

  .fps {
    order: 7;
    font: 11.5px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
  }

  /* The JIT marker sits immediately left of the readout it explains, in
     the same 11.5px mono, one shade further back: it is context for the
     number, not a second number. (#658) */
  .jitmark {
    order: 6;
    font: 11.5px/1 var(--mono);
    color: var(--text-dim);
    opacity: 0.62;
    white-space: nowrap;
    margin-right: 8px;
  }

  /* mockup S5 sets the playground readout off from the Layout chip by a
     further 14px — the console's fps has the spacer to its left instead */
  .fps.pg {
    margin-left: 14px;
  }

  .tab {
    display: flex;
    align-items: center;
    padding: 0 12px;
    background: transparent;
    border: none;
    /* flush with the header's own bottom border (mockup S1) */
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    border-radius: 0;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--text);
  }

  /* the active tab is BRIGHTER, not amber — the amber is the underline
     (mockup S1; Jeremy, 2026-09-19) */
  .tab.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  /* mockup S1c: two rows — [wordmark · chip · spacer · fps] then a scrolling
     tab strip. */
  @media (max-width: 600px) {
    .hdr {
      flex-direction: column;
      align-items: stretch;
      height: auto;
      gap: 0;
      padding: 0;
    }

    .hdrtop {
      display: flex;
      align-items: center;
      gap: 10px;
      height: 40px;
      padding: 0 12px;
    }

    .tabs {
      height: 38px;
      margin-left: 0;
      padding: 0 8px;
      overflow-x: auto;
    }

    /* no room for it beside the chip; Settings keeps its own. A console's
       header is then exactly the mock's two rows (S1c). */
    .bri {
      display: none;
    }

    /* the playground's Layout chip has no such fallback, so it keeps a row of
       its own under the tab strip rather than crowding the identity line */
    .ctl {
      padding: 0 12px 10px;
    }
  }

  /* first-load cover over the whole app (header included) */
  .boot {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: var(--bg);
    color: var(--text-dim);
    font-size: 15px;
  }

  /* Full-width, above every tab: the device is unreachable for a reason the
     user has to act on outside the app (#162). Wider and wordier than the
     .banner family on purpose — it carries instructions, not a status. */
  .blocked-bar {
    margin: 8px 12px 0;
    padding: 10px 12px;
    border: 1px solid var(--error);
    border-radius: 6px;
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: #f2c4c4;
    font-size: 13px;
    line-height: 1.5;
  }

  .blocked-bar code {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .blocked-bar ul {
    margin: 6px 0 0;
    padding-left: 20px;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: conn-spin 0.7s linear infinite;
  }

  @keyframes conn-spin {
    to {
      transform: rotate(1turn);
    }
  }
</style>
