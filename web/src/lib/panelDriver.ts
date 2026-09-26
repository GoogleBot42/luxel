// LED layout › Panel module — the pure half (Gitea #401/#525/#778).
//
// The disclosure is INSIDE the LED layout card, not in Advanced (Jeremy,
// 2026-09-26): "The panel driver section doesn't belong in Advanced. That
// dropdown belongs closer or in the LED layout section. It's not something
// people can optionally configure, but once it is configured they probably
// won't touch it again, so being collapsed still makes sense." So it is one
// collapsed row under the arrangement, carrying the scan rate as well as the
// four `panel` fields — everything printed on the back of a module.
//
// The HUB75 driver's bit depth, pixel clock, chip init sequence and latch
// blanking are DEVICE SETTINGS now, not this build's constants: `/api/layout`
// reports them as a `driver` block and takes them back as one wire line,
//
//     panel <planes> <clock_mhz> <chip> <blank>
//
// which the firmware merges into the stored Layout and applies at the next
// boot. So there are two readings of the same four values — the CONFIGURED
// one the form edits and the LIVE one the DMA is running — and the card's job
// is to say which is on the panel right now. That decision lives here,
// Svelte-free and fetch-free, so it is unit tested against fixtures
// (`web/tests/panelDriver.test.mjs`) rather than eyeballed on a board: the
// interesting states (a configured driver that did not fit, panel output that
// never came up) are exactly the ones a healthy bench panel never shows.
//
// Firmware without the `panel` line reports no `driver` at all. That is NOT a
// supported reading of the card any more (Gitea #771): a panel board whose
// firmware carries no `driver` block is a console/firmware mismatch and the
// card says so, because the read-only plaque it used to draw made a rollback
// look like a fixed setting. `PANEL_DRIVER_DEFAULT` survives here only as the
// refresh ESTIMATE's fallback, which the LED layout card still needs.

import type { LayoutWire, LiveDriverWire, PanelDriverWire } from "./device";
import {
  estimatedRefreshHz,
  PANEL_DRIVER_DEFAULT,
  type PanelDriver,
  type RefreshInput,
} from "./settingsCaps.ts";

/** The `panel` line's own four values (the wire's spelling, so a line is
 *  built by writing them out in order and nothing has to be mapped). */
export interface PanelDriverConfig {
  planes: number;
  clock_mhz: number;
  chip: string;
  blank: number;
}

/** Bit depth: 4..8, and the refresh halves per extra plane. */
export const PLANE_CHOICES: readonly number[] = [4, 5, 6, 7, 8];

/**
 * The pixel clocks to offer when the device does not say (firmware between
 * #525 and #771 reports a `driver` block with no `clocks`).
 *
 * A FIXED LIST, not a range — Gitea #771. It is `PanelDriver::CLOCKS` in
 * `crates/luxel-core/src/layout.rs`: every rate esp-hal can reach with an
 * integer LCD_CAM divider (`source / (2·N)` off XTAL 40 MHz or PLL_D2
 * 240 MHz), capped at the FM6124 datasheet's 30 MHz and floored at 8, below
 * which a 7-plane 64×64 rescan flickers. The device's own list always wins.
 */
export const CLOCK_CHOICES_DEFAULT: readonly number[] = [8, 10, 12, 15, 20, 24, 30];

/** The top of the list: the FM6124 datasheet ceiling, with no margin. Above it
 *  the bench panel's two halves mis-sampled at 40 MHz — which is why nothing
 *  above 30 is offered at all (`firmware/src/hub75.rs`). */
export const CLOCK_CEILING_MHZ = 30;

/** Latch blanking clocks: OE off for N clocks at the start of a row block and
 *  again before the latch word. 1 is the stock template.
 *
 *  The one panel field that applies LIVE (Gitea #778) — it is control bits in
 *  the framebuffer words, so the firmware re-formats its buffers between
 *  frames. It is therefore the only one the user tunes by LOOKING at the
 *  panel, which is why it must not cost a reboot per attempt. Nothing here
 *  calls `noteRebootPending` for it; the device's `reboot_required` reply says
 *  `false` and `panelDriverState` leaves it out of the live comparison. */
export const BLANK_MIN = 0;
export const BLANK_MAX = 8;

/** The driver every board boots with, as the `panel` line spells it. */
export const PANEL_DRIVER_LINE_DEFAULT: PanelDriverConfig = {
  planes: PANEL_DRIVER_DEFAULT.planes,
  clock_mhz: Math.round(PANEL_DRIVER_DEFAULT.clockHz / 1e6),
  chip: "shiftreg",
  blank: 1,
};

/** What each chip value means, for the select. A chip the firmware names and
 *  this table does not is shown verbatim rather than hidden — the device's
 *  list is the authority (`driver.chips`). */
