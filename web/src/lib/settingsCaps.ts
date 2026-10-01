// Which Settings fields exist on THIS device (proposal §5.3, §5.3b, §5.7,
// Gitea #469).
//
// The rule the whole page obeys: a control is **absent** unless the device
// advertises the thing it acts on — never disabled, never inferred from a
// board name. The inputs are the `caps` block `/api/status` publishes (#464)
// and the Layout `/api/layout` reports (#465); the output is one flat record
// of booleans the Svelte markup reads with `{#if}`.
//
// It lives here, free of Svelte and of `fetch`, so the gating is unit-tested
// against fixtures for the four devices the audit table names — strip,
// HUB75 panel, regular 2D built from strips, 3D/irregular map
// (`web/tests/settingsCaps.test.mjs`) — rather than driven through a browser
// one board at a time.

import type { DeviceCaps } from "./device";

/** What the LED layout section can be. `map` is a coordinate SOURCE, not a
 *  dimensionality (proposal §5.4d) — it is a kind here because that is what
 *  the `/api/layout` wire calls it.
 *
 *  `lattice` is the PICKER's fourth entry (mockup S3b: "Strip · Matrix · 3D ·
 *  custom map") and the one kind the wire does not name: a 3D lattice is
 *  installed as a coordinate map, so `/api/layout` reports it as `map` with
 *  `dims: 3`. [`uiLayoutKind`] is the one place that derives it. */
export type LayoutKind = "strip" | "matrix" | "lattice" | "map";

/**
 * The kind the PICKER shows for a Layout the device reports. Everything but
 * the 3D lattice is the wire's own word; a `map` Layout whose coordinates
 * are 3D is the lattice the 3D kind installs (Gitea #538).
 */
export function uiLayoutKind(wireKind: "strip" | "matrix" | "map", dims: 1 | 2 | 3): LayoutKind {
  return wireKind === "map" && dims === 3 ? "lattice" : wireKind;
}

/**
 * The conservative reading for a device that does not publish `caps` at all
 * (firmware older than #464, where `deviceCaps` is null). It advertises only
 * what every build has always had: a strip driver, one output, a power model
 * and the two spatial stages. Everything that needs a NEWER firmware feature
 * — a panel, PSRAM, OTA, reboot — reads false, so an old device shows the
 * fields it really has and none it does not.
 */
export const FALLBACK_CAPS: DeviceCaps = {
  strip_driver: true,
  panel: false,
  outputs: 1,
  power_cap: true,
  blur_glow: true,
  layers: 1,
  text_slots: 0,
  reboot: false,
  ota: false,
  psram: false,
  assets: false,
};

/** The Layout facts the gating keys off — a subset of `/api/layout`'s GET. */
export interface LayoutFacts {
  kind: LayoutKind;
  /** 1 strip · 2 matrix or 2D map · 3 lattice or 3D map. */
  dims: 1 | 2 | 3;
  /** Addressable as a `w`×`h` lattice; false for a coordinate cloud. */
  regular: boolean;
  /** Tiles in the chain (`cols × rows`); 1 on a single-tile matrix. */
  panels: number;
}

/** Every visibility decision the Settings page makes, in one record. */
export interface SettingsVisibility {
  // ---- LED layout ----
  /** The Strip / Matrix / Custom picker — only where the board offers a
   *  choice. A HUB75 board has none, and the summary line carries the kind
   *  instead (§5.7: the disabled "Strip" in the mockup was the bug). */
  kindPicker: boolean;
  /** The picker's options, in display order. */
  kindOptions: LayoutKind[];
  /** Pixel count, LED type, colour order, data pin — a strip driver's. */
  stripFields: boolean;
  /** The `pw`×`ph` size fields (one panel, or the whole grid at 1×1). */
  panelSize: boolean;
  /** The **Panel module** disclosure inside LED layout (Gitea #778): the scan
   *  rate plus the `panel` line's four — the module's own properties, as
   *  opposed to how the modules are arranged. It was Advanced › Panel driver
   *  until Jeremy moved it here, 2026-09-26. Also what makes the size row read
   *  `Panel` rather than `Size`: on a HUB75 board those fields are ONE
   *  module's, not the whole grid's. */
  panelModule: boolean;
  /** `Panels [c] across × [r] down`. */
  panelCounts: boolean;
  /** Start corner · run direction · snake. ONE row: it describes the chain
   *  through the tiles, or — at `cols`=`rows`=1 — the pixel run through the
   *  grid, which is the same widget one level down (§5.3, S3c/S3d). A
   *  STRIP-built matrix only: a panel board describes its wall in the
   *  picture editor instead and has no rule row at all (Gitea #920). */
  wiringRow: boolean;
  /** …and that row reads as pixel wiring rather than a panel chain. */
  wiringIsPixels: boolean;
  /** The odd lines' tiles hang upside-down (`rot[1]`, upright or 180°) — a
   *  strip-built matrix with tiles to alternate. */
  rot180: boolean;
  /** The arrangement SVG: panel grid, chain path, per-tile scan direction. */
  arrangement: boolean;
  /** The arrangement picture IS the editor (Gitea #920): a panel board
   *  transcribes its wall panel by panel — click a cell, say which panel of
   *  the ribbon it is and turn it until its arrow points up — with the
   *  device's Identify test card to read the numbers off. It is the ONLY way
   *  a panel board describes its wall (Jeremy: "completely remove the
   *  regular pattern settings"), so a lone panel gets it too: that is how a
   *  single panel hung turned is said. A strip-built matrix keeps the
   *  read-only picture and its pixel-wiring row. */
  chainEditor: boolean;
  /** The estimated-refresh readout (panels × planes × clock). */
  estimatedRefresh: boolean;
  /** The Outputs table (§5.3b) — a board with more than one physical output. */
  outputsTable: boolean;
  /** `+ Add output`: a spare physical output exists. */
  addOutput: boolean;
  /** The Projection SECTION — this Layout offers at least one choice.
   *
   *  Since #538 a Layout never shows a pattern BIGGER than itself, so the
   *  whole table is: a 1D Layout offers nothing (2D and 3D patterns are not
   *  shown on a strip at all) · a 2D Layout offers the 1D row · a 3D Layout
   *  offers the 1D and 2D rows. The section is therefore ABSENT on a strip —
   *  with no explanatory copy, which is the rule Jeremy asked for
   *  (docs/spec/projection.md §1). */
  projection: boolean;
  /** The `w × h × d` lattice fields (the 3D kind is the picked one). */
  latticeFields: boolean;

