<script lang="ts">
  // Luxel-to-Luxel sync: one device broadcasts the timebase, the rest follow.
  import { device, refreshSync, syncStatus } from "../stores/device";

  function onSyncModeChange(e: Event): void {
    const mode = (e.target as HTMLSelectElement).value as "off" | "leader" | "follower";
    void (async () => {
      await $device?.setSync(mode);
      void refreshSync();
    })();
  }
</script>

<div class="field">
  <span class="flabel">Role</span>
  <select data-role="sync-mode" value={$syncStatus?.mode ?? "off"} on:change={onSyncModeChange}>
    <option value="off">off</option>
    <option value="leader">leader</option>
    <option value="follower">follower</option>
  </select>
  <span class="mono dim" data-role="sync-status">
    {$syncStatus?.mode === "follower"
      ? $syncStatus.leader
        ? `following (offset ${$syncStatus.leader.offsetMs}ms)`
        : "waiting for a leader…"
      : $syncStatus?.mode === "leader"
        ? "broadcasting the timebase"
        : ""}
  </span>
</div>
<p class="dim hint">
  Run the same pattern on several Luxels and they stay phase-locked: one device leads
  (broadcasting its clock on UDP :4049) and the rest follow. The leader also relays its
  sensor data, so one microphone can drive every strip.
</p>
