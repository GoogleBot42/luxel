<script lang="ts">
  // THE scene editor (mockups S7, S7b, S7d, S7e, S7f, S7g; Gitea #480).
  //
  // A THIRD full-screen screen, peer of the pattern editor and the map editor
  // — `data-role="scene-editor-view"` on its own `<main>`, because all three
  // are mounted at once and an unscoped selector would silently resolve to
  // the pattern editor's (.claude/rules/web.md).
  //
  // Three columns, one job each: what is stacked (left), what it looks like
  // (centre), what the selected layer does (right) — so nothing about a layer
  // is edited in two places. Selection is the only link between them.
  //
  // LIVE PUSH (#563/#585): an edit reaches the device only while the scene
  // being edited is the one the device is SHOWING. Opening a scene, or
  // editing any other one, touches nothing until Save.
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import "../components/scene/scene.css";
  import "../components/editor-frame.css";
  import LayerList from "../components/scene/LayerList.svelte";
  import SceneStage from "../components/scene/SceneStage.svelte";
  import PatternInspector from "../components/scene/PatternInspector.svelte";
  import TextInspector from "../components/scene/TextInspector.svelte";
  import SpriteInspector from "../components/scene/SpriteInspector.svelte";
  import ColorInspector from "../components/scene/ColorInspector.svelte";
  import PatternPicker from "../components/PatternPicker.svelte";
  import Popover from "../components/Popover.svelte";
  import { SceneRenderer, playgroundPatternId } from "../lib/sceneRender";
  import {
    MAX_SCENE_NAME,
    newLayer,
    parseSpriteTag,
    patternLayerCount,
    serializeScene,
    truncateUtf8,
    type Layer,
    type LayerKind,
    type Rect,
    type Scene,
  } from "../lib/scene";
  import { listPatterns, savePattern as savePatternLocally } from "../lib/store";
  import { confirm } from "../stores/dialog";
  import {
    device,
    deviceFps,
    deviceOutFps,
    devicePatterns,
    isPlayground,
    refreshDevicePatterns,
  } from "../stores/device";
  import { layout, layoutKey } from "../stores/geometry";
  import { compileToBytecode, luxel, previewFps } from "../stores/pattern";
  import {
    activateScene,
    activeSceneId,
    cancelLivePush,
    deleteScene,
    duplicateScene,
    layerCap,
    livePushScene,
    refreshScenes,
    saveScene,
    sceneById,
    scenes,
  } from "../stores/scenes";

  /** Mounted always; shown when the route says so (like the other screens). */
  export let active = false;
  /** Which scene — `#/scenes/<id>`. Empty = a new, unsaved one. */
  export let sceneId = "";

  const dispatch = createEventDispatcher<{ back: void; open: string }>();

  /** The working copy. The store's record is never mutated in place. */
  let doc: Scene = { id: "", name: "New scene", layers: [] };
  /** What `doc` looked like when it was last saved — the dirty test. */
  let savedWire = "";
  let selected = -1;
  let loadedId: string | null = null;
  let paused = false;
  let menuOpen = false;
  let moreBtn: HTMLElement | null = null;
  let pickerOpen = false;
  let pickingFor = -1;
  let pickerBusy = "";
  let pickerError = "";
  let stage: SceneStage | undefined;
  let renaming = false;
  let nameDraft = "";

  // ---- loading ----
  // `$scenes` is the reader; `sceneId` is the route. Re-reading whenever the
  // id changes (and NOT while it is the same) is what keeps an in-progress
  // edit from being stomped by the 2 Hz poll.
  $: if (active) maybeAdopt(sceneId, $scenes);

  // A deep link (`#/scenes/<id>` in a fresh tab) lands HERE, not on the
  // Scenes page, so this screen asks for the library itself rather than
  // waiting for a page it never opened to do it.
  $: if (active) void refreshScenes();

  /** Adopt `id` once the store actually holds it — a deep link lands here
   *  before the first `/api/scenes` answers, and adopting too early would
   *  open a blank document over a real scene. */
  function maybeAdopt(id: string, list: readonly Scene[]): void {
    if (id === loadedId) return;
    if (id !== "" && !list.some((s) => s.id === id)) return;
    adopt(id);
  }

  function adopt(id: string): void {
    loadedId = id;
    const found = id === "" ? null : sceneById(id);
    doc = found ? structuredClone(found) : { id: "", name: "New scene", layers: [] };
    savedWire = serializeScene(doc);
    selected = doc.layers.length > 0 ? doc.layers.length - 1 : -1;
    rebuild(true);
  }

  $: dirty = serializeScene(doc) !== savedWire;

  /** The mock's `saved · on device` / `unsaved changes` (S7 / S7e). */
  $: saveState = dirty
    ? "unsaved changes"
    : doc.id === ""
      ? "not saved yet"
      : $device
        ? "saved · on device"
        : "saved";

  // ---- the composite ----

  let renderer: SceneRenderer | null = null;
  let rafId = 0;

  let fpsAvg = 0;

  /** The scene renders on the ONE Layout — the device's on a console, the
   *  `Preview as` choice in the playground. `deviceGeometry()` owns the
   *  console half and nothing here may hand it anything else (#539/#573). */
  $: rig = $layout;
  $: gridW = rig.w;
  $: gridH = rig.h;

  /** Source by store id. A device pattern's source streams in with the list;
   *  a playground one is keyed by a hash of its name. */
  function lookup(id: string): string | null {
    const dev = $devicePatterns.find((p) => p.id === id);
    if (dev?.source !== undefined) return dev.source;
    for (const p of listPatterns()) if (playgroundPatternId(p.name) === id) return p.source;
    return null;
  }

  function patternNameOf(id: string): string {
    const dev = $devicePatterns.find((p) => p.id === id);
    if (dev) return dev.name;
    for (const p of listPatterns()) if (playgroundPatternId(p.name) === id) return p.name;
    return "";
  }

  /** The Layout's IDENTITY, not the object: `$layout` is derived and hands
   *  out a fresh object on every dependency change, so comparing references
   *  would rebuild every engine several times a second. */
  $: rigKey = layoutKey(rig);
  let builtFor = "";
  let boundSources = "";

  /** Which layers' sources the browser HAS. A device pattern's source streams
   *  in after the list does, so the wire block can be unchanged while the
   *  picture is still missing a layer — that is the second rebuild trigger. */
  function sourceKey(s: Scene): string {
    return s.layers
      .map((l) =>
        l.body.kind === "pat" || l.body.kind === "sprite"
          ? (lookup(l.body.kind === "pat" ? l.body.pat.id : l.body.id) === null ? "0" : "1")
          : "-",
      )
      .join("");
  }

  function rebuild(force = false): void {
    if (!$luxel || !active) return;
    if (!renderer || builtFor !== rigKey) {
      renderer?.free();
      renderer = new SceneRenderer($luxel, rig);
      builtFor = rigKey;
      force = true;
    }
    const sk = sourceKey(doc);
    if (sk !== boundSources) {
      boundSources = sk;
      force = true;
    }
    renderer.setScene(doc, lookup, force);
  }

  // A re-render whenever the record changes. `setScene` compares the WIRE, so
  // a no-op edit (re-selecting a layer) costs nothing.
  $: if (active && $luxel) {
    void doc;
    void rigKey;
    void $devicePatterns;
    rebuild();
  }

  /** Per-frame bookkeeping, deliberately in an OBJECT: mutating a field is
   *  not an assignment, so the render loop does not invalidate the component
   *  sixty times a second just to keep a clock. Only `fpsAvg` — which the
   *  cost line reads — is published, and only twice a second. */
  const clock = { last: 0, fps: 0, shown: 0 };

  function tick(now: number): void {
    rafId = requestAnimationFrame(tick);
    if (!active || paused || !renderer) return;
    const dt = clock.last === 0 ? 16 : Math.min(200, now - clock.last);
    clock.last = now;
    clock.fps = clock.fps === 0 ? 1000 / dt : clock.fps * 0.9 + (1000 / dt) * 0.1;
    if (now - clock.shown > 500) {
      clock.shown = now;
      fpsAvg = clock.fps;
    }
    const px = renderer.frame(dt);
    if (px) stage?.draw(px);
  }

  // ONE ticker for the life of the screen, started in `onMount`. NOT in a
  // `$:` block: a reactive statement that both reads and assigns the same
  // variable is its own dependency and re-runs forever (.claude/rules/web.md).
  onMount(() => {
    rafId = requestAnimationFrame(tick);
  });

  onDestroy(() => {
    cancelAnimationFrame(rafId);
    renderer?.free();
    cancelLivePush();
  });

  /** What the preview column's dim line says (S7 / S7f). */
  $: shownFps = $device ? ($deviceOutFps > 0 ? $deviceOutFps : $deviceFps) : Math.round($previewFps);
  $: dimsLine = `${gridW}×${gridH} · ${shownFps} fps${$device ? " on device" : ""}`;

  $: patternLayers = patternLayerCount(doc);

  /** Sprite metadata for the layer list (`sprite · 9×8`). */
  $: spriteDims = spriteDimsOf(doc, $devicePatterns);
  $: patternNames = patternNamesOf(doc, $devicePatterns);

  function spriteDimsOf(s: Scene, _dev: unknown): Record<number, string> {
    const out: Record<number, string> = {};
    s.layers.forEach((l, i) => {
      if (l.body.kind !== "sprite") return;
      const tag = parseSpriteTag(lookup(l.body.id) ?? "");
      if (tag) out[i] = `sprite · ${tag.w}×${tag.h}`;
    });
    return out;
  }

  function patternNamesOf(s: Scene, _dev: unknown): Record<number, string> {
    const out: Record<number, string> = {};
    s.layers.forEach((l, i) => {
      if (l.body.kind === "pat") out[i] = patternNameOf(l.body.pat.id);
    });
    return out;
  }

  $: sel = selected >= 0 ? (doc.layers[selected] ?? null) : null;
  $: selSpriteTag = sel?.body.kind === "sprite" ? parseSpriteTag(lookup(sel.body.id) ?? "") : null;

  // ---- editing ----

  /** Every mutation goes through here: it replaces the record, and pushes it
   *  to the device ONLY when this scene is the running one. */
  function commit(next: Scene): void {
    doc = next;
    livePushScene(doc);
  }

  function replaceLayer(at: number, l: Layer): void {
    commit({ ...doc, layers: doc.layers.map((x, i) => (i === at ? l : x)) });
  }

  function onAdd(kind: LayerKind): void {
    const l = newLayer(kind);
    if (kind === "color") l.style.rect = { x: 0, y: Math.max(0, gridH - 6), w: 0, h: 6 };
    if (kind === "text") l.style.rect = { x: 0, y: Math.floor(gridH / 2) - 4, w: 0, h: 8 };
    commit({ ...doc, layers: [...doc.layers, l] });
    selected = doc.layers.length - 1;
    if (kind === "pat" || kind === "sprite") {
      pickingFor = selected;
      pickerOpen = true;
    }
  }

  function onReorder(from: number, to: number): void {
    const list = [...doc.layers];
    const [moved] = list.splice(from, 1);
    if (!moved) return;
    list.splice(to, 0, moved);
    commit({ ...doc, layers: list });
    selected = to;
  }

  function onToggle(at: number): void {
    const l = doc.layers[at];
    if (!l) return;
    replaceLayer(at, { ...l, style: { ...l.style, visible: !l.style.visible } });
  }

  function onRect(r: Rect): void {
    if (!sel || selected < 0) return;
    replaceLayer(selected, { ...sel, style: { ...sel.style, rect: r } });
  }

  async function onDeleteLayer(): Promise<void> {
    if (!sel || selected < 0) return;
    const ok = await confirm({
      title: `Delete “${sel.name || "this layer"}”?`,
      body: "The layer is removed from this scene. Nothing else changes.",
      confirmLabel: "Delete layer",
      danger: true,
    });
    if (!ok) return;
    const at = selected;
    commit({ ...doc, layers: doc.layers.filter((_, i) => i !== at) });
    selected = Math.min(at, doc.layers.length - 1);
  }

  /**
   * A pick from THE picker. A LIBRARY pattern is source neither store has
   * seen, so picking one is a SAVE followed by the binding — the same rule
   * `pages/Playlist.svelte` keeps, because a scene layer names a stored
   * pattern by id and nothing else.
   */
  async function onPick(
    e: CustomEvent<{ id: string; kind: string; name: string; source?: string }>,
  ): Promise<void> {
    const at = pickingFor;
    const l = doc.layers[at];
    if (!l) {
      pickerOpen = false;
      return;
    }
    let id = e.detail.id;
    if (e.detail.kind === "library" && e.detail.source !== undefined) {
      pickerBusy = e.detail.name;
      id = await saveLibraryPick(e.detail.name, e.detail.source);
      pickerBusy = "";
      if (id === "") {
        pickerError = `Couldn't save “${e.detail.name}”.`;
        return;
      }
    }
    pickerOpen = false;
    pickerError = "";
    if (l.body.kind === "pat") {
      replaceLayer(at, { ...l, body: { kind: "pat", pat: { ...l.body.pat, id, controls: {} } } });
    } else if (l.body.kind === "sprite") {
      replaceLayer(at, { ...l, body: { kind: "sprite", id } });
    }
  }

  /** Store a library pattern and return the id a layer can bind. The
   *  playground's library is keyed by NAME, so its id is a hash of one
   *  (`playgroundPatternId`); a device assigns its own. */
  async function saveLibraryPick(name: string, source: string): Promise<string> {
    const d = $device;
    if (!d) {
      savePatternLocally(name, source);
      return playgroundPatternId(name);
    }
    const bc = compileToBytecode(source);
    if (!bc) return "";
    try {
      const r = await d.savePattern(name, source, bc);
      if (!r.ok) return "";
      await refreshDevicePatterns();
      return r.id || ($devicePatterns.find((p) => p.name === name)?.id ?? "");
    } catch {
      return "";
    }
  }

  // ---- the document's verbs ----

  async function save(): Promise<void> {
    const r = await saveScene(doc);
    if (!r.ok) return;
    if (r.id && doc.id === "") {
      doc = { ...doc, id: r.id };
      loadedId = r.id;
      dispatch("open", r.id);
    }
    savedWire = serializeScene(doc);
  }

  async function playOnDevice(): Promise<void> {
    menuOpen = false;
    if (doc.id === "") await save();
    if (doc.id !== "") await activateScene(doc.id);
  }

  async function duplicate(): Promise<void> {
    menuOpen = false;
    if (doc.id === "") return;
    const id = await duplicateScene(doc.id);
    if (id) dispatch("open", id);
  }

  async function remove(): Promise<void> {
    menuOpen = false;
    if (doc.id === "") return;
    const ok = await confirm({
      title: `Delete “${doc.name}”?`,
      body: "The scene is removed from this device. The patterns it used are untouched.",
      confirmLabel: "Delete scene",
      danger: true,
    });
    if (!ok) return;
    if (await deleteScene(doc.id)) dispatch("back");
  }

  /** A scene name is ≤ 64 BYTES on the wire, and a refusal the console could
   *  have prevented is the console's bug — clamp rather than let the device
   *  say no (`MAX_SCENE_NAME`, docs/spec/scenes.md §1). */
  function commitName(): void {
    renaming = false;
    const name = truncateUtf8(nameDraft.trim(), MAX_SCENE_NAME);
    if (name !== "") commit({ ...doc, name });
  }

  function onNameKey(e: KeyboardEvent): void {
    if (e.key === "Enter") commitName();
    if (e.key === "Escape") renaming = false;
  }
