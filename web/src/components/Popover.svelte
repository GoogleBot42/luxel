<script lang="ts">
  // THE popover: the ⋯ menus (editor, tile, playlist), the "Preview as"
  // chooser, and the projection override popup all mount through this one.
  //
  // It owns geometry and dismissal only — the LOOKS are the global `.menu` /
  // `.pop` rules in app.css, because a popover's items are slot content and
  // Svelte compiles slotted markup in the CALLER's scope (a component cannot
  // style what it was handed).
  //
  // Positioned `fixed` off the anchor's viewport rect rather than
  // `absolute`-inside-the-trigger: a menu opened from a tile in the scrolling
  // grid would otherwise be clipped by it. That means it must DODGE the
  // viewport on all four sides — flip above when the bottom would overflow,
  // clamp left/right/top — which is the bug Jeremy hit on every dropdown
  // (2026-09-19).
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";

  /** Whether the popover is mounted-and-shown. The OWNER holds this state. */
  export let open = false;
  /** The element to hang it off — usually the button that opened it. */
  export let anchor: HTMLElement | null = null;
  /** `menu` = the 214px verb list; `pop` = the 296px chooser (mockup S5). */
  export let kind: "menu" | "pop" = "menu";
  /** Which edge of the anchor the popover lines up with before dodging. */
  export let align: "start" | "end" = "end";
  /** `data-role` for the e2e harnesses; the items carry their own. */
  export let dataRole = "";
  /** ARIA role of the container (`menu`, or `dialog` for a chooser). */
  export let ariaRole = "menu";

  const dispatch = createEventDispatcher<{ close: void }>();

  /** Gap between the anchor and the popover (mockup `.menu` top offset). */
  const GAP = 7;
  /** Minimum distance kept from every viewport edge. */
  const EDGE = 8;

  let el: HTMLElement | null = null;
  let x = 0;
  let y = 0;
  let placed = false;
  /** The last rect the anchor had while it was actually on screen. A hover
   *  affordance — the tile's ⋯, which lives in a `:hover` strip — STOPS being
   *  hovered the moment this popover covers the pointer, and a zero-sized
   *  rect would otherwise fling the menu into the top-left corner on the
   *  next reposition. */
  let lastRect: DOMRect | null = null;

  /** Measure and place. Runs after the DOM has the popover, so the real
   *  width/height are known — nothing here assumes the CSS width. */
  async function place(): Promise<void> {
    if (!open || !anchor) {
      placed = false;
      lastRect = null;
      return;
    }
    await tick();
    if (!el || !anchor) return;
    const live = anchor.getBoundingClientRect();
    if (live.width || live.height) lastRect = live;
    const a = live.width || live.height ? live : (lastRect ?? live);
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    // horizontal: line up with the chosen edge, then clamp into view. The
    // max() keeps the clamp sane when the popover is wider than the viewport.
    let nx = align === "end" ? a.right - w : a.left;
    nx = Math.min(Math.max(nx, EDGE), Math.max(EDGE, vw - w - EDGE));
    // vertical: below the anchor, flipping above when that would overflow,
    // and clamped when neither side fits (a short viewport).
    let ny = a.bottom + GAP;
    if (ny + h > vh - EDGE) {
      const above = a.top - h - GAP;
      ny = above >= EDGE ? above : Math.max(EDGE, vh - h - EDGE);
    }
    x = nx;
    y = ny;
    placed = true;
  }

  // re-place whenever it opens or the anchor changes
  $: {
    open;
    anchor;
    void place();
  }

  function onWindowClick(e: MouseEvent): void {
    if (!open) return;
    const t = e.target as Node;
    if (el?.contains(t)) {
      // A verb list closes the moment one of its verbs is picked; a chooser
      // (`pop`) does not — you set several fields in it before leaving.
      if (kind === "menu" && (t as HTMLElement).closest?.("button")) dispatch("close");
      return;
    }
    if (anchor?.contains(t)) return;
    dispatch("close");
  }

  function onKeydown(e: KeyboardEvent): void {
    if (open && e.key === "Escape") dispatch("close");
  }

  /** Any scroll — including one inside the tile grid, which is why this is a
   *  capturing document listener rather than `on:scroll` on the window. */
  function onScroll(): void {
    if (open) void place();
  }

  onMount(() => {
    document.addEventListener("scroll", onScroll, true);
  });

  onDestroy(() => {
    document.removeEventListener("scroll", onScroll, true);
  });
</script>

<svelte:window on:click={onWindowClick} on:keydown={onKeydown} on:resize={() => void place()} />

{#if open}
  <div
    bind:this={el}
    class={kind}
    role={ariaRole}
    data-role={dataRole || null}
    style={`left:${x}px;top:${y}px;visibility:${placed ? "visible" : "hidden"}`}
  >
    <slot />
  </div>
{/if}
