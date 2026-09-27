<script lang="ts">
  // THE colour ramp editor — one component, both of the app's mounts for it:
  // Settings › Advanced › Output processing › Color ramp (the device's own
  // output palette) and the scene editor's per-layer Color ramp. Gitea #787,
  // mock frames S8a–S8i in docs/design/webui-v2/mockups.html, design record in
  // docs/design/webui-v2/ramp-editor.md. It REPLACES `GradientEditor.svelte`
  // and the two divergent copies before it; there is no second implementation
  // of any of this (#697).
  //
  // Jeremy, 2026-09-26: "a complete UI redesign of color ramp (it is a
  // horrible/confusing interface, and very buggy)". The 21-defect inventory
  // that drove the redesign is the first comment on #787; every rule that can
  // lose the user's work lives in `lib/gradient.ts` with `web/tests` on it,
  // because `web/tests` cannot import a `.svelte` file.
  //
  // ── The shape, in one paragraph ────────────────────────────────────────
  // The bar IS the model: click empty bar to add a stop, drag to move, drag
  // off to remove, click to select. It is the engine's own 256-entry LUT
  // painted as a 256x1 canvas (`lx_palette_lut` from wasm, #748) — never a CSS
  // `linear-gradient`, which cannot express the clamp, the truncating 16.16
  // interpolation or the `floor(v·255)` quantization, and that mismatch is why
  // the pre-#734 preview visibly disagreed with the device. There is exactly
  // ONE per-stop row, for the one SELECTED stop, and the selection is the
  // stop's IDENTITY, not its index. The stage is explained in body text. The
  // preview is a labelled pair — what the pattern renders, and what the LEDs
  // do with the ramp at the current amount — with the amount slider beside it.
  // Presets, Reset, Clear, a real Undo. And the editor says which of #563's
  // modes it is in, so wiring that wrong is visible.
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import ColorPicker from "./ColorPicker.svelte";
  import { paintBar, paintGrid } from "../lib/draw";
  import { hexToRgb, rgbToHex, type Rgb } from "../lib/color";
  import {
    MAX_STOPS,
    MIN_STOPS,
    RAMP_PRESETS,
    addStopAt,
    assignIds,
    clampByte,
    clampPct,
    clusterStops,
    history,
    lumaWedge,
    matchingPreset,
    modeText,
    moveStopTo,
    nextInCluster,
    normalizeHex,
    paintLut,
    rampLut,
    remapFrame,
    removeStopId,
    setStopHex,
    stopBytes,
    stripIds,
    syncVerdict,
    trailingCommit,
    widestGapPos,
    type GradientStop,
    type IdStop,
    type RampMode,
  } from "../lib/gradient";
  import { Engine, Luxel } from "../lib/luxel";
  import { compileForLayout, layoutSignature, THUMB_MAX_CELLS } from "../stores/geometry";
  import { luxel } from "../stores/pattern";

  /** The ramp as its owner holds it — the DEVICE's / the scene record's truth.
   *  The editor keeps its own working copy and re-reads this while open. */
  export let stops: readonly GradientStop[] = [];
  /** Blend amount 0..100 — the scene's `Ramp.pct`, the device's
   *  `paletteAmount`. */
  export let amount = 100;
  /** `data-role` prefix (`out-palette` / `scene-ramp`): every role this
   *  component emits hangs off it, which is what lets the two mounts keep the
   *  names their harnesses and `mockdiff.map.json` already use. */
  export let role = "ramp";
  /** The bar's own role — the two mounts named it differently before #787. */
  export let previewRole = "";
  /** Which of #563's modes this mount is in. A VISIBLE property. */
  export let mode: RampMode = "preview";
  /** Whose pixels the ramp recolours: the whole device, or one scene layer.
   *  Decides the statement line, the destructive verb and the preview labels —
   *  the three places S8a and S8f deliberately differ. */
  export let scope: "device" | "layer" = "device";
  /** Open (the editor) or collapsed (the summary row, S8e). Bindable. */
  export let expanded = false;
  /** The pattern whose pixels the right-hand preview cell shows through the
   *  ramp: the device's running pattern, or the layer's. `null` = nothing
   *  running, and the left cell falls back to a brightness wedge LABELLED as
   *  one (S8h) rather than going empty. */
  export let patternSource: string | null | undefined = null;
  /** Its name, for the preview caption. */
  export let patternName = "";

  const dispatch = createEventDispatcher<{
    /** Mid-gesture: cheap, local, do NOT push it to a device. */
    input: { stops: GradientStop[]; amount: number };
    /** Committed: the value to store or push. */
    change: { stops: GradientStop[]; amount: number };
    /** The ramp itself is gone — the one destructive action, confirmed. */
    clear: null;
  }>();

  /** How far off the bar a drag has to go to mean "delete this stop". */
  const DROP_OFF = 28;
  /** Below this card width the editor is one column with short labels — the
   *  mock reflows on the CARD, not the window (S8f is 288 px inside a 1200 px
   *  console). */
  const NARROW_PX = 520;
  /** Handles closer together than this many SCREEN pixels collapse into one
   *  with a count badge (S8h, inventory item 15). */
  const CLUSTER_PX = 3;

  // ── the working copy ───────────────────────────────────────────────────

  let work: IdStop[] = [];
  let workAmount = amount;
  /** The external value the editor last adopted or last committed OUTWARD.
   *  `null` until the first sync. Comparing against it is how an echo of our
   *  own write is told from the device changing underneath us. */
  let loaded: GradientStop[] | null = null;
  let loadedAmount = amount;
  /** The last list this editor put into the world, COMMITTED OR NOT. A mount
   *  that echoes an `input` straight back through its `stops` prop (the
   *  Settings card does, optimistically) would otherwise look exactly like the
   *  device changing underneath us, and a single arrow-key nudge raised the
   *  conflict strip against itself. */
  let mine: GradientStop[] | null = null;
  /** Selection is an identity (inventory item 8): an index outlived its stop
   *  and every edit through the panel then silently edited the neighbour. */
  let selId: number | null = null;

  const hist = history<{ stops: IdStop[]; amount: number }>({ stops: [], amount });
  /** `hist.canUndo()` as a REACTIVE value. A `$:` block or a template that
   *  called the method would never re-run on a push — Svelte tracks variables
   *  the block NAMES, not state inside an object a call reads
   *  (.claude/rules/web.md). */
  let edited = false;

  /** Remember the BEFORE state for Undo — once per gesture, never once per
   *  pointermove. */
  function push(): void {
    hist.push({ stops: work, amount: workAmount });
    edited = true;
  }

  /** The device's ramp moved while this editor had local edits (S8i). Nothing
   *  is sent until the user chooses (inventory item 1 — the old editor pushed
   *  its stale stops AND its stale amount over whatever the device held). */
  let conflict: { stops: GradientStop[]; amount: number } | null = null;
  /** The Clear/Remove confirm strip is up. */
  let confirming = false;

  let card: HTMLElement | null = null;
  let bar: HTMLCanvasElement | null = null;
  let srcCanvas: HTMLCanvasElement | null = null;
  let outCanvas: HTMLCanvasElement | null = null;
  let hnds: HTMLElement | null = null;
  let barW = 0;
  let narrow = false;
  let drag: { id: number; ptr: number; off: boolean } | null = null;
  let keyMoved = false;
  /** Is the left preview cell showing a real pattern, or the brightness wedge?
   *  Assigned where the frames are, not derived from `patternSource` — a
   *  source that does not COMPILE has no picture either, and a caption that
   *  named a pattern over a wedge would be the same class of lie as the old
   *  control's two unlabelled bars (inventory item 16). */
  let previewLive = false;

  // ── the LUT, from the engine ───────────────────────────────────────────
  // `lx_palette_lut` is the single source (#748): `outpipe::fill_palette_lut`
  // cooks it and `outpipe::palette_remap_frame` blends it, both the engine's
  // own functions. `rampLut` is the synchronous FALLBACK for the first paint
  // before the wasm module is up, and `web/tests/paletteLut.test.mjs` pins the
  // two against each other so they cannot drift.
  function lutFor(list: readonly GradientStop[], pct: number, lx: Luxel | undefined): Uint8Array {
    return lx?.paletteLut(stopBytes(list), pct) ?? rampLut(list, pct);
  }

  // ── syncing the owner's value in ───────────────────────────────────────
  //
  // THIS BELONGS ABOVE EVERY `$:` THAT DERIVES FROM `work`, and it cost a
  // debug cycle here. `syncIn` assigns `work` inside a called function, which
  // Svelte cannot see, so the compiler orders this statement by SOURCE
  // POSITION alone — and an invalidation raised during `$$.update()` schedules
  // no second flush. With it at the bottom, the markup (which reads `work`
  // directly) drew four handles' worth of state while every derived value
  // above still said "no stops": the header, the cluster list, the previews
  // and the floor check were all one flush behind, for ever
  // (.claude/rules/web.md; `pages/SceneEditor.svelte` carries the same note).
  $: syncIn(stops, amount);

  $: plain = stripIds(work);
  $: lut100 = lutFor(plain, 100, $luxel);
  $: lutAmt = lutFor(plain, workAmount, $luxel);
  // Every dependency is NAMED in the block's own syntax — a bare `paint()`
  // closing over them would silently stop re-running (.claude/rules/web.md).
  $: paintLut(bar, lut100);
  $: clusters = clusterStops(work, barW > 0 ? (CLUSTER_PX / barW) * 255 : 0);
  $: sel = work.find((s) => s.id === selId) ?? null;
  $: selCluster = clusters.find((c) => selId !== null && c.ids.includes(selId)) ?? null;
  $: atCap = work.length >= MAX_STOPS;
  $: atFloor = work.length <= MIN_STOPS;
  $: preset = matchingPreset(plain);
  $: gapPos = widestGapPos(work);
  $: effMode = conflict ? "hold" : mode;

  // ── the copy, in one place ─────────────────────────────────────────────
  // Inventory item 17: nothing in the old control said what the stage does,
  // which was the single biggest contributor to "confusing". It is body text
  // here, never a hover title, and it is never hidden.

  // The three halves of the statement line, so the mock's ONE bold run lands
  // where the mock puts it: leading the sentence for the device ("Brightness →
  // color."), inside it for a layer ("Recolors this layer by **brightness**").
  const EDGE = "Below the first stop and above the last, the end color continues.";

  $: stateHead = scope === "layer" ? "Recolors this layer by " : "";
  $: stateBold = scope === "layer" ? "brightness" : "Brightness → color.";
  $: stateTail =
    scope === "layer"
      ? `: dark pixels take the left end, bright pixels the right. ${EDGE}`
      : narrow
        ? ` Dark pixels take the left end, bright pixels the right. ${EDGE}`
        : ` Each pixel's brightness picks a color from this ramp: dark pixels take the left end, bright pixels the right.${work.length === 0 ? "" : ` ${EDGE}`}`;

  $: countText =
    work.length === 0
      ? "no stops"
      : atCap
        ? `${MAX_STOPS} stops · the most a ramp can hold`
        : `${work.length} stop${work.length === 1 ? "" : "s"}${edited ? " · edited" : ""}`;

  $: affordance = atCap
    ? "the bar is full — remove a stop before adding another"
    : narrow
      ? "tap to add · drag to move · drag off to remove"
      : "click the bar to add a stop · drag to move · drag off to remove";

  $: destructive = scope === "layer" ? "Remove ramp" : "Clear ramp";
  $: summary = `${work.length === 0 ? "no stops" : `${work.length} stop${work.length === 1 ? "" : "s"}`} · ${workAmount} % — ${
    scope === "layer" ? "recolors this layer by brightness" : "recolors the frame by brightness"
  }`;
  $: outCap =
    scope === "layer"
      ? "through the ramp"
      : narrow
        ? `LEDs · ${workAmount} %`
        : `LEDs · ramp at ${workAmount} %`;

  function syncIn(next: readonly GradientStop[], nextAmount: number): void {
    const list = next.map((s) => ({ pos: clampByte(s.pos), hex: normalizeHex(s.hex) }));
    // An EMPTY ramp carries no amount worth adopting: the stage is off, and
    // `DELETE /api/output/palette` deliberately stores 0 with it
    // (`firmware/src/outpal.rs::clear`). Taking that 0 back would be inventory
    // item 7 by another route — clear the ramp, build a new one from a preset,
    // and it silently does nothing. The editor keeps the amount it had, so the
    // next build goes out at the blend the user was using.
    const amt = list.length === 0 ? workAmount : clampPct(nextAmount);
    // The three-way decision is a RULE, so it lives in `lib/gradient.ts` with
    // tests on it — including the one that says an echo of this editor's own
    // optimistic `input` is never a conflict, which is the trap that muted the
    // whole control when one emitter forgot to record `mine`.
    const verdict = syncVerdict({
      incoming: list,
      incomingAmount: amt,
      loaded,
      loadedAmount,
      mine,
      workAmount,
      edited,
    });
    if (verdict === "ignore") return;
    // a poll landing mid-gesture must not snap a handle back
    if (drag) return;
    if (verdict === "adopt") {
      adopt(list, amt);
      return;
    }
    // it changed underneath local edits: hold, show the strip, send nothing
    conflict = { stops: list, amount: amt };
  }

  function adopt(list: GradientStop[], amt: number): void {
    work = assignIds(list);
    workAmount = amt;
    loaded = list;
    loadedAmount = amt;
    mine = list;
    selId = null;
    hist.reset({ stops: work, amount: amt });
    edited = false;
    conflict = null;
  }

  // ── emitting ───────────────────────────────────────────────────────────

  /** `commit` distinguishes a finished edit from a gesture frame. EVERY write
   *  is computed from the list AT THIS MOMENT — never from a snapshot a timer
   *  captured, which is what inventory item 2 was. */
  function emit(commit: boolean): void {
    if (conflict) return; // S8i: nothing is sent until the user chooses
    const list = stripIds(work);
    mine = list;
    if (commit) {
      colorLatch.cancel();
      loaded = list;
      loadedAmount = workAmount;
      dispatch("change", { stops: list, amount: workAmount });
    } else {
      dispatch("input", { stops: list, amount: workAmount });
    }
  }

  /** Apply an edit. `remember` pushes the BEFORE state for Undo — once per
   *  gesture, not once per pointermove. */
  function apply(next: IdStop[], commit: boolean, remember = true): void {
    if (remember) push();
    work = next;
    emit(commit);
  }

  function setAmount(v: number, commit: boolean, remember = true): void {
    const next = clampPct(v);
    if (next === workAmount && !commit) return;
    if (remember) push();
    workAmount = next;
    emit(commit);
  }

  /** A slider drag is a storm of `input` events and ONE `change`: remember the
   *  before-state once, emit locally per frame, commit at the end. */
  let amtGesture = false;

  function onAmountSlide(v: number): void {
    setAmount(v, false, !amtGesture);
    amtGesture = true;
  }

  function onAmountCommit(v: number): void {
    setAmount(v, true, !amtGesture);
    amtGesture = false;
  }

  /** `ColorPicker` emits on every pointermove across its saturation field, so
   *  a colour edit is a storm like a drag is: live locally, ONE committed
   *  write when the hand stops (or when the popover closes, S8b). */
  const colorLatch = trailingCommit(
    250,
    () => true,
    () => {
      const list = stripIds(work);
      mine = list;
      loaded = list;
      loadedAmount = workAmount;
      if (!conflict) dispatch("change", { stops: list, amount: workAmount });
    },
  );

  onDestroy(() => colorLatch.cancel());

  function onColor(rgb: number[]): void {
    if (selId === null) return;
    const c: Rgb = [rgb[0] ?? 0, rgb[1] ?? 0, rgb[2] ?? 0];
    const hex = rgbToHex(c).replace(/^#/, "");
    // ONE history entry per colour GESTURE, and it has to be the state before
    // the first frame of it — a picker drag is a pointermove storm.
    if (!colorLatch.pending()) push();
    work = setStopHex(work, selId, hex);
    // through `emit`, like every other edit: it is what records `mine`, and a
    // hand-rolled `dispatch("input", …)` here raised the conflict strip
    // against the editor's OWN optimistic echo on the first colour change of
    // any session — after which nothing it did reached the device again.
    emit(false);
    colorLatch.arm();
  }

  const unit = (hex: string): number[] => hexToRgb(hex) ?? [0, 0, 0];

  // ── add / move / remove ────────────────────────────────────────────────

  function addAt(pos: number, commit: boolean): number | null {
    const r = addStopAt(work, pos, lut100);
    if (!r) return null;
    push();
    work = r.stops;
    selId = r.id;
    emit(commit);
    return r.id;
  }

  /** `+ Add stop` — into the WIDEST GAP, never past the last stop (item 5). */
  function addIntoGap(): void {
    if (gapPos === null) return;
    addAt(gapPos, true);
  }

  function move(id: number, pos: number, commit: boolean, remember = false): void {
    apply(moveStopTo(work, id, pos), commit, remember);
  }

  /** A removal removes a stop or is REFUSED — it never escalates into
   *  destroying the whole ramp (item 3). `Clear ramp` is the named action. */
  function removeSelected(): void {
    if (selId === null) return;
    const r = removeStopId(work, selId);
    if (!r) return;
    push();
    work = r.stops;
    selId = r.select;
    emit(true);
  }

  function pickPreset(name: string): void {
    const p = RAMP_PRESETS.find((x) => x.name === name);
    if (!p) return;
    push();
    work = assignIds(p.stops);
    selId = null;
    // a preset NEVER touches the amount (item 7)
    emit(true);
  }

  function undo(): void {
    const prev = hist.undo();
    if (!prev) return;
    work = prev.stops;
    workAmount = prev.amount;
    edited = hist.canUndo();
    if (selId !== null && !work.some((s) => s.id === selId)) selId = null;
    emit(true);
  }

  function reset(): void {
    const b = hist.base();
    push();
    work = b.stops;
    workAmount = b.amount;
    if (selId !== null && !work.some((s) => s.id === selId)) selId = null;
    emit(true);
  }

  /** The one destructive action, confirmed, and it does NOT touch the amount
   *  (item 7: the old `clear` silently set it to 0). */
  function doClear(): void {
    confirming = false;
    work = [];
    selId = null;
    loaded = [];
    mine = [];
    loadedAmount = workAmount;
    // Clearing SETTLES a conflict — it is a deliberate action that supersedes
    // whatever the device did. Leaving it set muted `emit()` for ever, and it
    // could not self-heal: with `loaded`, `mine` and the incoming list all
    // empty, `syncIn` returns on its echo guard and never reaches `adopt()`,
    // which is the only other thing that clears it.
    conflict = null;
    hist.reset({ stops: [], amount: workAmount });
    edited = false;
    dispatch("clear", null);
  }

  function keepMine(): void {
    const c = conflict;
    conflict = null;
    if (!c) return;
    // my ramp wins: send it, so the device and the editor agree again
    emit(true);
  }

  function loadTheirs(): void {
    const c = conflict;
    if (!c) return;
    conflict = null;
    adopt(c.stops, c.amount);
  }

  // ── the bar ────────────────────────────────────────────────────────────

  function posFromX(x: number): number {
    const r = bar?.getBoundingClientRect();
    if (!r || r.width === 0) return 0;
    return clampByte(((x - r.left) / r.width) * 255);
  }

  function onDown(e: PointerEvent): void {
    if (confirming) return;
    const mk = (e.target as Element | null)?.closest?.("[data-stop-id]");
    let id: number | null;
    if (mk instanceof HTMLElement) {
      id = Number(mk.dataset["stopId"] ?? -1);
      if (!work.some((s) => s.id === id)) return;
      selId = id;
    } else {
      // A press that MISSED a handle only adds a stop when it landed on the
      // bar. The handle row hangs 12 px below the bar's bottom edge so a 24 px
      // touch box can straddle it; a press down there used to add a stop
      // (inventory item 13) and now does nothing at all.
      const r = bar?.getBoundingClientRect();
      if (!r || e.clientY < r.top || e.clientY > r.bottom) return;
      // at the cap the bar is a `not-allowed` zone and the affordance line
      // states the reason — it does not silently do nothing (item 12)
      if (atCap) return;
      id = addAt(posFromX(e.clientX), false);
      if (id === null) return;
    }
    drag = { id, ptr: e.pointerId, off: false };
    const host = e.currentTarget;
    if (host instanceof HTMLElement) host.setPointerCapture(e.pointerId);
  }

  function onMove(e: PointerEvent): void {
    const d = drag;
    if (!d || e.pointerId !== d.ptr) return;
    const r = bar?.getBoundingClientRect();
    const off =
      !!r && !atFloor && (e.clientY < r.top - DROP_OFF || e.clientY > r.bottom + DROP_OFF);
    if (off !== d.off) drag = { ...d, off };
    if (!off) move(d.id, posFromX(e.clientX), false);
  }

  function onUp(e: PointerEvent): void {
    const d = drag;
    drag = null;
    if (!d || e.pointerId !== d.ptr) return;
    if (d.off) {
      selId = d.id;
      removeSelected();
      return;
    }
    emit(true);
    // a poll may have been held off while the gesture ran
    syncIn(stops, amount);
  }

  /** A handle is a slider, so it takes the keys one takes. A key that cannot
   *  move anything does nothing and writes NOTHING (inventory item 11: every
   *  arrow press was a full device write, no-ops included — five identical
   *  POSTs of an unchanged palette in the repro). */
  function onStopKey(e: KeyboardEvent, id: number): void {
    const cur = work.find((s) => s.id === id);
    if (!cur) return;
    const step = e.shiftKey ? 16 : 1;
    let next: number;
    switch (e.key) {
      case "ArrowLeft":
      case "ArrowDown":
        next = cur.pos - step;
        break;
      case "ArrowRight":
      case "ArrowUp":
        next = cur.pos + step;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = 255;
        break;
      case "Delete":
      case "Backspace":
        e.preventDefault();
        selId = id;
        removeSelected();
        return;
      default:
        return;
    }
    e.preventDefault();
    selId = id;
    if (clampByte(next) === cur.pos) return; // a no-op writes nothing
    if (!keyMoved) push();
    keyMoved = true;
    move(id, next, false);
    void refocus(id);
  }

  function onStopKeyUp(e: KeyboardEvent): void {
    if (!keyMoved) return;
    if (!/^(Arrow|Home|End)/.test(e.key)) return;
    keyMoved = false;
    emit(true);
  }

  async function refocus(id: number): Promise<void> {
    await tick();
    hnds?.querySelector<HTMLElement>(`[data-stop-id="${id}"]`)?.focus();
  }

  function onCardKey(e: KeyboardEvent): void {
    if (!expanded || !(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== "z") return;
    if (!edited) return;
    // only when the focus is inside THIS editor — the console has several
    // screens mounted at once and a global Ctrl-Z would undo the wrong thing
    const el = document.activeElement;
    if (!card || !(el instanceof Node) || !card.contains(el)) return;
    e.preventDefault();
    undo();
  }

  // ── the two number fields ──────────────────────────────────────────────
  // An emptied field is "no change", never 0 (item 4), and on a refusal the
  // box is put back to the value in effect rather than left showing one the
  // model did not take (item 9).

  function onPosField(el: HTMLInputElement): void {
    const v = Number(el.value.trim());
    const cur = sel;
    if (!cur) return;
    if (el.value.trim() === "" || !Number.isFinite(v)) {
      el.value = String(cur.pos);
      return;
    }
    const next = clampByte(v);
    el.value = String(next);
    if (next === cur.pos) return;
    move(cur.id, next, true, true);
  }

  // ── the preview pair ───────────────────────────────────────────────────
  // ONE engine, ONE frame per tick, painted twice: the pattern as it renders,
  // and the same pixels through the ramp. The right cell is
  // `outpipe::palette_remap_frame`'s own arithmetic over the 100 %-cooked
  // table, which is what the LEDs will do.

  const FPS_MS = 100;
  let engine: Engine | undefined;
  let built: string | null = null;
  let builtRig = "";
  let shape: "bar" | "grid" = "bar";
  let gridW = 0;
  let gridH = 0;
  let raf = 0;
  let last = 0;
  let livePx: Uint8Array | null = null;

  $: rigKey = $layoutSignature;
  $: syncEngine(patternSource, rigKey, $luxel, expanded);

  function syncEngine(
    src: string | null | undefined,
    rk: string,
    lx: Luxel | undefined,
    open: boolean,
  ): void {
    if (!lx) return;
    if (!open || typeof src !== "string" || src === "") {
      if (built !== null) release();
      return;
    }
    if (src === built && rk === builtRig) return;
    built = src;
    builtRig = rk;
    engine?.free();
    engine = undefined;
    livePx = null;
    const r = compileForLayout(lx, src, THUMB_MAX_CELLS);
    if ("engine" in r) {
      r.engine.setWallClock(Date.now() / 1000);
      engine = r.engine;
      const l = r.layout;
      shape = l.regular && l.w > 1 && l.h > 1 ? "grid" : "bar";
      gridW = l.w;
      gridH = l.h;
    }
  }

  function release(): void {
    engine?.free();
    engine = undefined;
    built = null;
    livePx = null;
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine || now - last < FPS_MS) return;
    const dt = last === 0 ? 16 : Math.min(now - last, 100);
    last = now;
    livePx = engine.frame(dt);
    engine.takeError();
    paintPair(livePx);
  }

  function paintPair(px: Uint8Array): void {
    previewLive = true;
    if (shape === "grid" && gridW > 0 && gridH > 0) {
      if (srcCanvas) paintGrid(srcCanvas, px, gridW, gridH);
      if (outCanvas) paintGrid(outCanvas, remapFrame(px, lut100, workAmount), gridW, gridH);
    } else {
      if (srcCanvas) paintBar(srcCanvas, px);
      if (outCanvas) paintBar(outCanvas, remapFrame(px, lut100, workAmount));
    }
  }

  /** With nothing running the left cell is an honest brightness wedge and the
   *  right cell is that wedge through the ramp — which is the LUT itself
   *  (S8h). Never an empty box, never a picture that claims a pattern. */
  const WEDGE = lumaWedge(256);
  $: repaintIdle(srcCanvas, outCanvas, lutAmt, engine, livePx);

  function repaintIdle(
    sc: HTMLCanvasElement | null,
    oc: HTMLCanvasElement | null,
    amtLut: Uint8Array,
    eng: Engine | undefined,
    px: Uint8Array | null,
  ): void {
    if (eng && px) {
      previewLive = true;
      paintPair(px);
      return;
    }
    previewLive = false;
    if (sc) paintBar(sc, WEDGE);
    if (oc) paintLut(oc, amtLut);
  }

  /** The mock reflows on the CARD's width and clusters handles by SCREEN
   *  pixels, so both numbers have to be measured rather than assumed. A
   *  reactive re-observe because `bar` only exists in the expanded branch and
   *  `card` changes element between the two. */
  let ro: ResizeObserver | null = null;

  $: observe(card, bar);

  function observe(c: HTMLElement | null, b: HTMLCanvasElement | null): void {
    if (!ro) return;
    ro.disconnect();
    if (c) ro.observe(c);
    if (b) ro.observe(b);
    measure();
  }

  // THESE TWO BELONG HERE, below `repaintIdle` — they read `previewLive`,
  // which it assigns, and Svelte orders a `$:` whose dependency is written
  // inside a called function by SOURCE POSITION alone. Declared up with the
  // rest of the copy they were computed once, with `previewLive` still false,
  // and the scene mount said "nothing is running" over a layer that was
  // plainly painting (.claude/rules/web.md; `syncIn` above carries the same
  // note — this is the second time the same trap bit this file).
  $: srcCap = !previewLive
    ? "brightness 0 → 255"
    : scope === "layer"
      ? "layer"
      : `pattern · ${patternName}`;
  $: amountHint = previewLive
    ? "0 % = the pattern's own colors · 100 % = only the ramp's"
    : "nothing is running — the wedge stands in for a pattern";

  onMount(() => {
    raf = requestAnimationFrame(loop);
    if (typeof ResizeObserver === "function") ro = new ResizeObserver(() => measure());
    observe(card, bar);
    return () => {
      cancelAnimationFrame(raf);
      ro?.disconnect();
      ro = null;
      release();
    };
  });

  function measure(): void {
    barW = bar?.getBoundingClientRect().width ?? 0;
    const w = card?.getBoundingClientRect().width ?? 0;
    if (w > 0) narrow = w < NARROW_PX;
  }

  // the collapsed row's read-only ticks and its bar
  let colBar: HTMLCanvasElement | null = null;
  $: paintLut(colBar, lut100);
</script>

<!-- `Ctrl-Z` is bound on the WINDOW and gated on the focus being inside this
     card: the editor is a card in a page, not a modal, so a keydown handler on
     the card itself would only fire for a focused child anyway — and a plain
     `<div>` with one is an a11y error the compiler is right about. -->
<svelte:window on:keydown={onCardKey} />

{#if expanded}
  <!-- S8a / S8f: the whole control. `.recard` is the mock's card. -->
  <div class="recard" class:narrow data-role={`${role}-card`} bind:this={card}>
    <div class="rehead">
      <div class="slabel">Color ramp</div>
      <span class="ct2" data-role={`${role}-count`}>{countText}</span>
      <button class="btn sm" type="button" data-role={`${role}-done`} on:click={() => (expanded = false)}
        >Done</button
      >
    </div>

    <p class="restate" data-role={`${role}-statement`}>
      {stateHead}<b>{stateBold}</b>{stateTail}
    </p>

    {#if work.length === 0}
      <!-- S8h (1): nothing honest to paint, so the bar SAYS the stage is off
           rather than showing a black gradient the device will not produce
           (inventory items 6 and 16) -->
      <div class="rebarempty" data-role={`${role}-empty`}>
        no stops — the pattern's own colors go straight to the LEDs
      </div>
    {:else}
      <!-- The bar and its handles are ONE pointer surface — a press anywhere
           on the bar adds a stop and drags it, a press on a handle drags that.
           The wrapper is the surface rather than the handle row, because the
           mock's affordance is "click the BAR"; `onDown` refuses a press that
           lands in the handle row's 12 px below the bar's bottom edge, which is
           inventory item 13 (a miss in that gutter used to add a stop). -->
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="rebarwrap"
        on:pointerdown={onDown}
        on:pointermove={onMove}
        on:pointerup={onUp}
        on:pointercancel={onUp}
      >
        <canvas
          class="rebar"
          class:full={atCap}
          bind:this={bar}
          data-role={previewRole === "" ? `${role}-preview` : previewRole}
          aria-hidden="true"
        ></canvas>
        {#if sel}
          <div
            class="rebub"
            data-role={`${role}-bubble`}
            style={`left:${((sel.pos / 255) * 100).toFixed(2)}%`}
          >
            {sel.pos}
          </div>
        {/if}
        <div
          class="rehnds"
          role="group"
          aria-label={scope === "layer" ? "layer color ramp" : "device color ramp"}
          data-role={`${role}-stops`}
          bind:this={hnds}
        >
          <!-- Selection happens on `pointerdown` (that is what makes press-and-
               drag one gesture), but a handle is a real `<button>`, so it also
               selects on FOCUS and on CLICK: tabbing to a stop or hitting Enter
               on it must open the per-stop row the same way pressing it does. -->
          {#each clusters as c, n (c.pos + ":" + c.ids.join(","))}
            {@const id = (selId !== null && c.ids.includes(selId) ? selId : c.ids[0]) ?? 0}
            {@const st = work.find((s) => s.id === id)}
            <button
              class="rehnd"
              class:on={selId !== null && c.ids.includes(selId)}
              class:dropping={drag !== null && c.ids.includes(drag.id) && drag.off}
              type="button"
              data-stop={n}
              data-stop-id={id}
              data-role={`${role}-stop`}
              role="slider"
              tabindex="0"
              aria-label={`ramp stop at ${st?.pos ?? 0}, #${st?.hex ?? "000000"}`}
              aria-valuemin={0}
              aria-valuemax={255}
              aria-valuenow={st?.pos ?? 0}
              aria-valuetext={`${st?.pos ?? 0} of 255, #${st?.hex ?? "000000"}`}
              style={`left:${((c.pos / 255) * 100).toFixed(2)}%`}
              on:focus={() => (selId = id)}
              on:click={() => (selId = id)}
              on:keydown={(e) => onStopKey(e, id)}
              on:keyup={onStopKeyUp}
            >
              <i style={`background:#${st?.hex ?? "000000"}`}></i>
              {#if c.ids.length > 1}<span class="ct">{c.ids.length}</span>{/if}
            </button>
          {/each}
        </div>
      </div>
      <div class="rescale">
        <span>0</span>
        <span class="mid" data-role={`${role}-affordance`}>{affordance}</span>
        <span>255</span>
      </div>

      {#if sel}
        <div class="resel" data-role={`${role}-selected`}>
          <span class="k">{narrow ? "Selected" : "Selected stop"}</span>
          <span class="pick" data-role={`${role}-color`}>
            <ColorPicker
              kind="rgb"
              variant="chip"
              value={unit(sel.hex)}
              label={`ramp stop at ${sel.pos}`}
              on:input={(e) => onColor(e.detail)}
            />
          </span>
          <span class="repos"
            >at <input
              class="inp num xs"
              type="number"
              min="0"
              max="255"
              data-role={`${role}-pos`}
              value={sel.pos}
              on:change={(e) => onPosField(e.currentTarget)}
            />{narrow ? "" : " of 255"}</span
          >
          {#if selCluster && selCluster.ids.length > 1}
            <span class="k">{selCluster.ids.length} stops here</span>
            <button
              class="btn sm"
              type="button"
              data-role={`${role}-next`}
              on:click={() => (selId = nextInCluster(clusters, selId ?? 0) ?? selId)}
              >next of {selCluster.ids.length}</button
            >
          {/if}
          <!-- §5.7's ONE deliberate exception: a budget the user has to
               learn, disabled WITH a `data-reason` (#529). #787 item 3. -->
          <button
            class="btn sm del"
            type="button"
            data-role={`${role}-remove`}
            disabled={atFloor}
            data-reason={atFloor ? `a ramp needs at least ${MIN_STOPS} stops` : undefined}
            on:click={removeSelected}>{narrow ? "Remove" : "Remove stop"}</button
          >
        </div>
      {:else}
        <div class="resel none" data-role={`${role}-selected`}>
          no stop selected — click a handle on the bar
        </div>
      {/if}

      {#if atFloor}
        <div class="rereason" data-role={`${role}-reason`}>
          a ramp needs at least two stops — <i>{destructive}</i> is how you turn the stage off
        </div>
      {/if}
    {/if}

    {#if conflict}
      <div class="renote" data-role={`${role}-conflict`}>
        The device's ramp changed while you were editing — {conflict.stops.length} stops, {conflict.amount}
        %.
        <span class="group">
          <button class="btn sm" type="button" data-role={`${role}-keep`} on:click={keepMine}
            >Keep mine</button
          >
          <button class="btn sm" type="button" data-role={`${role}-theirs`} on:click={loadTheirs}
            >Load theirs</button
          >
        </span>
      </div>
    {/if}

    {#if work.length > 0}
      <div class="regrid">
        <div class="pairwrap">
          <div class="repair">
            <div class="recell">
              <div class="cap2">{srcCap}</div>
              <canvas
                class={shape === "grid" ? "sq" : "strip"}
                bind:this={srcCanvas}
                data-role={`${role}-preview-src`}
                aria-hidden="true"
              ></canvas>
            </div>
            <div class="rearr">→</div>
            <div class="recell">
              <div class="cap2">{outCap}</div>
              <canvas
                class={shape === "grid" ? "sq" : "strip"}
                bind:this={outCanvas}
                data-role={`${role}-preview-out`}
                aria-hidden="true"
              ></canvas>
            </div>
          </div>
        </div>
        <div>
          <div class="cap2">Amount</div>
          <div class="bigslider">
            <input
              type="range"
              min="0"
              max="100"
              step="1"
              class="slider"
              data-role={`${role}-amount`}
              aria-label="ramp amount"
              value={workAmount}
              on:input={(e) => onAmountSlide(Number(e.currentTarget.value))}
              on:change={(e) => onAmountCommit(Number(e.currentTarget.value))}
            />
            <span class="val" data-role={`${role}-amount-val`}>{workAmount} %</span>
          </div>
          <div class="hint amthint" data-role={`${role}-amount-hint`}>{amountHint}</div>
        </div>
      </div>
    {/if}

    <div class="represets" data-role={`${role}-presets`}>
      <span class="rlab">{work.length === 0 ? "Start from" : "Presets"}</span>
      {#each RAMP_PRESETS as p (p.name)}
        <button
          class="rechip"
          class:on={preset === p.name}
          type="button"
          data-role={`${role}-preset`}
          data-preset={p.name}
          on:click={() => pickPreset(p.name)}
        >
          <i
            style={`background:linear-gradient(90deg,${p.stops
              .map((s) => `#${s.hex} ${((s.pos / 255) * 100).toFixed(1)}%`)
              .join(",")})`}
          ></i>
          <span>{p.name}</span>
        </button>
      {/each}
    </div>

    <!-- after the presets (S8h card 1): they are the way IN from empty and
         this is the caveat on them, not a preamble to them -->
    {#if work.length === 0 && workAmount === 0}
      <div class="renote" data-role={`${role}-amount-note`}>
        Amount is 0 % — a ramp built now would have no visible effect until you raise it.
        <span class="group"
          ><button
            class="btn sm"
            type="button"
            data-role={`${role}-amount-fix`}
            on:click={() => setAmount(100, true)}>Set amount to 100 %</button
          ></span
        >
      </div>
    {/if}

    <div class="reacts">
      <button
        class="btn sm"
        type="button"
        data-role={`${role}-add`}
        disabled={gapPos === null}
        data-reason={gapPos === null
          ? atCap
            ? `all ${MAX_STOPS} stops are used — remove one to add another`
            : "a ramp needs at least two stops — pick a preset"
          : undefined}
        on:click={addIntoGap}>+ Add stop</button
      >
      <button
        class="btn sm"
        type="button"
        data-role={`${role}-undo`}
        disabled={!edited}
        data-reason={edited ? undefined : "nothing to undo yet"}
        on:click={undo}>Undo</button
      >
      <button
        class="btn sm"
        type="button"
        data-role={`${role}-reset`}
        disabled={!edited}
        data-reason={edited ? undefined : "the ramp is as you opened it"}
        on:click={reset}>Reset</button
      >
      {#if !narrow}
        <span class="hint" data-role={`${role}-cap`}>
          {#if atCap}all {MAX_STOPS} stops are used — remove one to add another{:else if work.length === 0}a
            ramp is 0 stops (off) or at least 2 — never 1{:else}adds into the widest gap ·
            <span class="mono">Ctrl-Z</span> undoes{/if}
        </span>
      {/if}
      {#if work.length > 0}
        <button
          class="btn sm del"
          type="button"
          data-role={`${role}-clear`}
          on:click={() => (confirming = true)}>{destructive}</button
        >
      {/if}
    </div>

    {#if confirming}
      <div class="renote" data-role={`${role}-confirm`}>
        {scope === "layer" ? "Remove the ramp?" : "Clear the ramp?"} The {work.length} stops are removed
        and the {scope === "layer" ? "layer keeps its own colors" : "pattern's own colors go straight to the LEDs"}.
        The amount stays at {workAmount} %.
        <span class="group">
          <button
            class="btn sm"
            type="button"
            data-role={`${role}-confirm-cancel`}
            on:click={() => (confirming = false)}>Cancel</button
          >
          <button
            class="btn sm del"
            type="button"
            data-role={`${role}-confirm-ok`}
            on:click={doClear}>{destructive}</button
          >
        </span>
      </div>
    {/if}

    <div class="moderow">
      <span class="remode" class:save={effMode !== "live"} data-role={`${role}-mode`}>
        <i></i>{effMode === "hold" ? "Nothing is sent until you choose." : modeText(mode)}
      </span>
    </div>
  </div>
{:else}
  <!-- S8e: the collapsed summary row. A CANVAS painted from the LUT, so the
       row already shows what the device will do; the read-only ticks stay
       (nothing here is draggable — `Edit…` is the affordance). -->
  <div data-role={`${role}-collapsed`} bind:this={card}>
    <div class="recol">
      <div>
        <canvas
          class="colbar"
          bind:this={colBar}
          data-role={previewRole === "" ? `${role}-preview` : previewRole}
          aria-hidden="true"
        ></canvas>
        <div class="stoprow" data-role={`${role}-ticks`}>
          {#each work as s (s.id)}
            <span
              class="stopmk"
              style={`left:${((s.pos / 255) * 100).toFixed(2)}%;background:#${s.hex}`}
            ></span>
          {/each}
        </div>
      </div>
      <button class="lnk" type="button" data-role={`${role}-edit`} on:click={() => (expanded = true)}
        >Edit…</button
      >
    </div>
    <!-- S8e puts the summary under the WHOLE row — the bar, the ticks and the
         link — not inside the left column, where it would lose the link's width
         and wrap a line early in the 288px inspector -->
    <div class="hint summary" data-role={`${role}-summary`}>{summary}</div>
  </div>
{/if}

<style>
  /* Every number below is quotable from mockups.html's batch-7 block — the
     mocks are the visual spec (.claude/rules/web.md, Jeremy 2026-09-19). */

  .recard {
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
  }

  .rehead {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 9px;
  }

  .rehead .ct2 {
    margin-left: auto;
    font: 11.5px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
  }

  /* the one-line statement of what the stage DOES (inventory item 17) */
  .restate {
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-dim);
    margin: 0;
    max-width: 600px;
  }

  .restate b {
    color: var(--text);
    font-weight: 600;
  }

  .narrow .restate {
    font-size: 12px;
  }

  /* 24px of clearance above the bar for the selected stop's position bubble */
  .rebarwrap {
    position: relative;
    margin-top: 24px;
    touch-action: none;
  }

  /* The bar IS the engine's 256-entry LUT (#748), a 256x1 canvas stretched */
  .rebar {
    display: block;
    width: 100%;
    height: 44px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: #000;
    image-rendering: pixelated;
    cursor: copy;
  }

  /* at the cap the bar is a `not-allowed` zone (item 12) */
  .rebar.full {
    cursor: not-allowed;
  }

  .rehnds {
    position: relative;
    height: 24px;
    margin-top: -12px;
  }

  /* 12px of paint inside a 24x24 HIT BOX — §5.7's touch floor (#529, #703).
     The handle sits ON the gradient; there is no gutter under the bar to miss
     into any more (item 13). */
  .rehnd {
    position: absolute;
    top: 0;
    width: 24px;
    height: 24px;
    margin-left: -12px;
    padding: 0;
    border: none;
    border-radius: 0;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: grab;
  }

  .rehnd:hover {
    border-color: transparent;
  }

  .rehnd i {
    display: block;
    width: 12px;
    height: 12px;
    border-radius: 2px;
    transform: rotate(45deg);
    border: 1px solid rgba(255, 255, 255, 0.75);
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.55);
  }

  .rehnd.on i {
    width: 14px;
    height: 14px;
    border-color: var(--accent);
    box-shadow:
      0 0 0 2px rgba(232, 163, 61, 0.3),
      0 0 0 1px rgba(0, 0, 0, 0.55);
  }

  .rehnd.dropping i {
    opacity: 0.35;
  }

  .rehnd:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  /* two or more stops within 3px collapse to ONE handle with a count */
  .rehnd .ct {
    position: absolute;
    top: -2px;
    right: -2px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--accent);
    color: #16110a;
    font: 600 9px/14px var(--mono);
    text-align: center;
  }

  .rescale {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 7px;
    font: 11px/1 var(--mono);
    color: var(--text-dim);
  }

  .rescale .mid {
    flex: 1;
    text-align: right;
    font-family: var(--sans);
    font-size: 12px;
  }

  .narrow .rescale {
    flex-wrap: wrap;
    justify-content: space-between;
  }

  .narrow .rescale .mid {
    order: 3;
    width: 100%;
    flex: none;
    text-align: left;
    margin-top: 4px;
  }

  /* the selected stop's position, over the bar, in accent — so the number in
     the field and the thing on the bar are visibly the same stop, without a
     `Stop 4` header that renumbers itself mid-drag */
  .rebub {
    position: absolute;
    top: -21px;
    transform: translateX(-50%);
    height: 17px;
    padding: 0 6px;
    border-radius: 4px;
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px rgba(232, 163, 61, 0.55);
    color: var(--accent);
    font: 11px/17px var(--mono);
    white-space: nowrap;
    /* it floats over the bar's pointer surface; it must not eat a press */
    pointer-events: none;
  }

  /* ONE row, for the one selected stop — not a permanent per-stop list */
  .resel {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
    padding: 9px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-panel);
    flex-wrap: wrap;
  }

  .narrow .resel {
    gap: 8px;
  }

  .resel .k {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
  }

  .resel .btn.del {
    margin-left: auto;
  }

  .resel.none {
    color: var(--text-dim);
    font-size: 12.5px;
    background: transparent;
    border-style: dashed;
  }

  .repos {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: 12px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
  }

  .repos .inp.num {
    width: 56px;
  }

  .narrow .repos .inp.num {
    width: 52px;
  }

  .pick {
    display: inline-flex;
  }

  /* mockups.html `.btn.del`: the destructive verb is text, not a box */
  .btn.del {
    padding: 0;
    border-color: transparent;
    background: transparent;
    color: var(--error);
  }

  .btn.del:hover {
    border-color: transparent;
    filter: brightness(1.15);
  }

  .rereason {
    margin-top: 9px;
    font-size: 12px;
    color: #eda0a0;
  }

  .renote {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
    padding: 8px 10px;
    border-radius: 6px;
    background: rgba(217, 163, 67, 0.12);
    border: 1px solid rgba(217, 163, 67, 0.32);
    color: #e5bd74;
    font-size: 12px;
    line-height: 1.4;
    flex-wrap: wrap;
  }

  /* mockups.html `.group` (:58) — the same rule `pages/Playlist.svelte`
     carries; only the `margin-left:auto` is this strip's own */
  .renote .group {
    margin-left: auto;
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  /* preview pair + amount, side by side (#537: "amount slider next to the
     preview"; inventory item 22: amount was a number input in a button row) */
  .regrid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 250px;
    gap: 18px;
    margin-top: 15px;
    align-items: start;
  }

  .narrow .regrid {
    grid-template-columns: minmax(0, 1fr);
    gap: 14px;
  }

  .repair {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 14px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
  }

  .cap2 {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
    margin-bottom: 5px;
  }

  /* the mock clips only the PREVIEW cells' captions (`.recell .cap2`) — the
     `Amount` caption beside them is short and must not carry an ellipsis it
     can never use */
  .recell .cap2 {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .regrid > div > .cap2 {
    margin-bottom: 6px;
  }

  .recell canvas {
    display: block;
    width: 100%;
    background: #000;
    border: 1px solid var(--border);
    border-radius: 4px;
    image-rendering: pixelated;
  }

  .recell canvas.sq {
    aspect-ratio: 1 / 1;
  }

  .recell canvas.strip {
    aspect-ratio: 8 / 1;
  }

  .rearr {
    color: var(--text-dim);
    font-size: 13px;
    text-align: center;
    padding-top: 21px;
  }

  .bigslider {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  /* #703's fix: the painted track stays the mock's 6px, the BOX grows so a
     thumb is hittable by touch. */
  .bigslider .slider {
    height: 20px;
  }

  .bigslider .slider::-webkit-slider-runnable-track {
    height: 6px;
    border-radius: 3px;
  }

  .bigslider .slider::-moz-range-track {
    height: 6px;
    border-radius: 3px;
  }

  .bigslider .slider::-webkit-slider-thumb {
    width: 18px;
    height: 18px;
    margin-top: -6px;
  }

  .bigslider .slider::-moz-range-thumb {
    width: 18px;
    height: 18px;
  }

  .bigslider .val {
    font: 13px/1 var(--mono);
    color: var(--text);
    min-width: 52px;
    text-align: right;
  }

  .hint {
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-dim);
  }

  .amthint {
    margin-top: 7px;
  }

  .represets {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 15px;
  }

  .narrow .represets {
    gap: 5px;
    margin-top: 13px;
  }

  .represets .rlab {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
    align-self: center;
  }

  .narrow .represets .rlab {
    width: 100%;
    margin-bottom: 1px;
  }

  .rechip {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 84px;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-panel);
  }

  .narrow .rechip {
    width: calc(33.333% - 4px);
  }

  .rechip i {
    display: block;
    height: 14px;
    border-radius: 3px;
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  .rechip span {
    font-size: 11px;
    color: var(--text-dim);
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rechip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .rechip.on span {
    color: var(--text);
  }

  .reacts {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
    padding-top: 13px;
    border-top: 1px solid var(--border);
    flex-wrap: wrap;
  }

  .reacts .btn.del {
    margin-left: auto;
  }

  .reacts .hint {
    font-size: 11.5px;
  }

  .moderow {
    margin-top: 11px;
  }

  /* which mode the editor is in — #563's live-vs-save split, STATED */
  .remode {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
  }

  .remode i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ok);
    flex: none;
  }

  .remode.save i {
    background: #5b6070;
  }

  .narrow .remode {
    white-space: normal;
    align-items: flex-start;
  }

  .narrow .remode i {
    margin-top: 5px;
  }

  /* no stops at all: there is nothing honest to paint */
  .rebarempty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 44px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    background: var(--bg-panel);
    color: var(--text-dim);
    font-size: 12px;
  }

  /* ---- the collapsed row (S8e) ---- */

  .recol {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .recol > div {
    flex: 1;
    min-width: 0;
  }

  .recol .lnk {
    padding-top: 4px;
    padding-left: 0;
    padding-right: 0;
    border: none;
    background: transparent;
    font-size: 12.5px;
    color: var(--accent);
    white-space: nowrap;
  }

  .recol .lnk:hover {
    border-color: transparent;
  }

  .colbar {
    display: block;
    width: 100%;
    height: 22px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: #000;
    image-rendering: pixelated;
  }

  /* the existing read-only ticks: 9px, no touch box needed because nothing
     here is draggable (the row is a summary) */
  .stoprow {
    position: relative;
    height: 12px;
    margin-top: 4px;
  }

  .stopmk {
    position: absolute;
    top: 0;
    width: 9px;
    height: 9px;
    margin-left: -5px;
    border-radius: 2px;
    transform: rotate(45deg);
    border: 1px solid rgba(255, 255, 255, 0.45);
  }

  .summary {
    margin-top: 6px;
  }

  /* The phone: every hit target >= 24px, which `mockdiff --sweep` enforces at
     <= 420px (§5.7, #529). The slider's BOX grows; its paint does not (#703). */
  @media (max-width: 420px) {
    .bigslider .slider {
      height: 24px;
    }

    .amthint {
      display: none;
    }
  }
</style>
