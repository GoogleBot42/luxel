<script lang="ts">
  // One top-level Settings section (proposal §5.3, mockup S3): a small
  // uppercase label, a hairline rule across the rest of the row, an optional
  // mono note at the right end, and the form beneath it.
  //
  // Only the FORM carries a panel background — the header is type and a rule.
  // That is what kills the card-in-card look the nine v1 cards had.
  export let title: string;
  /** `data-role` for the section body, so a query can be scoped to it. */
  export let role = "";
</script>

<section class="sect">
  <div class="secthead">
    <div class="slabel">{title}</div>
    <div class="rule"></div>
  </div>
  <div class="form" data-role={role || undefined}>
    <slot />
  </div>
</section>