  // ---- Advanced ----
  /** Power cap (mA): needs a per-pixel current model. */
  powerCap: boolean;
  /** Blur + glow: need neighbours AND the board's frame budget. */
  blurGlow: boolean;
  /** How to word blur/glow: along the strip, or across the grid. */
  blurGlowScope: "strip" | "grid";
  /** The PSRAM line under Storage. */
  psram: boolean;
  /** `Update…` — the device takes a firmware image over the network. */
  ota: boolean;
  /** `Reboot into setup AP` — the device can reboot itself. */
  reboot: boolean;
}

/**
 * The one gating function. `caps` null = firmware older than #464, which
 * falls back to [`FALLBACK_CAPS`] rather than to a board name.
 */
export function settingsVisibility(
  caps: DeviceCaps | null,
  layout: LayoutFacts,
): SettingsVisibility {
  const c = caps ?? FALLBACK_CAPS;
  const matrix = layout.kind === "matrix";
  const tiles = Math.max(1, Math.round(layout.panels) || 1);
  return {
    kindPicker: c.strip_driver,
    // A 3D lattice is driven pixel by pixel down one wire, so it is offered
    // exactly where a strip driver is (mockup S3b; Gitea #538).
    kindOptions: c.strip_driver ? ["strip", "matrix", "lattice", "map"] : ["matrix"],
    stripFields: c.strip_driver,
    panelSize: matrix,
    panelModule: matrix && c.panel,
    panelCounts: matrix,
    wiringRow: matrix && !c.panel,
    wiringIsPixels: matrix && !c.panel && tiles === 1,
    rot180: matrix && !c.panel && tiles > 1,
    arrangement: matrix,
    chainEditor: matrix && c.panel,
    estimatedRefresh: matrix && c.panel,
    outputsTable: c.outputs > 1,
    addOutput: c.outputs > 1,
    projection: layout.dims > 1,
    latticeFields: layout.kind === "lattice",
    powerCap: c.power_cap,
    blurGlow: c.blur_glow,
    blurGlowScope: layout.dims === 1 ? "strip" : "grid",
    psram: c.psram,
    ota: c.ota,
    reboot: c.reboot,
  };
}

// ---- time zones (mockup S3 `Clock & time zone`; Gitea #538) ----
//
// The device stores an OFFSET (`/api/clock` `tzMinutes`) — a board with 2 KB
// of NVS is never going to carry the IANA database. The browser has it, so
// the console offers real zone names and sends what the device understands.

/**
 * A zone's CURRENT offset from UTC, in minutes — DST included, because
 * `longOffset` formats the INSTANT rather than the standard rule.
 * `America/Denver` is −360 in winter and −300 in summer, which is exactly
 * what a device driving `clockHour()` wants.
 *
 * Returns 0 for a zone this engine does not know (and for `UTC`).
 */
export function zoneOffsetMinutes(zone: string, at: Date = new Date()): number {
  let text = "";
  try {
    text =
      new Intl.DateTimeFormat("en-US", { timeZone: zone, timeZoneName: "longOffset" })
        .formatToParts(at)
        .find((p) => p.type === "timeZoneName")?.value ?? "";
  } catch {
    return 0;
  }
  const m = /GMT([+-])(\d{1,2})(?::(\d{2}))?/.exec(text);
  if (!m) return 0;
  const sign = m[1] === "-" ? -1 : 1;
  return sign * (Number(m[2]) * 60 + Number(m[3] ?? 0));
}

