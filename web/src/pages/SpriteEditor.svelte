<script lang="ts">
  // THE sprite editor (Gitea #741 — the full redesign).
  //
  // Jeremy's verdict on what was here before: "the sprite draw tools are sad,
  // have tiny confusing buttons, and incredibly unintuitive. The color palette
  // doesn't make sense, etc. It needs a full redesign." Every complaint in
  // #741 is answered by a decision in this file, so each is named where it is
  // answered:
  //
  //   * moving a sprite layer — there is no paint mode on the scene stage any
  //     more. Drawing happens HERE, on its own screen, and a selected sprite
  //     layer moves with the marquee exactly like every other layer
  //     (`pages/SceneEditor.svelte`, `components/scene/SpriteTools.svelte`
  //     deleted).
  //   * the "recents" filling with colours you dragged past — there are NO
  //     recents. The "In use" grid is read off the record, so a colour appears
  //     there only once it has been painted (`usedColors`).
  //   * "click or drag on the preview to paint · erased pixels are
  //     transparent" and "a frame strip appears at 2+" — both gone; the
  //     checkerboard says what transparent means and the frame strip is real.
  //   * the 16-colour limit — 255 now, the format's own, enforced in ONE place
  //     (`colorIndex` in lib/sprite.ts) with one sentence.
  //   * "1:1 · sprites are never scaled" — scaling is a scene-layer choice
  //     now (Fit in the sprite inspector), not a fact about sprites.
  //
  // A FOURTH full-screen screen, peer of the pattern, map and scene editors —
  // `data-role="sprite-editor-view"` on its own `<main>`, because all of them
  // are mounted at once and an unscoped selector would resolve to the wrong
  // one (.claude/rules/web.md).
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import "../components/scene/scene.css";
  import "../components/editor-frame.css";
  import AsyncButton from "../components/AsyncButton.svelte";
  import NameField from "../components/NameField.svelte";
  import Popover from "../components/Popover.svelte";
  import SceneSwatch from "../components/scene/SceneSwatch.svelte";
  import FrameStrip from "../components/sprite/FrameStrip.svelte";
  import SpriteCanvas from "../components/sprite/SpriteCanvas.svelte";
  import SpriteThumb from "../components/sprite/SpriteThumb.svelte";
  import {
    addFrame,
    colorIndex,
    deleteFrame,
    duplicateFrame,
    encodeSprite,
    fill,
    frameAt,
    hexToRgb8,
    moveFrame,
    newSprite,
    paint,
    resize,
    rgb8ToHex,
    rgbCss,
    SPRITE_MAX_EDGE,
    SPRITE_MAX_FPS,
    SPRITE_MAX_NAME,
    spriteBytes,
    usedColors,
    type Rgb8,
    type Sprite,
    type SpriteTool,
  } from "../lib/sprite";
  import { confirm } from "../stores/dialog";
  import { isPlayground } from "../stores/device";
  import { layout } from "../stores/geometry";
  import { note } from "../stores/notify";
  import {
    deleteSprite,
    duplicateSprite,
    loadSprite,
    refreshSprites,
    saveSprite,
    spriteMaxBytes,
    spriteSaving,
    sprites,
    usedBy,
  } from "../stores/sprites";
  import { scenes } from "../stores/scenes";

  /** Mounted always; shown when the route says so (like the other screens). */
  export let active = false;

  /** Which sprite — `#/sprites/<id>`. Empty = a new, unsaved one. */
  export let spriteId = "";

  /**
   * Where the back button goes. `""` = the Sprites tab; a scene id = the
   * scene editor that sent us here, and the label becomes `← Scene`.
   *
   * It is a PROP rather than a route segment on purpose: it is "where I came
   * from", not "what is open", and putting it in the URL would make a shared
   * link claim a return trip the recipient never took.
   */
  export let returnScene = "";

  const dispatch = createEventDispatcher<{ back: void; open: string; scene: string }>();

  // ---- the document ----

  /** The working copy. The store's record is never mutated in place. */
  let doc: Sprite = newSprite("Sprite", 8, 8);
  /** The record `doc` was last saved as — the dirty test, in BYTES, because
   *  that is the only canonical form a sprite has. */
  let savedWire = "";
  let loadedId: string | null = null;

  /** The id the document has in the store; `""` until its first save. */
  let docId = "";

  $: wire = bytesKey(encodeSprite(doc));
  $: dirty = wire !== savedWire;
  $: bytes = spriteBytes(doc);
  $: overCap = bytes > $spriteMaxBytes;

  /** The header's contract string, on the ATTRIBUTE as every other editor
   *  carries it (#738): the Save button says "Save"/"Saved" itself. */
  $: saveState = dirty ? "unsaved changes" : docId === "" ? "not saved yet" : $isPlayground ? "saved · in browser" : "saved · on device";
  $: saveWhere = $isPlayground && !dirty && docId !== "" ? "in browser" : "";

  // ---- loading ----
  // `$sprites` is the reader; `spriteId` is the route. Re-reading only when
  // the id CHANGES is what keeps an in-progress drawing from being stomped by
  // the 2 Hz poll.
  $: if (active) void maybeAdopt(spriteId, $sprites);

  // A deep link (`#/sprites/<id>` in a fresh tab) lands HERE, not on the
  // Sprites tab, so this screen asks for the library itself.
  $: if (active) void refreshSprites();

  async function maybeAdopt(id: string, list: readonly { id: string }[]): Promise<void> {
    if (id === loadedId) return;
    if (id !== "" && !list.some((s) => s.id === id)) return;
    loadedId = id;
    const found = id === "" ? null : await loadSprite(id);
    docId = found ? id : "";
    doc = found ? clone(found) : newSprite(freshName(), 8, 8);
    savedWire = bytesKey(encodeSprite(doc));
    frame = 0;
    playing = false;
    undoStack = [];
    redoStack = [];
    brush = usedColors(doc)[0]?.rgb ?? [232, 163, 61];
  }

  function clone(s: Sprite): Sprite {
    return {
      ...s,
      palette: s.palette.map((c) => [c[0], c[1], c[2]] as Rgb8),
      index: new Uint8Array(s.index),
    };
  }

  function freshName(): string {
    const taken = new Set($sprites.map((s) => s.name));
    for (let n = 1; n < 1000; n++) {
      const name = `Sprite ${n}`;
      if (!taken.has(name)) return name;
    }
    return "Sprite";
  }

  /** A cheap identity for a record's bytes — the dirty test and nothing else,
   *  so a hash would be overkill and a string is exact. */
  function bytesKey(b: Uint8Array): string {
    let s = "";
    for (let i = 0; i < b.length; i += 4096) s += String.fromCharCode(...b.subarray(i, i + 4096));
    return s;
  }

  // ---- tools ----

  let tool: SpriteTool = "pencil";
  /** The colour the pencil and the fill paint with. */
  let brush: Rgb8 = [232, 163, 61];
  let frame = 0;
  let playing = false;
  let onion = false;

  const TOOLS: { id: SpriteTool; label: string; key: string; path: string[] }[] = [
    { id: "pencil", label: "Pencil", key: "1", path: ["M4 20l4-1L20 7a2 2 0 0 0-3-3L5 16z", "M15 6l3 3"] },
    { id: "eraser", label: "Eraser", key: "2", path: ["M7 20h13", "M5 16l7-7 6 6-5 5H7z"] },
    {
      id: "fill",
      label: "Fill",
      key: "3",
      path: ["M5 11l7-7 7 7-7 7z", "M19 15c1.4 1.9 2 2.9 2 3.8a2 2 0 1 1-4 0c0-.9.6-1.9 2-3.8z"],
    },
    { id: "pick", label: "Pick", key: "4", path: ["M13 3l8 8-3 1-6 6-4 1-2-2 1-4 6-6z", "M6 18l-2 2"] },
  ];

  /** The colours the DRAWING is made of, in first-appearance order — #741's
   *  answer to the recents strip: a colour is here because it was painted,
   *  never because the picker emitted it while you dragged. */
  $: inUse = usedColors(doc);
  $: brushHex = rgb8ToHex(brush);

  function setBrushHex(hex: string): void {
    const rgb = hexToRgb8(hex);
    if (rgb) brush = rgb;
  }

  // ---- undo / redo ----
  //
  // One step per STROKE, not per texel: a drag is one thing you did, so it is
  // one thing to undo. A resize, a frame op and a palette change are each one
  // step too. Capped at 100 — a 64×64×255 record is 1 MB, and a hundred of
  // those is not something to hold in a tab.

  const UNDO_CAP = 100;
  let undoStack: Sprite[] = [];
  let redoStack: Sprite[] = [];
  /** True between pointerdown and pointerup: the stroke's step is already on
   *  the stack, so the texels after the first must not push their own. */
  let stroking = false;

  function push(): void {
    undoStack = [...undoStack.slice(-(UNDO_CAP - 1)), clone(doc)];
    redoStack = [];
  }

  function undo(): void {
    const prev = undoStack[undoStack.length - 1];
    if (!prev) return;
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack, clone(doc)];
    doc = prev;
    clampFrame();
  }

  function redo(): void {
    const next = redoStack[redoStack.length - 1];
    if (!next) return;
    redoStack = redoStack.slice(0, -1);
    undoStack = [...undoStack, clone(doc)];
    doc = next;
    clampFrame();
  }

  function clampFrame(): void {
    if (frame >= doc.frames) frame = doc.frames - 1;
  }

  // ---- drawing ----

  function onCell(
    e: CustomEvent<{ col: number; row: number; phase: "down" | "move"; erase: boolean; sample: boolean }>,
  ): void {
    const { col, row, phase, erase, sample } = e.detail;
    // Shift is a temporary eyedropper and so is the Pick tool; neither is an
    // edit, so neither takes an undo step.
    if (sample || tool === "pick") {
      const k = doc.index[frame * doc.w * doc.h + row * doc.w + col] ?? 0;
      const rgb = k === 0 ? null : doc.palette[k - 1];
      if (rgb) brush = [rgb[0], rgb[1], rgb[2]];
      return;
    }
    const erasing = erase || tool === "eraser";
    // A fill is a CLICK, not a drag — dragging a bucket across a drawing is
    // never what anyone meant.
    if (tool === "fill" && phase === "move") return;

    if (phase === "down") {
      push();
      stroking = true;
    } else if (!stroking) {
      return;
    }

    let k = 0;
    let next = doc;
    if (!erasing) {
      const slot = colorIndex(doc, brush);
      if (typeof slot === "string") {
        // THE one enforcement point's refusal, said once (#741 item 14).
        note("sprite", slot, 5000);
        return;
      }
      next = slot.sprite;
      k = slot.k;
    }
    doc = tool === "fill" ? fill(next, frame, col, row, k) : paint(next, frame, col, row, k);
  }

  function endStroke(): void {
    stroking = false;
  }

  // ---- the sprite's own fields ----

  function setSize(field: "w" | "h", el: HTMLInputElement): void {
    const v = Math.max(1, Math.min(SPRITE_MAX_EDGE, Math.round(Number(el.value))));
    if (!Number.isFinite(v)) {
      el.value = String(doc[field]);
      return;
    }
    // Clamp IN the field rather than refusing, so what you see is what is
    // stored (the rule every other size field in this app keeps).
    if (String(v) !== el.value) el.value = String(v);
    if (v === doc[field]) return;
    push();
    doc = field === "w" ? resize(doc, v, doc.h) : resize(doc, doc.w, v);
  }

  function setFps(el: HTMLInputElement): void {
    const v = Math.max(0, Math.min(SPRITE_MAX_FPS, Math.round(Number(el.value))));
    if (!Number.isFinite(v)) {
      el.value = String(doc.fps);
      return;
    }
    if (String(v) !== el.value) el.value = String(v);
    if (v === doc.fps) return;
    push();
    doc = { ...doc, fps: v };
  }

  function commitName(name: string): void {
    if (name === doc.name) return;
    push();
    doc = { ...doc, name };
  }

  // ---- frames ----

  function onAddFrame(at: number): void {
    push();
    doc = addFrame(doc, at);
    frame = Math.min(at + 1, doc.frames - 1);
  }

  function onDuplicateFrame(at: number): void {
    push();
    doc = duplicateFrame(doc, at);
    frame = Math.min(at + 1, doc.frames - 1);
  }

  function onDeleteFrame(at: number): void {
    if (doc.frames <= 1) return;
    push();
    doc = deleteFrame(doc, at);
    frame = Math.min(at, doc.frames - 1);
  }

  function onMoveFrame(from: number, to: number): void {
    if (to < 0 || to >= doc.frames) return;
    push();
    doc = moveFrame(doc, from, to);
    frame = to;
  }

  /** The strip's Play cycles the EDITED frame, so the canvas animates too —
   *  what you are looking at is what will run. */
  let playRaf = 0;
  let playFrom = 0;

  function loop(now: number): void {
    playRaf = requestAnimationFrame(loop);
    if (!playing) {
      playFrom = now;
      return;
    }
    frame = frameAt(doc, now - playFrom);
  }

  onMount(() => {
    playRaf = requestAnimationFrame(loop);
  });

  onDestroy(() => cancelAnimationFrame(playRaf));

  // ---- the document's verbs ----

  async function save(): Promise<boolean> {
    if (overCap) return false;
    const r = await saveSprite(doc, docId);
    if (!r.ok) return false;
    if (r.id && r.id !== docId) {
      docId = r.id;
      loadedId = r.id;
      // A NEW sprite gets its route, so a reload reopens what is open.
      dispatch("open", r.id);
    }
    savedWire = wire;
    return true;
  }

  let menuOpen = false;
  let moreBtn: HTMLElement | null = null;

  async function duplicate(): Promise<void> {
    menuOpen = false;
    if (docId === "") return;
    const id = await duplicateSprite(docId);
    if (id) dispatch("open", id);
  }

  async function remove(): Promise<void> {
    menuOpen = false;
    if (docId === "") return;
    const used = usedBy(docId);
    const ok = await confirm({
      title: `Delete “${doc.name}”?`,
      body:
        used.length > 0
          ? `The drawing is removed. ${used.length === 1 ? "The scene" : "The scenes"} ${used.join(", ")} will have a sprite layer with nothing to draw.`
          : "The drawing is removed. Nothing else changes.",
      confirmLabel: "Delete sprite",
      danger: true,
    });
    if (!ok) return;
    if (await deleteSprite(docId)) leave(true);
  }

  /** Back — and a document with unsaved changes asks first, through the app's
   *  ONE dialog primitive (`stores/dialog`), never `window.confirm`. */
  async function goBack(): Promise<void> {
    if (dirty) {
      const ok = await confirm({
        title: "Leave without saving?",
        body: `“${doc.name}” has changes that have not been saved.`,
        confirmLabel: "Discard changes",
        cancelLabel: "Keep editing",
        danger: true,
      });
      if (!ok) return;
    }
    leave(false);
  }

  function leave(deleted: boolean): void {
    if (deleted) savedWire = wire; // nothing left to warn about
    if (returnScene !== "") dispatch("scene", returnScene);
    else dispatch("back");
  }

  // ---- keys ----
  //
  // Only while this screen is up, and never while a field has the focus: 1-4
  // are tools, not text, but they are also digits someone is typing into
  // `w`/`h`/`fps`.

  function onKey(e: KeyboardEvent): void {
    if (!active) return;
    const t = e.target as HTMLElement | null;
    const tag = t?.tagName ?? "";
    if (tag === "INPUT" || tag === "TEXTAREA" || t?.isContentEditable) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
      return;
    }
    if (mod && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redo();
      return;
    }
    if (mod) return;
    const hit = TOOLS.find((x) => x.key === e.key);
    if (hit) {
      e.preventDefault();
      tool = hit.id;
      return;
    }
    if (e.key === "[") {
      e.preventDefault();
      frame = Math.max(0, frame - 1);
    } else if (e.key === "]") {
      e.preventDefault();
      frame = Math.min(doc.frames - 1, frame + 1);
    } else if (e.key === " ") {
      if (doc.fps === 0 || doc.frames < 2) return;
      e.preventDefault();
      playing = !playing;
    }
  }

  /** `usedBy` reads the SCENE store through `get()`, so `$scenes` is named as
   *  an argument here or this would never re-run as the library lands
   *  (.claude/rules/web.md). */
  $: used = usedByOf(docId, $scenes);

  function usedByOf(id: string, _scenes: unknown): string[] {
    return usedBy(id);
  }

  /** `1,234` — thousands separated, so a byte count is readable at a glance
   *  and does not depend on the browser's locale for its grouping. */
  function fmt(n: number): string {
    return n.toLocaleString("en-US");
  }

  // ---- the Preview's scale ----
  //
  // The fixture the sprite will end up on: the device's Layout on a console,
  // the `Preview as` choice in the playground — the ONE Layout, same as every
  // other screen (#539/#573).
  const PREVIEW_PX = 128;
  $: rig = $layout;
  $: panelZoom = Math.max(1, Math.floor(PREVIEW_PX / Math.max(8, rig.w || 64)));
