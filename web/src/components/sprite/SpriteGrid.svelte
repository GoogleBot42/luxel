<script lang="ts">
  // The Sprites tab's tile grid (Gitea #740) — `components/scene/SceneGrid`'s
  // peer, and deliberately the same tile: square picture, name, one mono meta
  // line, hover verbs, `⋯` for the rest. "The job here is making them feel
  // like patterns, not like a second app" is S6's note about scenes and it
  // applies twice over to sprites.
  //
  // Far cheaper than a scene tile: a sprite is texels, so there is no
  // compositor, no engine, no per-tile budget and no IntersectionObserver —
  // `SpriteThumb` shares ONE rAF across every thumb on the page.
  import { createEventDispatcher } from "svelte";
  import Popover from "../Popover.svelte";
  import SpriteThumb from "./SpriteThumb.svelte";
  import { spriteMetaLine } from "../../lib/sprite";
  import { cachedSprite, type SpriteMeta } from "../../stores/sprites";

  export let items: SpriteMeta[] = [];
  /** Bumped by the page whenever the record cache may have changed, so a tile
   *  whose pixels arrived after its row did repaints. The store's cache is a
   *  plain Map — nothing invalidates on a write — so the page publishes this
   *  instead (the `localRev` idiom the scene editor uses for its library). */
  export let rev = 0;

  const dispatch = createEventDispatcher<{
    edit: string;
    duplicate: string;
    remove: string;
  }>();

  let menuFor = "";
  let menuBtn: HTMLElement | null = null;

  /** Each row paired with its decoded record, when the store has one. Read
   *  through a function with `rev` NAMED as an argument: `cachedSprite` is not
   *  a store, so nothing here would re-run when a record lands. */
  $: tiles = items.map((m) => ({ meta: m, sprite: withRev(m.id, rev) }));

  function withRev(id: string, _rev: number) {
    return cachedSprite(id);
  }
</script>

<div class="tiles" data-role="sprites-grid">
  {#each tiles as t (t.meta.id)}
    <div class="tile" data-role="sprite-tile" data-sprite={t.meta.id}>
      <div class="thumb">
        <button
          class="face"
          data-role="sprite-tile-open"
          title="edit this sprite"
          on:click={() => dispatch("edit", t.meta.id)}
        >
          <SpriteThumb sprite={t.sprite} dataRole="sprite-tile-thumb" />
        </button>
        <div class="actions">
          <button class="btn sm" data-role="sprite-tile-edit" on:click={() => dispatch("edit", t.meta.id)}
            >Edit</button
          >
          <span class="spacer"></span>
          <button
            class="btn sm icon"
            data-role="sprite-tile-menu"
            aria-label="more actions"
            on:click|stopPropagation={(e) => {
              menuBtn = e.currentTarget;
              menuFor = menuFor === t.meta.id ? "" : t.meta.id;
            }}>⋯</button
          >
        </div>
      </div>
      <div class="meta">
        <div class="nm" data-role="sprite-tile-name">{t.meta.name}</div>
        <div class="sub" data-role="sprite-tile-meta">
          {spriteMetaLine(t.meta.w, t.meta.h, t.meta.frames)}
        </div>
        <button class="elink" data-role="sprite-tile-edit-link" on:click={() => dispatch("edit", t.meta.id)}
          >Edit</button
        >
      </div>
    </div>
  {/each}
</div>

<Popover
  open={menuFor !== ""}
  anchor={menuBtn}
  dataRole="sprite-tile-menu-popup"
  on:close={() => (menuFor = "")}
>
  <button
    class="mi"
    data-role="sprite-menu-edit"
    on:click={() => {
      dispatch("edit", menuFor);
      menuFor = "";
    }}>Edit</button
  >
  <div class="sepr"></div>
  <button
    class="mi"
    data-role="sprite-menu-duplicate"
    on:click={() => {
      dispatch("duplicate", menuFor);
      menuFor = "";
    }}>Duplicate</button
  >
  <button
    class="mi del"
    data-role="sprite-menu-delete"
    on:click={() => {
      dispatch("remove", menuFor);
      menuFor = "";
    }}>Delete</button
  >
</Popover>
