<script lang="ts">
  // Device settings, ranked (proposal §5.3, mockup S3; Gitea #469).
  //
  // Nine equal-weight cards became: the three things people actually open
  // Settings for — Device (name + brightness), LED layout, WiFi — as full
  // sections at the top, then ONE `Advanced` list where every collapsed row
  // states its current value, so the page answers "is X on?" without being
  // expanded. Nothing is disabled: a field the device does not advertise is
  // absent (`lib/settingsCaps.ts`, §5.7).
  //
  // The page owns the order, the 0.5 Hz refresh while it is visible, and the
  // two navigations out; each section owns its form and its endpoint.
  import { createEventDispatcher, onDestroy } from "svelte";
  import { settingsVisibility } from "../lib/settingsCaps";
  import {
    clockStatus,
    device,
    deviceCaps,
    deviceLayoutWire,
    deviceRescanHz,
    deviceStore,
    devicePatterns,
    deviceVersion,
    mqttStatus,
    netLive,
    outputStatus,
    pollSubscribe,
    refreshClock,
    refreshLayout,
    refreshMqtt,
    refreshOutput,
    refreshSync,
    syncStatus,
  } from "../stores/device";
  import { layout as geomLayout } from "../stores/geometry";
  import ClockCard from "../settings/ClockCard.svelte";
  import DeviceCard from "../settings/DeviceCard.svelte";
  import Disclosure from "../settings/Disclosure.svelte";
  import FirmwareCard from "../settings/FirmwareCard.svelte";
  import LayoutCard from "../settings/LayoutCard.svelte";
  import MqttCard from "../settings/MqttCard.svelte";
  import NetworkInputCard from "../settings/NetworkInputCard.svelte";
  import OutputCard from "../settings/OutputCard.svelte";
  import PanelDriverCard from "../settings/PanelDriverCard.svelte";
  import Section from "../settings/Section.svelte";
  import StorageCard from "../settings/StorageCard.svelte";
  import SyncCard from "../settings/SyncCard.svelte";
  import WifiCard from "../settings/WifiCard.svelte";
  import "../settings/cards.css";

  /** The tab is the visible one — gates the refresh subscription. */
  export let active = false;

  const dispatch = createEventDispatcher<{
    navigate: "patterns";
    pixelchange: void;
    openmap: void;
  }>();

  /** The polled half: the read-only status lines the disclosure rows carry.
   *  `/api/output` and `/api/layout` are read once on arrival instead — they
   *  are forms, and re-reading one under the user's fingers would fight their
   *  edits (and a layout POST's reply already IS the new state). DDP/E1.31
   *  liveness is NOT here: it is a field of `/api/status`, which the session
   *  poll already reads every second (#540).
   *
   *  ONE AT A TIME, deliberately (#540). Fired in parallel these three filled
   *  both fetchgate slots at once, and with the 1 Hz status poll alongside
   *  them the browser needed a third TCP connection — which is one more than
   *  a device has spare (`WEB_TASK_POOL_SIZE` 3, and a closing connection
   *  holds its slot for up to 2 s). Measured on the Athom with the Settings
   *  tab open: all three slots busy, a second client refused, and the page's
   *  own polls failing and retrying. Awaited in sequence the tab costs at
   *  most one connection beyond the status poll. */
  async function refreshLive(): Promise<void> {
    await refreshMqtt();
    await refreshSync();
    await refreshClock();
  }

  let unsubscribe: (() => void) | undefined;
  $: {
    unsubscribe?.();
    unsubscribe = undefined;
    if (active && $device) {
      void (async () => {
        await refreshLayout();
        await refreshOutput();
        await refreshLive();
      })();
      unsubscribe = pollSubscribe("settings", 2000, refreshLive);
    }
  }
  onDestroy(() => unsubscribe?.());

  $: vis = settingsVisibility($deviceCaps, {
    kind: $deviceLayoutWire?.kind ?? "strip",
    dims: $geomLayout.dims,
    regular: $geomLayout.regular,
    panels: Math.max(
      1,
      ($deviceLayoutWire?.matrix?.cols ?? 1) * ($deviceLayoutWire?.matrix?.rows ?? 1),
    ),
  });

  const kb = (n: number): string => `${(n / 1024).toFixed(0)} KB`;

  /** Every disclosure row carries a one-line status, so a collapsed page
   *  still answers "is X on?" (§5.3). These are the sentences. */
  $: outputStatusLine = (() => {
    const o = $outputStatus;
    if (!o) return "not available on this firmware";
    const parts = [`gamma ${(o.gamma / 10).toFixed(1)}`];
    if (vis.powerCap) parts.push(o.capMa > 0 ? `cap ${o.capMa} mA` : "no power cap");
    if (vis.blurGlow) parts.push(`blur ${o.blur}`, `glow ${o.glow}`);
    return parts.join(" · ");
  })();

  $: clockLine = $clockStatus
    ? `UTC${$clockStatus.tzMinutes >= 0 ? "+" : ""}${($clockStatus.tzMinutes / 60).toFixed(
        Math.abs($clockStatus.tzMinutes % 60) > 0 ? 1 : 0,
      )} · ${$clockStatus.synced ? "synced" : "not synced"}`
    : "not reported";

  $: syncLine =
    $syncStatus?.mode === "follower"
      ? $syncStatus.leader
        ? `following (offset ${$syncStatus.leader.offsetMs} ms)`
        : "follower · waiting for a leader"
      : ($syncStatus?.mode ?? "off");

  $: mqttLine = $mqttStatus?.connected
    ? `connected to ${$mqttStatus.host}`
    : $mqttStatus?.enabled
      ? `${$mqttStatus.host} · not connected`
      : "disabled";

  $: netinLine =
    $netLive === "ddp" ? "receiving DDP" : $netLive === "e131" ? "receiving E1.31" : "idle";

  $: storageLine = (() => {
    const n = $deviceStore?.patterns ?? $devicePatterns.length;
    const bytes = $deviceStore ? ` · ${kb($deviceStore.total - $deviceStore.used)} free` : "";
    return `${n} pattern${n === 1 ? "" : "s"}${bytes}`;
  })();

  $: firmwareLine = [
    $deviceVersion ? `v${$deviceVersion}` : "version unknown",
    vis.ota ? "Update…" : null,
    vis.reboot ? "Reboot into setup AP" : null,
  ]
    .filter(Boolean)
    .join(" · ");

  $: panelLine = `30 MHz · 7 planes${$deviceRescanHz > 0 ? ` · ${$deviceRescanHz} Hz` : ""}`;