</script>

<svelte:window on:keydown={onKey} />

<main
  class="editor-frame scenes sprite-editor"
  class:playground={$isPlayground}
  data-role="sprite-editor-view"
  hidden={!active}
>
  <header class="editor-header" data-role="sprite-editor-header">
    <button class="btn quiet back" data-role="sprite-editor-back" on:click={() => void goBack()}>
      <span class="backglyph" aria-hidden="true">←</span>
      <span class="backlabel">{returnScene === "" ? "Sprites" : "Scene"}</span>
    </button>

    <!-- THE rename control, the same component every editor header wears. The
         inspector column carries a read-only echo of the name and no second
         field: nothing about a sprite is edited in two places. -->
    <NameField
      value={doc.name}
      label="sprite name"
      dataRole="sprite-name"
      inputRole="sprite-name-input"
      errorRole="sprite-name-error"
      maxBytes={SPRITE_MAX_NAME}
      on:commit={(e) => commitName(e.detail)}
    />

    <span class="savestate" data-role="sprite-save-state" data-save-state={saveState}
      >{saveWhere}</span
    >

    <span class="spacer"></span>

    <AsyncButton
      cls="btn primary"
      dataRole="sprite-save"
      label="Save"
      doneLabel="Saved"
      settled={!dirty && docId !== ""}
      disabled={overCap}
      reason={overCap ? `this sprite is ${fmt(bytes)} B — the limit is ${fmt($spriteMaxBytes)} B. Make it smaller, drop a frame, or use fewer colours.` : ""}
      action={save}
    />

    <span class="overflow">
      <button
        class="btn icon"
        bind:this={moreBtn}
        data-role="sprite-overflow"
        title="more actions"
        aria-label="more actions"
        on:click={() => (menuOpen = !menuOpen)}>⋯</button
      >
      <Popover
        open={menuOpen}
        anchor={moreBtn}
        dataRole="sprite-menu"
        on:close={() => (menuOpen = false)}
      >
        <button class="mi" data-role="sprite-duplicate" on:click={() => void duplicate()}
          >Duplicate</button
        >
        {#if docId !== ""}
          <div class="sepr"></div>
          <button class="mi del" data-role="sprite-delete" on:click={() => void remove()}
            >Delete sprite</button
          >
        {/if}
      </Popover>
    </span>

    <!-- The header's rail segment, continuing the inspector column's own left
         border up through the bar — what stops Save at the same place every
         other editor stops it (#736 item 20). -->
    <span class="edhdr-rail statusonly"></span>
  </header>

  <div class="scene3">
    <div class="lcol" data-role="sprite-tools">
      <div class="rhead"><div class="slabel">Tools</div></div>

      <!-- FOUR LARGE LABELLED BUTTONS (#741). What was here before was three
           14px icon buttons with no words on them — "tiny confusing buttons".
           Icon AND word, 36px tall, and the keyboard shortcut on the row. -->
      <div class="toolgrid">
        {#each TOOLS as t (t.id)}
          <button
            class="tool"
            class:on={tool === t.id}
            data-role={`sprite-tool-${t.id}`}
            aria-pressed={tool === t.id}
            title={`${t.label} (${t.key})`}
            on:click={() => (tool = t.id)}
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              {#each t.path as d (d)}<path {d} />{/each}
            </svg>
            <span class="tnm">{t.label}</span>
            <span class="tkey">{t.key}</span>
          </button>
        {/each}
      </div>

      <div class="irule"></div>

      <div class="rhead"><div class="slabel">Colour</div></div>
      <!-- ONE picker — the app's one colour control (components/ColorPicker
           via SceneSwatch), not a second popover of its own. -->
      <SceneSwatch value={brushHex} label="sprite colour" dataRole="sprite-color" on:input={(e) => setBrushHex(e.detail)} />

      <div class="usehead">
        <span class="ilab">In use</span>
        <span class="rdim" data-role="sprite-inuse-count">{inUse.length}</span>
      </div>
      {#if inUse.length === 0}
        <p class="hint" data-role="sprite-inuse-empty">
          Colours appear here once you paint with them.
        </p>
      {:else}
        <div class="inuse" data-role="sprite-inuse">
          {#each inUse as c (c.k)}
            <button
              class="sw"
              class:cur={c.rgb[0] === brush[0] && c.rgb[1] === brush[1] && c.rgb[2] === brush[2]}
              data-role="sprite-inuse-swatch"
              data-value={rgb8ToHex(c.rgb)}
              title={`use ${rgbCss(c.rgb)}`}
              aria-label={`use ${rgbCss(c.rgb)}`}
              style={`background:${rgbCss(c.rgb)}`}
              on:click={() => (brush = [c.rgb[0], c.rgb[1], c.rgb[2]])}
            ></button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="ccol" data-role="sprite-stage">
      <div class="cstack">
        <div class="rhead">
          <div class="slabel">Drawing</div>
          <div class="rdim" data-role="sprite-stage-dims">{doc.w}×{doc.h}</div>
        </div>

        <SpriteCanvas
          sprite={doc}
          {frame}
          {onion}
          on:cell={onCell}
          on:end={endStroke}
        />

        <FrameStrip
          sprite={doc}
          {frame}
          {playing}
          {onion}
          on:select={(e) => (frame = e.detail)}
          on:add={(e) => onAddFrame(e.detail)}
          on:duplicate={(e) => onDuplicateFrame(e.detail)}
          on:remove={(e) => onDeleteFrame(e.detail)}
          on:move={(e) => onMoveFrame(e.detail.from, e.detail.to)}
          on:playing={(e) => (playing = e.detail)}
          on:onion={(e) => (onion = e.detail)}
        />
      </div>
    </div>

    <div class="rcol" data-role="sprite-inspector">
      <div class="rhead">
        <div class="slabel">Sprite</div>
        {#if $spriteSaving}<div class="rdim" style="margin-left:auto" data-role="sprite-saving">saving…</div>{/if}
      </div>

      <!-- Name is the HEADER's control; this is the echo, so the value is
           visible in the column that describes the record without being
           editable in two places. -->
      <div class="irow">
        <div class="ilab">Name</div>
        <div class="hint" data-role="sprite-name-echo">{doc.name}</div>
      </div>

      <div class="irow">
        <div class="ilab">Size</div>
        <div class="boxrow" data-role="sprite-size">
          <label
            >w <input
              class="inp"
              type="number"
              min="1"
              max={SPRITE_MAX_EDGE}
              data-role="sprite-w"
              value={doc.w}
              on:change={(e) => setSize("w", e.currentTarget)}
            /></label
          >
          <label
            >h <input
              class="inp"
              type="number"
              min="1"
              max={SPRITE_MAX_EDGE}
              data-role="sprite-h"
              value={doc.h}
              on:change={(e) => setSize("h", e.currentTarget)}
            /></label
          >
          <span class="un">px</span>
        </div>
      </div>

      <div class="irow">
        <div class="ilab">Frames</div>
        <div class="hint" data-role="sprite-frames-count">
          {doc.frames} · managed in the strip
        </div>
      </div>

      <div class="irow start">
        <div class="ilab" style="padding-top:5px">FPS</div>
        <div>
          <input
            class="inp num xs"
            style="width:58px"
            type="number"
            min="0"
            max={SPRITE_MAX_FPS}
            data-role="sprite-fps"
            value={doc.fps}
            on:change={(e) => setFps(e.currentTarget)}
          />
          <div class="hint" style="margin-top:6px">0 = still</div>
        </div>
      </div>

      <div class="irow">
        <div class="ilab">Record</div>
        <div class="hint" class:over={overCap} data-role="sprite-bytes">
          {fmt(bytes)} of {fmt($spriteMaxBytes)} B
        </div>
      </div>

      <div class="irule"></div>

      <div class="rhead">
        <div class="slabel">Preview</div>
        <div class="rdim" data-role="sprite-preview-scale">on {rig.w}×{rig.h}</div>
      </div>
      <!-- On BLACK and at the fixture's own scale: the tile is `rig.w` texels
           wide, so the sprite sits in it exactly as big as it will be on the
           panel, animating at its own fps. A literal 1:1 (one screen pixel per
           texel) would be a 6 px speck in a 128 px box and tell you nothing. -->
      <div class="prev">
        <SpriteThumb
          sprite={doc}
          size={PREVIEW_PX}
          zoom={panelZoom}
          ground="black"
          dataRole="sprite-preview"
        />
      </div>

      <div class="irow" style="margin-top:12px">
        <div class="ilab">Used in</div>
        <div class="hint" data-role="sprite-usedby">
          {used.length === 0 ? "not used yet" : used.join(", ")}
        </div>
      </div>
    </div>
  </div>
</main>

<style>
  .sprite-editor :global(.scene3) {
    min-height: 0;
  }

  /* FOUR big buttons, two up — icon, word and the key that picks it. 36px
     tall so a finger can hit one, which three 14px icon buttons never were
     (#741: "tiny confusing buttons"). */
  .toolgrid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    margin-bottom: 4px;
  }

  .tool {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 36px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-panel);
    color: var(--text);
    font-size: 12.5px;
    text-align: left;
  }

  .tool:hover {
    border-color: var(--accent);
  }

  .tool.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #16110a;
  }

  .tnm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tkey {
    flex: none;
    font: 10px/1 var(--mono);
    opacity: 0.6;
  }

  .usehead {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin: 12px 0 7px;
  }

  .usehead .rdim {
    margin-left: auto;
  }

  /* The colours the drawing is MADE of. No fixed cell count: an empty grid of
     dashed placeholders was a limit drawn as furniture, and the limit is 255
     now (#741 item 14). */
  .inuse {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .sw {
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.16);
  }

  .sw.cur {
    box-shadow:
      0 0 0 2px var(--bg-panel),
      0 0 0 3px var(--accent);
  }

  /* the byte line turns to the danger colour when the record will not fit —
     and Save carries the reason (`AsyncButton`'s `reason`) */
  .hint.over {
    color: var(--error);
  }

  .prev {
    width: 128px;
    max-width: 100%;
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
  }

  .un {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
  }

  /* the spinners would not fit a 46px field (BoxRow keeps the same rule) */
  input[type="number"] {
    appearance: textfield;
    -moz-appearance: textfield;
  }

  input[type="number"]::-webkit-outer-spin-button,
  input[type="number"]::-webkit-inner-spin-button {
    appearance: none;
    margin: 0;
  }

  /* a finger needs 24px below the phone breakpoint (§5.7) */
  @media (max-width: 600px) {
    .sw {
      width: 26px;
      height: 26px;
    }

    .toolgrid {
      grid-template-columns: 1fr 1fr;
    }
  }
</style>