/** `America/Indiana/Knox` → `Indiana · Knox`: the region heads the
 *  `<optgroup>`, so the option itself need not repeat it. */
export function zoneLabel(zone: string): string {
  const parts = zone.split("/");
  return parts.slice(1).join(" · ").replace(/_/g, " ") || zone;
}

/** Zones grouped into the picker's `<optgroup>`s, regions and zones sorted. */
export function zonesByRegion(zones: readonly string[]): { region: string; zones: string[] }[] {
  const by = new Map<string, string[]>();
  for (const z of zones) {
    const region = z.includes("/") ? (z.split("/")[0] ?? "Other") : "Other";
    const list = by.get(region);
    if (list) list.push(z);
    else by.set(region, [z]);
  }
  return [...by.entries()]
    .map(([region, list]) => ({ region, zones: [...list].sort() }))
    .sort((a, b) => a.region.localeCompare(b.region));
}

/** `UTC-6`, `UTC+5:45`, `UTC+0` — the offset as the status line states it. */
export function offsetLabel(tzMinutes: number): string {
  const h = Math.trunc(Math.abs(tzMinutes) / 60);
  const mm = Math.abs(tzMinutes) % 60;
  return `UTC${tzMinutes < 0 ? "-" : "+"}${h}${mm ? `:${String(mm).padStart(2, "0")}` : ""}`;
}

// ---- estimated refresh (proposal §5.3, S3c; firmware side is Gitea #475) ----

/** What the HUB75 driver's refresh rate is spent on.
 *
 *  These are SETTINGS since Gitea #401/#525: `/api/layout`'s `driver` block
 *  reports what the device has stored, and `lib/panelDriver.ts` turns it into
 *  this pair. [`PANEL_DRIVER_DEFAULT`] is the FALLBACK for firmware that does
 *  not carry the `panel` line — never the model. */
export interface PanelDriver {
  /** LCD_CAM pixel clock in Hz. */
  clockHz: number;
  /** BCM bit depth: the refresh halves per extra plane. */
  planes: number;
  /** LSB on-time in pixel clocks — the refresh-multiplier schedule (Gitea
   *  #460 / #789 / #797). `0` or absent = FULL, the stock BCM schedule, which
   *  is what every caller meant before the field existed. */
  lsb?: number;
  /** Latch-blanking clocks (the `panel` line's fourth field); absent = 0. Only
   *  read when `lsb` is set: it shrinks the lit width the on-time is measured
   *  against. */
  blank?: number;
  /** Clocks the latch is held high — 3 on a DP3246, 1 otherwise; absent = 1.
   *  Same story as `blank`. */
  latch?: number;
}

/** Every board's boot default (`firmware/src/hub75.rs`: `CLOCK = 30 MHz`,
 *  `PLANES = 7`), and what the console assumes when a device reports no
 *  `driver` block at all. */
export const PANEL_DRIVER_DEFAULT: PanelDriver = { clockHz: 30_000_000, planes: 7 };

/** Below this the panel visibly flickers on camera and in fast motion. */
export const REFRESH_AMBER_HZ = 100;

/** The arrangement numbers the estimate needs. */
export interface RefreshInput {
  /** One panel's pixel width — the shift-register length per tile. */
  pw: number;
  /** One panel's pixel height. */
  ph: number;
  /** Tiles in the chain. */
  panels: number;
  /** Scan divisor (1/16, 1/32 …); 0 = derive it as `ph / 2`. */
  scan: number;
}

/** The address rows a `RefreshInput` scans (`scan` 0 = `ph / 2`). */
export function scanRows(a: RefreshInput): number {
  const ph = Math.max(0, Math.round(a.ph));
  return a.scan > 0 ? Math.round(a.scan) : Math.floor(ph / 2);
}

/**
 * The words ONE row block shifts out: `pw · panels · stripes`.
 *
 * `stripes = (ph / 2) / scan` — a 1/N-scan panel has fewer address rows but
 * each one clocks out `stripes` copies of the chain, so the product is the
 * panel's pixel count whatever the scan (Gitea #764). A scan that does not
 * divide `ph / 2` has no framebuffer (the device rejects it); it is read as
 * one stripe rather than a fraction.
 *
 * This is the `cols` the firmware's schedule arithmetic takes
 * (`luxel_hub75::Schedule`), which is why the lit width below is measured
 * against it and not against the panel's pixel width.
 */
export function rowBlockWords(a: RefreshInput): number {
  const pw = Math.max(0, Math.round(a.pw));
  const ph = Math.max(0, Math.round(a.ph));
  const panels = Math.max(0, Math.round(a.panels));
  const scan = scanRows(a);
  const half = Math.floor(ph / 2);
  const stripes = scan > 0 && half >= scan && half % scan === 0 ? half / scan : 1;
  return pw * panels * stripes;
}

