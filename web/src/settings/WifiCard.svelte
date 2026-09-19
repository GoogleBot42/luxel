<script lang="ts">
  // WiFi provisioning + the one-boot setup access point. Both reboot the
  // device, so both confirm first.
  import { device, wifiForm, wifiSource, wifiSsid } from "../stores/device";
  import { note, notes } from "../stores/notify";

  /** Save WiFi creds — the device stores them and reboots to apply. */
  function saveWifi(): void {
    const ssid = $wifiForm.ssid.trim();
    if (!ssid) return;
    if (!window.confirm(`Save WiFi and reboot the device to join "${ssid}"?`)) return;
    void (async () => {
      note("wifi", "saving…");
      const r = await $device?.setWifi(ssid, $wifiForm.password);
      if (r?.ok) {
        note("wifi", "saved — the device is rebooting to join the new network");
        wifiSsid.set(ssid);
        wifiSource.set("flash");
      } else {
        note("wifi", r?.error ? `failed: ${r.error}` : "save failed");
      }
    })();
  }

  function startApMode(): void {
    if (!window.confirm("Reboot the device into its setup access point? It leaves this network for one boot (rejoin it by saving WiFi from the AP, or just reboot it again).")) return;
    void (async () => {
      const r = await $device?.startApMode();
      note(
        "ap",
        r?.ok ? 'rebooting into AP "luxel-…" — connect to it at 192.168.4.1' : "failed",
        8000,
      );
    })();
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
      on:click={saveWifi}
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
    <button data-role="apmode" on:click={startApMode}>reboot into setup AP</button>
    <span class="dim">
      one boot only — good for re-provisioning; it comes back as a station afterwards
    </span>
    {#if $notes.ap}<span class="dim" data-role="apmode-note">{$notes.ap}</span>{/if}
  </div>
</section>
