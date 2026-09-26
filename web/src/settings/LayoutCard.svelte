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
  // data pin (`/api/datapin`). That split is now cosmetic for the first two:
  // since Gitea #550 `/api/layout` answers `reboot_required:false` for
  // output 0's protocol and colour order (and for any output's `count` /
  // `rev`) — only a pad, a driver instance and a FURTHER output's wire
  // format wait for a boot. Folding them onto `out 0` is Gitea #524; see
  // docs/api.md "Live vs reboot".
  //
  // Every field is gated by `lib/settingsCaps.ts` — absent, never disabled.
  import { createEventDispatcher } from "svelte";
  import PatternThumb from "../components/PatternThumb.svelte";
  import type { LayoutWire } from "../lib/device";
  import {
    REFRESH_AMBER_HZ,
    settingsVisibility,
    squarish,
    uiLayoutKind,
    type Corner,
    type LayoutKind,
    type RunDir,
  } from "../lib/settingsCaps";
  import { configuredDriver, panelRefreshHz } from "../lib/panelDriver";
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
  import { layout as geomLayout, layoutLabel, type Layout } from "../stores/geometry";
  import { clearApiError, note, notes, reportApiError } from "../stores/notify";
  import type { ApiErrorContext } from "../lib/apiErrors";
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
  /** The same four, as the hint beside the picker spells them (mockup S3b:
   *  `Strip · Matrix · 3D · custom map` — a sentence, not four proper nouns). */
  const KIND_HINT: Record<LayoutKind, string> = {
    strip: "Strip",
    matrix: "Matrix",
    lattice: "3D",
    map: "custom map",
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
  /** The panel driver the estimate is spent on: the device's STORED clock and
   *  bit depth since #401/#525, this build's constants on firmware that does
   *  not report them (`lib/panelDriver.ts`). */
  $: pdriver = configuredDriver(wire);
  /** The estimated rescan rate. Computed from the CONFIGURED driver wherever
   *  the device reports one, and from the device's own `est_hz` (#475) where
   *  it does not — same formula either way, `lib/panelDriver.ts` picks
   *  (docs/api.md "Panel arrangement"). */
  $: refreshHz = panelRefreshHz(wire, { pw: m.pw, ph: m.ph, panels, scan: m.scan });
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
  async function post(
    lines: string,
    field = "the wiring",
    ctx: Partial<ApiErrorContext> = {},
  ): Promise<boolean> {
    const r = await applyLayout(lines);
    if (!r.ok) {
      // The banner is the surface now (#538 round 2) — a refusal at the foot
      // of a 760px form is a refusal nobody reads.
      reportApiError(r.error, { scope: "layout", maxPixels: $pixelMax, line: r.line, ...ctx });
      return false;
    }
    clearApiError();
    note("layout", "saved", 2500);
    if (r.reboot_required) noteRebootPending(field);
    return true;
  }

  function matrixLine(patch: Partial<ReturnType<typeof matrixOf>>): string {
    const n = { ...m, ...patch };
    return `matrix ${n.pw} ${n.ph} ${n.cols} ${n.rows} ${n.start} ${n.dir} ${n.snake} ${n.rot180} ${n.scan}`;
  }

  function setMatrix(patch: Partial<ReturnType<typeof matrixOf>>): void {
    const n = { ...m, ...patch };
    const chain = { pw: n.pw, ph: n.ph, cols: n.cols, rows: n.rows };
    void (async () => {
      // Pre-validate against the board's ceiling the way `latOverMax` already
      // does for the lattice (#600). A chain over it is refused by the device
      // anyway; catching it here means the explanation arrives with the
      // numbers the FORM knows, and nothing is sent that cannot land.
      if (chain.pw * chain.ph * chain.cols * chain.rows > $pixelMax) {
        reportApiError("pw*ph*cols*rows out of range for this board", {
          scope: "layout",
          maxPixels: $pixelMax,
          chain,
        });
        return;
      }
      if (await post(matrixLine(patch), "the panel arrangement", { chain }))
        dispatch("pixelchange");
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
      else reportApiError(r?.error ?? "rejected", { scope: "output", field: "layout-proto" });
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
        reportApiError(r.error ?? "rejected", {
          scope: "layout",
          maxPixels: $pixelMax,
          pixels: w * h * d,
          field: "layout-lat-install",
        });
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
      reportApiError(r?.error ?? "save failed", { scope: "layout", field: "layout-datapin" });
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
  function summaryHint(
    px: number,
    tiles: number,
    isPanel: boolean,
    proto: string,
    pin: number,
    wires: number,
  ): string {
    const parts: string[] = [];
    if (kind === "matrix" && tiles > 1) parts.push(`${tiles} panels`);
    // With more than one wire the interesting fact is how many (mockups
    // S3j/S3k) — the pixel count is already the headline above.
    if (wires > 1) parts.push(`${wires} outputs`);
    else parts.push(`${px} pixel${px === 1 ? "" : "s"}`);
    if (isPanel) parts.push("HUB75");
    else if (proto) parts.push(proto.toUpperCase());
    if (!isPanel && wires <= 1 && $dataPins.length) parts.push(`GPIO ${pin}`);
    return parts.join(" · ");
  }

  /** The big line above the form. The mockups space a matrix's dimensions —
   *  `64 × 64 matrix` — where the header chip's compact `64×64 matrix` does
   *  not, so the spacing lives HERE rather than in `layoutLabel`, which every
   *  other surface shares (mockups S3, S3k). */
  function headline(l: Layout): string {
    return l.dims === 2 && l.regular ? `${l.w} × ${l.h} matrix` : layoutLabel(l);
  }
</script>

<div class="summary" data-role="layout-summary">
  <div>
    <div class="big" data-role="layout-headline">{headline($geomLayout)}</div>
    <div class="dim hint" data-role="layout-subhead">
      {summaryHint(
        $devicePixels || wire?.pixels || 0,
        panels,
        $deviceCaps?.panel ?? false,
        $deviceProtocol,
        $dataPin,
        outputs.length,
      )}
    </div>
  </div>
  {#if $luxel}<PatternThumb luxel={$luxel} source={SAMPLE} size="summary" />{/if}
</div>

{#if vis.kindPicker}
  <div class="field">
    <span class="flabel">Layout</span>
    <div class="fctl row g10">
      <select class="w150" data-role="layout-kind" value={kind} on:change={setKind}>
        {#each vis.kindOptions as k}
          <option value={k}>{KIND_LABEL[k]}</option>
        {/each}
      </select>
      <span class="dim hint">{vis.kindOptions.map((k) => KIND_HINT[k]).join(" · ")}</span>
    </div>
  </div>
{/if}

{#if vis.latticeFields}
  <!-- A 3D lattice: `w × h × d` cells wired as one run, installed as the
       device's coordinate map (#538 — "I cannot try 3D at all"). -->
  <div class="field top">
    <span class="flabel">Lattice</span>
    <div class="fctl">
      <div class="row">
      <input
        class="inp num"
        data-role="layout-lat-w"
        type="number"
        min="2"
        max={LAT_MAX}
        value={lat.w}
        on:change={(e) => (lat = { ...lat, w: Number(e.currentTarget.value) || 1 })}
      />
      <span class="dim tiny">×</span>
      <input
        class="inp num"
        data-role="layout-lat-h"
        type="number"
        min="2"
        max={LAT_MAX}
        value={lat.h}
        on:change={(e) => (lat = { ...lat, h: Number(e.currentTarget.value) || 1 })}
      />
      <span class="dim tiny">×</span>
      <input
        class="inp num"
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
          A lattice goes to the device as one coordinate per pixel in a single request, so it stops
          at {LAT_MAX}×{LAT_MAX}×{LAT_MAX} ({LAT_MAX ** 3} pixels) — Gitea #548 is the procedural
          form that would lift it.
        {:else}
          One run of {latPixels} pixels, wired x first, then y, then z. Installed as the device's
          coordinate map, so 3D patterns render natively and 1D/2D ones get the projections below.
        {/if}
      </p>
    </div>
  </div>
{/if}

<!-- mockup S3b: the Pixels cell is the field ALONE (an inline-block input),
     with what it costs said underneath rather than beside it -->
{#if kind === "strip"}
  <div class="field top">
    <span class="flabel">Pixels</span>
    <div class="fctl">
      <input
        class="inp num"
        data-role="layout-pixels"
        type="number"
        min="1"
        max={$pixelMax}
        value={$devicePixels}
        on:change={setPixels}
      />
      <p class="dim hint under">resized live — max {$pixelMax}, no reboot</p>
    </div>
  </div>
{/if}

{#if kind === "matrix"}
  <div class="field">
    <span class="flabel">{vis.panelScan ? "Panel" : "Size"}</span>
    <div class="fctl row">
      <input
        class="inp num"
        data-role="layout-pw"
        type="number"
        min="1"
        max="256"
        value={m.pw}
        on:change={(e) => setMatrix({ pw: Number(e.currentTarget.value) || 1 })}
      />
      <span class="dim tiny">×</span>
      <input
        class="inp num"
        data-role="layout-ph"
        type="number"
        min="1"
        max="256"
        value={m.ph}
        on:change={(e) => setMatrix({ ph: Number(e.currentTarget.value) || 1 })}
      />
      <span class="dim hint">px</span>
    </div>
  </div>

  <!-- mockup S3: the scan divisor is its OWN row, and its cell holds nothing
       but the picker — it is a different question from the panel's size -->
  {#if vis.panelScan}
    <div class="field">
      <span class="flabel">Panel scan</span>
      <div class="fctl">
        <select
          class="w170"
          data-role="layout-scan"
          value={String(m.scan)}
          on:change={(e) => setMatrix({ scan: Number(e.currentTarget.value) })}
        >
          {#each SCANS as s}
            <option value={String(s)}>{s === 0 ? "board default" : `1/${s}`}</option>
          {/each}
        </select>
      </div>
    </div>
  {/if}

  {#if vis.panelCounts}
    <div class="field">
      <span class="flabel">Panels</span>
      <div class="fctl row">
        <input
          class="inp num"
          data-role="layout-cols"
          type="number"
          min="1"
          max="16"
          value={m.cols}
          on:change={(e) => setMatrix({ cols: Number(e.currentTarget.value) || 1 })}
        />
        <span class="dim tiny">across ×</span>
        <input
          class="inp num"
          data-role="layout-rows"
          type="number"
          min="1"
          max="16"
          value={m.rows}
          on:change={(e) => setMatrix({ rows: Number(e.currentTarget.value) || 1 })}
        />
        <span class="dim tiny">down</span>
      </div>
    </div>
  {/if}

  {#if vis.wiringRow}
    <div class="field">
      <span class="flabel">{vis.wiringIsPixels ? "Pixel wiring" : "Chain"}</span>
      <div class="fctl row g10">
        <span class="dim tiny">starts</span>
        <select class="w126" data-role="layout-start" value={m.start} on:change={setStart}>
          {#each CORNERS as c}<option value={c.v}>{c.label}</option>{/each}
        </select>
        <span class="dim tiny">runs</span>
        <select class="w114" data-role="layout-dir" value={m.dir} on:change={setDir}>
          {#each DIRS as d}<option value={d.v}>{d.label}</option>{/each}
        </select>
        <label class="ckrow">
          <input
            class="cbxin"
            type="checkbox"
            data-role="layout-snake-input"
            checked={m.snake === 1}
            on:change={(e) => setMatrix({ snake: e.currentTarget.checked ? 1 : 0 })}
          />
          <span class="cbx" class:on={m.snake === 1} data-role="layout-snake"
            >{m.snake === 1 ? "✓" : ""}</span
          >
          serpentine
        </label>
        {#if vis.rot180}
          <label class="ckrow">
            <input
              class="cbxin"
              type="checkbox"
              data-role="layout-rot180-input"
              checked={m.rot180 === 1}
              on:change={(e) => setMatrix({ rot180: e.currentTarget.checked ? 1 : 0 })}
            />
            <span class="cbx" class:on={m.rot180 === 1} data-role="layout-rot180"
              >{m.rot180 === 1 ? "✓" : ""}</span
            >
            rotate alternate rows 180°
          </label>
        {/if}
      </div>
    </div>
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
      {panels} panel{panels === 1 ? "" : "s"} × {pdriver.planes} planes at
      {pdriver.clock_mhz} MHz.
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

{/if}

{#if vis.stripFields && !vis.outputsTable}
  <div class="field">
    <span class="flabel">LED type</span>
    <div class="fctl row">
      <select class="w150" data-role="layout-proto" value={$deviceProtocol} on:change={setProtocol}>
        {#each $protocolOptions as opt}<option value={opt}>{opt}</option>{/each}
      </select>
      {#if $outputStatus}
        <span class="dim tiny">Color order</span>
        <select class="w96" data-role="layout-order" value={$outputStatus.order} on:change={setOrder}>
          {#each ["rgb", "rbg", "grb", "gbr", "brg", "bgr"] as c}
            <option value={c}>{c.toUpperCase()}</option>
          {/each}
        </select>
      {/if}
      <span class="dim hint">switched live — colours swapped? try GRB/BGR</span>
    </div>
  </div>

  <!-- Data pin, inline on a SINGLE-output board (mockup S3b). A board with
       more than one output states its pads in the Outputs table instead, and
       this row is absent there rather than saying the same thing twice.
       Absent too where the host publishes no pad list (§5.7 — the native
       mirror is one; Gitea #579). -->
  {#if $dataPins.length}
    <div class="field">
      <span class="flabel">Data pin</span>
      <div class="fctl row g10">
        <select
          class="w150"
          data-role="layout-datapin"
          value={String($dataPinChoice ?? $dataPinNext ?? $dataPin)}
          on:change={onDataPinPick}
        >
          {#each $dataPins as pin}
            <option value={String(pin)}>
              GPIO {pin}{pin === $dataPinDefault ? " (board default)" : ""}
            </option>
          {/each}
        </select>
        <!-- the button acts on a PENDING pin change; with none there is
             nothing to apply, so it is absent rather than dimmed (§5.7) -->
        {#if $device && ($dataPinChoice !== null || $dataPinNext !== null)}
          <button
            class="btn sm"
            data-role="layout-datapin-apply"
            on:click={() => void applyDataPin()}
          >
            apply &amp; reboot
          </button>
        {/if}
        <span class="dim hint" data-role="layout-datapin-note">
          {#if $notes.datapin}
            {$notes.datapin}
          {:else if $dataPinNext !== null}
            stored GPIO{$dataPinNext}, driving GPIO{$dataPin} until the next reboot
          {:else}
            applies after a reboot
          {/if}
        </span>
      </div>
    </div>
  {/if}
{/if}

<!-- The picture of the fixture comes AFTER the table that cuts it up and
     BEFORE the notes that close the form (mockup S3k) — so on a board with an
     Outputs table it is handed to the table as slot content rather than
     rendered beside it. -->
{#if vis.outputsTable}
  <OutputsTable
    {outputs}
    unit={outUnit}
    maxOutputs={$deviceCaps?.outputs ?? 1}
    pins={$dataPins}
    protocols={$protocolOptions}
    total={outTotal}
    on:apply={(e) => setOutputs(e.detail)}
  >
    {#if kind === "matrix" && showArrangement}
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
        outputCounts={outputs.map((o) => o.count)}
        drive={driven}
      />
    {/if}
  </OutputsTable>
{:else if kind === "matrix" && showArrangement}
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
    drive={driven}
  />
{/if}

{#if kind === "matrix"}
  <div class="notes" data-role="layout-notes">
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
    margin-bottom: 12px;
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

  /* mockup S3 `.summary canvas`: the fixture thumbnail goes to the far end */
  .summary :global(canvas) {
    margin-left: auto;
  }

  /* mockup S3 `.summary .big` */
  .big {
    font-size: 17px;
    font-weight: 600;
    color: var(--text);
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

  /* mockup S3 `.linkrow`: a hint and a link out, no rule above them — the
     link pushes itself to the far end with its own `margin-left:auto` */
  .linkrow {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
  }

  .latthumb {
    line-height: 0;
  }
</style>