// ---- faster refresh: the `lsb` schedule (Gitea #460 / #789 / #797) --------
//
// Stock BCM lights every plane for the whole row block and gets the binary
// weights by re-shifting plane `k` `2^k` times, so a rescan costs `2^planes − 1`
// row shifts and the LSB is lit for a whole shift even though its weight only
// needs a fraction of one. Set the LSB's on-time to `lsb` clocks instead and
// the planes whose on-time fits inside one shift are emitted ONCE with OE cut
// off early — `t` of them — so a rescan costs `E = t + 2^(planes − t) − 1`
// shifts instead of `2^planes − 1`.
//
// What that costs in BRIGHTNESS is NOT `lsb / W` — #789 shipped that and it was
// wrong, by up to a factor of 8. `lsb / W` is the duty cycle of one PASS, and
// truncating makes the pass SHORTER too: perceived brightness is on-time per
// unit TIME, and the panel runs `(2^planes − 1) / E` times as many passes per
// second. So, as a fraction of the stock schedule's peak,
//
//     brightness = lsb · (2^planes − 1) / (W · E)
//
// which is very nearly 1 at the TOP of each `t` step and falls linearly to half
// of that at the bottom of the same step (measured on Jeremy's panel
// 2026-09-26: `lsb 14` of `W 59` at 7 planes is visibly close to stock, not the
// 24 % the old model predicted).
//
// The refresh, conversely, is CONSTANT across a step and only moves when `lsb`
// crosses `W / 2^t`. So every position except a step's TOP —
// `lsb = floor(W / 2^t)` — is strictly worse than that top: identical Hz, less
// light. Which is why the card offers the step tops only (`lsbSteps`) rather
// than a continuous 1..W slider.
//
// Binary weights stay exact either way (plane `k` is lit `lsb · 2^k` clocks
// however it is emitted), which is what keeps a grey ramp monotonic.
//
// A truncating schedule also needs one EXTRA row block per plane — the
// trailing display block (Gitea #795, the row-31 bug). A row block shifts row
// `r` while OE displays the row latched before it, so a plane's LAST address
// row is displayed during block 0 of the NEXT plane, whose OE window is a
// different width once the planes are truncated: the last row of each half
// (rows 31 and 63 of a 64×64) came out with its bit weights rotated by one
// plane — wrong colour and wrong brightness, which is what Jeremy saw on the
// panel on 2026-09-27. So at `t > 0` block 0 runs with OE OFF, blocks
// `1..rows−1` display rows `0..rows−2` as before, and a trailing block
// (address `rows−1`, OE at the plane's own width, no latch) displays the last
// row. A plane therefore costs `rows + 1` row blocks instead of `rows`, which
// slows the rescan by `rows/(rows+1)` — 3 % at 1/32 scan — and costs the same
// 3 % of peak brightness, since the extra block adds pass TIME and no on-time.
// At `t = 0` (the stock schedule) the OE widths are all equal, no trailing
// block is needed, and the layout is byte-identical to what shipped.
//
// The same schedule arithmetic runs on the device in
// `crates/luxel-hub75/src/schedule.rs` — this is the browser's copy of it, so
// the readouts cannot lag the field the user just moved.

/**
 * Lit clocks of ONE row block, `W`: the words it shifts less the chip's latch
 * tail and the two blanking windows.
 *
 * Floored at 1. A template whose arithmetic comes out at or below zero cannot
 * light the panel at all, and 1 keeps every readout finite instead of
 * producing `Infinity` in a hint.
 */
export function litWidth(colsWords: number, blank = 0, latch = 1): number {
  const words = Math.max(0, Math.round(colsWords));
  const w = words - Math.max(0, Math.round(latch)) - 2 * Math.max(0, Math.round(blank));
  return Math.max(1, w);
}

/**
 * The on-time the panel actually runs, never 0: the configured `lsb` clamped
 * to the lit width, or that width itself when the configured value is `0`
 * (full). The same clamp the device reports as `driver.live.lsb`.
 */
export function lsbEffective(lsb: number, width: number): number {
  const w = Math.max(1, Math.round(width));
  const v = Math.round(Math.max(0, Number(lsb) || 0));
  return v <= 0 || v > w ? w : v;
}

/**
 * Truncated planes `t`: the largest `t ≤ planes − 1` with `lsb << t ≤ W`.
 *
 * Those planes are emitted once with OE cut off early; every plane above them
 * keeps its descriptor repeats, so the MSB is always a repeated plane. `t` is
 * 0 at full on-time, which IS the stock schedule.
 */
export function truncatedPlanes(lsbEff: number, width: number, planes: number): number {
  const p = Math.max(1, Math.round(planes));
  const w = Math.max(1, Math.round(width));
  const eff = Math.max(1, Math.round(lsbEff));
  let t = 0;
  while (t + 1 <= p - 1 && eff * 2 ** (t + 1) <= w) t += 1;
  return t;
}

/** Row shifts per rescan: `t + 2^(planes − t) − 1`. `t = 0` is stock BCM's
 *  `2^planes − 1`, the yardstick the refresh gain is against. */
export function emissions(planes: number, trunc = 0): number {
  const p = Math.max(1, Math.round(planes));
  const t = Math.min(p - 1, Math.max(0, Math.round(trunc)));
  return t + 2 ** (p - t) - 1;
}

