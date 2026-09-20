<script lang="ts">
  // "Changes to <fields> apply after a reboot" — pinned to the bottom of the
  // VIEWPORT, in the warning palette, on every screen until the device
  // restarts (Gitea #538).
  //
  // It replaces the line of dim 12px text that used to sit at the bottom of
  // the LED layout form: "for changes that need a reboot, the change at the
  // bottom is too subtle. It should be much more significant. Maybe attached
  // to the bottom/top of webpage with significant colors" (Jeremy,
  // 2026-09-19). It lives in the shell rather than in Settings because the
  // user walks away to the Patterns tab and the device is still running
  // something other than what the form shows.
  //
  // What lands here is the DEVICE's own `reboot_required`, never a guess —
  // `stores/device.ts` `noteRebootPending`. Protocol and colour order are
  // live on both hosts and never appear (docs/api.md "Live vs reboot").
  import { device, deviceCaps, rebootPending } from "../stores/device";
  import { confirm } from "../stores/dialog";

  let rebooting = false;

  /** `a`, `a and b`, `a, b and c` — the fields, in the order they changed. */
  function phrase(list: readonly string[]): string {
    if (list.length <= 1) return list[0] ?? "";
    return `${list.slice(0, -1).join(", ")} and ${list[list.length - 1]}`;
  }

  async function rebootNow(): Promise<void> {
    const ok = await confirm({
      title: "Reboot to apply?",
      body: "The device restarts and comes back with the settings below. The fixture goes dark for a few seconds.",
      confirmLabel: "Reboot",
      reboot: true,
    });
    if (!ok) return;
    rebooting = true;
    const r = await $device?.reboot();
    if (r?.ok) rebootPending.set([]);
    else rebooting = false;
  }
</script>

{#if $rebootPending.length > 0}
  <div class="rebootbar" data-role="reboot-bar" role="status">
    <span class="msg" data-role="reboot-bar-text">
      {#if rebooting}
        Rebooting — the fixture comes back with {phrase($rebootPending)} applied.
      {:else}
        Changes to {phrase($rebootPending)} apply after a reboot.
      {/if}
    </span>
    <!-- absent, never disabled (§5.7): a host that cannot reboot itself (the
         mirror) states the fact and leaves the power cycle to the human -->
    {#if $deviceCaps?.reboot && !rebooting}
      <button class="btn sm rb" data-role="reboot-now" on:click={() => void rebootNow()}>
        Reboot now
      </button>
    {/if}
  </div>
{/if}

<style>
  /* the mockups' `.capstrip`, to the number (padding, gap, radius, the
     1px rgba(217,163,67,.32) border, 11.5px on rgba(217,163,67,.12)) — the
     one thing that is NOT the mockup's is that it is pinned to the viewport
     rather than sitting in the flow, which is what Jeremy asked for */
  .rebootbar {
    position: fixed;
    left: 16px;
    right: 16px;
    bottom: 16px;
    z-index: 70;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: 1px solid rgba(217, 163, 67, 0.32);
    border-radius: 6px;
    background: rgba(217, 163, 67, 0.12);
    color: #e5bd74;
    font-size: 11.5px;
    white-space: nowrap;
    backdrop-filter: blur(6px);
  }

  .msg {
    min-width: 0;
  }

  /* the button goes to the far end, as `.capstrip a` does */
  .rebootbar .rb {
    margin-left: auto;
  }

  /* on the warning ground the button borrows the warning ink, not the
     page's — an amber-on-amber outline reads as part of the strip */
  .rebootbar .rb {
    flex: none;
    border-color: rgba(217, 163, 67, 0.55);
    background: rgba(217, 163, 67, 0.16);
    color: #f0d199;
  }

  .rebootbar .rb:hover {
    border-color: #e5bd74;
  }

  @media (max-width: 560px) {
    .rebootbar {
      left: 8px;
      right: 8px;
      bottom: 8px;
      white-space: normal;
      text-align: left;
    }
  }
</style>
