<script lang="ts">
  export let vars: Record<string, number | number[]> = {};

  function fmt(v: number): string {
    return v.toFixed(4).replace(/\.?0+$/, "") || "0";
  }
</script>

{#if Object.keys(vars).length > 0}
  <table>
    <tbody>
      {#each Object.entries(vars) as [name, v] (name)}
        <tr>
          <td class="name mono">{name}</td>
          <td class="value mono">
            {#if Array.isArray(v)}
              [{v.slice(0, 8).map(fmt).join(", ")}{v.length > 8 ? `, …×${v.length}` : ""}]
            {:else}
              {fmt(v)}
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  td {
    padding: 2px 6px;
    border-bottom: 1px solid var(--border);
  }

  .name {
    color: var(--text-dim);
    width: 40%;
  }

  .value {
    color: var(--accent);
  }
</style>