/** The TRAILING display block a plane needs: `1` as soon as a plane is
 *  truncated, `0` for the stock schedule (Gitea #795).
 *
 *  Truncating makes the planes' OE windows different widths, and a row block
 *  displays the row latched by the block BEFORE it — so without this block the
 *  last address row of a plane is displayed under the next plane's window and
 *  comes out a plane-rotated weight (rows 31 and 63 of a 64×64). Mirrors
 *  `Schedule::needs_trail` / `Geometry::trail` on the device. */
export function trailingBlock(trunc: number): number {
  return Math.max(0, Math.round(trunc)) > 0 ? 1 : 0;
}

/** Row blocks ONE plane clocks out: the address `rows`, plus the trailing
 *  display block when the schedule truncates. `Geometry::blocks` on the
 *  device. */
export function rowBlocksPerPlane(rows: number, trunc: number): number {
  return Math.max(0, Math.round(rows)) + trailingBlock(trunc);
}

/**
 * Peak brightness as a fraction of the STOCK schedule's, 0..1:
 *
 * ```text
 * brightness = lsb_eff · (2^planes − 1) · rows
 *              ───────────────────────────────    E = emissions(planes, t)
 *                 W · E · (rows + trail)          trail = 1 when t > 0
 * ```
 *
 * `lsb_eff / W` is the duty cycle of one pass and is NOT the answer — that is
 * what #789 shipped, and it understated a truncating schedule by up to 8×.
 * Truncating also shortens the pass, so the panel runs `(2^planes − 1) / E`
 * times as many passes per second and gets that factor back.
 *
 * The `rows / (rows + trail)` tail is the trailing display block (#795): a
 * truncating pass clocks out one more row block per plane than the stock pass
 * does, which is pass TIME with no extra on-time, so it costs a flat
 * `1/(rows+1)` of the light — 3 % at 1/32 scan. `rows` is the ADDRESS rows
 * (`scanRows`), not the panel height.
 *
 * Still independent of WHICH plane — every plane's on-time scales by the same
 * factor, so the ramp keeps its shape and only the peak moves — but no longer
 * independent of the bit DEPTH, because `E` is built from it. It compounds with
 * the ordinary `brightness` channel LUT rather than replacing it.
 *
 * At the top of each `t` step this is within a few percent of full (bench
 * `W` 61 at 7 planes, 1/32 scan: 94.6 % at `lsb` 30, 91.8 % at 15, 78.5 % at
 * 7) and it falls linearly with `lsb` INSIDE a step (48.9 % at `lsb` 8, which
 * runs the same 430 Hz as 15).
 */
export function peakBrightnessFraction(
  lsbEff: number,
  width: number,
  planes: number,
  rows: number,
): number {
  const w = Math.max(1, Math.round(width));
  const p = Math.max(1, Math.round(planes));
  const r = Math.max(1, Math.round(rows));
  const eff = Math.min(w, Math.max(0, Math.round(Math.max(0, lsbEff))));
  const trunc = truncatedPlanes(Math.max(1, eff), w, p);
  const shifts = emissions(p, trunc);
  const blocks = rowBlocksPerPlane(r, trunc);
  return Math.min(1, Math.max(0, (eff * (2 ** p - 1) * r) / (w * shifts * blocks)));
}

/** One offered position of the `lsb` control: the TOP of a truncation step,
 *  which is the only `lsb` on that step worth having — same Hz as every
 *  smaller value on it, more light. */
export interface LsbStep {
  /** Truncated planes, `t`. 0 = the stock schedule. */
  t: number;
  /** The EFFECTIVE on-time this step runs: `W` at `t = 0`, else
   *  `floor(W / 2^t)`. */
  lsb: number;
  /** What to PUT ON THE WIRE for it: `0` at `t = 0` (full, so the setting
   *  follows a later change to `W`), else the clocks themselves. */
  wire: number;
  /** Row shifts per rescan, `E`. */
  emissions: number;
  /** The trailing display block this step needs: 1 at `t > 0`, 0 at the stock
   *  schedule (#795). A plane costs `rows + trail` row blocks. */
  trail: number;
  /** Peak brightness as a fraction of the stock schedule's. */
  brightness: number;
}

/**
 * The `lsb` positions worth offering for a lit width and a bit depth,
 * brightest (and slowest) first: one per truncation step `t`, at that step's
 * TOP.
 *
 * Inside a step the refresh does not move and the brightness falls linearly, so
 * a continuous slider spends most of its travel on positions strictly worse
 * than one of these (#797). `t` runs 0 .. `planes − 1`, but stops early once
 * `2^t > W`: `floor(W / 2^t)` would be 0 clocks, which is no schedule at all.
 * A 64×64 bench panel (`W` 61) at 7 planes therefore offers six positions, not
 * seven.
 *
 * The NOMINAL refresh multiplier of step `t` is `2^t`; the true ratio is
 * `(2^planes − 1) · rows / (E · (rows + 1))`, a few percent under it (at 7
 * planes and 1/32 scan: 1.92×, 3.73×, 6.84× … against a nominal 2/4/8),
 * which is why the card prints the Hz beside the × label. `rows` is the
 * address rows, needed for the trailing display block's share of both the
 * refresh and the brightness (#795).
 */
