<script lang="ts">
  // Advanced › Panel driver (`caps.panel`) — what the HUB75 refresh rate is
  // spent on, and what it actually measures out at.
  //
  // Since Gitea #401/#525 these are SETTINGS: `/api/layout` reports a
  // `driver` block and takes one `panel <planes> <clock_mhz> <chip> <blank>`
  // line back, which the firmware stores and applies at the next boot. So
  // every field here writes through the same `applyLayout` path the LED
  // layout form uses, adopts the reply as the new state, and a
  // `reboot_required` reply goes to the sticky reboot bar rather than to a
  // line of dim text (`noteRebootPending`, #538).
  //
  // Firmware older than that reports no `driver` block, and the card is what
  // it was: the build's constants, stated rather than offered, because a
  // control that cannot move is worse than a sentence (§5.7). The decision
  // between the two — and between live / reboot-to-apply / did-not-fit /
  // output-off — is `lib/panelDriver.ts`, unit tested.
  import { PANEL_DRIVER_DEFAULT } from "../lib/settingsCaps";
  import {
    BLANK_MAX,
    BLANK_MIN,
    chipLabel,
    CLOCK_MAX_MHZ,
    CLOCK_MIN_MHZ,
    CLOCK_WARN_MHZ,
    clampBlank,
    clampClock,
    configuredDriver,
    driverWire,
    panelDriverState,
    panelGeometryOf,
    panelLine,
    panelRefreshHz,
    phrase,
    PLANE_CHOICES,
    type PanelDriverConfig,
  } from "../lib/panelDriver";
  import {
    applyLayout,
    deviceLayoutWire,
    deviceOutFps,
    deviceRescanHz,
    noteRebootPending,
  } from "../stores/device";
  import { clearApiError, note, reportApiError } from "../stores/notify";

  $: wire = $deviceLayoutWire;
  $: driver = driverWire(wire);
  /** The stored driver — the device's when it reports one, this build's
   *  constants otherwise. Every field's value comes from here, so the form
   *  only ever shows what the device has confirmed. */
  $: cfg = configuredDriver(wire);
  /** The arrangement the framebuffer is sized from, for the live/stored
   *  comparison and for the estimate. Null when the Layout is not a matrix
   *  yet (nothing to compare, the driver values still are). */
  $: geom = panelGeometryOf(wire);
  $: state = panelDriverState(driver, geom);
  /** The estimate for the CONFIGURED values — computed here whenever the
   *  device reports its driver, so it tracks the fields above rather than the
   *  last reply's number (`lib/panelDriver.ts`). */
  $: estHz = geom
    ? panelRefreshHz(wire, { pw: geom.pw, ph: geom.ph, panels: geom.chain, scan: geom.scan })
    : 0;
  $: clockWarn = cfg.clock_mhz > CLOCK_WARN_MHZ;
  /** The chips this firmware can init, in its own order — never a list here. */
  $: chips = driver?.chips ?? [];
  $: fbKb = driver?.live ? (driver.live.fb_bytes / 1024).toFixed(1) : "";

  /** One POST per user action, the reply IS the new state — the LED layout
   *  form's rule (docs/api.md), and the same reboot-bar handling. */
  async function set(patch: Partial<PanelDriverConfig>): Promise<void> {
    const next = { ...cfg, ...patch };
    const r = await applyLayout(panelLine(next));
    if (!r.ok) {
      reportApiError(r.error, { scope: "layout", line: r.line });
      return;
    }
    clearApiError();
    note("layout", "saved", 2500);
    if (r.reboot_required) noteRebootPending("the panel driver");
  }
</script>

