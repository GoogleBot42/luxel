<script lang="ts">
  // Advanced › Panel driver (`caps.panel`) — what the HUB75 refresh rate is
  // spent on, and what it actually measures out at.
  //
  // Clock and bit depth are COMPILE-TIME constants in the firmware today
  // (`firmware/src/hub75.rs`: the framebuffer type is const-generic and
  // DMA-static), so they are stated, not edited — and the page says so rather
  // than showing a control that cannot move. Gitea #525 puts them on the wire;
  // Gitea #475 makes the arrangement they feed real.
  import { PANEL_DRIVER_DEFAULT } from "../lib/settingsCaps";
  import { deviceOutFps, deviceRescanHz } from "../stores/device";
</script>

<div class="field">
  <span class="flabel">Pixel clock</span>
  <span class="mono" data-role="panel-clock">{PANEL_DRIVER_DEFAULT.clockHz / 1e6} MHz</span>
  <span class="dim hint">
    the FM6124 datasheet ceiling with no margin; 40 MHz mis-samples the panel's two halves
  </span>
</div>
<div class="field">
  <span class="flabel">Bit planes</span>
  <span class="mono" data-role="panel-planes">{PANEL_DRIVER_DEFAULT.planes}</span>
  <span class="dim hint">
    BCM bit depth — the refresh halves per extra plane and each one costs a framebuffer
  </span>
</div>
<div class="field">
  <span class="flabel">Rescan</span>
  <span class="mono" data-role="panel-rescan">
    {$deviceRescanHz > 0 ? `${$deviceRescanHz} Hz` : "—"}
  </span>
  <span class="dim hint">
    measured on the device: how often the panel redraws itself. Frames the panel actually
    displayed: {$deviceOutFps} fps.
  </span>
</div>
<p class="dim hint">
  Both values are this firmware build's constants, not settings — the LED layout section's
  estimated refresh is computed from them.
</p>