export function lsbSteps(width: number, planes: number, rows: number): LsbStep[] {
  const w = Math.max(1, Math.round(width));
  const p = Math.max(1, Math.round(planes));
  const r = Math.max(1, Math.round(rows));
  const steps: LsbStep[] = [];
  for (let t = 0; t <= p - 1; t++) {
    const lsb = Math.floor(w / 2 ** t);
    if (lsb < 1) break;
    steps.push({
      t,
      lsb,
      wire: t === 0 ? 0 : lsb,
      emissions: emissions(p, t),
      trail: trailingBlock(t),
      brightness: peakBrightnessFraction(lsb, w, p, r),
    });
  }
  return steps;
}

/**
 * Which of those steps a stored `lsb` reads as: the one whose truncation count
 * it is actually running.
 *
 * A value BETWEEN two step tops — a Layout stored by #789's continuous slider,
 * say — is shown at its own step rather than silently rewritten, so the refresh
 * the readout names is the one the panel really does; the wire value only
 * changes once the user moves the control.
 */
export function lsbStepIndex(
  steps: readonly LsbStep[],
  lsbEff: number,
  width: number,
  planes: number,
): number {
  const t = truncatedPlanes(Math.max(1, Math.round(lsbEff)), width, planes);
  const i = steps.findIndex((s) => s.t === t);
  return i < 0 ? 0 : i;
}

/**
 * Estimated panel refresh in Hz.
 *
 * One rescan shifts `rowBlockWords` words for each row block of a plane, once
 * per emission:
 *
 * ```text
 * Hz = clock / (pw · panels · stripes · (scan + trail) · emissions)
 * ```
 *
 * `emissions` is `2^planes − 1` for the stock BCM schedule — and that is what
 * a `driver` with no `lsb` (or `lsb: 0`) means, so every caller from before
 * Gitea #789 gets exactly the number it used to. A truncating `lsb` lowers it
 * to `t + 2^(planes − t) − 1`; `blank` and `latch` only matter there, because
 * they set the lit width the on-time is clamped against.
 *
 * `trail` is the trailing display block (#795): `1` as soon as a plane is
 * truncated, `0` for the stock schedule, so a truncating rescan is
 * `(scan + 1)/scan` longer than the emission count alone suggests — 3 % at
 * 1/32 scan.
 *
 * The stock model is the one the bench measurements fit exactly (Gitea #255,
 * the table in `firmware/src/hub75.rs`): one 64×64 panel at 1/32 scan and 7
 * planes reads 77 Hz at 20 MHz, 115 Hz at 30 MHz, 154 Hz at 40 MHz, and 58 Hz
 * at 8 planes — the numbers measured on the Seengreat panel. At 30 MHz with
 * `blank` 1 on a shift register (`W` = 61) an `lsb` of 30 reads ~222 Hz and an
 * `lsb` of 8 reads ~430 Hz.
 *
 * Returns 0 when the inputs cannot describe a panel.
 */
export function estimatedRefreshHz(
  a: RefreshInput,
  driver: PanelDriver = PANEL_DRIVER_DEFAULT,
): number {
  const colsWords = rowBlockWords(a);
  const planes = Math.max(1, Math.round(driver.planes));
  const width = litWidth(colsWords, driver.blank ?? 0, driver.latch ?? 1);
  const eff = lsbEffective(driver.lsb ?? 0, width);
  const trunc = truncatedPlanes(eff, width, planes);
  const shifts = emissions(planes, trunc);
  const clocks = colsWords * rowBlocksPerPlane(scanRows(a), trunc) * shifts;
  if (clocks <= 0 || driver.clockHz <= 0) return 0;
  return driver.clockHz / clocks;
}

/**
 * The `w × h` a strip of `pixels` becomes when the user picks Matrix: the
 * factor pair closest to square, so the pixel count is PRESERVED (120 px is
 * 12×10, not 11×11 rounded up). A count with no usable factor pair — a prime,
 * or anything whose best pair is a 1×n line — falls back to `ceil(√n)` square,
 * which does change the count; that is honest, and the field is right there.
 */
export function squarish(pixels: number): { w: number; h: number } {
  const n = Math.max(1, Math.round(pixels) || 1);
  for (let h = Math.floor(Math.sqrt(n)); h >= 2; h--) {
    if (n % h === 0) return { w: n / h, h };
  }
  const side = Math.max(1, Math.ceil(Math.sqrt(n)));
  return { w: side, h: side };
}

// ---- the chain (the arrangement SVG's geometry, and #475's remap order) ----

export type Corner = "tl" | "tr" | "bl" | "br";
export type RunDir = "row" | "col";

