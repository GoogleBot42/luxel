<script lang="ts">
  // Device — identity and the ONE control people open Settings for
  // (proposal §5.3, mockup S3: "Name · Brightness").
  //
  // Geometry used to live here (Pixels, LED protocol, Data pin); it is all in
  // LED layout now, behind one endpoint (Gitea #469).
  import {
    brightness,
    brightnessMax,
    device,
    deviceCaps,
    deviceName,
    deviceProtocol,
    setDeviceName,
  } from "../stores/device";
  import { note, notes } from "../stores/notify";

  /** Live brightness: the device applies it immediately and persists it. */
  function onBrightnessChange(e: Event): void {
    const v = Number((e.target as HTMLInputElement).value);
    brightness.set(v);
    void $device?.setBrightness(v);
  }

  /**
   * Rename on commit (blur / Enter), not per keystroke: every accepted POST
   * asks for a reboot, and a per-keystroke write would queue one bar entry
   * per letter. The store updates from the device's REPLY, so the header
   * chip and the title row follow the moment it lands (Gitea #538).
   */
  function onNameCommit(e: Event): void {
    const el = e.target as HTMLInputElement;
    const want = el.value.trim();
    if (want === $deviceName) return;
    void (async () => {
      const r = await setDeviceName(want);
      if (r.ok) note("devname", want ? "saved" : "cleared — back to the board's own name", 2500);
      else {
        note("devname", r.error ?? "rejected", 6000);
        el.value = $deviceName; // the device kept its old name; so does the field
      }
    })();
  }

  function onNameKey(e: KeyboardEvent): void {
    if (e.key === "Enter") (e.target as HTMLInputElement).blur();
  }

  /** Per driver, because the number means something different on each
   *  (§5.7: "brightness hint text | per driver"). */
  $: brightnessHint = $deviceCaps?.panel
    ? "The LED driver's current limit. Applied live and saved on the device — it dims the panel, not the previews on this page."
    : $deviceProtocol === "sk9822"
      ? "The SK9822's own 5-bit current limiter, so it dims without losing colour depth. Applied live and saved on the device."
      : "A software scale over every pixel before it reaches the strip. Applied live and saved on the device.";
</script>

<!-- Absent, never disabled (§5.7): firmware older than #538 publishes no name
     at all, and there is nothing to rename on it. -->
{#if $deviceName}
  <div class="field">
    <span class="flabel">Name</span>
    <div class="fctl" class:row={!!$notes.devname}>
      <!-- `value`, not `bind:` — the DEVICE owns the name, and the field shows
           what it last confirmed (a rejected POST puts the old one back) -->
      <input
        class="inp mono namefield"
        data-role="device-name"
        maxlength="32"
        value={$deviceName}
        on:change={onNameCommit}
        on:keydown={onNameKey}
      />
      {#if $notes.devname}
        <span class="dim hint" data-role="device-name-note">{$notes.devname}</span>
      {/if}
    </div>
  </div>
{/if}

<!-- mockup S3: the label hangs at the top of a row whose cell is a slider
     with its explanation under it -->
<div class="field top" data-role="device-brightness">
  <span class="flabel">Brightness</span>
  <div class="fctl">
    <div class="big">
      <input
        type="range"
        class="slider"
        data-role="brightness"
        min="0"
        max={$brightnessMax}
        step="1"
        value={$brightness}
        on:input={onBrightnessChange}
      />
      <span class="mono bval" data-role="brightness-val">{$brightness} / {$brightnessMax}</span>
    </div>
    <p class="dim hint under">{brightnessHint}</p>
  </div>
</div>

<style>
  /* the page's first control, and it looks like it — mockup S3's `.bigslider`
     is the slider and its readout on one 14px-gapped line */
  .big {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 20px;
  }

  .slider {
    flex: 1;
    min-width: 80px;
    height: 20px;
  }

  .bval {
    font: 13px/1 var(--mono);
    min-width: 52px;
    text-align: right;
  }

  /* mockup S3: `<input class="inp mono" style="width:260px">` */
  .namefield {
    width: 260px;
  }

  @media (max-width: 560px) {
    .namefield {
      max-width: 100%;
    }

    /* the mockup's 20px slider is a mouse target; a thumb is not */
    .big,
    .slider {
      height: 24px;
    }
  }
</style>
