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
  import { offsetLabel, psramLine, settingsVisibility, uiLayoutKind } from "../lib/settingsCaps";
  import {
    applyLayout,
    clockStatus,
    device,
    deviceCaps,
    deviceLabel,
    deviceLayoutWire,
    devicePsramFree,
    devicePsramTotal,
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
  import { layout as geomLayout, layoutLabel } from "../stores/geometry";
  import { luxel } from "../stores/pattern";
  import ClockCard from "../settings/ClockCard.svelte";
  import DeviceCard from "../settings/DeviceCard.svelte";
  import Disclosure from "../settings/Disclosure.svelte";
  import FirmwareCard from "../settings/FirmwareCard.svelte";
  import LayoutCard from "../settings/LayoutCard.svelte";
  import MqttCard from "../settings/MqttCard.svelte";
  import NetworkInputCard from "../settings/NetworkInputCard.svelte";
  import OutputCard from "../settings/OutputCard.svelte";
  import ProjectionBlock from "../settings/ProjectionBlock.svelte";
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

  /** `luxel-f6b0a8 · v0.1.44` (mockup S3) — WHICH board, and what it runs.
   *  Never the base URL, and never the words "served from this device": a
   *  line that does not name the device tells nobody which one they are
   *  looking at (Jeremy, 2026-09-19). */
  $: subtitle = [$deviceLabel, $deviceVersion ? `v${$deviceVersion}` : ""]
    .filter(Boolean)
    .join(" · ");

  $: vis = settingsVisibility($deviceCaps, {
    kind: uiLayoutKind($deviceLayoutWire?.kind ?? "strip", $geomLayout.dims),
    dims: $geomLayout.dims,
    regular: $geomLayout.regular,
    panels: Math.max(
      1,
      ($deviceLayoutWire?.matrix?.cols ?? 1) * ($deviceLayoutWire?.matrix?.rows ?? 1),
    ),
  });

  /** The note at the right end of the LED layout header. It is there to say
   *  WHICH fixture the section is about when there is more than one wire to
   *  keep straight (mockups S3j/S3k); a single-output board's summary line
   *  inside the form already says it, and S3 draws no note at all. */
  $: layoutNote =
    ($deviceCaps?.outputs ?? 1) > 1
      ? [$deviceLabel, `${$deviceLayoutWire?.outputs?.length ?? 1} outputs`]
          .filter(Boolean)
          .join(" · ")
      : "";

  /** The fixture the Projection section is about (mockups S3e–S3g). A
   *  lattice also states its pixel count, which its `8×8×8` alone does not
   *  give you at a glance. */
  $: projectionNote =
    $geomLayout.dims === 3
      ? `${layoutLabel($geomLayout)} · ${$geomLayout.pixels} px`
      : layoutLabel($geomLayout);

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

  // mockup S3: `UTC-6 · synced` — the same words the open form states.
  $: clockLine = $clockStatus
    ? `${offsetLabel($clockStatus.tzMinutes)} · ${$clockStatus.synced ? "synced" : "not synced"}`
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
    // a collapsed row still answers "how much have I got?" — the arena is
    // the biggest number on the page and belongs in the one-line status too
    // (Jeremy, 2026-09-20)
    const arena = vis.psram ? psramLine($devicePsramFree, $devicePsramTotal) : null;
    return `${n} pattern${n === 1 ? "" : "s"}${bytes}${arena ? ` · PSRAM ${arena}` : ""}`;
  })();

  $: firmwareLine = [
    $deviceVersion ? `v${$deviceVersion}` : "version unknown",
    vis.ota ? "Update…" : null,
    vis.reboot ? "Reboot into setup AP" : null,
  ]
    .filter(Boolean)
    .join(" · ");

</script>

<div class="settings-tab" data-role="settings-panel" hidden={!active}>
  <div class="settings">
    <div class="titlerow">
      <h1>Settings</h1>
      <span class="mono tiny dim" data-role="settings-subtitle">{subtitle}</span>
    </div>

    <Section title="Device" role="sect-device">
      <DeviceCard />
    </Section>

    <Section title="LED layout" role="sect-layout" note={layoutNote}>
      <LayoutCard
        on:pixelchange={() => dispatch("pixelchange")}
        on:openmap={() => dispatch("openmap")}
      />
    </Section>

    <!-- Projection is its OWN section (mockup S3e), not a block inside the
         LED layout form: it is about patterns, not about wiring, and it needs
         the section rule to separate the two (Jeremy, 2026-09-19).
         It is ABSENT where the Layout offers no choice — a strip shows only
         1D patterns since #538 — with no explanatory copy. -->
    {#if vis.projection}
      <Section title="Projection" role="sect-projection" note={projectionNote}>
        <ProjectionBlock
          luxel={$luxel ?? null}
          layout={$geomLayout}
          projection={$geomLayout.projection}
          {active}
          on:set={(e) => void applyLayout(`proj${e.detail.dims}d ${e.detail.mode}`)}
        />
      </Section>
    {/if}

    <Section title="WiFi" role="sect-wifi" row>
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

      <!-- Panel driver was HERE until 2026-09-26 (Gitea #778). It is the
           `Panel module` disclosure inside the LED layout section now —
           Jeremy: "The panel driver section doesn't belong in Advanced. That
           dropdown belongs closer or in the LED layout section." -->

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