export const CHIP_LABELS: Record<string, string> = {
  shiftreg: "Plain shift register (FM6124, SM16208, ICN2037…)",
  fm6126a: "FM6126A (register init)",
  icn2038s: "ICN2038S (register init)",
  dp3246: "DP3246 (register init, 3-clock latch)",
};

/** The same four, short enough to sit inside a sentence about the live driver. */
export const CHIP_SHORT: Record<string, string> = {
  shiftreg: "plain shift register",
  fm6126a: "FM6126A",
  icn2038s: "ICN2038S",
  dp3246: "DP3246",
};

export function chipLabel(chip: string): string {
  return CHIP_LABELS[chip] ?? chip;
}

export function chipShort(chip: string): string {
  return CHIP_SHORT[chip] ?? chip;
}

/** The `driver` block, or `null` on firmware that does not carry one. */
export function driverWire(wire: LayoutWire | null): PanelDriverWire | null {
  return wire?.driver ?? null;
}

/** The CONFIGURED driver: the device's stored values when it reports them,
 *  this build's constants otherwise. */
export function configuredDriver(wire: LayoutWire | null): PanelDriverConfig {
  const d = driverWire(wire);
  if (!d) return { ...PANEL_DRIVER_LINE_DEFAULT };
  return { planes: d.planes, clock_mhz: d.clock_mhz, chip: d.chip, blank: d.blank };
}

/** The clock/planes pair `estimatedRefreshHz` takes, for the CONFIGURED
 *  driver — so the estimate beside the measured rescan describes what the
 *  form says, which is the whole point of showing both. */
export function refreshDriver(wire: LayoutWire | null): PanelDriver {
  const d = configuredDriver(wire);
  return { clockHz: d.clock_mhz * 1e6, planes: d.planes };
}

/**
 * The rescan estimate to show for a matrix Layout, Hz.
 *
 * Where the device reports a `driver` block it has TOLD the browser its clock
 * and bit depth, so the estimate is computed here from the configured values:
 * the same formula the firmware runs, over inputs the form can change, which
 * means the number cannot lag the field the user just edited. Without a block
 * the device's own `est_hz` wins (#475) — it knows constants the browser can
 * only assume — and `PANEL_DRIVER_DEFAULT` is the last resort.
 */
export function panelRefreshHz(wire: LayoutWire | null, a: RefreshInput): number {
  const est = wire?.matrix?.est_hz;
  if (driverWire(wire) === null && typeof est === "number") return est;
  return estimatedRefreshHz(a, refreshDriver(wire));
}

/** The one write: `panel <planes> <clock_mhz> <chip> <blank>`. A POST carries
 *  this line alone — the firmware merges it into the stored Layout, so the
 *  matrix line does not have to be resent (and must not be, or a concurrent
 *  edit elsewhere in the form would be clobbered by a stale copy). */
export function panelLine(d: PanelDriverConfig): string {
  return `panel ${d.planes} ${d.clock_mhz} ${d.chip} ${d.blank}`;
}

/**
 * The clocks the device takes, ascending — its own `clocks` when it reports
 * one, this build's list otherwise.
 *
 * The CONFIGURED value is always included even when it is not on the list, so
 * a device that stored 40 MHz under older firmware shows 40 rather than
 * silently reading as something it is not. `clockSupported()` is how the card
 * marks that option.
 */
export function clockChoices(driver: PanelDriverWire | null): number[] {
  const offered = driver?.clocks?.length ? driver.clocks : CLOCK_CHOICES_DEFAULT;
  const list = new Set<number>(offered);
  if (driver) list.add(driver.clock_mhz);
  return [...list].sort((a, b) => a - b);
}

/** Whether the device says it accepts `mhz` (as opposed to merely holding it). */
export function clockSupported(driver: PanelDriverWire | null, mhz: number): boolean {
  const offered = driver?.clocks?.length ? driver.clocks : CLOCK_CHOICES_DEFAULT;
  return offered.includes(mhz);
}

/** Snap a clock to the nearest OFFERED value, so the form cannot post a line
 *  the device is going to refuse. Ties go to the lower (safer) clock. */
export function snapClock(mhz: number, driver: PanelDriverWire | null): number {
  const offered = driver?.clocks?.length ? [...driver.clocks] : [...CLOCK_CHOICES_DEFAULT];
  const list = offered.sort((a, b) => a - b);
  const want = Number(mhz);
  if (!Number.isFinite(want)) return list[list.length - 1] ?? CLOCK_CEILING_MHZ;
  let best = list[0] ?? CLOCK_CEILING_MHZ;
  for (const c of list) if (Math.abs(c - want) < Math.abs(best - want)) best = c;
  return best;
}

