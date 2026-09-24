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
  import SpriteTools from "../components/scene/SpriteTools.svelte";
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
  import {
    emitSprite,
    fillSprite,
    hsvKey,
    newSprite,
    paintSprite,
    parseSprite,
    resizeSprite,
    spritePalette,
    type Sprite,
    type SpriteTool,
  } from "../lib/sprite";
  import type { Hsv } from "../lib/color";
  import { listPatterns, savePattern as savePatternLocally } from "../lib/store";
  import { confirm } from "../stores/dialog";
  import {
    device,
    deviceFps,
    deviceOutFps,
    devicePatterns,
    isPlayground,
    pollSubscribe,
    refreshClock,
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
  import { startTextSlotPoll, textSlots } from "../stores/textSlots";

  /** Mounted always; shown when the route says so (like the other screens). */
  export let active = false;

  /** Which scene — `#/scenes/<id>`. Empty = a new, unsaved one. */
  export let sceneId = "";

  const dispatch = createEventDispatcher<{ back: void; open: string }>();

  // A text layer reads two pieces of device state this screen is the only
  // caller for: the slot table and whether the clock is synced (Settings ›
  // Clock owns the other reader). Both are polled only while this screen is
  // up, on the ONE scheduler. The assignment lives in a FUNCTION: a `$:` that
  // both reads and assigns the same variable is its own dependency
  // (.claude/rules/web.md).
  let stopTextPoll: (() => void) | undefined;

  function syncTextPoll(on: boolean): void {
    if (on && !stopTextPoll) {
      const slots = startTextSlotPoll();
      const clock = pollSubscribe("scene-clock", 2000, refreshClock);
      stopTextPoll = () => {
        slots();
        clock();
      };
    } else if (!on && stopTextPoll) {
      stopTextPoll();
      stopTextPoll = undefined;
    }
  }

  $: syncTextPoll(active);

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
    // A different document: forget any sprite draft, the store is the truth
    // again (and a pending write was already flushed by its own timer).
    drafts = {};
    hoverCell = null;
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
    // A sprite being painted answers from the DRAFT until the store catches
    // up, so the composite shows the pixel the moment it is drawn (#481).
    const draft = drafts[id];
    if (draft !== undefined) return draft;
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
      .map((l) => {
        if (l.body.kind !== "pat" && l.body.kind !== "sprite") return "-";
        const id = l.body.kind === "pat" ? l.body.pat.id : l.body.id;
        const src = lookup(id);
        if (src === null) return "0";
        // A sprite being painted changes its SOURCE without changing the
        // scene's wire, so its content is part of the key (#481) — otherwise
        // `setScene` would see no change and the drawn pixel never appears.
        const draft = drafts[id];
        return draft === undefined ? "1" : playgroundPatternId(draft);
      })
      .join("/");
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

  // A re-render whenever anything the composite is built FROM changes.
  // `setScene` compares the WIRE, so a no-op edit (re-selecting a layer)
  // costs nothing. The dependencies are ARGUMENTS: a `void x` inside a
  // reactive EXPRESSION is not one as far as Svelte is concerned, and a
  // sprite draft that did not re-bind was invisible until a painted pixel
  // failed to appear (.claude/rules/web.md).
  $: if (active && $luxel) rebuildOn(doc, rigKey, $devicePatterns, drafts);

  function rebuildOn(_doc: unknown, _rig: unknown, _dev: unknown, _drafts: unknown): void {
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
    // The compositor reads no clock and no slot table — the HOST resolves a
    // `slot` source and hands the string down (docs/spec/scenes.md §2).
    const px = renderer.frame(dt, (n) => $textSlots[n] ?? "");
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
    // A sprite painted a moment before leaving the screen still has to land
    // in the store — the idle timer would be cancelled with the component.
    clearTimeout(saveTimer);
    void flushSprite();
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
  // `drafts` and `$devicePatterns` are read INSIDE `lookup`, so they have to
  // be named here or this never re-runs. They are ARGUMENTS and not a `void
  // drafts` in the expression, because Svelte does NOT count a `void x` there
  // as a dependency: with one, the selected sprite froze at whatever the
  // source was when the layer was picked and every painted pixel was applied
  // to that same stale copy — three strokes left one pixel
  // (.claude/rules/web.md).
  $: selSpriteSrc = spriteSourceOf(sel, drafts, $devicePatterns);

  function spriteSourceOf(l: Layer | null, _drafts: unknown, _dev: unknown): string {
    return l?.body.kind === "sprite" ? (lookup(l.body.id) ?? "") : "";
  }
  $: selSpriteTag = sel?.body.kind === "sprite" ? parseSpriteTag(selSpriteSrc) : null;
  /** The texels, when the source is one the tool row can paint on. */
  $: selSprite = sel?.body.kind === "sprite" ? parseSprite(selSpriteSrc) : null;

  // ---- drawing on a sprite (#481, mockup S7c) ----------------------------
  //
  // The tool row paints into a DRAFT source, which `lookup` answers with, so
  // the composite redraws on the same frame as the click; the store is
  // written behind it on an idle timer, because a device that took one POST
  // per painted pixel would spend a drag rewriting flash.

  let tool: SpriteTool = "pencil";
  let brush: Hsv = [0, 1, 1];
  let recents: Hsv[] = [];
  let hoverCell: { col: number; row: number } | null = null;
  /** Unsaved sprite sources, by pattern id. */
  let drafts: Record<string, string> = {};
  let saveTimer = 0;
  let savingSprite = false;
  let pendingId = "";

  /** A sprite layer whose pixels are readable is what puts the tool row on
   *  the screen and turns the marquee into a guide. */
  $: painting = sel?.body.kind === "sprite" && selSprite !== null;
  $: palette = selSprite ? spritePalette(selSprite) : [];

  /** Selecting a sprite loads the brush with the sprite's own first colour —
   *  you are far more often touching up a drawing than starting a new one,
   *  and a brush that came from somewhere else is a colour you did not ask
   *  for. Once you pick, your pick stays (`brushFor` is the guard). */
  let brushFor = "";
  $: if (sel?.body.kind === "sprite" && sel.body.id !== brushFor && palette.length > 0) {
    brushFor = sel.body.id;
    brush = palette[0] ?? brush;
  }

  /** The recent swatches (S7c). Until anything has been picked they are the
   *  sprite's other colours — the ones the drawing is already made of. */
  $: toolRecents =
    recents.length > 0
      ? recents
      : palette.filter((c) => hsvKey(c) !== hsvKey(brush)).slice(0, 6);

  /** Stage cell → sprite texel, or null when the pointer is outside the
   *  layer's box. A sprite is never scaled, so this is a translation. */
  function texelAt(col: number, row: number): { col: number; row: number } | null {
    if (!sel || !selSprite) return null;
    const c = col - sel.style.rect.x;
    const r = row - sel.style.rect.y;
    if (c < 0 || r < 0 || c >= selSprite.tag.w || r >= selSprite.tag.h) return null;
    return { col: c, row: r };
  }

  function onCell(e: CustomEvent<{ col: number; row: number; down: boolean }>): void {
    if (!painting || !sel || !selSprite || sel.body.kind !== "sprite") return;
    const at = texelAt(e.detail.col, e.detail.row);
    if (!at) return;
    // Frame 0 is what is edited: S7c's sprite has one, and a frame strip is
    // what picks another (the hint under Frames says it appears at 2+).
    let next: Sprite;
    if (tool === "eraser") {
      next = paintSprite(selSprite, 0, at.col, at.row, null);
    } else if (tool === "fill") {
      if (!e.detail.down) return; // a fill is a click, not a drag
      next = fillSprite(selSprite, 0, at.col, at.row, brush);
    } else {
      next = paintSprite(selSprite, 0, at.col, at.row, brush);
    }
    if (next === selSprite) return;
    putDraft(sel.body.id, next);
  }

  /** Install a painted sprite: the draft the preview compiles, then the store
   *  write on an idle timer. */
  function putDraft(id: string, sprite: Sprite): void {
    drafts = { ...drafts, [id]: emitSprite(sprite) };
    pendingId = id;
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => void flushSprite(), 600);
  }

  async function flushSprite(): Promise<void> {
    const id = pendingId;
    const src = drafts[id];
    if (id === "" || src === undefined) return;
    pendingId = "";
    savingSprite = true;
    try {
      const next = await storeSprite(id, src);
      if (next !== "" && next !== id) {
        // The device answered with a different id for the overwritten row —
        // re-point every layer that named the old one.
        commit({
          ...doc,
          layers: doc.layers.map((l) =>
            l.body.kind === "sprite" && l.body.id === id ? { ...l, body: { kind: "sprite", id: next } } : l,
          ),
        });
        drafts = { ...drafts, [next]: src };
      }
    } finally {
      savingSprite = false;
    }
  }

  /** Write a sprite's pattern back to the store it came from. Returns the id
   *  it now has. A same-name save OVERWRITES (pages/Editor.svelte), which is
   *  exactly the semantics a sprite edit wants. */
  async function storeSprite(id: string, source: string): Promise<string> {
    const name = patternNameOf(id);
    if (name === "") return id;
    const d = $device;
    if (!d) {
      savePatternLocally(name, source);
      return playgroundPatternId(name);
    }
    const bc = compileToBytecode(source);
    if (!bc) return id;
    try {
      const r = await d.savePattern(name, source, bc);
      if (!r.ok) return id;
      await refreshDevicePatterns([id]);
      if (r.id && r.id !== "") return r.id;
      return $devicePatterns.find((p) => p.name === name)?.id ?? id;
    } catch {
      return id;
    }
  }

  /** The tool row's colour. Recents are most-recent-first, deduplicated, six
   *  deep (S7c draws six). */
  function setBrush(c: Hsv): void {
    brush = c;
    const key = (x: Hsv): string => `${x[0]},${x[1]},${x[2]}`;
    recents = [c, ...recents.filter((r) => key(r) !== key(c))].slice(0, 6);
  }

  /** Size / Frames on the inspector rewrite the sprite itself. */
  function onSpriteResize(e: CustomEvent<{ w: number; h: number; frames: number; fps: number }>): void {
    if (!sel || !selSprite || sel.body.kind !== "sprite") return;
    const next = resizeSprite(selSprite, e.detail);
    putDraft(sel.body.id, next);
    // the box mirrors the sprite — a sprite is never scaled (S7c)
    replaceLayer(selected, {
      ...sel,
      style: { ...sel.style, rect: { ...sel.style.rect, w: next.tag.w, h: next.tag.h } },
    });
  }

  /** `New…` on the sprite row, and what `Add layer › Sprite` falls back to
   *  when the store holds no sprite yet: a blank 16×16 the tool row can draw
   *  on immediately. Neither S1 nor S7 draws a `New sprite…` entry, so this
   *  is the affordance's only home (see docs/web-architecture.md). */
  async function freshSprite(at: number): Promise<void> {
    const l = doc.layers[at];
    if (!l || l.body.kind !== "sprite") return;
    const name = freshSpriteName();
    const source = emitSprite(newSprite(16, 16), name);
    let id = "";
    const d = $device;
    if (!d) {
      savePatternLocally(name, source);
      id = playgroundPatternId(name);
    } else {
      const bc = compileToBytecode(source);
      if (!bc) return;
      const r = await d.savePattern(name, source, bc);
      if (!r.ok) return;
      await refreshDevicePatterns();
      id = r.id && r.id !== "" ? r.id : ($devicePatterns.find((p) => p.name === name)?.id ?? "");
    }
    if (id === "") return;
    drafts = { ...drafts, [id]: source };
    commit({
      ...doc,
      layers: doc.layers.map((x, i) =>
        i === at
          ? { ...x, body: { kind: "sprite", id }, style: { ...x.style, rect: { ...x.style.rect, w: 16, h: 16 } } }
          : x,
      ),
    });
    selected = at;
  }

  function freshSpriteName(): string {
    const taken = new Set<string>([
      ...$devicePatterns.map((p) => p.name),
      ...listPatterns().map((p) => p.name),
    ]);
    for (let n = 1; n < 1000; n++) {
      const name = `Sprite ${n}`;
      if (!taken.has(name)) return name;
    }
    return `Sprite ${Date.now()}`;
  }

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
    if (kind === "pat" || kind === "sprite") void pickFor(selected, kind);
  }

  /**
   * Open the picker for a layer — and, for a sprite with nothing to pick,
   * make one instead (neither S1 nor S7 draws a `New sprite…` entry anywhere,
   * so Add layer › Sprite on an empty store IS the creation path).
   *
   * It re-reads the library first. `devicePatterns` is refreshed on demand,
   * not polled, so a pattern saved from another tab — or by a harness — is
   * invisible here until something asks; and "there are no sprites" decided
   * on a stale list silently makes a blank one over a store that has some.
   * The same reason a row whose SOURCE has not streamed in yet counts as
   * unknown rather than as "not a sprite".
   */
  async function pickFor(at: number, kind: "pat" | "sprite"): Promise<void> {
    if ($device) await refreshDevicePatterns();
    if (kind === "sprite" && spriteRows().length === 0 && !sourcesPending()) {
      await freshSprite(at);
      return;
    }
    pickingFor = at;
    pickerOpen = true;
  }

  /** The stored patterns that are SPRITES — what the picker offers a sprite
   *  layer (#700). A device row whose source has not streamed in yet cannot
   *  be classified, so it is left out rather than offered as a maybe. */
  function spriteRows(): typeof $devicePatterns {
    return $devicePatterns.filter((p) => parseSpriteTag(p.source ?? "") !== null);
  }

  /** Is any stored pattern still un-classifiable — its source not streamed in
   *  yet? Then "there are no sprites" is not something we know. */
  function sourcesPending(): boolean {
    return $devicePatterns.some((p) => p.source === undefined);
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
      paintMode={painting}
      markCell={painting ? hoverCell : null}
      on:rect={(e) => onRect(e.detail)}
      on:pause={(e) => (paused = e.detail)}
      on:cell={onCell}
      on:hover={(e) => (hoverCell = e.detail)}
    >
      <svelte:fragment slot="tools">
        {#if painting}
          <SpriteTools
            {tool}
            color={brush}
            recents={toolRecents}
            paletteSize={palette.length}
            on:tool={(e) => (tool = e.detail)}
            on:color={(e) => setBrush(e.detail)}
          />
        {/if}
      </svelte:fragment>
    </SceneStage>

    <div class="rcol" data-role="scene-inspector">
      {#if sel && sel.body.kind === "pat"}
        <PatternInspector
          layer={sel}
          engine={renderer?.engineAt(selected) ?? null}
          source={lookup(sel.body.pat.id) ?? undefined}
          patternName={patternNameOf(sel.body.pat.id)}
          {rig}
          on:change={(e) => replaceLayer(selected, e.detail)}
          on:pick={() => void pickFor(selected, "pat")}
          on:delete={() => void onDeleteLayer()}
        />
      {:else if sel && sel.body.kind === "text"}
        <TextInspector
          layer={sel}
          {gridW}
          on:change={(e) => replaceLayer(selected, e.detail)}
          on:delete={() => void onDeleteLayer()}
        />
      {:else if sel && sel.body.kind === "sprite"}
        <SpriteInspector
          layer={sel}
          tag={selSpriteTag}
          sprite={selSprite}
          saving={savingSprite}
          on:change={(e) => replaceLayer(selected, e.detail)}
          on:resize={onSpriteResize}
          on:fresh={() => void freshSprite(selected)}
          on:pick={() => void pickFor(selected, "sprite")}
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
    patterns={doc.layers[pickingFor]?.body.kind === "sprite" ? spriteRows() : $devicePatterns}
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
