<script lang="ts">
  // Identity + the fixed hardware facts: address, pixel count, LED protocol,
  // strip data pin, and the local status line.
  import { createEventDispatcher } from "svelte";
  import {
    dataPin,
    dataPinChoice,
    dataPinDefault,
    dataPinNext,
    dataPins,
    device,
    deviceError,
    deviceProtocol,
    devicePixels,
    pixelMax,
    protocolOptions,
  } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { layout } from "../stores/geometry";
  import { note, notes } from "../stores/notify";
  import { previewFps, runtimeError } from "../stores/pattern";

  const dispatch = createEventDispatcher<{ pixelchange: void }>();

  /** Live pixel-count change: the device resizes its strip (no reboot); we
   *  re-anchor the local preview to the new count. */
  function onPixelCountChange(e: Event): void {
    const n = Math.max(1, Math.min($pixelMax, Number((e.target as HTMLInputElement).value) || 1));
    void (async () => {
      const r = await $device?.setConfig(n);
      if (r?.ok) {
        devicePixels.set(r.pixels ?? n);
        // the arrangement resets to a plain strip at the new count (any grid/
        // map was derived from the old count)
        layout.set({ kind: "strip", pixels: $devicePixels });
        dispatch("pixelchange");
      } else if (r) {
        deviceError.set(r.error ? `config: ${r.error}` : "config change failed");
      }
    })();
  }

  /** Live LED-protocol change: the device reconfigures its driver (no reboot). */
  function onProtocolChange(e: Event): void {
    const name = (e.target as HTMLSelectElement).value;
    deviceProtocol.set(name);
    void (async () => {
      const r = await $device?.setProtocol(name);
      if (r?.ok && r.protocol) deviceProtocol.set(r.protocol);
      else if (r?.error) deviceError.set(`protocol: ${r.error}`);
    })();
  }

  /** Strip DATA pin picker (Gitea #154). Unlike the protocol/pixel fields this
   *  is NOT live: the pick sits in the form until "apply & reboot", because the
   *  device reboots to rebind its SPI driver, and a mis-click that darkens the
   *  strip should take a deliberate second step. */
  function onDataPinPick(e: Event): void {
    const v = Number((e.target as HTMLSelectElement).value);
    dataPinChoice.set(v === $dataPin && $dataPinNext === null ? null : v);
    note("datapin", "");
  }

  async function applyDataPin(): Promise<void> {
    const pin = $dataPinChoice ?? $dataPinNext;
    if (pin === null) return;
    const ok = await confirm({
      title: `Move the strip data line to GPIO${pin}?`,
      body:
        `The driver rebinds to GPIO${pin}${pin === $dataPinDefault ? " (the board default)" : ""}. ` +
        "The strip stays dark until it is wired to that pin.",
      confirmLabel: "Apply & reboot",
      reboot: true,
    });
    if (!ok) return;
    note("datapin", "saving…");
    const r = await $device?.setDataPin(pin === $dataPinDefault ? "default" : pin);
    if (r?.ok) {
      note("datapin", `saved — the device is rebooting with data on GPIO${r.data_pin ?? pin}`);
      dataPinNext.set(pin);
      dataPinChoice.set(null);
    } else {
      note("datapin", r?.error ? `failed: ${r.error}` : "save failed");
    }
  }
</script>

<section class="card">
  <h2>Device</h2>
  <div class="field">
    <span class="flabel">Address</span>
    <input class="mono grow" value={$device?.base || "served from device"} disabled />
  </div>
  <div class="field">
    <span class="flabel">Pixels</span>
    <input
      class="num"
      data-role="cfg-pixels"
      type="number"
      min="1"
      max={$pixelMax}
      value={$devicePixels}
      on:change={onPixelCountChange}
    />
    <span class="dim">resized live — max {$pixelMax}, no reboot</span>
  </div>
  <div class="field">
    <span class="flabel">LED protocol</span>
    <select data-role="cfg-protocol" value={$deviceProtocol} on:change={onProtocolChange}>
      {#each $protocolOptions as opt}
        <option value={opt}>{opt}</option>
      {/each}
    </select>
    <span class="dim">match your strip — switched live (no reboot)</span>
  </div>
  {#if $dataPins.length}
    <div class="field">
      <span class="flabel">Data pin</span>
      <select
        data-role="cfg-datapin"
        value={String($dataPinChoice ?? $dataPinNext ?? $dataPin)}
        on:change={onDataPinPick}
      >
        {#each $dataPins as pin}
          <option value={String(pin)}>GPIO{pin}{pin === $dataPinDefault ? " (board default)" : ""}</option>
        {/each}
      </select>
      <button
        data-role="cfg-datapin-apply"
        disabled={!$device || ($dataPinChoice === null && $dataPinNext === null)}
        on:click={() => void applyDataPin()}
      >
        apply &amp; reboot
      </button>
      <span class="dim" data-role="cfg-datapin-note">
        {#if $notes.datapin}
          {$notes.datapin}
        {:else if $dataPinNext !== null}
          stored GPIO{$dataPinNext}, driving GPIO{$dataPin} until the next reboot
        {:else}
          driving GPIO{$dataPin} — where the strip's DATA wire goes; applied on reboot
        {/if}
      </span>
    </div>
  {/if}
  <div class="field">
    <span class="flabel">Status</span>
    <span class="mono dim">
      {$previewFps.toFixed(0)} fps (local preview){$runtimeError
        ? ` · vmerr: ${$runtimeError.message}`
        : ""}
    </span>
  </div>
</section>