</script>

<main
  class="editor-frame scenes scene-editor"
  class:playground={$isPlayground}
  data-role="scene-editor-view"
  hidden={!active}
>
  <header class="editor-header" data-role="scene-editor-header">
    <button class="btn quiet back" data-role="scene-editor-back" on:click={() => dispatch("back")}>
      <span class="backglyph" aria-hidden="true">←</span>
      <span class="backlabel">Scenes</span>
    </button>

    {#if renaming}
      <input
        class="nameedit"
        data-role="scene-name-input"
        bind:value={nameDraft}
        aria-label="scene name"
        on:keydown={onNameKey}
        on:blur={commitName}
      />
    {:else}
      <button
        class="nameedit"
        data-role="scene-name"
        title="click to rename"
        on:click|stopPropagation={() => {
          nameDraft = doc.name;
          renaming = true;
        }}
      >
        <span class="nametext">{doc.name}</span>
      </button>
    {/if}

    <span class="savestate" data-role="scene-save-state">{saveState}</span>

    <span class="spacer"></span>

    <button class="btn primary" data-role="scene-save" on:click={() => void save()}>Save</button>

    <span class="overflow">
      <button
        class="btn icon"
        bind:this={moreBtn}
        data-role="scene-overflow"
        title="more actions"
        aria-label="more actions"
        on:click={() => (menuOpen = !menuOpen)}>⋯</button
      >
      <Popover
        open={menuOpen}
        anchor={moreBtn}
        dataRole="scene-menu"
        on:close={() => (menuOpen = false)}
      >
        <!-- Playing a scene is a takeover of the fixture, so it lives with
             the other document verbs rather than beside Save (S7's header is
             back · name · state · Save · ⋯, and nothing else). Absent in the
             playground, which has no device — never disabled. -->
        {#if $device}
          <button class="mi" data-role="scene-play-device" on:click={() => void playOnDevice()}
            >▶ Play on device</button
          >
        {/if}
        <button class="mi" data-role="scene-duplicate" on:click={() => void duplicate()}
          >Duplicate</button
        >
        {#if doc.id !== ""}
          <div class="sepr"></div>
          <button class="mi del" data-role="scene-delete" on:click={() => void remove()}
            >Delete scene</button
          >
        {/if}
      </Popover>
    </span>
  </header>

  <div class="scene3">
    <LayerList
      scene={doc}
      {selected}
      layerCap={$layerCap}
      {patternLayers}
      {spriteDims}
      {patternNames}
      on:select={(e) => (selected = e.detail)}
      on:toggle={(e) => onToggle(e.detail)}
      on:reorder={(e) => onReorder(e.detail.from, e.detail.to)}
      on:add={(e) => onAdd(e.detail)}
    />

    <SceneStage
      bind:this={stage}
      w={gridW}
      h={gridH}
      rect={sel?.style.rect ?? null}
      {patternLayers}
      layerCap={$layerCap}
      fps={$device ? shownFps : fpsAvg}
      {dimsLine}
      {paused}
      on:rect={(e) => onRect(e.detail)}
      on:pause={(e) => (paused = e.detail)}
    />

    <div class="rcol" data-role="scene-inspector">
      {#if sel && sel.body.kind === "pat"}
        <PatternInspector
          layer={sel}
          engine={renderer?.engineAt(selected) ?? null}
          source={lookup(sel.body.pat.id) ?? undefined}
          patternName={patternNameOf(sel.body.pat.id)}
          {rig}
          on:change={(e) => replaceLayer(selected, e.detail)}
          on:pick={() => {
            pickingFor = selected;
            pickerOpen = true;
          }}
          on:delete={() => void onDeleteLayer()}
        />
      {:else if sel && sel.body.kind === "text"}
        <TextInspector
          layer={sel}
          on:change={(e) => replaceLayer(selected, e.detail)}
          on:delete={() => void onDeleteLayer()}
        />
      {:else if sel && sel.body.kind === "sprite"}
        <SpriteInspector
          layer={sel}
          tag={selSpriteTag}
          on:change={(e) => replaceLayer(selected, e.detail)}
          on:pick={() => {
            pickingFor = selected;
            pickerOpen = true;
          }}
          on:delete={() => void onDeleteLayer()}
        />
      {:else if sel && sel.body.kind === "color"}
        <ColorInspector
          layer={sel}
          on:change={(e) => replaceLayer(selected, e.detail)}
          on:delete={() => void onDeleteLayer()}
        />
      {:else}
        <div class="rhead" style="margin-bottom:12px"><div class="slabel">Layer</div></div>
        <p class="hint" data-role="scene-no-selection">
          Pick a layer on the left to edit it, or add one.
        </p>
      {/if}
    </div>
  </div>

  <PatternPicker
    luxel={$luxel}
    open={pickerOpen}
    title={doc.layers[pickingFor]?.body.kind === "sprite" ? "Choose a sprite" : "Choose a pattern"}
    patterns={$devicePatterns}
    busy={pickerBusy}
    error={pickerError}
    on:pick={(e) => void onPick(e)}
    on:close={() => {
      pickerOpen = false;
      pickerError = "";
    }}
  />
</main>

<style>
  /* The scene editor's body is the three columns, not the code/rail split —
     `.editor-frame` gives it the header row and the full-height grid. */
  .scene-editor :global(.scene3) {
    min-height: 0;
  }
</style>
