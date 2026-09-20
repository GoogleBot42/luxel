<script lang="ts">
  // LED layout — the ONE home for geometry (proposal §5.3/§5.3b/§5.4d,
  // mockups S3, S3b–S3d, S3j/S3k; Gitea #469).
  //
  // Before this section, geometry lived in the pattern editor's playback bar
  // (shape select, pixel field, install-grid, install-map) and in three
  // Settings cards (Pixels, LED protocol, Data pin). It is all here now, and
  // it all speaks ONE endpoint:
  //
  //   POST /api/layout — one POST per user action, and the REPLY is the new
  //   state (docs/api.md). Nothing re-GETs, so the page never shows a value
  //   the device has not confirmed.
  //
  // Three fields deliberately keep their live endpoints on a SINGLE-output
  // board: LED type (`/api/protocol`), colour order (`/api/output`) and the
  // data pin (`/api/datapin`). An `out` line is built once at BOOT (#474/#475),
  // so routing a colour-order change through it would answer
  // `reboot_required` for something that is live today. Whether the firmware
  // should apply those two fields live instead — and the split then go away —
  // is Gitea #524; see docs/api.md "Live vs reboot".
  //
  // Every field is gated by `lib/settingsCaps.ts` — absent, never disabled.
  import { createEventDispatcher } from "svelte";
  import PatternThumb from "../components/PatternThumb.svelte";
  import type { LayoutWire } from "../lib/device";
  import {
    estimatedRefreshHz,
    REFRESH_AMBER_HZ,
    settingsVisibility,
    squarish,
    uiLayoutKind,
    PANEL_DRIVER_DEFAULT,
    type Corner,
    type LayoutKind,
    type RunDir,
  } from "../lib/settingsCaps";
  import {
    cloudLayout,
    latticeCoords,
    latticeDimsOf,
    latticeMapFits,
    maxLatticeSide,
  } from "../lib/geometry";
  import {
    applyLayout,
    dataPin,
    dataPinChoice,
    dataPinDefault,
    dataPinNext,
    dataPins,
    device,
    deviceCaps,
    deviceLayoutWire,
    deviceMapCoords,
    deviceProtocol,
    devicePixels,
    deviceRescanHz,
    installLattice,
    noteRebootPending,
    outputStatus,
    pixelMax,
    protocolOptions,
    refreshOutput,
  } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { layout as geomLayout, layoutLabel } from "../stores/geometry";
  import { note, notes } from "../stores/notify";
  import { luxel } from "../stores/pattern";
  import ArrangementSvg from "./ArrangementSvg.svelte";
  import OutputsTable from "./OutputsTable.svelte";

  const dispatch = createEventDispatcher<{ pixelchange: void; openmap: void }>();

  /** A sample 1D pattern: the summary thumbnail is a live picture of the
   *  FIXTURE, so it takes the Layout's shape straight from the geometry
   *  store (bar / grid / cloud / scatter) with no shape logic of its own. */
  const SAMPLE = "export function render(index) { hsv(index / pixelCount, 1, 1) }";

  const SCANS = [0, 4, 8, 16, 32];
  const CORNERS: { v: Corner; label: string }[] = [
    { v: "tl", label: "top-left" },
    { v: "tr", label: "top-right" },
    { v: "bl", label: "bottom-left" },
    { v: "br", label: "bottom-right" },
  ];
  const DIRS: { v: RunDir; label: string }[] = [
    { v: "row", label: "horizontal" },
    { v: "col", label: "vertical" },
  ];
  const KIND_LABEL: Record<LayoutKind, string> = {
    strip: "Strip",
    matrix: "Matrix",
    lattice: "3D",
    map: "Custom map",
  };

  /** The biggest side a lattice can have and still install in one POST. */
  const LAT_MAX = maxLatticeSide();

  /** The device's Layout, or a strip standing in until `/api/layout` answers.
   *  `kind` is the PICKER's vocabulary: a 3D lattice rides the wire's `map`
   *  kind, and `uiLayoutKind` is the one place that tells them apart. */
  $: wire = $deviceLayoutWire;
  $: kind = kindChoice ?? uiLayoutKind(wire?.kind ?? "strip", $geomLayout.dims);
  // `$:` only tracks what its own syntax names, so the pixel count is an
  // ARGUMENT — `matrixOf` sizes a not-yet-configured matrix from it
  // (.claude/rules/web.md).
  $: m = matrixOf(wire, $devicePixels);
  $: panels = Math.max(1, m.cols * m.rows);
  $: vis = settingsVisibility($deviceCaps, {
    kind,
    dims: $geomLayout.dims,
    regular: $geomLayout.regular,
    panels,
  });
  $: outputs = wire?.outputs ?? [];
  $: outUnit = kind === "matrix" ? ("panels" as const) : ("pixels" as const);
  $: outTotal = kind === "matrix" ? panels : ($devicePixels || (wire?.pixels ?? 0));
  /** The arrangement picture: a chain when there are tiles to thread, the
   *  pixel run inside one matrix otherwise — and nothing at all for a single
   *  HUB75 panel, whose pixel order is the panel's own scan, not a wiring
   *  choice anybody makes (mockup S3 has no picture). */
  $: arrangementMode = panels > 1 ? ("chain" as const) : ("pixels" as const);
  $: showArrangement = vis.arrangement && (panels > 1 || !($deviceCaps?.panel ?? false));
  /** The estimated rescan rate. The DEVICE's `est_hz` wins when it reports
   *  one (#475) — it knows its own clock and bit depth; the browser model is
   *  the same formula over the same inputs and stands in for a host that does
   *  not (docs/api.md "Panel arrangement"). */
  $: refreshHz = m.est_hz ?? estimatedRefreshHz({ pw: m.pw, ph: m.ph, panels, scan: m.scan });
  $: refreshLow = refreshHz > 0 && refreshHz < REFRESH_AMBER_HZ;
  /** Tiles past what the board's framebuffer can shift out are DARK (#475). */
  $: driven = m.drive ?? 0;
  $: darkTiles = driven > 0 ? Math.max(0, panels - driven) : 0;
  /** The picker while the user is mid-change. A 3D lattice is not a field
   *  edit but an INSTALL — picking `3D` reveals `w × h × d` and a button,
   *  and the device stays what it is until that button is pressed. */
  let kindChoice: LayoutKind | null = null;
  /** The lattice in the fields: the installed one when there is one, else a
   *  cube that fits (`LAT_MAX`³ is the biggest single POST — see
   *  `lib/geometry.ts` `LAYOUT_BODY_BUDGET`). */
  let lat = { w: 0, h: 0, d: 0 };
  $: installedLat = $geomLayout.dims === 3 && $geomLayout.regular
    ? { w: $geomLayout.w, h: $geomLayout.h, d: $geomLayout.d }
    : ($deviceMapCoords ? latticeDimsOf($deviceMapCoords) : null);
  $: if (lat.w === 0) lat = installedLat ?? { w: LAT_MAX, h: LAT_MAX, d: LAT_MAX };
  $: latPixels = Math.max(1, lat.w) * Math.max(1, lat.h) * Math.max(1, lat.d);
  /** The rig the thumbnail draws: what the fields describe, not what the
   *  device is, so the picture answers "what am I about to install?". */
  $: latPreview = cloudLayout(
    latticeCoords(Math.max(1, lat.w), Math.max(1, lat.h), Math.max(1, lat.d)),
    $geomLayout.projection,
  );
  $: latOverBody = !latticeMapFits(Math.max(1, lat.w), Math.max(1, lat.h), Math.max(1, lat.d));
  $: latOverMax = latPixels > $pixelMax;
  $: latSame =
    installedLat !== null &&
    installedLat.w === lat.w &&
    installedLat.h === lat.h &&
    installedLat.d === lat.d;

  function matrixOf(w: LayoutWire | null, px: number): {
    pw: number;
    ph: number;
    cols: number;
    rows: number;
    start: Corner;
    dir: RunDir;
    snake: number;
    rot180: number;
    scan: number;
    est_hz?: number;
    drive?: number;
  } {
    const square = squarish(px || w?.pixels || 1);
    return (
      w?.matrix ?? {
        pw: w?.kind === "matrix" ? w.w : square.w,
        ph: w?.kind === "matrix" ? w.h : square.h,
        cols: 1,
        rows: 1,
        start: "tl",
        dir: "row",
        snake: 0,
        rot180: 0,
        scan: 0,
      }
    );
  }

  /** The one write path: POST, then let the reply be the state.
   *
   *  A `reboot_required` reply is the DEVICE saying it stored the change but
   *  is still running the old one — that goes to the sticky reboot bar
   *  (`noteRebootPending`), which is on screen on every tab until the reboot,
   *  not into a line of dim text at the bottom of this form (#538). */
  async function post(lines: string, field = "the wiring"): Promise<boolean> {
    const r = await applyLayout(lines);
    if (!r.ok) {
      note("layout", r.line ? `${r.error} (line ${r.line})` : r.error, 6000);
      return false;
    }
    note("layout", "saved", 2500);
    if (r.reboot_required) noteRebootPending(field);
    return true;
  }

  function matrixLine(patch: Partial<ReturnType<typeof matrixOf>>): string {
    const n = { ...m, ...patch };
    return `matrix ${n.pw} ${n.ph} ${n.cols} ${n.rows} ${n.start} ${n.dir} ${n.snake} ${n.rot180} ${n.scan}`;
  }

  function setMatrix(patch: Partial<ReturnType<typeof matrixOf>>): void {
    void (async () => {
      if (await post(matrixLine(patch), "the panel arrangement")) dispatch("pixelchange");
    })();
  }

  function setPixels(e: Event): void {
    const n = Math.max(1, Math.min($pixelMax, Number((e.target as HTMLInputElement).value) || 1));
    void (async () => {
      if (await post(`strip ${n}`, "the pixel count")) dispatch("pixelchange");
    })();
  }

  function setKind(e: Event): void {
    const want = (e.target as HTMLSelectElement).value as LayoutKind;
    if (want === kind) return;
    if (want === "map") {
      // A custom map is a PROGRAM, not a form: the map editor installs it,
      // and the Layout becomes `map` when it does. Nothing to POST here.
      (e.target as HTMLSelectElement).value = kind;
      dispatch("openmap");
      return;
    }
    if (want === "lattice") {
      // Not a field edit: reveal `w × h × d` and wait for Install.
      kindChoice = "lattice";
      return;
    }
    kindChoice = null;
    void (async () => {
      const px = $devicePixels || wire?.pixels || 1;
      // Matrix keeps the pixel count: 120 px is 12×10, not 11×11 rounded up,
      // so Strip → Matrix → Strip round-trips instead of growing the fixture.
      const square = squarish(px);
      const line =
        want === "strip"
          ? `strip ${px}`
          : matrixLine({ pw: m.pw > 1 ? m.pw : square.w, ph: m.ph > 1 ? m.ph : square.h });
      if (await post(line, "the layout")) dispatch("pixelchange");
    })();
  }

  function setOutputs(list: LayoutWire["outputs"]): void {
    const lines = list.length
      ? list
          .map((o, i) => `out ${i} ${o.pin} ${o.proto} ${o.order} ${Math.max(0, Math.round(o.count))}${o.rev ? " rev" : ""}`)
          .join("\n")
      : "out none";
    void post(lines, "the output table");
  }

  /** The two enum selects. Svelte's template parser does not take a TS `as`
   *  inside an event expression, so the narrowing lives here. */
  function setStart(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    setMatrix({ start: (CORNERS.find((c) => c.v === v)?.v ?? "tl") satisfies Corner });
  }

  function setDir(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    setMatrix({ dir: (DIRS.find((d) => d.v === v)?.v ?? "row") satisfies RunDir });
  }

  /** LED type: live on `/api/protocol` while there is one output (see the
   *  header comment). The Layout's stored table follows it on the device. */
  function setProtocol(e: Event): void {
    const name = (e.target as HTMLSelectElement).value;
    deviceProtocol.set(name);
    void (async () => {
      const r = await $device?.setProtocol(name);
      if (r?.ok && r.protocol) deviceProtocol.set(r.protocol);
      else note("layout", r?.error ? `LED type: ${r.error}` : "LED type: rejected", 6000);
    })();
  }

  /** Colour order: live on `/api/output`, which owns the whole output chain. */
  function setOrder(e: Event): void {
    const order = (e.target as HTMLSelectElement).value;
    const o = $outputStatus;
    if (!o) return;
    outputStatus.set({ ...o, order });
    void (async () => {
      await $device?.setOutput(order, o.gamma, o.capMa, o.brightCurve, o.blur, o.glow);
      void refreshOutput();
    })();
  }

  /**
   * Install the lattice in the fields as the device's geometry (#538).
   *
   * Two POSTs — `strip <pixels>` to size the pixel space, then `map 3 …` to
   * fill it — because a COORDINATE map does not resize anything and the
   * grammar takes one shape line per body (`stores/device.ts`
   * `installLattice`). The whole lattice has to fit ONE request, which is
   * what caps the side at `LAT_MAX`; Gitea #548 is the procedural form that
   * would lift it.
   */
  function installLatticeNow(): void {
    const w = Math.max(1, lat.w);
    const h = Math.max(1, lat.h);
    const d = Math.max(1, lat.d);
    void (async () => {
      note("layout", "installing…");
      const r = await installLattice(w, h, d);
      if (!r.ok) {
        note("layout", r.error ?? "rejected", 6000);
        return;
      }
      kindChoice = null;
      note("layout", `saved — ${w}×${h}×${d} lattice`, 2500);
      dispatch("pixelchange");
    })();
  }

  /** The strip DATA pin: NOT live — the device reboots to rebind its driver,
   *  so the pick sits in the form until a deliberate second step, behind the
   *  reboot confirmation (§5.3: nothing reboots from a bare button). */
  function onDataPinPick(e: Event): void {
    const v = Number((e.target as HTMLSelectElement).value);
    dataPinChoice.set(v === $dataPin && $dataPinNext === null ? null : v);
    note("datapin", "");
  }

  async function applyDataPin(): Promise<void> {
    const pin = $dataPinChoice ?? $dataPinNext;
    if (pin === null) return;
    const ok = await confirm({
      title: `Move the strip data line to GPIO${pin}?`,
      body:
        `The driver rebinds to GPIO${pin}${pin === $dataPinDefault ? " (the board default)" : ""}. ` +
        "The strip stays dark until it is wired to that pin.",
      confirmLabel: "Apply & reboot",
      reboot: true,
    });
    if (!ok) return;
    note("datapin", "saving…");
    const r = await $device?.setDataPin(pin === $dataPinDefault ? "default" : pin);
    if (r?.ok) {
      note("datapin", `saved — the device is rebooting with data on GPIO${r.data_pin ?? pin}`);
      dataPinNext.set(pin);
      dataPinChoice.set(null);
      noteRebootPending("the data pin");
    } else {
      note("datapin", r?.error ? `failed: ${r.error}` : "save failed");
    }
  }

  /**
   * The line under the big summary: what this fixture IS, in one breath.
   *
   * Every input is an ARGUMENT, because this is called from the markup: a
   * markup expression is re-evaluated with the dirty bits of the things it
   * NAMES, so `{summaryHint()}` names only the function and would never run
   * again (.claude/rules/web.md — the mirror image of the `$:` trap).
   */
  function summaryHint(px: number, tiles: number, isPanel: boolean, proto: string, pin: number): string {
    const parts: string[] = [];
    if (kind === "matrix" && tiles > 1) parts.push(`${tiles} panels`);
    parts.push(`${px} pixel${px === 1 ? "" : "s"}`);
    if (isPanel) parts.push("HUB75");
    else if (proto) parts.push(proto.toUpperCase());
    if (!isPanel && $dataPins.length) parts.push(`GPIO ${pin}`);
    return parts.join(" · ");
  }