/** Same for the blanking clocks. */
export function clampBlank(n: number): number {
  const v = Math.round(Number(n) || 0);
  return Math.min(BLANK_MAX, Math.max(BLANK_MIN, v));
}

/** The configured arrangement the framebuffer is sized from — the `matrix`
 *  line's numbers, reduced to what `live` reports back. */
export interface PanelGeometry {
  pw: number;
  ph: number;
  /** Tiles in the chain (`cols × rows`). */
  chain: number;
  /** The scan divisor as stored; 0 = the board's own, which is `ph / 2`. */
  scan: number;
}

/** The configured arrangement a Layout reports, or null when it is not a
 *  matrix (nothing to size a framebuffer from). */
export function panelGeometryOf(wire: LayoutWire | null): PanelGeometry | null {
  const m = wire?.matrix;
  if (!m) return null;
  return { pw: m.pw, ph: m.ph, chain: Math.max(1, m.cols * m.rows), scan: m.scan };
}

/** The address rows a `PanelGeometry` boots with (`scan` 0 = `ph / 2`). */
export function effectiveScan(g: PanelGeometry): number {
  return g.scan > 0 ? Math.round(g.scan) : Math.floor(g.ph / 2);
}

// ---- the scan rate (Gitea #778) ------------------------------------------
//
// The field used to offer `board default` for `scan 0`. Jeremy, 2026-09-26:
// "Is that even possible for luxel to know?" — it is NOT. HUB75 is a
// write-only bus: the firmware cannot ask a module anything, and `0` does not
// mean "ask the board", it means "the usual ratio for this height", `ph / 2`.
// So the options are the RATIOS THEMSELVES, the way they are printed on the
// back of a module (`1/32S`), with the usual one for the configured height
// marked as such. `Standard (1/32 for a 64-tall panel)` was tried and
// rejected as confusing: short and concrete instead.

/** The scan ratios this form offers, largest first — a fixed candidate set the
 *  panel height then filters. */
const SCAN_CANDIDATES: readonly number[] = [32, 16, 8, 4];

/** One entry of the Scan rate select. */
export interface ScanOption {
  /** Address rows, i.e. the `N` of `1/N`. */
  scan: number;
  /** `1/32`, or `1/32 (usual for 64 rows)` for the `ph / 2` one. */
  label: string;
  /** Is this the ratio a `ph`-tall module normally has? */
  usual: boolean;
}

/**
 * The scan ratios a `ph`-tall panel can be told to run at, largest first.
 *
 * A HUB75 panel shifts two half-height rows at once, so its full address depth
 * is `ph / 2`; a shallower `scan` stripes the framebuffer `(ph/2)/scan` ways
 * and therefore has to DIVIDE it exactly (the firmware refuses anything else —
 * docs/api.md). So a 64-row module offers 1/32, 1/16, 1/8 and 1/4, and a
 * 32-row one does not offer 1/32 at all: 32 does not divide 16.
 *
 * `cur` is the stored value (`0` = the usual one) and is always included, so a
 * device holding a ratio this list does not carry still shows it rather than
 * silently reading as something else.
 */
export function scanOptions(ph: number, cur = 0): ScanOption[] {
  const half = Math.max(1, Math.floor(Math.max(0, Math.round(ph)) / 2));
  const want = new Set<number>([...SCAN_CANDIDATES, half]);
  if (cur > 0) want.add(Math.round(cur));
  return [...want]
    .filter((s) => s > 0 && s <= half && half % s === 0)
    .sort((a, b) => b - a)
    .map((s) => ({
      scan: s,
      usual: s === half,
      label: s === half ? `1/${s} (usual for ${half * 2} rows)` : `1/${s}`,
    }));
}

/**
 * The number to PUT ON THE WIRE for a picked ratio.
 *
 * Picking the usual one sends `0`, which is what makes it follow a later
 * height change instead of pinning the panel to today's number — the same
 * reason `scan 0` exists on the wire at all. Anything else sends the ratio.
 */
export function scanWire(ph: number, picked: number): number {
  const half = Math.max(1, Math.floor(Math.max(0, Math.round(ph)) / 2));
  return Math.round(picked) === half ? 0 : Math.round(picked);
}

/** The scan ratio the select is SHOWING: the stored one, or the usual one for
 *  this height when nothing is pinned. */
export function scanShown(ph: number, scan: number): number {
  return effectiveScan({ pw: 0, ph, chain: 1, scan });
}

