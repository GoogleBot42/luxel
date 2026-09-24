<script lang="ts">
  // `Add to scene ▸` — one row of a ⋯ menu with a submenu of the device's
  // scenes and `New scene…` (proposal §5.4b, mock S2e). The SAME row in the
  // editor's header menu and in a Patterns tile's menu, so the shortcut from
  // "the place you just tuned the look" is identical wherever you are.
  //
  // It is a row, not a page: whether it exists at all is the CALLER's call
  // (`scenesAvailable` in `lib/sceneWire.ts` — a scene needs a regular 2D
  // grid, so on a strip, lattice or custom-map console the item is ABSENT, not
  // disabled; S2e draws that menu as simply one item shorter).
  //
  // The parent row is a <div role="menuitem">, deliberately not a <button>:
  // `components/Popover.svelte` closes a `menu` popover on any button click
  // inside it, and opening a submenu must not close the menu it is in.
  import { createEventDispatcher, tick } from "svelte";

  /** The device's scenes, newest last (`GET /api/scenes`). May be empty — the
   *  submenu still offers `New scene…`. */
  export let scenes: { id: string; name: string }[] = [];

  const dispatch = createEventDispatcher<{ pick: { id: string; name: string }; create: void }>();

  let open = false;
  let rowEl: HTMLElement | undefined;
  let subEl: HTMLElement | undefined;
  /** The submenu opens to the right (S2e); near the viewport edge — which is
   *  where the editor's ⋯ always is — it opens to the left instead. */
  let flip = false;

  async function show(): Promise<void> {
    open = true;
    await tick();
    if (!rowEl || !subEl) return;
    const r = rowEl.getBoundingClientRect();
    flip = r.right + 6 + subEl.offsetWidth > window.innerWidth - 8;
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Enter" || e.key === " " || e.key === "ArrowRight") {
      e.preventDefault();
      void show();
    } else if (e.key === "ArrowLeft" || e.key === "Escape") {
      open = false;
    }
  }
</script>

<!-- svelte-ignore a11y-no-noninteractive-element-to-interactive-role -->
<div
  class="mi parent"
  class:open
  bind:this={rowEl}
  role="menuitem"
  tabindex="0"
  aria-haspopup="true"
  aria-expanded={open}
  data-role="add-to-scene"
  on:mouseenter={() => void show()}
  on:mouseleave={() => (open = false)}
  on:click={() => (open ? (open = false) : void show())}
  on:keydown={onKey}
>
  Add to scene<span class="caret">▸</span>
  <!-- kept in the DOM and hidden rather than unmounted: a closed submenu is
       still something a harness (and a screen reader) can find. -->
  <div class="sub" class:flip class:open bind:this={subEl} role="menu" data-role="add-to-scene-menu">
    {#each scenes as s (s.id)}
      <button
        class="mi"
        role="menuitem"
        data-role="add-to-scene-item"
        data-id={s.id}
        on:click={() => dispatch("pick", { id: s.id, name: s.name })}
      >
        {s.name || s.id}
      </button>
    {/each}
    {#if scenes.length > 0}<div class="sepr"></div>{/if}
    <button
      class="mi"
      role="menuitem"
      data-role="add-to-scene-new"
      on:click={() => dispatch("create")}
    >
      New scene…
    </button>
  </div>
</div>

<style>
  /* S2e `.menu .mi.parent{display:flex;align-items:center;position:relative}` */
  .mi.parent {
    display: flex;
    align-items: center;
    position: relative;
  }

  .mi.parent.open,
  .mi.parent:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  /* S2e `.menu .mi .caret{margin-left:auto;color:var(--text-dim);font-size:10px}` */
  .mi .caret {
    margin-left: auto;
    color: var(--text-dim);
    font-size: 10px;
  }

  /* S2e `.menu .sub` — 184px, 4px padding, the menu's own chrome */
  .mi .sub {
    display: none;
    position: absolute;
    left: calc(100% + 6px);
    top: -5px;
    width: 184px;
    padding: 4px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 14px 34px rgba(0, 0, 0, 0.65);
    z-index: 1;
  }

  .mi .sub.open {
    display: block;
  }

  /* the editor's ⋯ sits at the right edge of the screen: a submenu to the
     right of it would be off-screen, so it opens on the other side. The mock
     has room for the right-hand side and cannot draw this case. */
  .mi .sub.flip {
    left: auto;
    right: calc(100% + 6px);
  }

  /* the submenu's own rows are `.menu .mi` (app.css) — it is inside a `.menu` */
</style>
