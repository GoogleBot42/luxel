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
  import { spriteMetaLine, type Sprite } from "../../lib/sprite";
  import { cachedSprite, type SpriteMeta } from "../../stores/sprites";

  export let items: SpriteMeta[] = [];
  /**
   * Which backing these rows came from (Gitea #785). `store` is the live
   * library — edit, duplicate, delete. `library` is the SHIPPED one, which is
   * read-only, so the tile carries the one verb that makes sense there: copy it
   * into the live library. Not a disabled Edit with a reason (§5.7) — a shipped
   * sprite is not a sprite of yours that you cannot edit, it is a different
   * kind of thing.
   */
  export let mode: "store" | "library" = "store";
  /**
   * Records by row id, for rows whose pixels are NOT in the sprite store's
   * cache — i.e. the shipped library, which has no store ids at all and keys on
   * its slugs. Null means "read the store's cache", which is the live case.
   */
  export let records: ReadonlyMap<string, Sprite> | null = null;
  /** Bumped by the page whenever the record cache may have changed, so a tile
   *  whose pixels arrived after its row did repaints. The store's cache is a
   *  plain Map — nothing invalidates on a write — so the page publishes this
   *  instead (the `localRev` idiom the scene editor uses for its library). */
  export let rev = 0;

  const dispatch = createEventDispatcher<{
    edit: string;
    duplicate: string;
    remove: string;
    /** `library` mode: copy this row into the live library. */
    add: string;
  }>();

  let menuFor = "";
  let menuBtn: HTMLElement | null = null;

  /** Each row paired with its decoded record, when the store has one. Read
   *  through a function with `rev` NAMED as an argument: `cachedSprite` is not
   *  a store, so nothing here would re-run when a record lands. */
  $: tiles = items.map((m) => ({ meta: m, sprite: withRev(m.id, rev, records) }));

  function withRev(id: string, _rev: number, from: ReadonlyMap<string, Sprite> | null) {
    return from ? (from.get(id) ?? null) : cachedSprite(id);
  }
</script>

<div class="tiles" data-role="sprites-grid" data-source={mode}>
  {#each tiles as t (t.meta.id)}
    <div class="tile" data-role="sprite-tile" data-sprite={t.meta.id}>
      <div class="thumb">
        <button
          class="face"
          data-role="sprite-tile-open"
          title={mode === "library" ? "copy this sprite into your sprites" : "edit this sprite"}
          on:click={() => dispatch(mode === "library" ? "add" : "edit", t.meta.id)}
        >
          <SpriteThumb sprite={t.sprite} dataRole="sprite-tile-thumb" />
        </button>
        <div class="actions">
          {#if mode === "library"}
            <button
              class="btn sm"
              data-role="sprite-tile-add"
              title="copy this sprite into your sprites"
              on:click|stopPropagation={() => dispatch("add", t.meta.id)}>+ Add</button
            >
          {:else}
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
          {/if}
        </div>
      </div>
      <div class="meta">
        <div class="nm" data-role="sprite-tile-name">{t.meta.name}</div>
        <div class="sub" data-role="sprite-tile-meta">
          {spriteMetaLine(t.meta.w, t.meta.h, t.meta.frames)}
        </div>
        {#if mode === "library"}
          <button class="elink" data-role="sprite-tile-add-link" on:click={() => dispatch("add", t.meta.id)}
            >Add</button
          >
        {:else}
          <button class="elink" data-role="sprite-tile-edit-link" on:click={() => dispatch("edit", t.meta.id)}
            >Edit</button
          >
        {/if}
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