/** One tile of the arrangement, in chain order. */
export interface ChainTile {
  /** 0-based position along the chain — `index + 1` is what the SVG prints. */
  index: number;
  /** Tile column / row in the grid, 0-based from the top-left. */
  col: number;
  row: number;
  /** Which LINE of the chain this tile is on: a row of tiles under
   *  `dir: "row"`, a column under `dir: "col"`. `snake` reverses the odd
   *  ones, and `rot` says how each parity's tiles are mounted — degrees
   *  clockwise, `[even lines, odd lines]` (Gitea #917). */
  line: number;
  /** This tile's scan runs right-to-left (the snake's return leg, or a
   *  start corner on the right), i.e. it is drawn mirrored. */
  flipX: boolean;
  /** …and top-to-bottom is reversed. */
  flipY: boolean;
  /** How the tile is MOUNTED: degrees clockwise from upright, 0|90|180|270
   *  (Gitea #917/#920). From the rule's `rot[line % 2]`, or the device's own
   *  per-tile `tiles` entry. */
  turns: number;
}

/**
 * The order the chain threads `cols × rows` tiles, from the `IN` connector.
 *
 * `start` is the corner the chain begins at, `dir` whether it advances along
 * a row (x first) or a column (y first), and `snake` whether alternate
 * lines run backwards. Pure, so the SVG and (later) the firmware's boot-time
 * remap (#475) can be checked against the same table.
 */
export function chainOrder(
  cols: number,
  rows: number,
  start: Corner,
  dir: RunDir,
  snake: boolean,
  rot: readonly [number, number] = [0, 0],
): ChainTile[] {
  const nc = Math.max(1, Math.round(cols) || 1);
  const nr = Math.max(1, Math.round(rows) || 1);
  const fromRight = start === "tr" || start === "br";
  // (the firmware walks the same order — docs/api.md "Panel arrangement")
  const fromBottom = start === "bl" || start === "br";
  const byRow = dir === "row";
  // `outer` counts the lines the chain crosses, `inner` the tiles in one line
  const outer = byRow ? nr : nc;
  const inner = byRow ? nc : nr;
  const tiles: ChainTile[] = [];
  for (let o = 0; o < outer; o++) {
    // a snaked chain runs every other line backwards
    const back = snake && o % 2 === 1;
    // the line itself is counted from the start corner's side
    const line = byRow ? (fromBottom ? nr - 1 - o : o) : (fromRight ? nc - 1 - o : o);
    // …and the line runs away from that side, reversed on a snake's return
    const reversed = (byRow ? fromRight : fromBottom) !== back;
    for (let p = 0; p < inner; p++) {
      const along = reversed ? inner - 1 - p : p;
      tiles.push({
        index: tiles.length,
        line: o,
        col: byRow ? along : line,
        row: byRow ? line : along,
        // the chain enters this tile from its right / bottom edge
        flipX: byRow && reversed,
        flipY: !byRow && reversed,
        turns: normTurn(rot[o % 2] ?? 0),
      });
    }
  }
  return tiles;
}

// ---- the explicit chain (Gitea #920: transcribe the wall panel by panel) ----
//
// `GET /api/layout`'s matrix block carries `tiles: [[cx, cy, deg], …]` — the
// chain in RIBBON order (entry 0 is the panel the ribbon enters, shown as
// panel 1), each grid cell exactly once. The editor edits that list and
// posts it back whole as ONE `chain` line; these are its pure halves.

/** One `tiles` entry: grid column, grid row (0-based from the top-left) and
 *  the mount rotation in degrees clockwise. */
export type WireTile = readonly [number, number, number];

/** A rotation folded into 0|90|180|270. */
export function normTurn(deg: number): number {
  const q = Math.round((Number(deg) || 0) / 90);
  return (((q % 4) + 4) % 4) * 90;
}

/**
 * The picture's tiles from the device's `tiles` list. `rule` is the same
 * wall walked by the rule fields (`chainOrder`): a derived list takes its
 * `line`/`flipX`/`flipY` from it, cell by cell, so a strip-built matrix still
 * draws its scan arrows; an explicit list has no lines to speak of.
 */
export function chainFromWire(
  tiles: readonly WireTile[],
  rule: readonly ChainTile[],
  explicit: boolean,
): ChainTile[] {
  return tiles.map(([cx, cy, deg], index) => {
    const r = explicit ? undefined : rule.find((t) => t.col === cx && t.row === cy);
    return {
      index,
      col: cx,
      row: cy,
      line: r?.line ?? 0,
      flipX: r?.flipX ?? false,
      flipY: r?.flipY ?? false,
      turns: normTurn(deg),
    };
  });
}

/** The `chain` wire line for a whole list, in ribbon order. An empty list is
 *  the bare `chain` that clears the explicit list back to the rule. */
export function chainLine(tiles: readonly WireTile[]): string {
  if (tiles.length === 0) return "chain";
  return `chain ${tiles.map(([cx, cy, deg]) => `${cx},${cy},${normTurn(deg)}`).join(" ")}`;
}

