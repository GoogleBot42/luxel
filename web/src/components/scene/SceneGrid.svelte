<script lang="ts">
  // The Scenes page's tile grid (mockups S6, S6d, S9): the same square
  // device-shaped thumbnail, the same `playing` ring and the same hover verbs
  // a pattern tile has — a scene plays exactly where a pattern plays.
  //
  // Each tile is a COMPOSITE, so it is a `SceneRenderer` (a compositor plus
  // one engine per pattern layer) rather than one engine. That is expensive
  // enough that the grid copies `components/Gallery.svelte`'s discipline
  // instead of `PatternThumb`'s: ONE shared rAF ticker with a per-frame
  // budget, an engine cap, and an IntersectionObserver so a tile below the
  // fold costs nothing.
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import Popover from "../Popover.svelte";
  import { paintGrid } from "../../lib/draw";
  import { SceneRenderer, type SourceLookup } from "../../lib/sceneRender";
  import { thumbLayout, type Layout } from "../../stores/geometry";
  import type { Luxel } from "../../lib/luxel";
  import type { Scene } from "../../lib/scene";

  export let luxel: Luxel | null = null;
  export let items: Scene[] = [];
  /** The scene the device is playing — that tile gets the ring and the pill. */
  export let playingId = "";
  /** The rig every tile renders on (the device's Layout, shrunk). */
  export let rig: Layout;
  export let lookup: SourceLookup = () => null;

  const dispatch = createEventDispatcher<{
    play: string;
    edit: string;
    duplicate: string;
    remove: string;
  }>();

  /** Live composites at once. A two-pattern-layer scene is two engines, so
     this is the same ~40-engine ceiling the gallery keeps. */
  const RENDER_CAP = 12;
  const STEP_BUDGET = 3;
  const TILE_FPS_MS = 100;

  interface Tile {
    scene: Scene;
    canvas?: HTMLCanvasElement;
    renderer?: SceneRenderer;
    visible: boolean;
    last: number;
    seen: number;
  }

  let tiles: Tile[] = [];
  let raf = 0;
  let menuFor = "";
  let menuBtn: HTMLElement | null = null;

  /** The live tiles, kept OUTSIDE the reactive variable: a `$:` that both
   *  reads and assigns `tiles` would be its own dependency and re-run forever
   *  (.claude/rules/web.md). The map is the memory; `tiles` is the view of it
   *  the markup walks. */
  const held = new Map<string, Tile>();

  $: tiles = adopt(items);

  function adopt(list: readonly Scene[]): Tile[] {
    const next: Tile[] = [];
    const live = new Set<string>();
    for (const s of list) {
      live.add(s.id);
      const t = held.get(s.id);
      if (t) {
        t.scene = s;
        next.push(t);
      } else {
        const made: Tile = { scene: s, visible: false, last: 0, seen: 0 };
        held.set(s.id, made);
        next.push(made);
      }
    }
    for (const [id, t] of held) {
      if (live.has(id)) continue;
      t.renderer?.free();
      held.delete(id);
    }
    return next;
  }

  $: thumbRig = thumbLayout(rig, 1024);

  /**
   * The tile's canvas and its visibility are captured with ACTIONS, never
   * `bind:this={t.canvas}`.
   *
   * A `bind:this` into an `{#each}` item writes back through the array, which
   * invalidates `tiles`, which re-runs the keyed block, which re-fires the
   * binding — an unbreakable flush loop that froze this page solid the first
   * time round (Svelte 4; the same trap `.claude/rules/web.md` warns about for
   * `$:` self-dependencies). An action's node goes into the non-reactive
   * `held` map instead, and nothing is invalidated at all.
   */
  const io =
    typeof IntersectionObserver === "undefined"
      ? undefined
      : new IntersectionObserver(
          (entries) => {
            for (const e of entries) {
              const t = held.get((e.target as HTMLElement).dataset.scene ?? "");
              if (t) t.visible = e.isIntersecting;
            }
          },
          { rootMargin: "120px" },
        );

  function observe(node: HTMLElement, id: string) {
    node.dataset.scene = id;
    io?.observe(node);
    return {
      destroy() {
        io?.unobserve(node);
      },
    };
  }

  function tileCanvas(node: HTMLCanvasElement, id: string) {
    const t = held.get(id);
    if (t) t.canvas = node;
    return {
      destroy() {
        const cur = held.get(id);
        if (cur?.canvas === node) cur.canvas = undefined;
      },
    };
  }

  function build(t: Tile): void {
    if (!luxel || t.renderer) return;
    const live = tiles.filter((x) => x.renderer).length;
    if (live >= RENDER_CAP) reclaim();
    const r = new SceneRenderer(luxel, thumbRig);
    if (r.setScene(t.scene, lookup)) {
      r.free();
      return;
    }
    t.renderer = r;
  }

  /** Drop the renderer nobody has looked at for longest. */
  function reclaim(): void {
    let oldest: Tile | undefined;
    for (const t of tiles) if (t.renderer && (!oldest || t.seen < oldest.seen)) oldest = t;
    if (oldest) {
      oldest.renderer?.free();
      oldest.renderer = undefined;
    }
  }

  function tick(now: number): void {
    raf = requestAnimationFrame(tick);
    let stepped = 0;
    for (const t of tiles) {
      if (!t.visible || stepped >= STEP_BUDGET) continue;
      if (now - t.last < TILE_FPS_MS) continue;
      t.seen = now;
      if (!t.renderer) build(t);
      const px = t.renderer?.frame(now - (t.last || now - 100));
      t.last = now;
      if (px && t.canvas) paintGrid(t.canvas, px, thumbRig.w, thumbRig.h);
      stepped++;
    }
  }

  onMount(() => {
    raf = requestAnimationFrame(tick);
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    io?.disconnect();
    for (const t of tiles) t.renderer?.free();
  });