/**
 * What the panel is doing relative to what is stored.
 *
 * - `unknown` — the device reports no `driver` block (firmware before #525).
 *   On a panel board that is a console/firmware MISMATCH: the card says so and
 *   offers nothing, rather than drawing a plaque of this build's constants
 *   (Gitea #771).
 * - `disabled` — `live` is null: panel output is off. Nothing is being
 *   shifted out, whatever is stored.
 * - `fallback` — the configured geometry/driver did not fit, so the board
 *   default is running. The user has to make it smaller, not reboot.
 * - `pending` — stored and different from live: reboot to apply.
 * - `live` — the panel is running exactly what the form shows.
 */
export type PanelDriverStatus = "unknown" | "disabled" | "fallback" | "pending" | "live";

export interface PanelDriverState {
  status: PanelDriverStatus;
  /** Which settings the running driver does not have, in the order the card
   *  lists them. Empty unless something differs — including in `fallback`,
   *  where it is what the board refused. */
  changed: readonly string[];
  /** The running driver in one phrase, `""` when there is none. */
  live: string;
}

/** `7 planes · 30 MHz · plain shift register · blanking 1 · 64×64 1/32`. */
export function liveSummary(live: LiveDriverWire | null): string {
  if (!live) return "";
  return [
    `${live.planes} planes`,
    `${live.clock_mhz} MHz`,
    chipShort(live.chip),
    `blanking ${live.blank}`,
    `${live.w}×${live.h} 1/${live.scan}`,
  ].join(" · ");
}

/**
 * The card's state, from the `driver` block and the configured arrangement.
 *
 * `geom` null means the Layout is not a matrix (or has not arrived yet): the
 * four driver values are still compared, the geometry simply is not.
 */
export function panelDriverState(
  driver: PanelDriverWire | null,
  geom: PanelGeometry | null,
): PanelDriverState {
  if (!driver) return { status: "unknown", changed: [], live: "" };
  const live = driver.live;
  if (!live) return { status: "disabled", changed: [], live: "" };
  const changed: string[] = [];
  if (driver.planes !== live.planes) changed.push("bit planes");
  if (driver.clock_mhz !== live.clock_mhz) changed.push("the pixel clock");
  if (driver.chip !== live.chip) changed.push("the driver chip");
  // `blank` is deliberately NOT compared (Gitea #778): the firmware applies it
  // on its next frame, so `live.blank` legitimately lags the reply to the POST
  // that changed it by one frame. Comparing it here would put "reboot to
  // apply" under the one control that does not need one — and the whole point
  // of making blanking live is that it is tuned by watching the panel.
  if (geom) {
    const w = Math.max(0, Math.round(geom.pw)) * Math.max(1, Math.round(geom.chain));
    if (live.w !== w || live.h !== Math.round(geom.ph)) changed.push("the panel size");
    if (live.scan !== effectiveScan(geom)) changed.push("the panel scan");
  }
  const summary = liveSummary(live);
  if (live.fallback) return { status: "fallback", changed, live: summary };
  return { status: changed.length > 0 ? "pending" : "live", changed, live: summary };
}

/** `a`, `a and b`, `a, b and c` — the same joining the reboot bar does. */
export function phrase(list: readonly string[]): string {
  if (list.length <= 1) return list[0] ?? "";
  return `${list.slice(0, -1).join(", ")} and ${list[list.length - 1]}`;
}

/**
 * The collapsed Panel module row: `1/32 scan · plain shift register · 20 MHz ·
 * 7 planes · blanking 1`.
 *
 * Everything printed on the back of a module, in that order, from the
 * CONFIGURED values — and then a word when the configured reading and the
 * running one cannot agree, because a collapsed row is the only thing a user
 * who never opens it reads. The refresh rate is NOT in here: the estimated /
 * measured readout sits immediately under this row in the LED layout card
 * (Gitea #778 moved it there), so repeating it would say the same number twice.
 */
export function panelModuleLine(
  wire: LayoutWire | null,
  state: PanelDriverState = panelDriverState(driverWire(wire), panelGeometryOf(wire)),
): string {
  const g = panelGeometryOf(wire);
  const scan = g ? `1/${effectiveScan(g)} scan` : "";
  // No `driver` block: the row states a firmware mismatch, and must not state
  // this build's constants as though they were the device's — plausible
  // numbers in a row nobody opens is exactly how the #771 rollback read as
  // "the settings are fine".
  if (state.status === "unknown") {
    return [scan, "firmware too old"].filter(Boolean).join(" · ");
  }
  const d = configuredDriver(wire);
  const bits = [
    scan,
    chipShort(d.chip),
    `${d.clock_mhz} MHz`,
    `${d.planes} planes`,
    `blanking ${d.blank}`,
  ].filter(Boolean);
  if (state.status === "disabled") bits.push("output off");
  else if (state.status === "fallback") bits.push("not applied");
  else if (state.status === "pending") bits.push("reboot to apply");
  return bits.join(" · ");
}
