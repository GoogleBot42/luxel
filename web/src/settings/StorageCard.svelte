<script lang="ts">
  // Advanced › Storage — the pattern store, the heap, and (caps-gated) the
  // external PSRAM arena. Read-only: nothing here is a setting, it is what
  // the device has left.
  import {
    deviceCaps,
    deviceEngineHeap,
    deviceHeapFree,
    devicePatterns,
    deviceStore,
  } from "../stores/device";

  const kb = (n: number): string => `${(n / 1024).toFixed(1)} KB`;
</script>

<div class="field">
  <span class="flabel">Patterns</span>
  <div class="fctl row g10">
    <span class="mono" data-role="storage-patterns">
      {$deviceStore?.patterns ?? $devicePatterns.length} stored
    </span>
    {#if $deviceStore}
      <span class="dim hint" data-role="storage-bytes">
        {kb($deviceStore.used)} of {kb($deviceStore.total)} used{$deviceStore.dead
          ? ` · ${kb($deviceStore.dead)} reclaimable on the next compaction`
          : ""}
      </span>
    {:else}
      <span class="dim hint">this host does not report store occupancy</span>
    {/if}
  </div>
</div>
<div class="field">
  <span class="flabel">Free heap</span>
  <div class="fctl row g10">
    <span class="mono" data-role="storage-heap">
      {$deviceHeapFree > 0 ? kb($deviceHeapFree) : "—"}
    </span>
    <span class="dim hint">
      {$deviceEngineHeap > 0
        ? `the running pattern holds ${kb($deviceEngineHeap)}, handed back before the next one loads`
        : "measured with the current pattern still loaded"}
    </span>
  </div>
</div>
{#if $deviceCaps?.psram}
  <div class="field">
    <span class="flabel">PSRAM</span>
    <div class="fctl row g10">
      <span class="mono" data-role="storage-psram">present</span>
      <span class="dim hint">
        pattern arrays live in the external arena instead of internal DRAM, so a big
        <code>array()</code> costs the engine nothing
      </span>
    </div>
  </div>
{/if}
