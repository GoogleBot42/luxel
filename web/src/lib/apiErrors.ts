// The ONE table that turns a device's terse `{"ok":false,"error":…}` into a
// sentence (Gitea #538 round 2, #600).
//
// The firmware and the mirror share `luxel_core::layout`'s grammar errors, so
// their wording is one vocabulary — `pw*ph*cols*rows out of range for this
// board` is the device naming a variable, not explaining anything. Every one
// of those strings gets a sentence here, built from what the FORM knows: the
// board's ceiling, the geometry the user asked for, the field it belongs to.
//
// Rules this module keeps:
//
//  * Pure. No stores, no DOM, no fetch — `web/tests/apiErrors.test.mjs`
//    drives it over fixtures.
//  * The device's own words are never thrown away: they come back as
//    `details`, which the banner prints under the sentence.
//  * A message this table does not know still gets a scope sentence and the
//    raw text, never a blank banner.
//  * `field` is a `data-role`, so the banner can highlight the control the
//    message is about without any form knowing it was mentioned.

/** Which form raised it — decides the fallback sentence and the default field. */
export type ApiErrorScope =
  | "layout"
  | "name"
  | "wifi"
  | "mqtt"
  | "clock"
  | "output"
  | "playlist"
  | "pattern"
  /** The Scenes page and the scene editor (Gitea #480). */
  | "scene"
  /** The Sprites page and the sprite editor (Gitea #740). */
  | "sprite"
  /** Installing a firmware image or a release package (Gitea #643). */
  | "ota"
  | "device";

export interface ApiErrorContext {
  scope: ApiErrorScope;
  /** The board's pixel ceiling (`/api/status` `max_pixels`), when known. */
  maxPixels?: number;
  /** The panel chain the rejected body described, when it was a `matrix`. */
  chain?: { pw: number; ph: number; cols: number; rows: number };
  /** The pixel count the rejected body asked for, when it was a `strip`. */
  pixels?: number;
  /** The `data-role` to highlight when the message does not imply one. */
  field?: string;
  /** 1-based line of the rejected body, when the reply carried one. */
  line?: number;
  /** What the action was about, for the scopes that name a thing
   *  (`Couldn't save "Aurora 2D"`). */
  subject?: string;
  /** The raw message. `explainApiError` fills this in itself, so an entry can
   *  read numbers back out of the device's own words without every caller
   *  having to repeat them. */
  raw?: string;
}

export interface ApiErrorExplained {
  /** The sentence the banner leads with. */
  text: string;
  /** The device's own words (plus the line it pointed at), kept verbatim. */
  details: string;
  /** `data-role` of the control this is about, when there is one. */
  field?: string;
}