{#if driver}
  <!-- the editable form: the device carries the `panel` line -->
  <div class="field">
    <span class="flabel">Bit planes</span>
    <div class="fctl row g10">
      <select
        class="w96"
        data-role="panel-planes"
        value={String(cfg.planes)}
        on:change={(e) => void set({ planes: Number(e.currentTarget.value) })}
      >
        {#each PLANE_CHOICES as p}
          <option value={String(p)}>{p}</option>
        {/each}
      </select>
      <span class="dim hint">
        BCM bit depth — the refresh halves per extra plane and each one costs a framebuffer
      </span>
    </div>
  </div>

  <div class="field top">
    <span class="flabel">Pixel clock</span>
    <div class="fctl">
      <div class="row g10">
        <input
          class="inp num"
          data-role="panel-clock"
          type="number"
          min={CLOCK_MIN_MHZ}
          max={CLOCK_MAX_MHZ}
          value={cfg.clock_mhz}
          on:change={(e) => void set({ clock_mhz: clampClock(Number(e.currentTarget.value)) })}
        />
        <span class="dim hint">MHz — the LCD_CAM clock the panel is shifted at</span>
      </div>
      {#if clockWarn}
        <p class="warn hint under" data-role="panel-clock-warn">
          {cfg.clock_mhz} MHz is above the FM6124 datasheet limit — 40 MHz produced a split panel on
          the bench. Check the panel before leaving it here.
        </p>
      {:else}
        <p class="dim hint under">
          {CLOCK_WARN_MHZ} MHz is the FM6124 datasheet ceiling; the refresh scales with it.
        </p>
      {/if}
    </div>
  </div>

  <div class="field">
    <span class="flabel">Driver chip</span>
    <div class="fctl row g10">
      <select
        class="chipsel"
        data-role="panel-chip"
        value={cfg.chip}
        on:change={(e) => void set({ chip: e.currentTarget.value })}
      >
        {#each chips as c}
          <option value={c}>{chipLabel(c)}</option>
        {/each}
      </select>
      <span class="dim hint">a register-init chip stays dark until its sequence is sent</span>
    </div>
  </div>

  <div class="field">
    <span class="flabel">Latch blanking</span>
    <div class="fctl row g10">
      <input
        class="inp num"
        data-role="panel-blank"
        type="number"
        min={BLANK_MIN}
        max={BLANK_MAX}
        value={cfg.blank}
        on:change={(e) => void set({ blank: clampBlank(Number(e.currentTarget.value)) })}
      />
      <span class="dim hint">latch blanking clocks — raise it if you see ghosting</span>
    </div>
  </div>

  <div class="refresh" class:amber={state.status !== "live"} data-role="panel-state-row">
    <span class="dot"></span>
    <span>Rescan</span>
    <span class="hz mono" data-role="panel-rescan">
      {$deviceRescanHz > 0 ? `${$deviceRescanHz} Hz measured` : "not measured yet"}
    </span>
    <span class="dim mono" data-role="panel-est">· {estHz.toFixed(0)} Hz estimated</span>
  </div>

  <p class="hint" class:dim={state.status === "live"} class:warn={state.status !== "live"}
    data-role="panel-state" data-state={state.status}>
    {#if state.status === "disabled"}
      <strong>Panel output is off.</strong>
      This firmware could not bring up a framebuffer at all, so nothing is being shifted out
      whatever is stored here. Fewer bit planes, or a smaller panel / shorter chain, is what frees
      the internal RAM one framebuffer needs.
    {:else if state.status === "fallback"}
      <strong>Not applied — the configured driver did not fit.</strong>
      The board booted its own default instead: {state.live}. One framebuffer holds every bit plane
      and is {fbKb} KB of internal RAM here, so lower the bit planes above — or the panel size in
      LED layout — and reboot again.
    {:else if state.status === "pending"}
      <strong>Reboot to apply.</strong>
      Stored, but the panel is still running {state.live} — the framebuffer and the LCD_CAM clock are
      built at boot. Waiting on {phrase(state.changed)}.
    {:else}
      Running exactly what is set here: {state.live}. Its framebuffer is {fbKb} KB. Frames the panel
      actually displayed: {$deviceOutFps} fps.
    {/if}
  </p>
{:else}
  <!-- firmware before the `panel` line: the build's constants, stated -->
  <div class="field">
    <span class="flabel">Pixel clock</span>
    <div class="fctl row g10">
      <span class="mono" data-role="panel-clock">{PANEL_DRIVER_DEFAULT.clockHz / 1e6} MHz</span>
      <span class="dim hint">
        the FM6124 datasheet ceiling with no margin; 40 MHz mis-samples the panel's two halves
      </span>
    </div>
  </div>
  <div class="field">
    <span class="flabel">Bit planes</span>
    <div class="fctl row g10">
      <span class="mono" data-role="panel-planes">{PANEL_DRIVER_DEFAULT.planes}</span>
      <span class="dim hint">
        BCM bit depth — the refresh halves per extra plane and each one costs a framebuffer
      </span>
    </div>
  </div>
  <div class="field">
    <span class="flabel">Rescan</span>
    <div class="fctl row g10">
      <span class="mono" data-role="panel-rescan">
        {$deviceRescanHz > 0 ? `${$deviceRescanHz} Hz` : "—"}
      </span>
      <span class="dim hint">
        measured on the device: how often the panel redraws itself. Frames the panel actually
        displayed: {$deviceOutFps} fps.
      </span>
    </div>
  </div>
  <p class="dim hint" data-role="panel-state" data-state="unknown">
    Both values are this firmware build's constants, not settings — the LED layout section's
    estimated refresh is computed from them. A build that carries the panel settings (Gitea #525)
    offers them here instead.
  </p>
{/if}

<style>
  /* the LED layout card's refresh strip, to the same numbers — this row
     answers the same question (mockup S3 `.refresh`) */
  .refresh {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin-top: 8px;
  }

  .refresh .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    flex: none;
  }

  .refresh.amber .dot {
    background: var(--warn);
  }

  .refresh .hz {
    font-size: 15px;
  }

  .refresh.amber .hz {
    color: var(--warn);
  }

  .warn {
    color: #e5bd74;
  }

  /* The chip names are sentences, so this one select sizes to its widest
     option rather than sitting on the w-ladder — which means clearing the
     chevron (`background-position: right 9px`) by hand, or the longest label
     runs under it. */
  .chipsel {
    max-width: 100%;
    padding-right: 26px;
  }
</style>
