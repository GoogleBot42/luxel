<script lang="ts">
  // MQTT / Home Assistant broker details. Applied live, no reboot.
  import { device, mqttForm, mqttStatus, refreshMqtt } from "../stores/device";
  import { note, notes } from "../stores/notify";

  /** Save is always live (§5.7): it validates and explains rather than
   *  greying itself out when the device is unreachable (Gitea #529). */
  function saveMqtt(): void {
    void (async () => {
      if (!$device) {
        note("mqtt", "device unreachable — reload to retry");
        return;
      }
      note("mqtt", "saving…");
      const host = $mqttForm.host.trim();
      const r = await $device?.setMqtt($mqttForm.host.trim(), $mqttForm.port, $mqttForm.user.trim(), $mqttForm.pass);
      if (r?.ok) {
        note("mqtt", host ? "saved — connecting to the broker…" : "saved — MQTT disabled");
        mqttForm.update((f) => ({ ...f, pass: "" }));
      } else {
        note("mqtt", r?.error ? `failed: ${r.error}` : "save failed");
      }
      void refreshMqtt();
    })();
  }
</script>

<div class="field">
  <span class="flabel">Status</span>
  <span class="mono" data-role="mqtt-status">
    {$mqttStatus?.connected ? "connected" : $mqttStatus?.enabled ? "not connected" : "disabled"}
  </span>
</div>
<div class="field">
  <span class="flabel">Broker</span>
  <input
    class="grow"
    data-role="mqtt-host"
    placeholder="host or IP (blank = disable)"
    bind:value={$mqttForm.host}
  />
  <input
    class="num"
    data-role="mqtt-port"
    type="number"
    min="1"
    max="65535"
    bind:value={$mqttForm.port}
  />
</div>
<div class="field">
  <span class="flabel">User</span>
  <input class="grow" data-role="mqtt-user" placeholder="optional" bind:value={$mqttForm.user} />
</div>
<div class="field">
  <span class="flabel">Password</span>
  <input
    class="grow"
    data-role="mqtt-pass"
    type="password"
    placeholder={$mqttStatus?.hasPass ? "(saved — retype to change)" : "optional"}
    bind:value={$mqttForm.pass}
  />
</div>
<div class="field">
  <button class="primary" data-role="mqtt-save" on:click={saveMqtt}>
    save
  </button>
  {#if $notes.mqtt}<span class="dim" data-role="mqtt-note">{$notes.mqtt}</span>{/if}
</div>
<p class="dim hint">
  Point this at your MQTT broker (e.g. the Home Assistant Mosquitto add-on) and the
  device shows up in HA automatically: a light (power + brightness) and a pattern
  selector for the device library. Applied live, no reboot. Saving stores exactly what's
  entered — including a blank password.
</p>
