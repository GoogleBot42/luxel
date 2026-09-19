<script lang="ts">
  // Wall clock / timezone — what clockHour() patterns read.
  import { clockStatus, device, refreshClock } from "../stores/device";

  function onTzChange(e: Event): void {
    const v = Number((e.target as HTMLInputElement).value);
    if (!Number.isFinite(v)) return;
    void (async () => {
      await $device?.setClock(v * 60); // UI is hours, API is minutes
      void refreshClock();
    })();
  }

  const fmtDeviceTime = (unixLocal: number): string => {
    // the value is already local — render it without the browser's tz
    const d = new Date(unixLocal * 1000);
    return d.toLocaleString("en-US", { timeZone: "UTC", hour12: false });
  };
</script>

<div class="field">
  <span class="flabel">Device time</span>
  <span class="mono" data-role="clock-status">
    {$clockStatus?.synced ? fmtDeviceTime($clockStatus.local) : "not NTP-synced yet"}
  </span>
</div>
<div class="field">
  <span class="flabel">UTC offset</span>
  <input
    class="num"
    data-role="clock-tz"
    type="number"
    step="0.5"
    min="-14"
    max="14"
    value={($clockStatus?.tzMinutes ?? 0) / 60}
    on:change={onTzChange}
  />
  <span class="dim">hours (e.g. -6 for Mountain DST) — drives clockHour() patterns</span>
</div>
