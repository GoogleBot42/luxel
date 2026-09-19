<script lang="ts">
  // One Advanced row (proposal §5.3, mockup S3): a chevron, a title, and a
  // one-line STATUS so a collapsed page still answers "is X on?" without
  // opening anything. The body is a slot, mounted only while open — these are
  // forms nobody scrolls past, and an unmounted one costs no reactivity.
  export let title: string;
  /** The one-line status (`gamma 2.2 · blur 0 · glow 0`). Never empty: a row
   *  with nothing to say states its default instead. */
  export let status: string;
  /** `data-role` stem: `<role>-row`, `<role>-status`, `<role>-body`. */
  export let role: string;

  let open = false;
</script>

<div class="drow" class:open data-role={`${role}-row`}>
  <button
    class="dhead"
    data-role={`${role}-toggle`}
    aria-expanded={open}
    on:click={() => (open = !open)}
  >
    <i class="chev">{open ? "▾" : "▸"}</i>
    <span class="dtitle">{title}</span>
    <span class="st2" data-role={`${role}-status`}>{status}</span>
  </button>
</div>
{#if open}
  <div class="dbody" data-role={`${role}-body`}>
    <slot />
  </div>
{/if}
