<script lang="ts">
  // LED layout › Panel module — the collapsed disclosure's body
  // (`caps.panel`): everything printed on the back of a HUB75 module, plus
  // what the refresh rate is spent on.
  //
  // Since Gitea #401/#525 these are SETTINGS: `/api/layout` reports a
  // `driver` block and takes one `panel <planes> <clock_mhz> <chip> <blank>`
  // line back, which the firmware stores and applies — at the next boot for
  // three of the four, and on its NEXT FRAME for latch blanking (#778). So
  // every field here writes through the same `applyLayout` path the LED layout
  // form uses, adopts the reply as the new state, and lets the DEVICE's
  // `reboot_required` decide whether the sticky reboot bar is raised
  // (`noteRebootPending`, #538) — nothing here assumes.
  //
  // It lives in the LED layout card rather than in Advanced (Jeremy,
  // 2026-09-26): "It's not something people can optionally configure, but once
  // it is configured they probably won't touch it again, so being collapsed
  // still makes sense." The SCAN RATE is part of it for the same reason — it
  // is a property of the module, printed on its back next to everything else
  // here — even though it rides the `matrix` line, which is the LED layout
  // card's to write. Hence the `scan` prop and the `scan` event: one component
  // owns the matrix line, and it is not this one.
  //
  // Firmware older than #525 reports no `driver` block. The card used to draw
  // this build's constants as a read-only plaque there; it does NOT any more
  // (Gitea #771). The disclosure is only mounted on a panel board
  // (`caps.panel`), so a missing block means the console is newer than the
  // firmware — and on 2026-09-26 that state was reached by an unwanted OTA
  // ROLLBACK, where a plaque of plausible numbers made the settings look fixed
  // instead of gone. It states the mismatch and points at Firmware & recovery.
  //
  // The decision between live / reboot-to-apply / did-not-fit / output-off is
  // `lib/panelDriver.ts`, unit tested.
  import { createEventDispatcher } from "svelte";
  import {
    BLANK_MAX,
    BLANK_MIN,
    chipLabel,
    CLOCK_CEILING_MHZ,
    clampBlank,
    clockChoices,
    clockSupported,
    configuredDriver,
    driverWire,
    panelDriverState,
    panelGeometryOf,
    panelLine,
    phrase,
    PLANE_CHOICES,
    scanOptions,
    scanShown,
    scanWire,
    snapClock,
    type PanelDriverConfig,
  } from "../lib/panelDriver";
  import {
    applyLayout,
    deviceLayoutWire,
    deviceOutFps,
    noteRebootPending,
  } from "../stores/device";
  import { clearApiError, note, reportApiError } from "../stores/notify";

  /** The configured panel height — what the scan ratios are filtered by. */
  export let ph = 0;
  /** The stored scan divisor; `0` = the usual ratio for `ph`. */
  export let scan = 0;

  const dispatch = createEventDispatcher<{ scan: number }>();

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
  /** The chips this firmware can init, in its own order — never a list here. */
  $: chips = driver?.chips ?? [];
  /** Same for the pixel clock: a fixed dropdown over the device's own list
   *  (#771), plus whatever it currently holds so a value an older firmware
   *  accepted is visible rather than silently re-read. */
  $: clocks = clockChoices(driver);
  /** The scan ratios a `ph`-tall module can run at, and which of them is on.
   *  Never "board default": HUB75 is write-only, so the firmware cannot ask a
   *  module anything — `0` means the usual ratio, `ph / 2` (#778). */
  $: scans = scanOptions(ph, scan);
  $: scanNow = scanShown(ph, scan);
  $: fbKb = driver?.live ? (driver.live.fb_bytes / 1024).toFixed(1) : "";

  /** The device's refusal, in the card, beside the control it is about.
   *  The banner (`ErrorBar`) is still the prominent surface — Jeremy's rule
   *  for an `/api/layout` reply — but a rejected `panel` line is about ONE
   *  field, so its words belong here too (#771). Cleared by the next
   *  successful write. */
  let err = "";

  /** One POST per user action, the reply IS the new state — the LED layout
   *  form's rule (docs/api.md), and the same reboot-bar handling.
   *
   *  Whether a reboot is pending is the DEVICE's answer, never this form's:
   *  latch blanking now applies live (#778) and the other three do not, and
   *  `reboot_required` in the reply is the one place that distinction is
   *  authoritative. */
  async function set(patch: Partial<PanelDriverConfig>): Promise<void> {
    const next = { ...cfg, ...patch };
    const r = await applyLayout(panelLine(next));
    if (!r.ok) {
      // the banner's sentence AND the device's own words, here, next to the
      // field — never the generic "report this bug" copy, which is what a
      // rolled-back firmware's `unknown line` used to produce (#771)
      const ex = reportApiError(r.error, { scope: "layout", line: r.line });
      err = `${ex.text} The device said: ${ex.details}`;
      return;
    }
    err = "";
    clearApiError();
    note("layout", "saved", 2500);
    if (r.reboot_required) noteRebootPending("the panel module");
  }
