<script lang="ts">
  // Global output brightness — applied live and persisted by the device.
  import { brightness, brightnessMax, device } from "../stores/device";

  /** Live brightness: the device applies it immediately and persists it. */
  function onBrightnessChange(e: Event): void {
    const v = Number((e.target as HTMLInputElement).value);
    brightness.set(v);
    void $device?.setBrightness(v);
  }
</script>

<section class="card">
  <h2>Brightness</h2>
  <div class="field">
    <input
      type="range"
      class="grow"
      data-role="brightness"
      min="0"
      max={$brightnessMax}
      step="1"
      value={$brightness}
      on:input={onBrightnessChange}
    />
    <span class="mono dim" data-role="brightness-val">{$brightness}/{$brightnessMax}</span>
  </div>
  <p class="dim hint">
    Global output brightness (the LED driver's current limiter). Applied live and saved on
    the device. It dims the physical strip, not the preview above (which shows the pattern's
    colors at full range).
  </p>
</section>
