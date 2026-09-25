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
  import DeviceChip from "../components/DeviceChip.svelte";
  import AsyncButton from "../components/AsyncButton.svelte";
  import NameField from "../components/NameField.svelte";
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
  import { compileForLayout, layout, layoutKey } from "../stores/geometry";
  import { compileToBytecode, luxel } from "../stores/pattern";
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
  /** The inspector column: where the picker hangs, and the one region whose
   *  clicks must not dismiss it — the `Change…` button that opens it lives in
   *  there (`PatternPicker`'s `anchor`, #730). */
  let rcolEl: HTMLElement | null = null;
  let stage: SceneStage | undefined;
  /** The preview's frame budget — the pattern editor's `targetFps`, the same
   *  option set, the same meaning: 0 is "as fast as the browser will go"
   *  (#736 item 26). */
  let targetFps = 60;

  // ---- loading ----
  // `$scenes` is the reader; `sceneId` is the route. Re-reading whenever the
  // id changes (and NOT while it is the same) is what keeps an in-progress
  // edit from being stomped by the 2 Hz poll.
  $: if (active) maybeAdopt(sceneId, $scenes);

  // A deep link (`#/scenes/<id>` in a fresh tab) lands HERE, not on the
  // Scenes page, so this screen asks for the library itself rather than
  // waiting for a page it never opened to do it. The PATTERN library for the
  // same reason (#730): every layer's picture, its name and every picker row
  // comes out of `devicePatterns`, which is refreshed ON DEMAND and never
  // polled (.claude/rules/web.md) — so a screen that never asks shows
  // whatever the Patterns page happened to leave there, often nothing.
  $: if (active) {
    void refreshScenes();
    void refreshDevicePatterns();
  }

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
    // Opening a scene is not an edit: there is nothing to settle and nothing
    // on screen to be jarred, so it installs now and the edit debounce starts
    // from this wire (#736 item 28).
    clearTimeout(rebuildTimer);
    rebuildTimer = 0;
    scheduledWire = savedWire;
    rebuild(true);
  }

  $: dirty = serializeScene(doc) !== savedWire;

  /** The full save-state contract (S7 / S7e). Since #738 this is not what the
   *  header PRINTS — the Save button carries "Save"/"Saved" itself — it is
   *  what `data-save-state` carries for harnesses, exactly as the pattern
   *  editor does it. */
  $: saveState = dirty
    ? "unsaved changes"
    : doc.id === ""
      ? "not saved yet"
      : $device
        ? "saved · on device"
        : "saved · in browser";

  /** What is LEFT to print once the button says "Saved": WHERE it was saved,
   *  and only when that is not obvious. On a console a scene can only live on
   *  the device; in the playground it lives in this browser and nowhere else,
   *  which is the thing Jeremy asked to be visible (#742: "in browser should
   *  be displayed for scenes being edited"). */
  $: saveWhere = $isPlayground && !dirty && doc.id !== "" ? "in browser" : "";

  // ---- the composite ----

  let renderer: SceneRenderer | null = null;
  let rafId = 0;

  let fpsAvg = 0;

  /** The compositor's complaint about the whole record, if any. */
  let sceneError = "";
  /** Per-layer compile failures, by layer index (#731). */
  let layerErrors: Record<number, string> = {};

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
  /** The wire `renderer` was last actually REBUILT from — a scene whose wire
   *  has not changed does not re-report its errors (see `rebuild`). */
  let renderedWire: string | null = null;

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
    // The compositor's own parse error and each layer's compile error are
    // assigned HERE, with the render they belong to — a `$:` derived from a
    // value a reactive block's function assigns never re-runs
    // (.claude/rules/web.md). They are only re-read when `setScene` actually
    // rebuilt: its no-op path answers `null` for an unchanged wire, which
    // would otherwise clear the error of the scene still installed.
    const wire = serializeScene(doc);
    const err = renderer.setScene(doc, lookup, force);
    if (force || wire !== renderedWire) {
      sceneError = err ?? "";
      layerErrors = layerErrorsOf(doc, renderer);
    }
    renderedWire = wire;
  }

  /**
   * Which layers name a pattern that does NOT compile. `SceneRenderer` binds
   * no engine for one and the compositor then draws nothing at all, with
   * nothing on screen saying why — "sometimes when a pattern is selected, the
   * preview still shows nothing" (#731). Only a BROKEN layer costs a compile
   * here: a layer the renderer built an engine for is skipped, and so is one
   * whose source has not arrived yet (unknown is not an error).
   */
  function layerErrorsOf(s: Scene, r: SceneRenderer): Record<number, string> {
    const out: Record<number, string> = {};
    const lx = $luxel;
    if (!lx) return out;
    s.layers.forEach((l, i) => {
      if (l.body.kind !== "pat" && l.body.kind !== "sprite") return;
      if (r.engineAt(i)) return;
      const id = l.body.kind === "pat" ? l.body.pat.id : l.body.id;
      if (id === "") return; // nothing chosen yet — not a failure
      const src = lookup(id);
      if (src === null) return; // still streaming in
      const built = compileForLayout(lx, src, 0, l.body.kind === "pat" ? l.body.pat.proj : null, rig);
      if ("engine" in built) built.engine.free();
      else out[i] = built.line > 0 ? `line ${built.line}: ${built.message}` : built.message;
    });
    return out;
  }

  // ---- when the composite is rebuilt (#736 item 28) ----------------------
  //
  // Jeremy: "there should be a one second delay before starting the pattern
  // again after a change has been made which causes the preview to be
  // regenerated/restart. This is to make it less jarring."
  //
  // It is a performance fix wearing a UX fix's clothes. `SceneRenderer`'s
  // `setScene` drops and recompiles ONE wasm engine PER PATTERN AND SPRITE
  // LAYER on any change to the wire (`web/src/lib/sceneRender.ts`), and the
  // reactive block below fires on every keystroke in the text field, every
  // tick of the opacity slider and every `pointermove` of a marquee drag. So
  // a two-pattern scene was recompiling two engines a frame while you dragged
  // a box — and every one of those recompiles put both patterns back to
  // t = 0, which is the strobing Jeremy is describing.
  //
  // The rule: coalesce, and let the EXISTING composite keep running while the
  // edit settles. Nothing is torn down in the meantime, so the delay reads as
  // "it caught up", not as "it stopped".
  //
  // Two exemptions, both about latency you can feel:
  //   · a scene with no pattern and no sprite layer has no engine to restart
  //     and nothing expensive to rebuild, so its edits land at once — text,
  //     colour and geometry tweaking stays live;
  //   · a SPRITE DRAFT is the brush. #481's whole contract is that the
  //     composite shows the pixel on the frame it is painted, and a painted
  //     pixel changes `drafts` without changing the wire — so that is the
  //     signal, and it is never delayed.
  const REBUILD_DELAY_MS = 1000;
  let rebuildTimer = 0;
  /** The wire the last scheduling decision was made against. */
  let scheduledWire = "";

  // The dependencies are ARGUMENTS: a `void x` inside a reactive EXPRESSION
  // is not a dependency as far as Svelte is concerned, and a sprite draft
  // that did not re-bind was invisible until a painted pixel failed to appear
  // (.claude/rules/web.md).
  $: if (active && $luxel) rebuildOn(doc, rigKey, $devicePatterns, drafts);

  function rebuildOn(_doc: unknown, _rig: unknown, _dev: unknown, _drafts: unknown): void {
    const wire = serializeScene(doc);
    // A change that did NOT touch the wire is a draft (or a store row landing)
    // — paint, and the pixel is due now.
    const wireChanged = wire !== scheduledWire;
    scheduledWire = wire;
    scheduleRebuild(wireChanged && hasEngines(doc) ? REBUILD_DELAY_MS : 0);
  }

  /** Is there anything in this scene that a rebuild would RESTART? Text
   *  carries its scroll phase across `set_scene` (#733) and a colour layer
   *  has no clock, so a scene of those two is free to rebuild immediately. */
  function hasEngines(s: Scene): boolean {
    return s.layers.some((l) => l.body.kind === "pat" || l.body.kind === "sprite");
  }

  /** The assignment lives in a FUNCTION, never in the `$:` block: a reactive
   *  statement that both reads and assigns the same variable is its own
   *  dependency and re-runs for ever (.claude/rules/web.md). */
  function scheduleRebuild(delay: number): void {
    clearTimeout(rebuildTimer);
    if (delay === 0) {
      rebuildTimer = 0;
      rebuild();
      return;
    }
    rebuildTimer = window.setTimeout(() => {
      rebuildTimer = 0;
      rebuild();
    }, delay);
  }

  /** Per-frame bookkeeping, deliberately in an OBJECT: mutating a field is
   *  not an assignment, so the render loop does not invalidate the component
   *  sixty times a second just to keep a clock. Only `fpsAvg` — which the
   *  cost line reads — is published, and only twice a second. */
  const clock = { last: 0, fps: 0, shown: 0 };

  function tick(now: number): void {
    rafId = requestAnimationFrame(tick);
    if (!active || paused || !renderer) {
      // Keep the clock CURRENT while nothing is drawn (#736 item 28). It used
      // to be left at the moment of the pause, so the first frame after
      // Resume — or after coming back from another screen — carried the whole
      // gap, clamped to 200 ms, and every layer jumped an eighth of a second
      // forward before settling. The clamp hid how bad it was; it did not
      // stop it being visible.
      clock.last = now;
      return;
    }
    // The preview's own frame budget, the pattern editor's arithmetic exactly
    // (`minInterval` in pages/Editor.svelte): 0 means "every rAF". The -1 is
    // its slack for a frame that arrives a hair early.
    const minInterval = targetFps > 0 ? 1000 / targetFps - 1 : 0;
    if (minInterval > 0 && clock.last !== 0 && now - clock.last < minInterval) return;
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
    clearTimeout(rebuildTimer);
    renderer?.free();
    cancelLivePush();
    // A sprite painted a moment before leaving the screen still has to land
    // in the store — the idle timer would be cancelled with the component.
    clearTimeout(saveTimer);
    void flushSprite();
  });

  /** What the preview column's dim line says (S7 / S7f). On a console it is
   *  the DEVICE's output rate; off one it is this screen's OWN measured loop,
   *  not `previewFps` — that store is the pattern editor's render loop and
   *  reporting it here was quoting another screen's number (#736 item 25 is
   *  what put the line next to the picture it describes). */
  $: shownFps = $device ? ($deviceOutFps > 0 ? $deviceOutFps : $deviceFps) : Math.round(fpsAvg);
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
      localRev++; // the local library is not a store — publish the change
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
      localRev++; // the local library is not a store — publish the change
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
    else localRev++; // the playground's library is localStorage: re-read it
    // Read the store through the function rather than the `$:` value: a flush
    // may not have run between the refresh above and here.
    const rows = patternRows($devicePatterns, $device !== null, localRev);
    if (kind === "sprite" && spriteRowsOf(rows).length === 0 && !sourcesPending(rows)) {
      await freshSprite(at);
      return;
    }
    pickingFor = at;
    pickerOpen = true;
  }

  /** Bumped whenever the playground's local library changes — `lib/store.ts`
   *  is localStorage, not a store, so nothing invalidates on a save. */
  let localRev = 0;

  /**
   * EVERY pattern a layer could bind (#730). A console binds ids the DEVICE
   * holds, so its rows are `devicePatterns`; the playground's ids are hashes
   * of local names (`playgroundPatternId`) and its rows are `lib/store.ts`'s
   * — which `lookup()` has always resolved, while the picker was only ever
   * offered the device list. `devicePatterns` is EMPTY without a device (it
   * is only ever written by `refreshDevicePatterns`, which returns early with
   * no session), so the playground's picker opened on an empty list every
   * time, which is why `Change…` looked dead there.
   */
  function patternRows(
    dev: typeof $devicePatterns,
    connected: boolean,
    _rev: number,
  ): typeof $devicePatterns {
    if (connected) return dev;
    return listPatterns().map((p) => ({
      id: playgroundPatternId(p.name),
      name: p.name,
      source: p.source,
    }));
  }

  /** The stored patterns that are SPRITES — what the picker offers a sprite
   *  layer (#700). A row whose source has not streamed in yet cannot be
   *  classified, so it is not offered YET rather than denied for ever: the
   *  list is DERIVED from the store, so the row appears the moment its source
   *  lands, and `pickFor` refuses to conclude "there are no sprites" while
   *  any source is still pending. Offering an unclassified row as a sprite
   *  would be worse than waiting — a sprite layer bound to a pattern that is
   *  not one draws nothing. */
  function spriteRowsOf(rows: typeof $devicePatterns): typeof $devicePatterns {
    return rows.filter((p) => parseSpriteTag(p.source ?? "") !== null);
  }

  /** Is any stored pattern still un-classifiable — its source not streamed in
   *  yet? Then "there are no sprites" is not something we know. */
  function sourcesPending(rows: typeof $devicePatterns): boolean {
    return rows.some((p) => p.source === undefined);
  }

  // The picker's list. Assigned reactively, with every dependency NAMED, so
  // rows appear as their sources stream in (.claude/rules/web.md).
  $: allRows = patternRows($devicePatterns, $device !== null, localRev);
  $: pickingSprite = doc.layers[pickingFor]?.body.kind === "sprite";
  $: pickerPatterns = pickingSprite ? spriteRowsOf(allRows) : allRows;

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
      localRev++; // the local library is not a store — publish the change
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

  async function save(): Promise<boolean> {
    const r = await saveScene(doc);
    if (!r.ok) return false;
    if (r.id && doc.id === "") {
      doc = { ...doc, id: r.id };
      loadedId = r.id;
      dispatch("open", r.id);
    }
    savedWire = serializeScene(doc);
    return true;
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

  /** What `NameField` hands back: already trimmed, already non-empty, and
   *  already clamped to `MAX_SCENE_NAME` — a scene name is ≤ 64 BYTES on the
   *  wire and a refusal the console could have prevented is the console's
   *  bug (docs/spec/scenes.md §1). All that is left here is what a rename
   *  MEANS, which is a commit like any other edit. */
  function commitName(name: string): void {
    if (name !== doc.name) commit({ ...doc, name });
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

    <!-- THE rename control, shared with the pattern editor's header
         (components/NameField.svelte, #736 item 30). This screen used to
         carry a hand-copy of that markup which had lost the focus, the
         select-all and the empty-name refusal — "clicking on the name of the
         scene on the title bar doesn't have the user's cursor go into that
         new textbox… the user has to click again". One component now. -->
    <NameField
      value={doc.name}
      label="scene name"
      dataRole="scene-name"
      inputRole="scene-name-input"
      errorRole="scene-name-error"
      maxBytes={MAX_SCENE_NAME}
      on:commit={(e) => commitName(e.detail)}
    />

    <!-- The full contract lives on the ATTRIBUTE, not in the text (#738): the
         Save button says "Save"/"Saved" now, so printing "saved · on device"
         beside it said the same thing twice. Harnesses read
         `dataset.saveState`; the reader sees only the part the button cannot
         carry. -->
    <span class="savestate" data-role="scene-save-state" data-save-state={saveState}
      >{saveWhere}</span
    >

    <!-- The preview transport, in the SCENE's top bar rather than in the
         centre column's header (#736, the transport hand-off). The pattern
         editor's moved here for #739 because nobody found it where it was,
         and a control that lives in two different places on two screens that
         are otherwise the same chrome is the same problem one layer up. It is
         the same element, the same class, the same words: `.transport` in
         components/editor-frame.css, `Pause`/`Resume` and never `Play` —
         a scene's Play verb is `▶ Play on device`, in the ⋯ menu, and two
         play marks in one bar is how a bar stops meaning anything. -->
    <button
      class="btn transport"
      class:paused
      data-role="scene-pause"
      aria-pressed={paused}
      aria-label={paused ? "resume the preview" : "pause the preview"}
      title={paused ? "resume the composite preview" : "pause the composite preview"}
      on:click={() => (paused = !paused)}
    >
      {#if paused}
        <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M8 5.2 19.2 12 8 18.8Z" />
        </svg>
      {:else}
        <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <rect x="6" y="5" width="4" height="14" rx="1" />
          <rect x="14" y="5" width="4" height="14" rx="1" />
        </svg>
      {/if}
      <span class="tlabel">{paused ? "Resume" : "Pause"}</span>
    </button>

    <span class="spacer"></span>

    <!-- Save gives the same feedback as the pattern editor's (#738): the
         label becomes a spinner while the write is in flight, then "Saved"
         for a second, and it simply READS "Saved" whenever there is nothing
         dirty to write. `settled` is not `!dirty` alone — a scene that has
         never been saved is clean and stored nowhere, and a button claiming
         "Saved" there would be lying. -->
    <AsyncButton
      cls="btn primary"
      dataRole="scene-save"
      label="Save"
      doneLabel="Saved"
      settled={!dirty && doc.id !== ""}
      action={save}
    />

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

    <!-- The header's rail segment (#736 item 20). Its `border-left` is the
         INSPECTOR column's own left border, continued up through the bar —
         which is both what makes the header read as three columns like the
         body and what stops Save at the same place the pattern editor stops
         it. `statusonly` always: what it holds is a status readout, and the
         phone rules drop it rather than wrap the header onto a second row.
         The chip is the same one the pattern editor's rail carries, and for
         the same reason — the shell header does not exist over a full-screen
         screen, so without it nothing on this screen says which device the
         scene is being edited against (#538). The playground's rail is empty
         on purpose: its chip, `Preview as`, is a CONTROL, and the layout it
         sets belongs to the map screen this component cannot reach. -->
    <span class="edhdr-rail statusonly">
      {#if !$isPlayground}
        <DeviceChip />
      {/if}
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
      {layerErrors}
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
      {dimsLine}
      {targetFps}
      paintMode={painting}
      markCell={painting ? hoverCell : null}
      on:rect={(e) => onRect(e.detail)}
      on:targetfps={(e) => (targetFps = e.detail)}
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

    <div class="rcol" data-role="scene-inspector" bind:this={rcolEl}>
      <!-- THE picker hangs HERE, under the inspector column, because
           `.menu.full` is `position:absolute` against its nearest positioned
           ancestor: as a child of `<main class="editor-frame">` (which IS
           `position:relative`) it opened one full screen height below the
           fold — invisible, which is what "pressing Change… does nothing"
           looked like (#730). The wrapper is zero-height so the list starts
           at the top of the column, beside the button that opened it.

           No `scenes` are passed on purpose: a layer names a stored PATTERN
           and there are no nested scenes (docs/spec/scenes.md), so the
           picker's Scenes section is empty here by design. -->
      <div class="pickwrap">
        <PatternPicker
          luxel={$luxel}
          open={pickerOpen}
          anchor={rcolEl}
          title={pickingSprite ? "Choose a sprite" : "Choose a pattern"}
          patterns={pickerPatterns}
          busy={pickerBusy}
          error={pickerError}
          on:pick={(e) => void onPick(e)}
          on:close={() => {
            pickerOpen = false;
            pickerError = "";
          }}
        />
      </div>

      <!-- A layer that draws nothing SAYS why (#731): the compositor binds no
           engine for a pattern that will not compile and then renders an
           empty layer, which reads as "the preview is broken". -->
      {#if sceneError !== ""}
        <p class="lerr" data-role="scene-error">{sceneError}</p>
      {/if}
      {#if selected >= 0 && layerErrors[selected] !== undefined}
        <p class="lerr" data-role="scene-layer-error">
          This layer’s pattern does not compile — {layerErrors[selected]}
        </p>
      {/if}

      {#if sel && sel.body.kind === "pat"}
        <PatternInspector
          layer={sel}
          engine={renderer?.engineAt(selected) ?? null}
          source={lookup(sel.body.pat.id)}
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

</main>

<style>
  /* The scene editor's body is the three columns, not the code/rail split —
     `.editor-frame` gives it the header row and the full-height grid. */
  .scene-editor :global(.scene3) {
    min-height: 0;
  }

  /* The picker's own `.menu.full` is `left:0;right:0;top:calc(100% + 7px)`,
     so its anchor must be a zero-height box at the top of the inspector
     column — the list then spans the column and drops just under its head,
     next to the `Change…` button that opened it (#730). */
  .pickwrap {
    position: relative;
    height: 0;
  }

  /* a failure the user can act on, in the column that owns the layer */
  .lerr {
    margin: 0 0 12px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--error);
  }
</style>
