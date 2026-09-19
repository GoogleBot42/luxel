<script lang="ts">
  // WiFi provisioning + the one-boot setup access point. Both reboot the
  // device, so both confirm first.
  import { device, wifiForm, wifiSource, wifiSsid } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { note, notes } from "../stores/notify";

  /** Save WiFi creds — the device stores them and reboots to apply. */
  async function saveWifi(): Promise<void> {
    const ssid = $wifiForm.ssid.trim();
    if (!ssid) return;
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
</script>

<section class="card">
  <h2>WiFi</h2>
  <div class="field">
    <span class="flabel">Current</span>
    <span class="mono" data-role="wifi-current">
      {$wifiSsid ?? "—"}
      <span class="dim">
        ({$wifiSource === "flash" ? "saved" : $wifiSource === "builtin" ? "compiled-in" : "none"})
      </span>
    </span>
  </div>
  <div class="field">
    <span class="flabel">Network</span>
    <input class="grow" data-role="wifi-ssid" placeholder="SSID" bind:value={$wifiForm.ssid} />
  </div>
  <div class="field">
    <span class="flabel">Password</span>
    <input
      class="grow"
      data-role="wifi-pass"
      type="password"
      placeholder="password"
      bind:value={$wifiForm.password}
    />
  </div>
  <div class="field">
    <button
      class="primary"
      data-role="wifi-save"
      disabled={!$device || !$wifiForm.ssid.trim()}
      on:click={() => void saveWifi()}
    >
      save &amp; reboot
    </button>
    {#if $notes.wifi}<span class="dim" data-role="wifi-note">{$notes.wifi}</span>{/if}
  </div>
  <p class="dim hint">
    The device stores the credentials in flash and <strong>reboots</strong> to join the new
    network. A device with no way onto any network boots as an open access point
    (<span class="mono">luxel-xxxx</span> → <span class="mono">http://192.168.4.1/</span>)
    where this same page provisions it.
  </p>
  <div class="field">
    <button data-role="apmode" on:click={() => void startApMode()}>reboot into setup AP</button>
    <span class="dim">
      one boot only — good for re-provisioning; it comes back as a station afterwards
    </span>
    {#if $notes.ap}<span class="dim" data-role="apmode-note">{$notes.ap}</span>{/if}
  </div>
</section>
