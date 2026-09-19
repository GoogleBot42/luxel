<script lang="ts">
  // Device settings. The page is a list of cards; each card in ../settings/
  // owns its own form and its own endpoint. The page owns only the order, the
  // 0.5 Hz refresh while it is visible, and the one navigation link out.
  import { createEventDispatcher, onDestroy } from "svelte";
  import {
    device,
    devicePatterns,
    pollSubscribe,
    refreshClock,
    refreshMqtt,
    refreshNetLive,
    refreshOutput,
    refreshSync,
  } from "../stores/device";
  import BrightnessCard from "../settings/BrightnessCard.svelte";
  import ClockCard from "../settings/ClockCard.svelte";
  import DeviceCard from "../settings/DeviceCard.svelte";
  import MqttCard from "../settings/MqttCard.svelte";
  import NetworkInputCard from "../settings/NetworkInputCard.svelte";
  import OutputCard from "../settings/OutputCard.svelte";
  import SyncCard from "../settings/SyncCard.svelte";
  import WifiCard from "../settings/WifiCard.svelte";
  import "../settings/cards.css";

  /** The tab is the visible one — gates the refresh subscription. */
  export let active = false;

  const dispatch = createEventDispatcher<{ navigate: "patterns"; pixelchange: void; openmap: void }>();

  /** The polled half: the four read-only status lines. `/api/output` is read
   *  once on arrival instead — it is a form, and re-reading it under the
   *  user's fingers would fight their edits. */
  function refreshLive(): void {
    void refreshNetLive();
    void refreshMqtt();
    void refreshSync();
    void refreshClock();
  }

  let unsubscribe: (() => void) | undefined;
  $: {
    unsubscribe?.();
    unsubscribe = undefined;
    if (active && $device) {
      refreshLive();
      void refreshOutput();
      unsubscribe = pollSubscribe("settings", 2000, refreshLive);
    }
  }
  onDestroy(() => unsubscribe?.());
</script>

<div class="settings-tab" data-role="settings-panel" hidden={!active}>
  <div class="settings">
    <h1>Device settings</h1>

    <DeviceCard
      on:pixelchange={() => dispatch("pixelchange")}
      on:openmap={() => dispatch("openmap")}
    />
    <NetworkInputCard />
    <BrightnessCard />
    <WifiCard />
    <OutputCard />
    <ClockCard />
    <SyncCard />
    <MqttCard />

    <section class="card">
      <h2>Pattern library</h2>
      <p class="dim hint">
        {$devicePatterns.length} pattern{$devicePatterns.length === 1 ? "" : "s"} stored on the
        device. Manage them from the
        <button class="link" on:click={() => dispatch("navigate", "patterns")}>Patterns</button>
        tab.
      </p>
    </section>
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