</script>

<div class="summary" data-role="layout-summary">
  <div>
    <div class="big" data-role="layout-headline">{layoutLabel($geomLayout)}</div>
    <div class="dim hint" data-role="layout-subhead">
      {summaryHint(
        $devicePixels || wire?.pixels || 0,
        panels,
        $deviceCaps?.panel ?? false,
        $deviceProtocol,
        $dataPin,
      )}
    </div>
  </div>
  {#if $luxel}<PatternThumb luxel={$luxel} source={SAMPLE} size="summary" />{/if}
</div>

{#if vis.kindPicker}
  <div class="field">
    <span class="flabel">Layout</span>
    <select data-role="layout-kind" value={kind} on:change={setKind}>
      {#each vis.kindOptions as k}
        <option value={k}>{KIND_LABEL[k]}</option>
      {/each}
    </select>
    <span class="dim hint">{vis.kindOptions.map((k) => KIND_LABEL[k]).join(" · ")}</span>
  </div>
{/if}

{#if vis.latticeFields}
  <!-- A 3D lattice: `w × h × d` cells wired as one run, installed as the
       device's coordinate map (#538 — "I cannot try 3D at all"). -->
  <div class="field">
    <span class="flabel">Lattice</span>
    <input
      class="num"
      data-role="layout-lat-w"
      type="number"
      min="2"
      max={LAT_MAX}
      value={lat.w}
      on:change={(e) => (lat = { ...lat, w: Number(e.currentTarget.value) || 1 })}
    />
    <span class="dim">×</span>
    <input
      class="num"
      data-role="layout-lat-h"
      type="number"
      min="2"
      max={LAT_MAX}
      value={lat.h}
      on:change={(e) => (lat = { ...lat, h: Number(e.currentTarget.value) || 1 })}
    />
    <span class="dim">×</span>
    <input
      class="num"
      data-role="layout-lat-d"
      type="number"
      min="2"
      max={LAT_MAX}
      value={lat.d}
      on:change={(e) => (lat = { ...lat, d: Number(e.currentTarget.value) || 1 })}
    />
    <span class="dim hint" data-role="layout-lat-count">{latPixels} pixels</span>
    {#if $luxel}
      <span class="latthumb">
        <PatternThumb luxel={$luxel} source={SAMPLE} previewRig={latPreview} size="summary" />
      </span>
    {/if}
    <!-- absent, never disabled (§5.7) — except a lattice the device cannot
         take, which is a BUDGET the user has to learn, so it says why -->
    {#if latOverBody || latOverMax}
      <button
        class="btn sm"
        data-role="layout-lat-install"
        disabled
        data-reason={latOverMax
          ? `over this board's ${$pixelMax}-pixel ceiling`
          : `more coordinates than one request can carry — up to ${LAT_MAX}×${LAT_MAX}×${LAT_MAX}`}
      >
        Install
      </button>
    {:else if !latSame}
      <button class="btn sm primary" data-role="layout-lat-install" on:click={installLatticeNow}>
        Install
      </button>
    {/if}
  </div>
  <p class="dim hint under" data-role="layout-lat-note">
    {#if latOverMax}
      {latPixels} pixels is over this board's {$pixelMax}-pixel ceiling.
    {:else if latOverBody}
      A lattice goes to the device as one coordinate per pixel in a single request, so it stops at
      {LAT_MAX}×{LAT_MAX}×{LAT_MAX} ({LAT_MAX ** 3} pixels) — Gitea #548 is the procedural form
      that would lift it.
    {:else}
      One run of {latPixels} pixels, wired x first, then y, then z. Installed as the device's
      coordinate map, so 3D patterns render natively and 1D/2D ones get the projections below.
    {/if}
  </p>
{/if}

{#if kind === "strip"}
  <div class="field">
    <span class="flabel">Pixels</span>
    <input
      class="num"
      data-role="layout-pixels"
      type="number"
      min="1"
      max={$pixelMax}
      value={$devicePixels}
      on:change={setPixels}
    />
    <span class="dim hint">resized live — max {$pixelMax}, no reboot</span>
  </div>
{/if}

{#if kind === "matrix"}
  <div class="field">
    <span class="flabel">{vis.panelScan ? "Panel" : "Size"}</span>
    <input
      class="num"
      data-role="layout-pw"
      type="number"
      min="1"
      max="256"
      value={m.pw}
      on:change={(e) => setMatrix({ pw: Number(e.currentTarget.value) || 1 })}
    />
    <span class="dim">×</span>
    <input
      class="num"
      data-role="layout-ph"
      type="number"
      min="1"
      max="256"
      value={m.ph}
      on:change={(e) => setMatrix({ ph: Number(e.currentTarget.value) || 1 })}
    />
    <span class="dim">px</span>
    {#if vis.panelScan}
      <span class="dim">scan</span>
      <select
        data-role="layout-scan"
        value={String(m.scan)}
        on:change={(e) => setMatrix({ scan: Number(e.currentTarget.value) })}
      >
        {#each SCANS as s}
          <option value={String(s)}>{s === 0 ? "board default" : `1/${s}`}</option>
        {/each}
      </select>
    {/if}
  </div>

  {#if vis.panelCounts}
    <div class="field">
      <span class="flabel">Panels</span>
      <input
        class="num"
        data-role="layout-cols"
        type="number"
        min="1"
        max="16"
        value={m.cols}
        on:change={(e) => setMatrix({ cols: Number(e.currentTarget.value) || 1 })}
      />
      <span class="dim">across ×</span>
      <input
        class="num"
        data-role="layout-rows"
        type="number"
        min="1"
        max="16"
        value={m.rows}
        on:change={(e) => setMatrix({ rows: Number(e.currentTarget.value) || 1 })}
      />
      <span class="dim">down</span>
    </div>
  {/if}

  {#if vis.wiringRow}
    <div class="field">
      <span class="flabel">{vis.wiringIsPixels ? "Pixel wiring" : "Chain"}</span>
      <span class="dim">starts</span>
      <select
        data-role="layout-start"
        value={m.start}
        on:change={setStart}
      >
        {#each CORNERS as c}<option value={c.v}>{c.label}</option>{/each}
      </select>
      <span class="dim">runs</span>
      <select
        data-role="layout-dir"
        value={m.dir}
        on:change={setDir}
      >
        {#each DIRS as d}<option value={d.v}>{d.label}</option>{/each}
      </select>
      <label class="ckrow">
        <input
          type="checkbox"
          data-role="layout-snake"
          checked={m.snake === 1}
          on:change={(e) => setMatrix({ snake: e.currentTarget.checked ? 1 : 0 })}
        />
        serpentine (snake)
      </label>
      {#if vis.rot180}
        <label class="ckrow">
          <input
            type="checkbox"
            data-role="layout-rot180"
            checked={m.rot180 === 1}
            on:change={(e) => setMatrix({ rot180: e.currentTarget.checked ? 1 : 0 })}
          />
          rotate alternate rows 180°
        </label>
      {/if}
    </div>
  {/if}

  {#if showArrangement}
    <ArrangementSvg
      mode={arrangementMode}
      pw={m.pw}
      ph={m.ph}
      cols={m.cols}
      rows={m.rows}
      start={m.start}
      dir={m.dir}
      snake={m.snake === 1}
      rot180={m.rot180 === 1}
      outputCounts={vis.outputsTable ? outputs.map((o) => o.count) : []}
      drive={driven}
    />
  {/if}

  {#if vis.estimatedRefresh}
    <div class="refresh" class:amber={refreshLow} data-role="refresh">
      <span class="dot"></span>
      <span>Estimated refresh</span>
      <span class="hz mono" data-role="refresh-hz">{refreshHz.toFixed(0)} Hz</span>
      {#if $deviceRescanHz > 0}
        <span class="dim mono" data-role="refresh-measured">
          · {$deviceRescanHz} Hz measured now
        </span>
      {/if}
    </div>
    <p class="dim hint">
      {panels} panel{panels === 1 ? "" : "s"} × {PANEL_DRIVER_DEFAULT.planes} planes at
      {PANEL_DRIVER_DEFAULT.clockHz / 1e6} MHz.
      {#if refreshLow}
        Below {REFRESH_AMBER_HZ} Hz cameras and fast motion show flicker — use fewer panels per
        chain, or fewer bitplanes (Advanced › Panel driver).
      {/if}
    </p>
    {#if darkTiles > 0}
      <p class="dim hint" data-role="layout-dark">
        <strong>
          This board drives the first {driven} panel{driven === 1 ? "" : "s"} of the chain — the
          other {darkTiles} stay dark.
        </strong>
        Its framebuffer is sized at build time, so a longer chain is stored and reported but not
        shifted out.
      </p>
    {/if}
  {/if}

  <div class="notes dim hint" data-role="layout-notes">
    {#if $deviceCaps?.panel}
      <div>One chain per output on this board — its two headers are the same GPIOs wired twice.</div>
      <div>Every panel in a chain must have the same size and scan.</div>
    {/if}
    <div>
      Panel size applies live; the chain — panels across/down, start, direction, snake, rotation,
      scan — is built once at boot.
    </div>
  </div>
{/if}

{#if vis.stripFields && !vis.outputsTable}
  <div class="field">
    <span class="flabel">LED type</span>
    <select data-role="layout-proto" value={$deviceProtocol} on:change={setProtocol}>
      {#each $protocolOptions as opt}<option value={opt}>{opt}</option>{/each}
    </select>
    {#if $outputStatus}
      <span class="dim">colour order</span>
      <select data-role="layout-order" value={$outputStatus.order} on:change={setOrder}>
        {#each ["rgb", "rbg", "grb", "gbr", "brg", "bgr"] as c}
          <option value={c}>{c.toUpperCase()}</option>
        {/each}
      </select>
    {/if}
    <span class="dim hint">switched live — colours swapped? try GRB/BGR</span>
  </div>

  {#if $dataPins.length}
    <div class="field">
      <span class="flabel">Data pin</span>
      <select
        data-role="layout-datapin"
        value={String($dataPinChoice ?? $dataPinNext ?? $dataPin)}
        on:change={onDataPinPick}
      >
        {#each $dataPins as pin}
          <option value={String(pin)}>
            GPIO{pin}{pin === $dataPinDefault ? " (board default)" : ""}
          </option>
        {/each}
      </select>
      <!-- the button acts on a PENDING pin change; with none there is nothing
           to apply, so it is absent rather than dimmed (§5.7, Gitea #529) -->
      {#if $device && ($dataPinChoice !== null || $dataPinNext !== null)}
        <button data-role="layout-datapin-apply" on:click={() => void applyDataPin()}>
          apply &amp; reboot
        </button>
      {/if}
      <span class="dim hint" data-role="layout-datapin-note">
        {#if $notes.datapin}
          {$notes.datapin}
        {:else if $dataPinNext !== null}
          stored GPIO{$dataPinNext}, driving GPIO{$dataPin} until the next reboot
        {:else}
          where the strip's DATA wire goes — applied on reboot
        {/if}
      </span>
    </div>
  {/if}
{/if}

{#if vis.outputsTable}
  <OutputsTable
    {outputs}
    unit={outUnit}
    maxOutputs={$deviceCaps?.outputs ?? 1}
    pins={$dataPins}
    protocols={$protocolOptions}
    total={outTotal}
    on:apply={(e) => setOutputs(e.detail)}
  />
{/if}

<div class="linkrow">
  <span class="dim hint">For rings, sculptures, irregular layouts</span>
  <button class="link" data-role="layout-map-link" on:click={() => dispatch("openmap")}>
    Custom map program →
  </button>
</div>

<!-- A saved/rejected note only. "…and it needs a reboot" is the sticky bar's
     job now (settings/RebootBar.svelte, #538) — it has to be visible from the
     Patterns tab too, and a line of dim 12px text at the bottom of this form
     was not. -->
{#if $notes.layout}
  <p class="dim hint" data-role="layout-note">{$notes.layout}</p>
{/if}

<style>
  .summary {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 4px;
  }

  /* mockup S3/S3b: the headline and its sub-line sit on ONE baseline row */
  .summary > div:first-child {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
  }

  /* mockup S3 `.summary .big` */
  .big {
    font-size: 17px;
    font-weight: 600;
    color: var(--text);
  }

  .ckrow {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-dim);
  }

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
  }

  .refresh.amber .dot {
    background: #e8a33d;
  }

  .refresh .hz {
    font-size: 15px;
  }

  .refresh.amber .hz {
    color: #e8a33d;
  }

  .notes > div {
    margin: 2px 0;
  }

  /* mockup S3 `.linkrow`: a hint and a link out, no rule above them */
  .linkrow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 2px;
  }

  .latthumb {
    line-height: 0;
  }
</style>