</script>

{#if driver}
  <!-- the editable form: the device carries the `panel` line -->

  <!-- The scan rate is the module's, so it leads: it is the number printed on
       the back (`1/32S`), and getting it wrong is the most visible failure of
       the lot. It rides the `matrix` line, so the LED layout card writes it. -->
  <div class="field top">
    <span class="flabel">Scan rate</span>
    <div class="fctl">
      <select
        class="scansel"
        data-role="layout-scan"
        value={String(scanNow)}
        on:change={(e) => dispatch("scan", scanWire(ph, Number(e.currentTarget.value)))}
      >
        {#each scans as s}
          <option value={String(s.scan)}>{s.label}</option>
        {/each}
      </select>
      <p class="dim hint under">
        Printed on the module's back as 1/32S, 1/16S…; wrong = bands of the image in the wrong rows.
      </p>
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

  <div class="field top">
    <span class="flabel">Pixel clock</span>
    <div class="fctl">
      <div class="row g10">
        <select
          class="clocksel"
          data-role="panel-clock"
          value={String(cfg.clock_mhz)}
          on:change={(e) =>
            void set({ clock_mhz: snapClock(Number(e.currentTarget.value), driver) })}
        >
          {#each clocks as c}
            <option value={String(c)}
              >{c} MHz{clockSupported(driver, c) ? "" : " (not supported)"}</option
            >
          {/each}
        </select>
        <span class="dim hint">the LCD_CAM clock the panel is shifted at</span>
      </div>
      <p class="dim hint under">
        {CLOCK_CEILING_MHZ} MHz is the FM6124 datasheet ceiling — lower is safer on other driver
        chips, and the refresh scales with it.
      </p>
    </div>
  </div>

  <div class="field top">
    <span class="flabel">Bit planes</span>
    <div class="fctl">
      <div class="row g10">
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
        <span class="dim hint">BCM bit depth — colour steps per channel</span>
      </div>
      <p class="dim hint under">
        The refresh halves per extra plane, and every plane is in each framebuffer — so fewer is
        both faster and less internal RAM.
      </p>
    </div>
  </div>

  <div class="field top">
    <span class="flabel">Latch blanking</span>
    <div class="fctl">
      <div class="row g10">
        <input
          class="inp num"
          data-role="panel-blank"
          type="number"
          min={BLANK_MIN}
          max={BLANK_MAX}
          value={cfg.blank}
          on:change={(e) => void set({ blank: clampBlank(Number(e.currentTarget.value)) })}
        />
        <span class="dim hint">clocks with the LEDs off around the latch</span>
      </div>
      <p class="dim hint under">
        Applies straight away, no reboot — raise it while watching the panel until the ghosting
        between rows goes.
      </p>
    </div>
  </div>

  <!-- No rescan readout here: the LED layout card's `Estimated refresh … Hz
       measured now` row sits four lines below this disclosure and is the SAME
       two numbers over the same inputs (`panelRefreshHz` either way). Printing
       them twice on one screen was the first thing the move made obvious
       (Gitea #778). What stays is the verdict, which nothing else says. -->
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

  {#if err}
    <p class="warn hint" data-role="panel-error">{err}</p>
  {/if}
{:else}
  <!-- The disclosure is mounted on `caps.panel` boards only, so no `driver`
       block means the firmware is older than this console — say that, and offer
       nothing. A read-only plaque of plausible numbers is how a silent OTA
       rollback read as "the settings are fine" (Gitea #771). -->
  <p class="warn hint" data-role="panel-state" data-state="unknown">
    <strong>This console is newer than the firmware on the device</strong> (it reports no panel
    driver). Push matching firmware — Settings › Firmware &amp; recovery.
  </p>
{/if}

<style>
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

  /* the clock list is `30 MHz`-short unless the device holds a value it no
     longer offers (`40 MHz (not supported)`), so it sizes to its widest option
     off a w96 floor rather than sitting on the w-ladder — same chevron
     clearance as `.chipsel` */
  .clocksel {
    min-width: 96px;
    max-width: 100%;
    padding-right: 26px;
  }

  /* `1/32 (usual for 64 rows)` is the widest option and the only one that long,
     so this sizes to it off a w170 floor — same chevron clearance again. A
     bare `appearance:none` select still sizes to its WIDEST option, never to
     the current value (.claude/rules/web.md). */
  .scansel {
    min-width: 170px;
    max-width: 100%;
    padding-right: 26px;
  }
</style>
