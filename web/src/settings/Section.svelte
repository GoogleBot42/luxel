<script lang="ts">
  // One top-level Settings section (proposal §5.3, mockup S3): a small
  // uppercase label, a hairline rule across the rest of the row, an optional
  // mono note at the right end, and the form beneath it.
  //
  // Only the FORM carries a panel background — the header is type and a rule.
  // That is what kills the card-in-card look the nine v1 cards had.
  export let title: string;
  /** The mono dim note at the right end of the header row — the fixture a
   *  section is about (`64×64 matrix`). Absent when empty (mockup S3e). */
  export let note = "";
  /** `data-role` for the section body, so a query can be scoped to it. */
  export let role = "";
  /** A form whose content is ONE line rather than a stack of labelled rows —
   *  the WiFi section, which is a status and a button (mockup S3 sets
   *  `display:flex;align-items:center;gap:14px` inline on exactly that one). */
  export let row = false;
</script>

<section class="sect" data-role={role ? `${role}-section` : undefined}>
  <div class="secthead">
    <div class="slabel">{title}</div>
    <div class="rule"></div>
    {#if note}<div class="mono tiny dim" data-role={role ? `${role}-note` : undefined}>{note}</div>{/if}
  </div>
  <div class="form" class:row data-role={role || undefined}>
    <slot />
  </div>
</section>
