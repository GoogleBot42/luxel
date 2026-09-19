<script lang="ts">
  // Device — identity and the ONE control people open Settings for
  // (proposal §5.3: "Name · Brightness (large, first control on the page)").
  //
  // Geometry used to live here (Pixels, LED protocol, Data pin); it is all in
  // LED layout now, behind one endpoint (Gitea #469).
  import {
    brightness,
    brightnessMax,
    device,
    deviceBase,
    deviceCaps,
    deviceProtocol,
    deviceVersion,
  } from "../stores/device";

  /** Live brightness: the device applies it immediately and persists it. */
  function onBrightnessChange(e: Event): void {
    const v = Number((e.target as HTMLInputElement).value);
    brightness.set(v);
    void $device?.setBrightness(v);
  }

  /** The device's own name is fixed at flash time (`luxel-<mac suffix>`) and
   *  no endpoint renames it — so this row states it rather than pretending to
   *  be a field. It becomes an input the day the firmware grows the route. */
  $: address = $device?.base || $deviceBase || "";
  $: name = address ? address.replace(/^https?:\/\//, "") : "served from this device";

  /** Per driver, because the number means something different on each
   *  (§5.7: "brightness hint text | per driver"). */
  $: brightnessHint = $deviceCaps?.panel
    ? "The panel's global plane scale. Applied live and saved on the device — it dims the panel, not the previews on this page."
    : $deviceProtocol === "sk9822"
      ? "The SK9822's own 5-bit current limiter, so it dims without losing colour depth. Applied live and saved on the device."
      : "A software scale over every pixel before it reaches the strip. Applied live and saved on the device.";
</script>

<div class="field" data-role="device-brightness">
  <span class="flabel">Brightness</span>
  <input
    type="range"
    class="grow big"
    data-role="brightness"
    min="0"
    max={$brightnessMax}
    step="1"
    value={$brightness}
    on:input={onBrightnessChange}
  />
  <span class="mono" data-role="brightness-val">{$brightness} / {$brightnessMax}</span>
</div>
<p class="dim hint">{brightnessHint}</p>

<div class="field">
  <span class="flabel">Name</span>
  <span class="mono" data-role="device-name">{name}</span>
  <span class="dim hint">
    set when the firmware is flashed — this build has no rename endpoint.
    {#if $deviceVersion}<span class="mono">v{$deviceVersion}</span>{/if}
  </span>
</div>

<style>
  /* the page's first control, and it looks like it (mockup S3) */
  .big {
    height: 24px;
  }

  .field :global(.mono) {
    font-size: 13px;
  }
</style>