/** The ribbon position (0-based) of grid cell `cx,cy`, or -1. */
export function chainPositionOf(tiles: readonly WireTile[], cx: number, cy: number): number {
  return tiles.findIndex((t) => t[0] === cx && t[1] === cy);
}

/**
 * Give the tile at ribbon position `from` the position `to`. The list must
 * stay a permutation of the grid, so whichever cell held `to` takes `from`:
 * the two ENTRIES trade places, each cell keeping its own rotation.
 */
export function chainSwap(tiles: readonly WireTile[], from: number, to: number): WireTile[] {
  const out = tiles.map((t) => [t[0], t[1], t[2]] as const);
  const a = out[from];
  const b = out[to];
  if (a === undefined || b === undefined || from === to) return out;
  out[from] = b;
  out[to] = a;
  return out;
}

// Two rotations, one the INVERSE of the other (Gitea #920, Jeremy on the
// real wall: "the rotate button rotates the wrong direction"). The wire
// (`chain cx,cy,deg`, `tiles[i][2]`) carries the panel's MOUNT rotation —
// how far clockwise the module itself is turned. What the user watches is
// the PICTURE on that panel, and to stand a picture upright on a module
// turned 90° clockwise the remap turns the picture 90° ANTI-clockwise. So
// every place the console shows or edits a turn speaks the picture's
// rotation, `(360 − mount) % 360`, and only the wire speaks the mount's.

/** The picture rotation a panel shows for its wire (mount) rotation. */
export function pictureTurn(wireDeg: number): number {
  return normTurn(360 - normTurn(wireDeg));
}

/** The wire (mount) rotation that shows picture rotation `pictureDeg`. The
 *  same fold as `pictureTurn` — the inverse of a rotation is one — named
 *  separately so each call site says which way it is converting. */
export function wireTurn(pictureDeg: number): number {
  return normTurn(360 - normTurn(pictureDeg));
}

/** The same list with the tile at `at` turned so its PICTURE is rotated
 *  `pictureDeg` clockwise (stored as the mount rotation, `wireTurn`). */
export function chainTurn(tiles: readonly WireTile[], at: number, pictureDeg: number): WireTile[] {
  return tiles.map((t, i) =>
    i === at ? ([t[0], t[1], wireTurn(pictureDeg)] as const) : ([t[0], t[1], t[2]] as const),
  );
}

/** The next PICTURE rotation for the `↻ rotate` button: the picture on the
 *  panel turns a quarter clockwise on a square tile, a half turn otherwise
 *  (a quarter turn swaps a tile's axes, which the device refuses unless
 *  `pw == ph`). On the wire one press is `(mount + 270) % 360`. */
export function nextTurn(pictureDeg: number, square: boolean): number {
  return normTurn(normTurn(pictureDeg) + (square ? 90 : 180));
}

// ---- outputs (§5.3b: an output drives a consecutive run of ONE space) ----

/** A row of the Outputs table, with the range it computes for itself. */
export interface OutputRange {
  /** First index this output drives, 0-based (pixels) or 1-based (panels). */
  from: number;
  /** Last index, inclusive. */
  to: number;
}

/**
 * Where each output sits in the one index space. Pixels count from 0
 * (`pixels 300–599`), panels from 1 (`panels 3–4`) — the numbering each is
 * shown with elsewhere on the page.
 */
export function outputRanges(counts: readonly number[], unit: "pixels" | "panels"): OutputRange[] {
  let at = unit === "panels" ? 1 : 0;
  return counts.map((n) => {
    const count = Math.max(0, Math.round(n));
    const from = at;
    at += count;
    return { from, to: Math.max(from, at - 1) };
  });
}

/** Do the outputs partition the Layout exactly? The firmware refuses a POST
 *  that does not (docs/api.md, `/api/layout` validation), so the table says
 *  so before the user hits it. */
export function outputsSum(counts: readonly number[]): number {
  return counts.reduce((a, b) => a + Math.max(0, Math.round(b)), 0);
}

// ---- the external pattern-array arena (Gitea #253, #538) ----

/** `8 MB`, `512 KB` — the arena's size, with the `.0` of a whole number
 *  dropped so `8.0 MB` never reads as a measurement it is not. */
function arenaSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${mb % 1 === 0 ? mb.toFixed(0) : mb.toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

/**
 * The PSRAM readout: `8.0 MB free of 8 MB`.
 *
 * `present` was all the Storage row said until #538 round 2 — "show the
 * number" (Jeremy, 2026-09-20). `null` means the device advertises the arena
 * (`caps.psram`) but reports no figures for it: firmware older than the
 * `psram_free`/`psram_total` fields, where `present` is still the honest
 * answer. The FREE figure keeps one decimal even when whole, because it is a
 * live measurement and a moving `7.4` next to a fixed `8` is the point.
 */
export function psramLine(free: number, total: number): string | null {
  if (!(total > 0)) return null;
  const mb = free / (1024 * 1024);
  const f = mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.round(free / 1024)} KB`;
  return `${f} free of ${arenaSize(total)}`;
}
