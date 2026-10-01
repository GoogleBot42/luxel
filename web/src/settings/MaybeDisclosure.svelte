<script lang="ts">
  // A block that is a collapsed `Disclosure` on one board and plain inline
  // content on another — LED layout's rule row (start · runs · serpentine ·
  // turned) is folded under "Regular pattern" on a panel board, where the
  // wall is transcribed panel by panel instead (Gitea #920), and stays an
  // ordinary field row on a strip-built matrix. One copy of the markup
  // either way. `role` lands on the wrapper as well as on the Disclosure's
  // own `<role>-row/-toggle/-status/-body`.
  import Disclosure from "./Disclosure.svelte";

  export let wrap: boolean;
  export let title: string;
  export let status: string;
  export let role: string;
</script>

{#if wrap}
  <div class="disclist inline" data-role={role}>
    <Disclosure {title} {status} {role}>
      <slot name="note" />
      <slot />
    </Disclosure>
  </div>
{:else}
  <slot />
{/if}
