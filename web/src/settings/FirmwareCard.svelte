<script lang="ts">
  // Advanced › Firmware & recovery — the version, and the two actions that
  // reboot the device. Both are caps-gated (`caps.ota` / `caps.reboot`): the
  // mirror advertises neither, so on a mirror this row is a version line and
  // nothing else — absent, never disabled (§5.7).
  //
  // Neither reboots from a bare button: each goes through `confirm({reboot})`,
  // which renders the standing "the device reboots to apply this" line.
  import { device, deviceCaps, deviceSlot, deviceVersion } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { note, notes } from "../stores/notify";

  let fileInput: HTMLInputElement | undefined;
  let busy = false;

  async function startApMode(): Promise<void> {
    const ok = await confirm({
      title: "Reboot into the setup access point?",
      body: "The device leaves this network for one boot and comes back as an open AP (luxel-…, http://192.168.4.1/). Rejoin this network by saving WiFi from the AP, or just reboot it again.",
      confirmLabel: "Reboot into AP",
      reboot: true,
    });
    if (!ok) return;
    const r = await $device?.startApMode();
    note(
      "ap",
      r?.ok ? 'rebooting into AP "luxel-…" — connect to it at 192.168.4.1' : "failed",
      8000,
    );
  }

  /** Stream a firmware image to the inactive OTA slot. The device writes it,
   *  replies, and reboots into it ~400 ms later. */
  async function onImagePicked(e: Event): Promise<void> {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = ""; // so picking the same file twice fires again
    if (!file) return;
    const ok = await confirm({
      title: `Install ${file.name}?`,
      body: `${(file.size / 1024).toFixed(0)} KB is written to the inactive OTA slot and the device reboots into it. A bad image rolls back on the next boot.`,
      confirmLabel: "Install & reboot",
      reboot: true,
    });
    if (!ok) return;
    busy = true;
    note("ota", "uploading…");
    try {
      const r = await $device?.otaUpload(await file.arrayBuffer());
      note(
        "ota",
        r?.ok
          ? `wrote ${r.bytes ?? file.size} bytes — the device is rebooting into the new image`
          : `failed: ${r?.error ?? "rejected"}`,
        0,
      );
    } catch (err) {
      note("ota", `failed: ${String(err)}`, 0);
    } finally {
      busy = false;
    }
  }
</script>

<div class="field">
  <span class="flabel">Version</span>
  <span class="mono" data-role="fw-version">
    {$deviceVersion ? `v${$deviceVersion}` : "—"}
    {#if $deviceSlot}<span class="dim">· {$deviceSlot}</span>{/if}
  </span>
</div>

{#if $deviceCaps?.ota}
  <div class="field">
    <input
      bind:this={fileInput}
      type="file"
      accept=".bin,application/octet-stream"
      hidden
      data-role="fw-file"
      on:change={(e) => void onImagePicked(e)}
    />
    <button data-role="fw-update" disabled={busy} on:click={() => fileInput?.click()}>
      Update…
    </button>
    <span class="dim hint">
      pick a <span class="mono">luxel.bin</span> built for this board — it is written to the
      other OTA slot and the device <strong>reboots</strong> into it
    </span>
    {#if $notes.ota}<span class="dim" data-role="fw-note">{$notes.ota}</span>{/if}
  </div>
{/if}

{#if $deviceCaps?.reboot}
  <div class="field">
    <button data-role="apmode" on:click={() => void startApMode()}>Reboot into setup AP</button>
    <span class="dim hint">
      one boot only — good for re-provisioning; it comes back as a station afterwards
    </span>
    {#if $notes.ap}<span class="dim" data-role="apmode-note">{$notes.ap}</span>{/if}
  </div>
{/if}
