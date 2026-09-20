<script lang="ts">
  // DDP / E1.31 liveness. Read-only: the device just reports whether a stream
  // is currently driving the strip.
  import { netLive } from "../stores/device";
</script>

<div class="field">
  <span class="flabel">Status</span>
  <div class="fctl">
    <span class="mono" data-role="netin-status">
      {$netLive === "ddp" ? "receiving DDP" : $netLive === "e131" ? "receiving E1.31" : "idle"}
    </span>
  </div>
</div>
<p class="dim hint">
  The device listens for DDP on UDP :4048 and E1.31/sACN on UDP :5568 (universe 1 and
  up, 170 pixels each; multicast or unicast). Frames from xLights, LedFx, Resolume, etc.
  drive the strip directly; the running pattern resumes a couple of seconds after the
  stream stops.
</p>