</script>

<div class="tiles" data-role="scenes-grid">
  {#each tiles as t (t.scene.id)}
    <div class="tile" class:playing={t.scene.id === playingId} data-role="scene-tile" data-scene={t.scene.id}>
      <div class="thumb" use:observe={t.scene.id}>
        <button
          class="face"
          data-role="scene-tile-play"
          title="play this scene"
          on:click={() => dispatch("play", t.scene.id)}
        >
          <canvas use:tileCanvas={t.scene.id} width={thumbRig.w} height={thumbRig.h}></canvas>
        </button>
        {#if t.scene.id === playingId}
          <span class="pill" data-role="scene-playing">▶ playing</span>
        {/if}
        <div class="actions">
          <button class="btn sm" data-role="scene-tile-edit" on:click={() => dispatch("edit", t.scene.id)}
            >Edit</button
          >
          <span class="spacer"></span>
          <button
            class="btn sm icon"
            data-role="scene-tile-menu"
            aria-label="more actions"
            on:click|stopPropagation={(e) => {
              menuBtn = e.currentTarget;
              menuFor = menuFor === t.scene.id ? "" : t.scene.id;
            }}>⋯</button
          >
        </div>
      </div>
      <div class="meta">
        <div class="nm" data-role="scene-tile-name">{t.scene.name}</div>
        <div class="sub" data-role="scene-tile-layers">
          {t.scene.layers.length} layer{t.scene.layers.length === 1 ? "" : "s"}
        </div>
        <button class="elink" data-role="scene-tile-edit-link" on:click={() => dispatch("edit", t.scene.id)}
          >Edit</button
        >
      </div>
    </div>
  {/each}
</div>

<Popover
  open={menuFor !== ""}
  anchor={menuBtn}
  dataRole="scene-tile-menu-popup"
  on:close={() => (menuFor = "")}
>
  <button
    class="mi"
    data-role="scene-menu-play"
    on:click={() => {
      dispatch("play", menuFor);
      menuFor = "";
    }}>Play</button
  >
  <button
    class="mi"
    data-role="scene-menu-edit"
    on:click={() => {
      dispatch("edit", menuFor);
      menuFor = "";
    }}>Edit</button
  >
  <div class="sepr"></div>
  <button
    class="mi"
    data-role="scene-menu-duplicate"
    on:click={() => {
      dispatch("duplicate", menuFor);
      menuFor = "";
    }}>Duplicate</button
  >
  <button
    class="mi del"
    data-role="scene-menu-delete"
    on:click={() => {
      dispatch("remove", menuFor);
      menuFor = "";
    }}>Delete</button
  >
</Popover>