/** `4096` → `4,096`. Fixed grouping, so the tests do not depend on a locale. */
export function group(n: number): string {
  return String(Math.round(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * A chain of `cols × rows` tiles of `pw × ph` that fits under `cap`, keeping
 * the tile count and halving the panel's longer side until it does. `null`
 * when even one tile per position cannot be made to fit.
 *
 * This is the "you could do it like this instead" half of the HUB75 ceiling
 * message: the constraint is the frame buffer, so the answer is smaller
 * tiles, not fewer of them.
 */
export function fitTiles(
  chain: { pw: number; ph: number; cols: number; rows: number },
  cap: number,
): { pw: number; ph: number } | null {
  const tiles = Math.max(1, chain.cols * chain.rows);
  let { pw, ph } = chain;
  for (let guard = 0; guard < 12; guard++) {
    if (pw * ph * tiles <= cap) return { pw, ph };
    if (pw >= ph && pw % 2 === 0) pw /= 2;
    else if (ph % 2 === 0) ph /= 2;
    else return null;
  }
  return null;
}

interface Entry {
  /** Matches the device's raw `error` text. */
  match: RegExp;
  /** `data-role` this message is about. */
  field?: string;
  text: (c: ApiErrorContext) => string;
}

/**
 * The pixel ceiling, in full (#600) — for a panel and for a strip.
 *
 * The number is the board's own `max_pixels`, and the reason above it is
 * always RAM: a bigger pixel space needs buffers that have to be internal SRAM
 * (the panel's bitplane DMA frame buffers and descriptor rings; a strip's
 * protocol encode buffer), and those are sized from the layout. Since #768 the
 * cap is NOT the same thing as "the biggest chain that will actually boot":
 * the firmware sizes its framebuffers from the stored layout and falls back to
 * the default panel when they will not fit, so a chain can be under the cap
 * and still be refused at boot. This message explains the cap; the device's
 * own words (kept as `details`) explain a boot-time refusal.
 */
function pixelCeiling(c: ApiErrorContext): string {
  const cap = c.maxPixels ?? 0;
  const want = c.chain
    ? c.chain.pw * c.chain.ph * c.chain.cols * c.chain.rows
    : (c.pixels ?? 0);
  const head =
    want > 0 && cap > 0
      ? `${group(want)} px — this board tops out at ${group(cap)}.`
      : cap > 0
        ? `This board tops out at ${group(cap)} px.`
        : "That is more pixels than this board can drive.";
  const why =
    "The ceiling is RAM: the buffers a bigger pixel space needs have to live in internal " +
    "SRAM — the panel's bitplane DMA frame buffers and descriptor rings, a strip's encode " +
    "buffer — and the DMA cannot read them from PSRAM fast enough. It is a firmware limit, " +
    "not a setting.";
  const fit = c.chain && cap > 0 ? fitTiles(c.chain, cap) : null;
  const tiles = c.chain ? c.chain.cols * c.chain.rows : 0;
  const how =
    fit && c.chain && (fit.pw !== c.chain.pw || fit.ph !== c.chain.ph)
      ? ` At this size you can arrange ${countWord(tiles)} ${fit.pw}×${fit.ph} tiles ` +
        `(\`matrix ${fit.pw} ${fit.ph} ${c.chain.cols} ${c.chain.rows} tr row 0 0\`).`
      : "";
  return `${head} ${why}${how}`;
}

/** Spelled out to sixteen — the most tiles the 16384-px cap can hold at the
 *  32×32 the suggestion halves down to (#768). Digits past that. */
function countWord(n: number): string {
  return (
    [
      "no",
      "one",
      "two",
      "three",
      "four",
      "five",
      "six",
      "seven",
      "eight",
      "nine",
      "ten",
      "eleven",
      "twelve",
      "thirteen",
      "fourteen",
      "fifteen",
      "sixteen",
    ][n] ?? String(n)
  );
}

/** The table. Order matters only where two patterns could both match. */
const TABLE: Entry[] = [
  // ---- LED layout (crates/luxel-core/src/layout.rs) ----
  { match: /^pw\*ph\*cols\*rows out of range/, field: "layout-cols", text: pixelCeiling },
  { match: /^grid is larger than this board's pixel ceiling/, field: "layout-pw", text: pixelCeiling },
  { match: /^pixels out of range for this board/, field: "layout-pixels", text: pixelCeiling },
  {
    match: /^grid is wider or taller than 65535/,
    field: "layout-pw",
    text: () => "A grid side has to be under 65,535. Check the panel width and height.",
  },
  {
    match: /^this board is a matrix: strip is not a Layout it can take/,
    field: "layout-kind",
    text: () =>
      "This board drives a HUB75 matrix, so it cannot be configured as a strip. " +
      "Its shape is the panel chain below.",
  },
  {
    match: /^this board has no configurable strip output/,
    field: "layout-kind",
    text: () => "This board has no addressable-strip output to configure.",
  },
  {
    match: /^output index is past this board's output count/,
    field: "outputs",
    text: () => "That output does not exist on this board — it has fewer physical outputs.",
  },
  {
    match: /^duplicate output index/,
    field: "outputs",
    text: () => "Two rows claim the same output. Each output may appear once.",
  },
  {
    match: /^two outputs cannot share a data pin/,
    field: "outputs",
    text: () => "Two outputs are set to the same data pin. Give each one its own GPIO.",
  },
  {
    match: /^output count must be at least 1/,
    field: "outputs",
    text: () => "An output has to drive at least one pixel. Remove it, or give it a count.",
  },
  {
    match: /^pin is reserved or not an output on this board/,
    field: "layout-datapin",
    text: () =>
      "That GPIO is reserved or cannot drive data on this board. " +
      "Pick one of the pins the board advertises.",
  },
  {
    match: /^pin must be one of data_pins/,
    field: "layout-datapin",
    text: () => "That GPIO is not one this board offers for strip data.",
  },
  {
    match: /^only one strip\/matrix\/map line per body/,
    text: () =>
      "The console sent two shape lines at once. This is a bug in the app, not something " +
      "you did — please report it.",
  },
  // ---- the `panel` line (Gitea #401/#525, the list #771) ----
  //
  // The card's clock is a `<select>` over the device's own `driver.clocks`, so
  // a refusal here means the two disagree about the list — a version skew, not
  // a typo. Say what the device takes (its message names the whole list) and
  // point at the field.
  {
    match: /^panel: clock_mhz must be/,
    field: "panel-clock",
    text: (c) => {
      const list = /one of (\S+)/.exec(c.raw ?? "")?.[1]?.split("|").join(", ");
      return list
        ? `This device only takes these pixel clocks: ${list} MHz. Pick one of those — ` +
          "the list the console offered is from a different firmware."
        : "The device refused that pixel clock. Pick one of the values it offers.";
    },
  },
  { match: /^panel: planes must be/, field: "panel-planes", text: () =>
      "The device takes 4 to 8 bit planes. Pick one of the values it offers." },
  { match: /^panel: blank must be/, field: "panel-blank", text: () =>
      "Latch blanking is 0 to 8 clocks on this device." },
  {
    // Latch blanking applies LIVE (Gitea #778), so the device judges it against
    // the row block it is RUNNING and refuses one that would swallow the whole
    // OE window: `panel: blank 8 + 3 latch clocks leave no lit clock in a
    // 16-word row block`. The numbers are in `details`, so this is the "why".
    match: /^panel: blank \d+ \+ /,
    field: "panel-blank",
    text: () =>
      "That much blanking would leave no lit clock at all in this panel's row block — the " +
      "whole picture would go black. Lower it, or use a wider panel.",
  },
  {
    // The slider spans 1..W and the top posts `0`, so a refusal here is a
    // console/firmware skew rather than something the user typed (Gitea
    // #460/#789).
    match: /^panel: lsb must be/,
    field: "panel-lsb",
    text: () =>
      "The device takes an LSB on-time of 0 to 65,535 pixel clocks, 0 meaning the full " +
      "on-time. Move the Refresh control back to ×1 and try again.",
  },
  {
    // The boot-heap refusal (Gitea #822): the device predicted what this
    // panel would take out of internal SRAM at boot and it does not leave
    // enough for WiFi, the web server and the engine. `this panel would leave
    // 12,345 B of heap at boot (floor 65,536 B) — it cannot be driven on this
    // board`. The device's own numbers are the whole explanation, so they are
    // quoted back rather than paraphrased; `details` carries the line verbatim
    // as well.
    //
    // It is NOT the pixel ceiling: a chain can be well under `max_pixels` and
    // still be refused here, because the ceiling counts pixels and this counts
    // the bytes THIS board has left at boot (#768's cap raise is exactly what
    // made the two diverge).
    match: /^this panel would leave \d+ B of heap at boot/,
    field: "layout-cols",
    text: (c) => {
      const m = /leave (\d+) B of heap at boot \(floor (\d+) B\)/.exec(c.raw ?? "");
      const left = m ? group(Number(m[1])) : null;
      const floor = m ? group(Number(m[2])) : null;
      const nums =
        left && floor
          ? `it would leave ${left} bytes where the rest of the boot needs ${floor}`
          : "it would not leave enough for the rest of the boot";
      return (
        `This board does not have the internal memory to drive that panel — ${nums}. ` +
        "The frame buffers are sized from the panel and the chain, so use fewer bit " +
        "planes, a smaller panel, or fewer panels. Nothing was changed."
      );
    },
  },
  { match: /^panel: chip must be/, field: "panel-chip", text: () =>
      "This firmware does not know that driver chip. The list in the card is the " +
      "device's own (`driver.chips`), so this means the two disagree — push matching firmware." },
  {
    // The rolled-back-firmware case, and the one that must NEVER read as an
    // app bug (#771): a device whose grammar has no `panel` verb answers
    // `unknown line (want strip|matrix|map|out|proj…)`, and the want-list is
    // the device telling us exactly which verbs it has.
    match: /^unknown line \(want /,
    text: (c) => {
      const want = (/\(want ([^)]*)\)/.exec(c.raw ?? "")?.[1] ?? "").split("|");
      const missing = ["panel", "proj", "out"].filter((v) => !want.includes(v));
      if (missing.length > 0)
        return (
          "This device's firmware is older than this console — its LED layout grammar has no " +
          `\`${missing[0]}\` line. Push matching firmware: Settings › Firmware & recovery.`
        );
      return (
        "The device did not understand what the console sent. This is a bug in the app, " +
        "not something you did — please report it."
      );
    },
  },
  {
    match: /^(unknown line|expected|expected one of)/,
    text: () =>
      "The device did not understand what the console sent. This is a bug in the app, " +
      "not something you did — please report it.",
  },
  // ---- scenes (crates/luxel-core/src/scene.rs, Gitea #480) ----
  // The console serializes the block the device parses, so a `scene: line N`
  // is the APP's bug and says so. `scenes: store full` is the user's to act
  // on: every scene shares ONE 3840-byte blob, and the numbers say by how
  // much this save misses.
  {
    match: /^scenes: store full/,
    field: "scene-save",
    text: (c) => {
      const m = /\((\d+) of (\d+) B\)/.exec(c.raw ?? "");
      const want = m ? Number(m[1]) : 0;
      const cap = m ? Number(m[2]) : 3840;
      const over = want > cap ? ` — ${group(want - cap)} B over` : "";
      return (
        `Every scene on this device shares ${group(cap)} bytes of storage, and this save ` +
        `needs ${group(want)}${over}. Delete a scene, or take a layer out of this one.`
      );
    },
  },
  {
    match: /^scene: layer \d+ does not fit/,
    field: "scene-layers",
    text: () =>
      "One layer's box falls outside the fixture. Move it back onto the grid, or set its " +
      "width and height to 0 so it covers the whole thing.",
  },
  {
    // The scene store is written from an HTTP handler on whatever heap the
    // RUNNING scene left — two pattern layers on the Seengreat panel leave
    // ~10 KB. The device now refuses the save instead of rebooting on it
    // (Gitea #724), and the way out is to stop the scene first.
    match: /^scenes: not enough memory/,
    field: "scene-save",
    text: (c) => {
      const m = /\((\d+) B free\)/.exec(c.raw ?? "");
      const free = m ? ` — ${group(Number(m[1]))} bytes left` : "";
      return (
        `The device did not have the memory to write its scene store${free}. ` +
        "The scene that is playing is holding most of it: stop it (or play a lighter one) " +
        "and save again. Nothing was changed."
      );
    },
  },
  {
    match: /^scene: line \d+:/,
    field: "scene-save",
    text: () =>
      "The device could not read the scene the console sent. This is a bug in the app, not " +
      "something you did — please report it.",
  },
  // ---- stores and persistence, on every endpoint ----
  {
    match: /store refused to persist/,
    text: () =>
      "The change is running now, but the device could not write it to flash — it will be " +
      "gone after a reboot. Its storage may be full: delete a pattern and try again.",
  },
  {
    match: /^no such/,
    text: (c) =>
      c.subject
        ? `The device no longer has “${c.subject}”. Someone may have deleted it, or it was never saved.`
        : "The device no longer has that item.",
  },
  {
    match: /^(device unreachable|the device did not answer|failed to fetch|load failed)/i,
    text: (c) =>
      c.subject
        ? `Couldn't reach the device, so “${c.subject}” was not applied.`
        : "Couldn't reach the device, so nothing was applied.",
  },
];

/** What a scope says when the table has no entry for the raw message. */
const FALLBACK: Record<ApiErrorScope, string> = {
  layout: "The device refused this LED layout and kept the one it had.",
  name: "The device refused this name and kept the one it had.",
  wifi: "The device refused these WiFi settings and stayed on the network it was on.",
  mqtt: "The device refused these MQTT settings.",
  clock: "The device refused this clock setting.",
  output: "The device refused this output setting.",
  playlist: "The device refused this playlist change and kept the one it had.",
  pattern: "The device refused this pattern.",
  scene: "The device refused this scene and kept the one it had.",
  sprite: "The device refused this sprite and kept the one it had.",
  ota: "The release was not installed; the device is running what it was.",
  device: "The device refused the request.",
};

/**
 * Translate one `{"ok":false,"error":…,"line":N}` reply.
 *
 * Nothing is ever dropped: the sentence is the table's (or the scope's
 * fallback), and the device's own words — with the line it pointed at — are
 * the `details` the banner prints underneath.
 */
export function explainApiError(raw: string, ctx: ApiErrorContext): ApiErrorExplained {
  const msg = (raw ?? "").trim();
  const hit = TABLE.find((e) => e.match.test(msg));
  const details = ctx.line ? `${msg || "no message"} (line ${ctx.line})` : msg || "no message";
  const c = { ...ctx, raw: msg };
  return {
    text: hit ? hit.text(c) : FALLBACK[ctx.scope],
    details,
    field: hit?.field ?? ctx.field,
  };
}
