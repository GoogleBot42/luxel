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
  /** HUB75 scan divisor. */
  panelScan: boolean;
  /** `Panels [c] across × [r] down`. */
  panelCounts: boolean;
  /** Start corner · run direction · snake. ONE row: it describes the chain
   *  through the tiles, or — at `cols`=`rows`=1 — the pixel run through the
   *  grid, which is the same widget one level down (§5.3, S3c/S3d). */
  wiringRow: boolean;
  /** …and that row reads as pixel wiring rather than a panel chain. */
  wiringIsPixels: boolean;
  /** Rotate alternate rows 180° — only meaningful with tiles to alternate. */
  rot180: boolean;
  /** The arrangement SVG: panel grid, chain path, per-tile scan direction. */
  arrangement: boolean;
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
  /** The Panel driver disclosure (clock, planes, live rescan). */
  panelDriver: boolean;
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
    panelScan: matrix && c.panel,
    panelCounts: matrix,
    wiringRow: matrix,
    wiringIsPixels: matrix && tiles === 1,
    rot180: matrix && tiles > 1,
    arrangement: matrix,
    estimatedRefresh: matrix && c.panel,
    outputsTable: c.outputs > 1,
    addOutput: c.outputs > 1,
    projection: layout.dims > 1,
    latticeFields: layout.kind === "lattice",
    powerCap: c.power_cap,
    blurGlow: c.blur_glow,
    blurGlowScope: layout.dims === 1 ? "strip" : "grid",
    panelDriver: c.panel,
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

/**
 * Estimated panel refresh in Hz.
 *
 * One BCM frame shifts `pw × panels` columns for each of `scan` row
 * addresses, and the MSB plane is rescanned `2^(planes-1)` times, so a whole
 * frame is `2^planes − 1` scans of the chain:
 *
 * ```text
 * Hz = clock / (pw · panels · stripes · scan · (2^planes − 1))
 * ```
 *
 * `stripes = (ph / 2) / scan` — a 1/N-scan panel has fewer address rows but
 * each one clocks out `stripes` copies of the chain, so the product is the
 * panel's pixel count whatever the scan (Gitea #764; `arrange::est_hz` on
 * the device says the same).
 *
 * That is the model the bench measurements fit exactly (Gitea #255, the
 * table in `firmware/src/hub75.rs`): one 64×64 panel at 1/32 scan and 7
 * planes reads 77 Hz at 20 MHz, 115 Hz at 30 MHz, 154 Hz at 40 MHz, and 58 Hz
 * at 8 planes — the numbers measured on the Seengreat panel.
 *
 * Returns 0 when the inputs cannot describe a panel.
 */
export function estimatedRefreshHz(
  a: RefreshInput,
  driver: PanelDriver = PANEL_DRIVER_DEFAULT,
): number {
  const pw = Math.max(0, Math.round(a.pw));
  const ph = Math.max(0, Math.round(a.ph));
  const panels = Math.max(0, Math.round(a.panels));
  const scan = a.scan > 0 ? Math.round(a.scan) : Math.floor(ph / 2);
  // A scan that does not divide ph/2 has no framebuffer (the device rejects
  // it); read it as one stripe rather than a fraction.
  const half = Math.floor(ph / 2);
  const stripes = scan > 0 && half >= scan && half % scan === 0 ? half / scan : 1;
  const planes = Math.max(1, Math.round(driver.planes));
  const clocks = pw * panels * stripes * scan * (2 ** planes - 1);
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
   *  ones, and `rot180` marks their tiles as mounted upside-down. */
  line: number;
  /** This tile's scan runs right-to-left (the snake's return leg, or a
   *  start corner on the right), i.e. it is drawn mirrored. */
  flipX: boolean;
  /** …and top-to-bottom is reversed. */
  flipY: boolean;
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
      });
    }
  }
  return tiles;
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
