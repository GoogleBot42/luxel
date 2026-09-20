<script lang="ts">
  // "● luxel-f6b0a8 · 64×64 matrix" — WHICH board this console is driving and
  // what shape it is (mockup S1, proposal §4). It sits immediately after the
  // wordmark in the shell header, and the editor mounts the same component in
  // its header rail, where the shell header is gone (mockup S2).
  //
  // Its own component precisely so both headers show the same thing: the
  // device's identity is not the shell's property.
  import { device, deviceLabel } from "../stores/device";
  import { layoutName } from "../stores/geometry";
</script>

<span class="devchip" data-role="layout-chip" title="the device this console is driving">
  <i class="dot" class:live={$device !== null}></i>
  <span data-role="device-chip-name">{$deviceLabel}</span>
  <span aria-hidden="true">·</span>
  <span data-role="layout-label">{$layoutName}</span>
</span>

<style>
  .devchip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: 11.5px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
  }

  .dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--text-dim);
  }

  /* green only while a session is actually live */
  .dot.live {
    background: var(--ok);
  }
</style>
