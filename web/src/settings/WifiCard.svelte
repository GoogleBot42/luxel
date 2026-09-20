<script lang="ts">
  // WiFi (proposal §5.3): the connected network, and the provisioning form
  // COLLAPSED behind `Change network…` — joining a different network is a
  // once-in-an-install action, and it reboots, so it does not deserve four
  // permanently open fields above Advanced.
  //
  // `Reboot into setup AP` moved to Advanced › Firmware & recovery, where the
  // other reboot-requiring actions are grouped.
  import { device, deviceHost, wifiForm, wifiSource, wifiSsid } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { note, notes } from "../stores/notify";

  let open = false;
  let ssidInput: HTMLInputElement | undefined;

  /** Save WiFi creds — the device stores them and reboots to apply.
   *  Save is always live (§5.7): it VALIDATES and says why, rather than
   *  greying itself out and leaving the user to guess (Gitea #529). */
  async function saveWifi(): Promise<void> {
    const ssid = $wifiForm.ssid.trim();
    if (!ssid) {
      note("wifi", "enter a network name");
      ssidInput?.focus();
      return;
    }
    if (!$device) {
      note("wifi", "device unreachable — reload to retry");
      return;
    }
    const ok = await confirm({
      title: "Save WiFi and reboot?",
      body: `The credentials are stored in flash and the device joins "${ssid}" on the next boot. If they are wrong it comes back as an open setup access point.`,
      confirmLabel: "Save & reboot",
      reboot: true,
    });
    if (!ok) return;
    note("wifi", "saving…");
    const r = await $device?.setWifi(ssid, $wifiForm.password);
    if (r?.ok) {
      note("wifi", "saved — the device is rebooting to join the new network");
      wifiSsid.set(ssid);
      wifiSource.set("flash");
    } else {
      note("wifi", r?.error ? `failed: ${r.error}` : "save failed");
    }
  }
</script>

<!-- mockup S3: `● Connected to  home-iot · 192.168.0.238    [Change network…]`
     — the status and the button ARE the form's one line (`Section row`) -->
<div class="wifirow">
  <!-- green = this device is ON a network, which is a fact the page can only
       be reading because it is (mockup S3 draws it green). The SSID beside it
       is what the device STORED, and a host that stores none says so. -->
  <span class="dot" class:live={$device !== null}></span>
  <span class="tiny dim">Connected to</span>
  <span
    class="mono tiny"
    data-role="wifi-current"
    title={$wifiSource === "flash"
      ? "saved on the device"
      : $wifiSource === "builtin"
        ? "compiled into this firmware"
        : "no network stored"}>{$wifiSsid ?? "—"}</span
  >
  {#if $deviceHost}
    <span class="dim">·</span>
    <span class="mono tiny" data-role="wifi-address">{$deviceHost}</span>
  {/if}
</div>
<span class="spacer"></span>
<button class="btn" data-role="wifi-change" aria-expanded={open} on:click={() => (open = !open)}>
  {open ? "Cancel" : "Change network…"}
</button>

{#if open}
  <div class="field">
    <span class="flabel">Network</span>
    <div class="fctl">
      <input
        class="inp wide"
        data-role="wifi-ssid"
        placeholder="SSID"
        bind:this={ssidInput}
        bind:value={$wifiForm.ssid}
      />
    </div>
  </div>
  <div class="field">
    <span class="flabel">Password</span>
    <div class="fctl">
      <input
        class="inp wide"
        data-role="wifi-pass"
        type="password"
        placeholder="password"
        bind:value={$wifiForm.password}
      />
    </div>
  </div>
  <div class="field">
    <span class="flabel"></span>
    <div class="fctl row g10">
      <button class="btn primary" data-role="wifi-save" on:click={() => void saveWifi()}>
        save &amp; reboot
      </button>
      {#if $notes.wifi}<span class="dim hint" data-role="wifi-note">{$notes.wifi}</span>{/if}
    </div>
  </div>
  <p class="dim hint">
    The device stores the credentials in flash and <strong>reboots</strong> to join the new
    network. A device with no way onto any network boots as an open access point
    (<span class="mono">luxel-xxxx</span> → <span class="mono">http://192.168.4.1/</span>)
    where this same page provisions it.
  </p>
{/if}

<style>
  .wifirow {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .wide {
    width: 100%;
    max-width: 260px;
  }

  .spacer {
    flex: 1;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-dim);
    flex: none;
  }

  .dot.live {
    background: var(--ok);
  }
</style>