</script>

<div class="settings-tab" data-role="settings-panel" hidden={!active}>
  <div class="settings">
    <div class="titlerow">
      <h1>Settings</h1>
      <span class="mono tiny dim">{$device?.base || "served from this device"}</span>
    </div>

    <Section title="Device" role="sect-device">
      <DeviceCard />
    </Section>

    <Section title="LED layout" role="sect-layout">
      <LayoutCard
        {active}
        on:pixelchange={() => dispatch("pixelchange")}
        on:openmap={() => dispatch("openmap")}
      />
    </Section>

    <Section title="WiFi" role="sect-wifi">
      <WifiCard />
    </Section>

    <div class="advdiv">
      <div class="slabel">Advanced</div>
      <div class="rule"></div>
    </div>
    <p class="dim hint advnote">Settings most installs never touch.</p>

    <div class="disclist" data-role="advanced">
      <Disclosure title="Output processing" status={outputStatusLine} role="adv-output">
        <OutputCard
          showPowerCap={vis.powerCap}
          showBlurGlow={vis.blurGlow}
          scope={vis.blurGlowScope}
        />
      </Disclosure>

      {#if vis.panelDriver}
        <Disclosure title="Panel driver" status={panelLine} role="adv-panel">
          <PanelDriverCard />
        </Disclosure>
      {/if}

      <Disclosure title="Clock &amp; time zone" status={clockLine} role="adv-clock">
        <ClockCard />
      </Disclosure>

      <Disclosure title="Multi-device sync" status={syncLine} role="adv-sync">
        <SyncCard />
      </Disclosure>

      <Disclosure title="MQTT · Home Assistant" status={mqttLine} role="adv-mqtt">
        <MqttCard />
      </Disclosure>

      <Disclosure title="Network input (DDP / E1.31)" status={netinLine} role="adv-netin">
        <NetworkInputCard />
      </Disclosure>

      <Disclosure title="Storage" status={storageLine} role="adv-storage">
        <StorageCard />
        <p class="dim hint">
          Manage stored patterns from the
          <button class="link" on:click={() => dispatch("navigate", "patterns")}>Patterns</button>
          tab.
        </p>
      </Disclosure>

      <Disclosure title="Firmware &amp; recovery" status={firmwareLine} role="adv-firmware">
        <FirmwareCard />
      </Disclosure>
    </div>
  </div>
</div>

<style>
  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .settings-tab[hidden] {
    display: none;
  }

  .settings-tab {
    flex: 1;
    min-height: 0;
    background: var(--bg-panel);
    overflow-y: auto;
  }
</style>
