// Device-mode e2e: the built playground in real chromium, connected to
// `luxel serve` (the native mirror of the firmware API). Verifies connect,
// editor sync from the device, live-code push, preview streaming, controls,
// vars, compile errors, and disconnect.
//
// Usage (from web/): npm run build && node tools/device-e2e.mjs

import { execSync, spawn } from "node:child_process";
import dgram from "node:dgram";
import puppeteer from "puppeteer-core";
import {
  acceptDialog,
  cancelDialog,
  dialogTitle,
  disabledSweep,
  menuClick,
  menuHas,
  NO_NETIN,
  PORT as E2E,
  renameTo,
  saveState,
  waitDialog,
} from "./e2e-common.mjs";
import { lxpBody } from "./lxp.mjs";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

const PORT = E2E.web.device; // E2E_PORT + 2 (see tools/e2e-common.mjs)
/** Where screenshots land (matching e2e.mjs's convention). */
const shotDir = process.argv[2] ?? "/tmp";
const DEV_PORT = E2E.mirror.device; // E2E_PORT + 20
const DEV = `http://127.0.0.1:${DEV_PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The Patterns page (#467) replaced the Device Patterns tab: one page, one
// grid per SOURCE, all mounted with the inactive ones hidden — so a tile
// selector always names its source's grid.
const DGRID = '[data-role="patterns-grid"][data-source="device"]';
const DTILE = `${DGRID} .tile`;

/**
 * Expand one Advanced disclosure on the Settings page (A8, Gitea #469).
 * Idempotent. Its body is UNMOUNTED while collapsed, so every field inside
 * one — output processing, clock, sync, MQTT, network input, storage,
 * firmware — needs this before it can be driven or even queried.
 */
async function openAdv(page, role) {
  const sel = `[data-role="${role}-toggle"]`;
  await page.waitForSelector(sel, { timeout: 8000 });
  const open = await page.$eval(sel, (el) => el.getAttribute("aria-expanded") === "true");
  if (!open) await page.$eval(sel, (el) => el.click()); // the panel may be hidden
  await page.waitForSelector(`[data-role="${role}-body"]`, { timeout: 4000 });
}

/** Scroll the Settings panel (it is its own scroll container, so puppeteer's
 *  `fullPage` sees only the viewport) and screenshot it. */
async function shotSettings(pg, path, to = 0) {
  await pg.$eval(
    '[data-role="settings-panel"]',
    (el, y) => {
      el.scrollTop = y < 0 ? el.scrollHeight : y;
    },
    to,
  );
  await new Promise((r) => setTimeout(r, 350));
  await pg.screenshot({ path });
}

/** Hover a tile so its `▶ Play · Edit · ⋯` strip is on screen (it is
 *  display:none otherwise), then click one of its verbs. */
async function tileAction(page, tileSel, role) {
  const tile = await page.waitForSelector(tileSel, { timeout: 8000 });
  await tile.hover();
  await sleep(150);
  await page.click(`${tileSel} [data-role="${role}"]`);
}

execSync("cargo build -q -p luxel-cli", { stdio: "inherit", cwd: ".." });
// The main mirror runs at a DELIBERATELY slow 24 fps (a 41 ms render loop
// instead of ~8 ms) — the pacing that used to break the playlist transport:
// `POST /api/playlist/play` is applied by the render loop, so the UI's
// follow-up `GET /api/playlist` beats it and reads `playing: false`. That read
// once latched, because the playlist poll only ran while the UI already
// believed it was playing (Gitea #431). Keeping the mirror slow here is what
// covers the fix; a fast mirror hides the race entirely.
const device = spawn(
  "../target/debug/luxel",
  [
    "serve",
    "--port",
    String(DEV_PORT),
    "--pixels",
    "120",
    "--fps",
    "24",
    // network-input ports are global (DDP 4048 / sACN 5568) — take this run's
    // own pair so a concurrent session's mirror does not own them (#496)
    "--ddp-port",
    String(E2E.netin.ddp),
    "--e131-port",
    String(E2E.netin.e131),
  ],
  { stdio: ["ignore", "pipe", "inherit"] },
);
await new Promise((resolve, reject) => {
  device.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
  device.on("exit", () => reject(new Error("device mirror died")));
  setTimeout(() => reject(new Error("device mirror start timeout")), 30000);
});

const web = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  stdio: "ignore",
});
process.on("exit", () => {
  device.kill();
  web.kill();
});
await sleep(1500);

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  headless: true,
  args: [
    "--no-sandbox",
    "--disable-gpu",
    "--window-size=1400,900",
    // fake mic + auto-grant, for the mic-to-device forwarding check
    "--use-fake-device-for-media-stream",
    "--use-fake-ui-for-media-stream",
  ],
});

const fails = [];
const check = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) fails.push(name);
};

// Capacity fixtures (Gitea #15), sized against a mirror claiming 30 KB free:
// 10 KB of load headroom, array arena clamped at its 16 KB minimum. Arrays are
// 8 B/element, so the element count is the dial. One line each — CodeMirror
// auto-closes `{`, and short sources keep the typing fast.
const SMALL = "export function render(index) { hsv(index / pixelCount, 1, 1) }";
const arrayPattern = (n) =>
  `var a = array(${n})\nexport function render(index) { hsv(a[index % ${n}] + index / pixelCount, 1, 1) }`;
const ARRAY_TIGHT = arrayPattern(1000); // ~9.5 KB modelled — inside 10 KB, past 85%
const ARRAY_OVER = arrayPattern(1400); // ~12.7 KB modelled — past the runtime floor
const ARRAY_ARENA = arrayPattern(2100); // 16.8 KB of arrays — past the 16 KB arena
// The OTHER array wall (Gitea #420): the PB-compat ELEMENT ledger, which is a
// count, not a size, and which `array(pixelCount)` walks into as soon as the
// device is bigger than the editor's preview. Three channels at 4096 px are
// 12,300 units against a 10,236 budget; at the 120 px the other mirrors run
// they are nothing at all.
const THREE_CHANNELS = [
  "export var r = array(pixelCount)",
  "export var g = array(pixelCount)",
  "export var b = array(pixelCount)",
  "export function render(i) { rgb(r[i], g[i], b[i]) }",
].join("\n");

/** A real mouse click on a Settings control, scrolled into view first — the
 * Output card sits far down a scrolling panel, where a bare `page.click`
 * fails with "Node is either not clickable or not an HTMLElement". */
async function clickRole(page, role) {
  const sel = `[data-role="${role}"]`;
  await page.$eval(sel, (el) => el.scrollIntoView({ block: "center" }));
  await page.click(sel);
}

async function setEditor(page, text) {
  await page.click(".cm-content");
  await page.keyboard.down("Control");
  await page.keyboard.press("a");
  await page.keyboard.up("Control");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(text, { delay: 0 });
}

try {
  // ---- boot cover: no playground flash, device-aware message ----
  // On load the whole app is covered until the device's running pattern is
  // loaded (delaying /api/pattern makes the cover linger long enough to read).
  {
    const boot = await browser.newPage();
    await boot.setRequestInterception(true);
    boot.on("request", (r) =>
      r.url().includes("/api/pattern") ? setTimeout(() => r.continue(), 2000) : r.continue(),
    );
    boot.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
      waitUntil: "domcontentloaded",
    });
    // the delayed /api/pattern keeps the handshake (and the cover) up long
    // enough to observe the device-aware message
    const sawDeviceLabel = await boot
      .waitForFunction(
        () =>
          /running on the device/.test(
            document.querySelector('[data-role="boot-label"]')?.textContent ?? "",
          ),
        { timeout: 6000 },
      )
      .then(() => true)
      .catch(() => false);
    check("boot: device-aware loading message", sawDeviceLabel);
    // the cover is over the whole app during the connect — nothing flashes
    check("boot: cover up during device connect", (await boot.$('[data-role="boot"]')) !== null);
    await boot.close();
  }

  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  // No device-URL field any more: a real device serves the UI from its own
  // flash (auto-connect to same origin); here we use the `?device=` dev
  // override to point the built playground at the mirror, and it auto-connects.
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle0",
  });
  await page.waitForSelector(".cm-content");
  // The handshake still runs on load (so we know what pattern to open); the
  // editor syncs to the device's RUNNING pattern once it finishes. Wait for
  // that rather than a connection badge.
  const synced = await page
    .waitForFunction(
      () => document.querySelector(".cm-content")?.textContent?.includes("canonical default"),
      { timeout: 10000 },
    )
    .then(() => true)
    .catch(() => false);
  check("connect: editor synced to the device's running pattern", synced);

  // no connection chrome: the device is always connected for the API, so there
  // are no connect/disconnect/reconnect buttons and no URL field.
  check("device: no device-url field", (await page.$(".device-url")) === null);
  // Share is playground-only and now lives in the editor's ⋯ menu (#468), so
  // the check has to open the menu — an absent role is otherwise vacuous.
  check("device: no Share in the ⋯ menu", (await menuHas(page, "share")) === false);
  check("device: Save is labelled for the device", (await page.$eval('[data-role="save"]', (el) => el.textContent.trim())) === "Save to device");
  check("device: no reconnect button", (await page.$('[data-role="reconnect"]')) === null);
  const hasDisconnect = await page.$$eval("header button", (btns) =>
    btns.some((b) => /disconnect/i.test(b.textContent ?? "")),
  );
  check("device: no disconnect button", hasDisconnect === false);

  const px = await page.$eval('[data-role="layout-pixels"]', (el) => el.value);
  check("connect: pixel count from device", px === "120", `got ${px}`);

  // ---- the console's Layout is the DEVICE's (Gitea #463) ----
  // A 64x64 panel console opens on a 64x64 grid preview whatever the pattern
  // is, and every tile/thumbnail on it is square. `--board panel` is what
  // lets a 4096 px layout exist at all (the strip cap is 2048), so this is
  // also the panel-impersonation pass.
  {
    const MAP_PORT = E2E.mirror.map; // E2E_PORT + 25
    const MAPPED = `http://127.0.0.1:${MAP_PORT}`;
    const mappedDev = spawn(
      "../target/debug/luxel",
      ["serve", ...NO_NETIN, "--port", String(MAP_PORT), "--pixels", "4096", "--max-pixels", "4096"],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      mappedDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      mappedDev.on("exit", () => reject(new Error("mapped mirror died")));
      setTimeout(() => reject(new Error("mapped mirror start timeout")), 30000);
    });
    process.on("exit", () => mappedDev.kill());
    const mappedPage = await browser.newPage();
    try {
      await fetch(`${MAPPED}/api/map`, { method: "POST", body: "grid 64 64" });
      const m = await fetch(`${MAPPED}/api/map`).then((r) => r.json());
      check(
        "rig: mirror reports the procedural grid's shape",
        m.kind === "grid" && m.w === 64 && m.h === 64,
        JSON.stringify(m),
      );
      await fetch(`${MAPPED}/api/code`, {
        method: "POST",
        body: await lxpBody("", "export function render2D(index, x, y) { hsv(x, 1, y) }"),
      });
      await mappedPage.setViewport({ width: 1400, height: 900 });
      await mappedPage.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(MAPPED)}`, {
        waitUntil: "networkidle0",
      });
      // two stored patterns — one 2D, one 1D — so the Patterns list and the
      // playlist rows have thumbnails to shape
      const saveOn = async (base, name, src) =>
        (
          await (
            await fetch(`${base}/api/patterns`, { method: "POST", body: await lxpBody(name, src) })
          ).json()
        ).id;
      const id2d = await saveOn(
        MAPPED,
        "Panel 2D",
        "export function render2D(index, x, y) { hsv(x, 1, y) }",
      );
      // carries a slider too, so the playlist row shows BOTH halves of the
      // values chip: the item's own controls and the projection under them
      const id1d = await saveOn(
        MAPPED,
        "Panel 1D",
        "export function sliderHue(h) { g = h }\nexport function render(index) { hsv(g + index / pixelCount, 1, 1) }",
      );
      await fetch(`${MAPPED}/api/playlist`, {
        method: "POST",
        body: `D 5\nX 0\nI ${id2d} -1\nI ${id1d} -1\n`,
      });
      await mappedPage.setViewport({ width: 1400, height: 900 });
      await mappedPage.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(MAPPED)}`, {
        waitUntil: "networkidle0",
      });
      // Settings → LED layout states the fixture (A8, #469): the installed
      // 64×64 grid makes this Layout a `map`, and the headline is the shape.
      const r = await mappedPage
        .waitForFunction(
          () => {
            const k = document.querySelector('[data-role="layout-kind"]')?.value;
            const head = document.querySelector('[data-role="layout-headline"]')?.textContent;
            return head?.includes("64×64") ? `${k} ${head.trim()}` : false;
          },
          { timeout: 10000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(
        "layout: a 64x64 panel console opens on a 64x64 grid",
        r === "map 64×64 matrix",
        r,
      );
      const chip = await mappedPage
        .$eval('[data-role="layout-label"]', (el) => (el.textContent ?? "").trim())
        .catch(() => "");
      check("layout: the console header states the device's layout", chip === "64×64 matrix", chip);
      const shape = await mappedPage.$eval('[data-role="editor-view"] [data-role="preview"]', (el) => el.dataset.shape);
      check("layout: the console preview is a grid", shape === "grid", shape);
      check(
        "layout: the playground's Preview-as chip is absent on a console",
        (await mappedPage.$('[data-role="preview-as"]')) === null,
      );
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-editor.png` });

      // a 1D pattern on the panel: still the panel's shape, with a caption
      await fetch(`${MAPPED}/api/code`, {
        method: "POST",
        body: await lxpBody("", "export function render(index) { hsv(index / pixelCount, 1, 1) }"),
      });
      await mappedPage.reload({ waitUntil: "networkidle0" });
      await sleep(1500);
      const shape1d = await mappedPage.$eval('[data-role="editor-view"] [data-role="preview"]', (el) => el.dataset.shape);
      check("layout: a 1D pattern on a panel previews as the panel, not a bar", shape1d === "grid");

      // ---- the quiet Projection row (#468, proposal §5.4d) ----
      // Visible only when it can matter: the pattern's dims differ from the
      // Layout's AND the Layout offers more than one option. A 1D pattern on
      // a 64×64 matrix is exactly that case; a 2D one on the same matrix is
      // native and shows nothing at all (mockup S2d) — absent, not disabled.
      check(
        "projection: the row appears for a 1D pattern on a matrix",
        (await mappedPage.$('[data-role="projection-row"]')) !== null,
      );
      const projText = await mappedPage
        .$eval('[data-role="projection-value"]', (el) => (el.textContent ?? "").trim())
        .catch(() => "");
      check(
        "projection: it reads as the device default until overridden",
        projText.startsWith("device default ·"),
        projText,
      );
      await mappedPage.click('[data-role="projection-change"]');
      await mappedPage.waitForSelector('[data-role="projection-options"]', { timeout: 2000 });
      const projOpts = await mappedPage.$$eval('[data-role="projection-options"] button', (els) =>
        els.map((e) => (e.textContent ?? "").trim().split(" ·")[0]),
      );
      check(
        "projection: the engine lists by-index and the two axes",
        projOpts.length >= 3,
        projOpts.join(","),
      );
      await mappedPage.click('[data-role="projection-opt-x"]');
      await sleep(600);
      const projOvr = await mappedPage
        .$eval('[data-role="projection-value"]', (el) => (el.textContent ?? "").trim())
        .catch(() => "");
      check("projection: an override reads as one", /· override$/.test(projOvr), projOvr);
      check(
        "projection: an override offers reset, not change",
        (await mappedPage.$('[data-role="projection-reset"]')) !== null &&
          (await mappedPage.$('[data-role="projection-change"]')) === null,
      );
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-projection.png` });
      await mappedPage.click('[data-role="projection-reset"]');
      await sleep(500);
      check(
        "projection: reset goes back to the device default",
        (await mappedPage.$('[data-role="projection-change"]')) !== null,
      );
      // back to the 2D pattern: native on this Layout, so no row at all
      await fetch(`${MAPPED}/api/code`, {
        method: "POST",
        body: await lxpBody("", "export function render2D(index, x, y) { hsv(x, 1, y) }"),
      });
      await mappedPage.reload({ waitUntil: "networkidle0" });
      await sleep(1500);
      check(
        "projection: no row for a 2D pattern on a matrix",
        (await mappedPage.$('[data-role="projection-row"]')) === null,
      );

      // Patterns tiles + Playlist rows take the device's shape (#463: the
      // thumbnail used to be a 64-px bar on every board)
      await mappedPage.click('[data-role="editor-back"]');
      await mappedPage.click('[data-role="tab-patterns"]');
      await mappedPage.waitForSelector(`${DTILE} .thumb`, { timeout: 8000 });
      await sleep(1500);
      const thumbShapes = await mappedPage.$$eval(`${DTILE} .thumb`, (els) =>
        els.map((e) => e.dataset.shape),
      );
      check(
        "thumbs: on-device tiles are square on a panel console",
        thumbShapes.length >= 2 && thumbShapes.every((s) => s === "grid"),
        thumbShapes.join(","),
      );
      const tileCaps = await mappedPage.$$eval(`${DTILE} [data-role="tile-caption"]`, (els) =>
        els.map((e) => (e.textContent ?? "").trim()),
      );
      check(
        "tiles: the 1D device pattern says how it is projected",
        tileCaps.some((t) => /^1D · /.test(t)),
        tileCaps.join("|"),
      );
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-patterns.png` });

      // mobile (D9, S1c): two columns on a 390 px console
      await mappedPage.setViewport({ width: 390, height: 780 });
      await sleep(700);
      const panelCols = await mappedPage.$eval(
        `${DGRID} .tiles`,
        (el) => getComputedStyle(el).gridTemplateColumns.split(" ").length,
      );
      check("mobile 390 px: the console tile grid is 2 columns", panelCols === 2, `${panelCols}`);
      const wideTiles = await mappedPage.$$eval(
        `${DTILE}:not([hidden])`,
        (els) => els.filter((e) => e.getBoundingClientRect().right > 391).length,
      );
      check("mobile 390 px: no console tile overflows the column", wideTiles === 0, `${wideTiles}`);
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-patterns-390.png` });
      await mappedPage.setViewport({ width: 1400, height: 900 });
      await sleep(500);

      await mappedPage.click('[data-role="tab-playlist"]');
      await mappedPage.waitForSelector('[data-role="playlist-item"] .thumb', { timeout: 8000 });
      await sleep(1200);
      const rowShapes = await mappedPage.$$eval('[data-role="playlist-item"] .thumb', (els) =>
        els.map((e) => e.dataset.shape),
      );
      check(
        "thumbs: playlist rows are square on a panel console",
        rowShapes.length >= 2 && rowShapes.every((s) => s === "grid"),
        rowShapes.join(","),
      );
      const rowCaption = await mappedPage.$$eval('[data-role="playlist-item"] .thumb', (els) =>
        els.map((e) => e.getAttribute("title") ?? ""),
      );
      check(
        "thumbs: the 1D playlist item says how it is projected",
        rowCaption.some((t) => /^1D · /.test(t)),
        rowCaption.join(" | "),
      );

      // ---- per-item projection override (#470 / §5.4d) ----
      // The quiet Projection row is visible ONLY where it can matter: the
      // 1D item on this 2D console offers three modes, the 2D item is
      // native and offers none — so row 0 has no values chip at all.
      const chips = await mappedPage.$$eval('[data-role="playlist-item"]', (rows) =>
        rows.map((r) => (r.querySelector('[data-role="pl-values-toggle"]') === null ? "-" : "chip")),
      );
      check(
        "projection: the native 2D item offers no values chip, the 1D one does",
        chips[0] === "-" && chips[1] === "chip",
        chips.join(","),
      );
      // the row reuses the EDITOR's ProjectionRow, so it carries the same
      // data-roles — one component, one caption, one set of hooks (#468)
      await mappedPage.$$eval('[data-role="pl-values-toggle"]', (els) => els[0].click());
      await sleep(300);
      check(
        "projection: the 1D item's values open on the Projection row",
        (await mappedPage.$('[data-role="pl-values"] [data-role="projection-row"]')) !== null,
      );
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-playlist.png` });
      await mappedPage.click('[data-role="pl-values"] [data-role="projection-change"]');
      await mappedPage.waitForSelector('[data-role="projection-options"]', { timeout: 2000 });
      await mappedPage.click('[data-role="projection-opt-x"]');
      await sleep(700);
      const plProj = await (await fetch(`${MAPPED}/api/playlist`)).json();
      check(
        "projection: the override rides on the item and the device echoes it",
        plProj.items[1].proj === "x" && plProj.items[0].proj === undefined,
        JSON.stringify(plProj.items.map((i) => i.proj ?? null)),
      );
      const projVal = await mappedPage.$eval(
        '[data-role="pl-values"] [data-role="projection-value"]',
        (el) => (el.textContent ?? "").trim(),
      );
      check(
        "projection: the row reads the override in accent, not the default",
        /along x/i.test(projVal) && /override/i.test(projVal),
        projVal,
      );
      await mappedPage.click('[data-role="pl-values"] [data-role="projection-reset"]');
      await sleep(700);
      check(
        "projection: reset drops the override back to the device default",
        (await (await fetch(`${MAPPED}/api/playlist`)).json()).items[1].proj === undefined,
      );

      await mappedPage.$$eval('[data-role="pl-values-toggle"]', (els) => els[0].click());
      await sleep(200); // collapsed again, so the phone pass opens it itself
      const deskThumb = await mappedPage.$$eval(
        '[data-role="playlist-item"] .thumb canvas',
        (els) => els.map((e) => Math.round(e.getBoundingClientRect().width)),
      );
      check(
        "thumbs: a desktop row thumbnail is the full 48 px square",
        deskThumb.length > 0 && deskThumb.every((w) => w === 48),
        deskThumb.join(","),
      );

      // ---- the phone (D9: the playlist is the primary phone surface) ----
      await mappedPage.setViewport({ width: 390, height: 820 });
      await sleep(900);
      const overflows = await mappedPage.evaluate(() => {
        const panel = document.querySelector('[data-role="playlist-panel"]');
        return {
          doc: document.documentElement.scrollWidth > document.documentElement.clientWidth,
          panel: panel.scrollWidth > panel.clientWidth,
        };
      });
      check(
        "mobile: the playlist does not scroll sideways at 390 px",
        overflows.doc === false && overflows.panel === false,
        JSON.stringify(overflows),
      );
      // the values chip still works, and the thumbnails shrank
      await mappedPage.$$eval('[data-role="pl-values-toggle"]', (els) => els[0].click());
      await sleep(400);
      check(
        "mobile: the values chip still expands in place",
        (await mappedPage.$('[data-role="pl-values"] [data-role="projection-row"]')) !== null,
      );
      const thumbPx = await mappedPage.$$eval('[data-role="playlist-item"] .thumb canvas', (els) =>
        els.map((e) => Math.round(e.getBoundingClientRect().width)),
      );
      check(
        "mobile: row thumbnails are the smaller size",
        thumbPx.length > 0 && thumbPx.every((w) => w <= 40),
        thumbPx.join(","),
      );
      const targets = await mappedPage.$$eval(
        '[data-role="pl-values-toggle"], [data-role="pl-duration"], [data-role="pl-remove"]',
        (els) => els.map((e) => Math.round(e.getBoundingClientRect().height)),
      );
      check(
        "mobile: chips and ✕ are thumb-sized targets",
        targets.length > 0 && targets.every((h) => h >= 30),
        targets.join(","),
      );
      await mappedPage.screenshot({
        path: `${shotDir}/device-e2e-panel-playlist-mobile.png`,
        fullPage: true,
      });
      await mappedPage.setViewport({ width: 1400, height: 900 });
    } finally {
      await mappedPage.close();
      mappedDev.kill();
    }
  }

  // ---- status bar shows the DEVICE frame rate, not the preview loop (#381) ----
  // A mirror paced at 24 fps against the browser's ~60 Hz preview loop: a
  // readout that tracks requestAnimationFrame cannot pass this. Its own
  // mirror and page, so the main suite's device keeps its default pace.
  {
    const SLOW_PORT = E2E.mirror.slow; // E2E_PORT + 26
    const SLOW = `http://127.0.0.1:${SLOW_PORT}`;
    const slowDev = spawn(
      "../target/debug/luxel",
      ["serve", ...NO_NETIN, "--port", String(SLOW_PORT), "--pixels", "120", "--fps", "24"],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      slowDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      slowDev.on("exit", () => reject(new Error("paced mirror died")));
      setTimeout(() => reject(new Error("paced mirror start timeout")), 30000);
    });
    process.on("exit", () => slowDev.kill());
    const slowPage = await browser.newPage();
    try {
      await slowPage.setViewport({ width: 1400, height: 900 });
      await slowPage.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(SLOW)}`, {
        waitUntil: "networkidle0",
      });
      const shown = await slowPage
        .waitForFunction(
          () => {
            const t = (document.querySelector('[data-role="fps"]')?.textContent ?? "").trim();
            const m = /^device (\d+) fps$/.exec(t);
            return m && Number(m[1]) > 0 ? t : false;
          },
          { timeout: 8000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(
        "fps: status bar labels the number as the device's",
        /^device \d+ fps$/.test(shown),
        shown,
      );
      const devFps = (await fetch(`${SLOW}/api/status`).then((r) => r.json())).fps;
      const n = Number(/(\d+)/.exec(shown)?.[1] ?? -1);
      check(
        "fps: readout tracks the mirror's rate, not the 60 Hz preview loop",
        Math.abs(n - devFps) <= 3 && devFps <= 32,
        `readout ${n}, /api/status ${devFps}`,
      );
      await slowPage.screenshot({ path: `${shotDir}/device-e2e-fps-device.png` });
    } finally {
      await slowPage.close();
      slowDev.kill();
    }
  }

  // A pipelined HUB75 board reports what the PANEL displayed in `out_fps`,
  // with `rescan_hz` as its ceiling — the readout must prefer it over `fps`
  // and say so. The mirror impersonates one (it drives no panel).
  {
    const PANEL_PORT = E2E.mirror.panel; // E2E_PORT + 24
    const PANEL = `http://127.0.0.1:${PANEL_PORT}`;
    const panelDev = spawn(
      "../target/debug/luxel",
      [
        "serve",
        ...NO_NETIN,
        "--port",
        String(PANEL_PORT),
        "--pixels",
        "120",
        "--fps",
        "24",
        "--out-fps",
        "112",
        "--rescan-hz",
        "115",
      ],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      panelDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      panelDev.on("exit", () => reject(new Error("panel mirror died")));
      setTimeout(() => reject(new Error("panel mirror start timeout")), 30000);
    });
    process.on("exit", () => panelDev.kill());
    const panelPage = await browser.newPage();
    try {
      await panelPage.setViewport({ width: 1400, height: 900 });
      await panelPage.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(PANEL)}`, {
        waitUntil: "networkidle0",
      });
      const shown = await panelPage
        .waitForFunction(
          () => {
            const t = (document.querySelector('[data-role="fps"]')?.textContent ?? "").trim();
            return /^device \d+ fps \(panel\)$/.test(t) ? t : false;
          },
          { timeout: 8000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(
        "fps: a panel board's displayed rate (out_fps) wins over the render rate",
        shown === "device 112 fps (panel)",
        shown,
      );
      const title = await panelPage
        .$eval('[data-role="fps"]', (el) => el.getAttribute("title") ?? "")
        .catch(() => "");
      check(
        "fps: tooltip names the rescan ceiling and the local preview",
        /rescan 115 Hz/.test(title) && /local preview/.test(title),
        title,
      );
      await panelPage.screenshot({ path: `${shotDir}/device-e2e-fps-panel.png` });
    } finally {
      await panelPage.close();
      panelDev.kill();
    }
  }

  // the preview runs on the LOCAL engine now (no device pixel stream) — it
  // lights up from local rendering
  const lit = await page
    .waitForFunction(
      () => {
        const c = document.querySelector(".waterfall");
        if (!c) return 0;
        const d = c.getContext("2d").getImageData(0, 0, c.width, 1).data;
        let n = 0;
        for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 0) n++;
        return n > 60 ? n : 0;
      },
      { timeout: 8000 },
    )
    .then((h) => h.jsonValue())
    .catch(() => 0);
  check("preview: local engine renders (not the device stream)", lit > 60, `lit=${lit}`);

  // ---- Settings → LED layout (A8, Gitea #469) ----
  //
  // The kind picker exists only where the BOARD offers a choice — a strip
  // mirror does, and every write goes through `POST /api/layout`, whose reply
  // is the new state (no re-GET).
  const layoutSel = await page.$('[data-role="layout-kind"]');
  check("layout: the kind picker is present on a strip board", layoutSel !== null);
  const layoutOpts = await page.$$eval('[data-role="layout-kind"] option', (os) =>
    os.map((o) => o.value),
  );
  check(
    "layout: offers strip/matrix/custom map",
    ["strip", "matrix", "map"].every((k) => layoutOpts.includes(k)),
    layoutOpts.join(","),
  );
  // switching to Matrix POSTs one `matrix …` line and reveals its fields
  await page.select('[data-role="layout-kind"]', "matrix");
  await sleep(600);
  check("layout: matrix reveals the panel size inputs", (await page.$('[data-role="layout-pw"]')) !== null);
  const lay2d = await (await fetch(`${DEV}/api/layout`)).json();
  check(
    "layout: the kind change reached the device",
    lay2d.kind === "matrix" && lay2d.dims === 2,
    JSON.stringify({ kind: lay2d.kind, dims: lay2d.dims, w: lay2d.w, h: lay2d.h }),
  );
  // a geometry change must NOT re-push the pattern to the device
  const stillRunning = await (await fetch(`${DEV}/api/pattern`)).text();
  check(
    "layout: a kind switch doesn't disturb the device pattern",
    stillRunning.includes("canonical default pattern"),
  );
  // the arrangement widget draws the pixel run through a single-tile matrix
  check(
    "layout: a strip-built matrix gets the pixel-wiring picture",
    (await page.$('[data-role="arrangement"][data-mode="pixels"]')) !== null,
  );
  check(
    "layout: a board with no panel driver has no refresh estimate",
    (await page.$('[data-role="refresh"]')) === null,
  );
  // an arrangement change is stored-and-reported until a reboot builds it
  // (#475), and the page says so rather than pretending it applied
  await page.$eval('[data-role="layout-snake"]', (el) => el.click());
  const rebootNote = await page
    .waitForFunction(
      () => document.querySelector('[data-role="layout-note"]')?.textContent?.trim() ?? false,
      { timeout: 6000 },
    )
    .then((h) => h.jsonValue())
    .catch(() => "");
  check(
    "layout: an arrangement change reports reboot_required",
    /reboot/i.test(rebootNote),
    rebootNote,
  );
  const laySnake = await (await fetch(`${DEV}/api/layout`)).json();
  check("layout: the snake reached the device", laySnake.matrix?.snake === 1, JSON.stringify(laySnake.matrix));
  await page.select('[data-role="layout-kind"]', "strip"); // restore
  await sleep(600);

  // the projection DEFAULTS live here too, and are POSTed as one `proj1d` line
  {
    const cards = await page.$$('[data-role="projection-card"]');
    check("projection: the block offers the non-native kinds", cards.length > 0, `${cards.length} cards`);
    await page.$eval('[data-role="projection-kind"][data-dims="2"] [data-role="projection-card"]', (el) =>
      el.click(),
    );
    await sleep(600);
    const proj = await (await fetch(`${DEV}/api/layout`)).json();
    check(
      "projection: picking a card sets the device default",
      proj.proj?.proj2d === "x",
      JSON.stringify(proj.proj),
    );
  }

  // The editor configures no geometry at all since A8 (#469): no shape
  // select, no pixel field, no install buttons, no map link. Two full-screen
  // editors are mounted at once (A10, #471), so scope by the view.
  check(
    "layout: the editor rail no longer carries the LED layout block",
    (await page.$('[data-role="led-layout"]')) === null &&
      (await page.$('[data-role="editor-view"] [data-role="layout-kind"]')) === null,
  );
  check(
    "layout: the editor no longer links to, installs or clears the map",
    (await page.$('[data-role="subtab-map"]')) === null &&
      (await page.$('[data-role="editor-view"] [data-role="map-install"]')) === null &&
      (await page.$('[data-role="editor-view"] [data-role="map-clear"]')) === null,
  );

  // brightness (Phase 3): the Settings slider drives GET/POST /api/brightness
  // live (the settings panel is in the DOM even while the editor is open)
  const b0 = await (await fetch(`${DEV}/api/brightness`)).json();
  check(
    "brightness: GET returns {brightness,max}",
    typeof b0.brightness === "number" && b0.max === 31,
    JSON.stringify(b0),
  );
  await page.$eval('[data-role="brightness"]', (el) => {
    el.value = "20";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(400);
  const b1 = await (await fetch(`${DEV}/api/brightness`)).json();
  check("brightness: slider sets device brightness", b1.brightness === 20, JSON.stringify(b1));
  const bReadout = await page.$eval('[data-role="brightness-val"]', (el) => el.textContent ?? "");
  check("brightness: readout reflects it", bReadout.includes("20"), bReadout);

  // pixel count: the LED layout Pixels field resizes the strip LIVE through
  // `POST /api/layout strip N` (no reboot). `/api/config` is the deprecated
  // alias for the same state, so it must report the change too.
  const cfg0 = await (await fetch(`${DEV}/api/config`)).json();
  check("config: GET returns {pixels,max}", cfg0.pixels === 120 && cfg0.max >= 120, JSON.stringify(cfg0));
  await page.$eval('[data-role="layout-pixels"]', (el) => {
    el.value = "48";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(700);
  const cfg1 = await (await fetch(`${DEV}/api/config`)).json();
  check("config: field resizes the device live", cfg1.pixels === 48, JSON.stringify(cfg1));
  const lay48 = await (await fetch(`${DEV}/api/layout`)).json();
  check("layout: /api/layout agrees with its /api/config alias", lay48.pixels === 48, JSON.stringify(lay48.pixels));
  const stAfter = await (await fetch(`${DEV}/api/status`)).json();
  check("config: status reports the new count", stAfter.pixels === 48, JSON.stringify(stAfter));
  // the device now streams 48 px — verify the pixel buffer resized
  const pxLen = (await (await fetch(`${DEV}/api/pixels`)).arrayBuffer()).byteLength;
  check("config: pixel buffer resized (48×3)", pxLen === 48 * 3, `${pxLen} bytes`);
  await page.$eval('[data-role="layout-pixels"]', (el) => {
    el.value = "120";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(700); // restore for the rest of the suite

  // LED protocol (Phase 3): the Settings dropdown switches the driver live
  const p0 = await (await fetch(`${DEV}/api/protocol`)).json();
  check(
    "protocol: GET returns current + options",
    p0.protocol === "sk9822" && p0.options.includes("ws2812"),
    JSON.stringify(p0),
  );
  await page.select('[data-role="layout-proto"]', "ws2812");
  await sleep(500);
  const p1 = await (await fetch(`${DEV}/api/protocol`)).json();
  check("protocol: dropdown switches the device", p1.protocol === "ws2812", JSON.stringify(p1));
  const cfgP = await (await fetch(`${DEV}/api/config`)).json();
  check("protocol: config GET reflects it", cfgP.protocol === "ws2812", JSON.stringify(cfgP));
  await page.select('[data-role="layout-proto"]', "sk9822");
  await sleep(300); // restore

  // live-code push: slider-controlled solid color + exported var
  await setEditor(
    page,
    [
      "export var level = 0.25",
      "export function sliderBlue(v) { level = v }",
      "export function render(index) { rgb(0, 0, level) }",
    ].join("\n"),
  );
  await sleep(1400); // debounce (500) + swap + snapshot tick
  const res = await fetch(`${DEV}/api/pattern`);
  check("push: device runs the typed pattern", (await res.text()).includes("sliderBlue"));

  await page.waitForSelector('input[type="range"]', { timeout: 5000 });
  check("controls: slider appeared (from the local engine)", true);

  // move the slider to full; onControlSet pushes to the device too, so its
  // real pixels should go bright blue
  await page.$eval('input[type="range"]', (el) => {
    el.value = "1";
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(700);
  const pxBytes = new Uint8Array(await (await fetch(`${DEV}/api/pixels`)).arrayBuffer());
  check(
    "controls: slider drives device pixels (blue=255)",
    pxBytes[2] === 255 && pxBytes[0] === 0,
    `rgb=${pxBytes[0]},${pxBytes[1]},${pxBytes[2]}`,
  );

  // vars watcher shows the exported var from the device
  const varsText = await page.evaluate(() => document.body.innerText);
  check("vars: exported var visible", varsText.includes("level"), "");

  // compile error path: line/col from the local compile; the broken source is
  // NOT pushed, so the device keeps running the previous pattern
  await setEditor(page, "export function render(index) { hsv( }");
  await sleep(1000);
  // The code pane owns its errors since #468: a strip pinned under it, not a
  // banner in the rail a thousand pixels from the line.
  const strip = await page
    .$eval('[data-role="compile-error"]', (el) => el.textContent ?? "")
    .catch(() => "");
  check("errors: local diagnostics shown in the code pane", /^✗ line \d+ · /.test(strip.trim()), strip.trim());
  check("errors: no compile banner in the rail", (await page.$(".right .banner.error")) === null);
  const still = await fetch(`${DEV}/api/pattern`);
  check("errors: broken pattern not pushed (device keeps old)", (await still.text()).includes("sliderBlue"));

  // ---- device pattern library (CRUD via the Device Patterns tab) ----
  // restore a valid pattern in the editor first (the error test left junk)
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 0.4) }");
  await sleep(900);
  // No `page.on("dialog")` handler: naming and confirmations are in-app
  // dialogs since Gitea #472, driven through their data-roles below. A native
  // prompt/confirm reaching the browser would hang the run — which is the
  // regression this absence guards.

  // WiFi settings form (the panel is in the DOM even while the editor is
  // open). Since A8 the form is COLLAPSED behind `Change network…` — joining
  // a different network is a once-per-install action that reboots.
  const w0 = await (await fetch(`${DEV}/api/wifi`)).json();
  check("wifi: GET returns {ssid,source}", "source" in w0, JSON.stringify(w0));
  check("wifi: the provisioning form starts collapsed", (await page.$('[data-role="wifi-ssid"]')) === null);
  await page.$eval('[data-role="wifi-change"]', (el) => el.click());
  await page.waitForSelector('[data-role="wifi-ssid"]', { timeout: 4000 });
  await page.$eval('[data-role="wifi-ssid"]', (el) => {
    el.value = "TestNet";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.$eval('[data-role="wifi-pass"]', (el) => {
    el.value = "hunter2000";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(100);
  await page.$eval('[data-role="wifi-save"]', (el) => el.click()); // panel is hidden; click directly
  // reboot-requiring action → an in-app confirmation that says so (#472)
  await waitDialog(page);
  check("wifi: save opens a reboot confirmation", (await dialogTitle(page)) === "Save WiFi and reboot?");
  check(
    "wifi: the reboot dialog is labelled as rebooting",
    (await page.$('[data-role="dialog-reboot"]')) !== null,
  );
  await page.screenshot({ path: `${shotDir}/device-e2e-dialog-reboot.png` });
  await page.setViewport({ width: 390, height: 780 });
  await sleep(200);
  await page.screenshot({ path: `${shotDir}/device-e2e-dialog-reboot-390.png` });
  await page.setViewport({ width: 1400, height: 900 });
  await sleep(200);
  // cancel first: the device must NOT be reconfigured
  await cancelDialog(page);
  await sleep(300);
  check(
    "wifi: a cancelled confirmation writes nothing",
    (await (await fetch(`${DEV}/api/wifi`)).json()).ssid !== "TestNet",
  );
  await page.$eval('[data-role="wifi-save"]', (el) => el.click());
  await acceptDialog(page);
  await sleep(500);
  check(
    "wifi: save stores the SSID on the device",
    (await (await fetch(`${DEV}/api/wifi`)).json()).ssid === "TestNet",
  );

  // MQTT settings: the Settings form stores the broker config (no broker is
  // running here, so it stays "not connected" — the announce/command contract
  // is covered by luxel-core::hamqtt unit tests + a live mosquitto check)
  {
    await openAdv(page, "adv-mqtt");
    const m0 = await (await fetch(`${DEV}/api/mqtt`)).json();
    check("mqtt: GET returns disabled by default", m0.enabled === false, JSON.stringify(m0));
    await page.$eval('[data-role="mqtt-host"]', (el) => {
      el.value = "mqtt.example.test";
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await page.$eval('[data-role="mqtt-user"]', (el) => {
      el.value = "ha";
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await sleep(100);
    await page.$eval('[data-role="mqtt-save"]', (el) => el.click());
    await sleep(400);
    const m1 = await (await fetch(`${DEV}/api/mqtt`)).json();
    check(
      "mqtt: form stores the broker config",
      m1.enabled === true && m1.host === "mqtt.example.test" && m1.user === "ha",
      JSON.stringify(m1),
    );
    // blank host = disable
    await fetch(`${DEV}/api/mqtt`, { method: "POST", body: "\n\n\n" });
    const m2 = await (await fetch(`${DEV}/api/mqtt`)).json();
    check("mqtt: blank host disables", m2.enabled === false, JSON.stringify(m2));
  }

  // output pipeline: settings round-trip through the form + API
  {
    await openAdv(page, "adv-output");
    const o0 = await (await fetch(`${DEV}/api/output`)).json();
    check(
      "output: defaults",
      o0.order === "rgb" &&
        o0.gamma === 0 &&
        o0.capMa === 0 &&
        o0.brightCurve === 0 &&
        o0.blur === 0 &&
        o0.glow === 0,
      JSON.stringify(o0),
    );
    // colour order is a STRIP concern and lives in LED layout since A8 (#469)
    check("output: colour order left Output processing", (await page.$('[data-role="out-order"]')) === null);
    await page.select('[data-role="layout-order"]', "grb");
    await sleep(400);
    const o1 = await (await fetch(`${DEV}/api/output`)).json();
    check("output: the LED layout colour-order select applies", o1.order === "grb", JSON.stringify(o1));
    const r = await (
      await fetch(`${DEV}/api/output`, { method: "POST", body: "bgr 22 1500" })
    ).json();
    check("output: POST sets all three", r.ok === true && r.gamma === 22 && r.capMa === 1500, JSON.stringify(r));
    // post-process chain: the three trailing tokens are optional, so the
    // three-token POST above must have LEFT them alone
    check(
      "output: legacy 3-token POST keeps the post-process settings",
      r.brightCurve === 0 && r.blur === 0 && r.glow === 0,
      JSON.stringify(r),
    );
    const bad = await (await fetch(`${DEV}/api/output`, { method: "POST", body: "xyz 22 0" })).json();
    check("output: bad order rejected", bad.ok === false);
    const badBlur = await (
      await fetch(`${DEV}/api/output`, { method: "POST", body: "rgb 0 0 22 101 0" })
    ).json();
    check("output: out-of-range blur rejected", badBlur.ok === false, JSON.stringify(badBlur));
    const badCurve = await (
      await fetch(`${DEV}/api/output`, { method: "POST", body: "rgb 0 0 51 0 0" })
    ).json();
    check("output: out-of-range brightness curve rejected", badCurve.ok === false);
    // the three Settings fields drive them live
    for (const [role, value, key, want] of [
      ["out-brightcurve", "2.2", "brightCurve", 22],
      ["out-blur", "30", "blur", 30],
      ["out-glow", "45", "glow", 45],
    ]) {
      // same drive as every other Settings number field in this suite:
      // the Output card sits far down a scrolling panel, so a real click
      // lands on "Node is either not clickable" — set + dispatch instead
      await page.$eval(
        `[data-role="${role}"]`,
        (el, v) => {
          el.value = v;
          el.dispatchEvent(new Event("input", { bubbles: true }));
          el.dispatchEvent(new Event("change", { bubbles: true }));
        },
        value,
      );
      await sleep(400);
      const o = await (await fetch(`${DEV}/api/output`)).json();
      check(`output: ${role} field applies`, o[key] === want, JSON.stringify(o));
    }
    const oAll = await (await fetch(`${DEV}/api/output`)).json();
    check(
      "output: the three post-process fields coexist",
      oAll.brightCurve === 22 && oAll.blur === 30 && oAll.glow === 45,
      JSON.stringify(oAll),
    );
    await fetch(`${DEV}/api/output`, { method: "POST", body: "rgb 0 0 0 0 0" }); // restore
  }

  // ---- the console preview runs the DEVICE output chain (#466, wired by #468) ----
  // Colour order is the cheapest visible stage: with `bgr` the wire carries
  // the red channel in the blue slot, and the console preview draws
  // `lx_outpipe` bytes rather than the raw engine frame, so a pure-red pattern
  // must preview BLUE. The playground has no device chain and keeps drawing
  // the raw frame.
  {
    // The block above restored the device's order with a bare POST, which the
    // UI never sees (/api/output is a form, not a poll) — drive the select so
    // the store and the device agree before sampling.
    await page.select('[data-role="layout-order"]', "rgb");
    await sleep(500);
    await setEditor(page, "export function render(index) { rgb(1, 0, 0) }");
    await sleep(900);
    const sample = () =>
      page.$eval(".strip", (c) => {
        const d = c.getContext("2d").getImageData(0, 0, 1, 1).data;
        return [d[0], d[1], d[2]];
      });
    const raw = await sample();
    check("outpipe: rgb order previews the pattern's own red", raw[0] > 200 && raw[2] < 60, raw.join(","));
    // drive it through the Settings form so the store refreshes the way a
    // user's edit does (nothing polls /api/output — it is a form)
    await page.select('[data-role="layout-order"]', "bgr");
    await sleep(900);
    const swapped = await sample();
    check(
      "outpipe: the console preview follows the device's colour order",
      swapped[2] > 200 && swapped[0] < 60,
      swapped.join(","),
    );
    await page.screenshot({ path: `${shotDir}/device-e2e-outpipe.png` });
    await page.select('[data-role="layout-order"]', "rgb"); // restore
    await sleep(700);
    const restored = await sample();
    check("outpipe: restoring the order restores the preview", restored[0] > 200, restored.join(","));
  }

  // wall clock: tz round-trips and local time tracks the host (mirror =
  // host clock; the device gets it from NTP)
  {
    const r = await (await fetch(`${DEV}/api/clock`, { method: "POST", body: "-360" })).json();
    check("clock: tz accepted", r.ok === true && r.tzMinutes === -360, JSON.stringify(r));
    const c = await (await fetch(`${DEV}/api/clock`)).json();
    const expect = Math.floor(Date.now() / 1000) - 360 * 60;
    check(
      "clock: local = host - 6h",
      c.synced === true && Math.abs(c.local - expect) < 5,
      JSON.stringify(c),
    );
    // a clock pattern sees the shifted hour
    await fetch(`${DEV}/api/code`, {
      method: "POST",
      body: await lxpBody(
        "",
        "export var h\nexport function beforeRender(d) { h = clockHour() }\nexport function render(i) { hsv(0,0,0) }",
      ),
    });
    await sleep(600);
    const vars = await (await fetch(`${DEV}/api/vars`)).json();
    const wantHour = new Date((expect) * 1000).getUTCHours();
    check(
      "clock: clockHour() reflects the tz",
      Math.round(vars.h / 65536) === wantHour,
      `h=${vars.h / 65536} want ${wantHour}`,
    );
    await fetch(`${DEV}/api/clock`, { method: "POST", body: "0" });
    const bad = await (await fetch(`${DEV}/api/clock`, { method: "POST", body: "9999" })).json();
    check("clock: silly tz rejected", bad.ok === false);
  }

  // AP-mode provisioning surface (the real AP needs a radio; the mirror
  // covers the API shape and the settings button presence)
  {
    const ap = await (await fetch(`${DEV}/api/apmode`)).json();
    check("apmode: GET reports not-AP", ap.ap === false, JSON.stringify(ap));
    const trig = await (await fetch(`${DEV}/api/apmode`, { method: "POST", body: "" })).json();
    check("apmode: POST accepted", trig.ok === true, JSON.stringify(trig));
    // Absent, never disabled (§5.7): the mirror advertises `reboot:false` and
    // `ota:false`, so Firmware & recovery is a version line and nothing more.
    await openAdv(page, "adv-firmware");
    check("apmode: no reboot-to-AP button without caps.reboot", (await page.$('[data-role="apmode"]')) === null);
    check("firmware: the version row is always there", (await page.$('[data-role="fw-version"]')) !== null);
    check("firmware: no Update… without caps.ota", (await page.$('[data-role="fw-update"]')) === null);
  }

  // sync role: the Settings select round-trips through /api/sync (the full
  // two-device convergence story is covered by tools/sync-e2e.mjs)
  {
    await openAdv(page, "adv-sync");
    const s0 = await (await fetch(`${DEV}/api/sync`)).json();
    check("sync: defaults to off", s0.mode === "off" && s0.leader === null, JSON.stringify(s0));
    await page.select('[data-role="sync-mode"]', "leader");
    await sleep(400);
    const s1 = await (await fetch(`${DEV}/api/sync`)).json();
    check("sync: settings select sets the role", s1.mode === "leader", JSON.stringify(s1));
    await fetch(`${DEV}/api/sync`, { method: "POST", body: "off" }); // restore
  }

  // sensor injection: POST /api/sensors takes a raw sensor-board frame
  // ("SB1.0\0"…"END\0", the serial wire format) and feeds exported sensor
  // vars — the render path a physical PB sensor board will use
  {
    await fetch(`${DEV}/api/code`, {
      method: "POST",
      body: await lxpBody(
        "",
        "export var energyAverage\nexport var frequencyData\n" +
          "export function render(index) { hsv(0, 1, energyAverage) }",
      ),
    });
    await sleep(300);
    const sb = Buffer.alloc(98);
    sb.write("SB1.0\0", 0, "latin1");
    sb.writeUInt16LE(0x8000, 6); // frequencyData[0] = 0.5
    sb.writeUInt16LE(0x4000, 70); // energyAverage = 0.25
    sb.writeUInt16LE(440, 74); // maxFrequency = 440 Hz
    sb.write("END\0", 94, "latin1");
    const r = await (await fetch(`${DEV}/api/sensors`, { method: "POST", body: sb })).json();
    check("sensors: frame accepted", r.ok === true, JSON.stringify(r));
    await sleep(600); // vars snapshot refreshes every 250ms
    const vars = await (await fetch(`${DEV}/api/vars`)).json();
    check(
      "sensors: energyAverage landed (raw 16.16)",
      vars.energyAverage === 0x4000,
      JSON.stringify(vars.energyAverage),
    );
    check(
      "sensors: frequencyData[0] landed",
      Array.isArray(vars.frequencyData) && vars.frequencyData[0] === 0x8000,
      JSON.stringify(vars.frequencyData?.[0]),
    );
    const bad = await (
      await fetch(`${DEV}/api/sensors`, { method: "POST", body: "junk" })
    ).json();
    check("sensors: junk rejected", bad.ok === false, JSON.stringify(bad));

    // mic-to-device forwarding: with the sound toggle on in device mode,
    // the browser mic streams frames to the device (fake mic = a tone).
    // A real (trusted) click: AudioContext needs the user activation.
    //
    // The button exists ONLY while the pattern in the EDITOR binds sensor
    // variables (#468, proposal §5.7) — the injection above pushed straight to
    // the device, so put the same pattern in the editor to arm it.
    await setEditor(
      page,
      "export var energyAverage\nexport var frequencyData\n" +
        "export function render(index) { hsv(0, 1, energyAverage) }",
    );
    await sleep(900);
    check(
      "sensors: the mic button appears for a sensor pattern",
      (await page.$('[data-role="mic-toggle"]')) !== null,
    );
    await page.click('[data-role="mic-toggle"]');
    let forwarded = false;
    for (let i = 0; i < 16 && !forwarded; i++) {
      await sleep(250);
      const v = (await (await fetch(`${DEV}/api/vars`)).json()).energyAverage;
      // injected value above (0x4000) gets overwritten by live mic frames
      if (typeof v === "number" && v !== 0x4000 && v > 0) forwarded = true;
    }
    check("sensors: mic frames stream to the device", forwarded);
    await page.click('[data-role="mic-toggle"]'); // off
  }

  // network input: a DDP packet overrides the engine (status.live + pixels),
  // and the pattern resumes after the 2.5s timeout
  {
    const udp = dgram.createSocket("udp4");
    const ddp = Buffer.alloc(10 + 3);
    ddp[0] = 0x41; // v1 | push
    ddp[2] = 1; // type: RGB
    ddp[3] = 1; // dest: default output
    ddp.writeUInt16BE(3, 8); // length
    ddp.set([1, 2, 3], 10); // first pixel = rgb(1,2,3)
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => udp.send(ddp, E2E.netin.ddp, "127.0.0.1", r));
      await sleep(80);
    }
    udp.close();
    await sleep(150);
    const stLive = await (await fetch(`${DEV}/api/status`)).json();
    check("netin: DDP packet flips status.live", stLive.live === "ddp", JSON.stringify(stLive));
    const pxLive = Buffer.from(await (await fetch(`${DEV}/api/pixels`)).arrayBuffer());
    check(
      "netin: DDP data drives the pixel buffer",
      pxLive[0] === 1 && pxLive[1] === 2 && pxLive[2] === 3,
      pxLive.subarray(0, 3).join(","),
    );
    // the COLLAPSED Advanced row answers "is it on?" without being opened
    const netinRow = await page
      .$eval('[data-role="adv-netin-status"]', (el) => (el.textContent ?? "").trim())
      .catch(() => "");
    check("netin: the collapsed Advanced row states a status", netinRow.length > 0, netinRow);
    await openAdv(page, "adv-netin");
    check("netin: settings shows a status row", (await page.$('[data-role="netin-status"]')) !== null);
    await sleep(2700); // live timeout
    const stIdle = await (await fetch(`${DEV}/api/status`)).json();
    check("netin: pattern resumes after timeout", stIdle.live === null, JSON.stringify(stIdle));
  }

  // save: the name is the editor header's, edited inline (#468) — no dialog.
  // Re-state the pattern first: the sections above (outpipe, sensors) left
  // their own source in the editor, and the checks below look for this one.
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 0.4) }");
  await sleep(900);
  await renameTo(page, "device kept");
  check("library: the header takes the name inline", (await saveState(page)) === "unsaved");
  check("library: naming opens no dialog", (await page.$('[data-role="dialog"]')) === null);
  await page.click('[data-role="save"]');
  await sleep(900);
  check("library: a stored pattern reads 'saved · on device'", (await saveState(page)) === "saved · on device");
  const apiList = await (await fetch(`${DEV}/api/patterns`)).json();
  check(
    "library: save-to-device stores it",
    apiList.patterns?.some((p) => p.name === "device kept"),
    JSON.stringify(apiList),
  );
  // the Patterns page's "On device" source lists it (back out of the editor)
  await page.click('[data-role="editor-back"]');
  await sleep(400);
  check("library: back lands on the Patterns page", (await page.$('[data-role="patterns-panel"]:not([hidden])')) !== null);
  const listed = await page.$$eval(DTILE, (els) => els.map((e) => e.textContent ?? ""));
  check("library: the On device source lists it", listed.some((t) => t.includes("device kept")), listed.join("|"));
  // each device pattern renders a live preview thumbnail (#2) — its source is
  // fetched in the background, so wait for the canvas to appear
  const hasThumb = await page
    .waitForSelector(`${DTILE} canvas`, { timeout: 4000 })
    .then(() => true)
    .catch(() => false);
  check("library: device pattern shows a preview thumbnail", hasThumb);
  // and it takes the DEVICE's shape — a bar here, because this console is a
  // 120 px strip (the panel console's square tiles are checked above, #463)
  const stripThumbs = await page.$$eval(`${DTILE} .thumb`, (els) => els.map((e) => e.dataset.shape));
  check(
    "thumbs: on-device tiles are bars on a strip console",
    stripThumbs.length > 0 && stripThumbs.every((s) => s === "bar"),
    stripThumbs.join(","),
  );
  await page.screenshot({ path: `${shotDir}/device-e2e-strip-patterns.png` });
  // the tile spins while the source fetch + compile are in flight, then the
  // spinner drops once the first frame lands
  await page
    .waitForFunction(
      () => document.querySelectorAll('[data-role="tile-spinner"]').length === 0,
      { timeout: 6000 },
    )
    .catch(() => null);
  const thumbSpin = await page.$$eval('[data-role="tile-spinner"]', (els) => els.length);
  check("library: tile spinner clears after first frame", thumbSpin === 0, `${thumbSpin} left`);

  // ---- the Patterns page's source control and per-tile verbs (#467) ----
  {
    const savedId = (apiList.patterns ?? []).find((p) => p.name === "device kept")?.id ?? "";
    // the running pattern wears the 2 px ring + "▶ playing" pill
    const playingKeys = await page.$$eval(`${DTILE}.playing`, (els) =>
      els.map((e) => e.dataset.key),
    );
    check(
      "patterns: the running pattern's tile is marked playing",
      playingKeys.length === 1 && playingKeys[0] === savedId,
      `${playingKeys.join(",")} vs ${savedId}`,
    );
    check(
      "patterns: the playing pill is on that tile",
      (await page.$(`${DTILE}[data-key="${savedId}"] [data-role="tile-playing"]`)) !== null,
    );

    // source switching: exactly one grid on screen, and it is the one picked
    const shownSources = async () =>
      page.$$eval('[data-role="patterns-grid"]:not([hidden])', (els) =>
        els.map((e) => e.dataset.source),
      );
    check("patterns: the console opens on the On device source", (await shownSources()).join(",") === "device");
    await page.click('[data-role="patterns-source-library"]');
    await sleep(500);
    check("patterns: picking Library shows only the library grid", (await shownSources()).join(",") === "library");
    check(
      "patterns: the Library source has tiles",
      (await page.$$eval('[data-source="library"] .tile', (els) => els.length)) > 50,
    );
    await page.screenshot({ path: `${shotDir}/device-e2e-strip-library.png` });
    await page.click('[data-role="patterns-source-device"]');
    await sleep(600);
    check("patterns: picking On device shows only that grid", (await shownSources()).join(",") === "device");

    // a second stored pattern, so the tile verbs below have a victim nothing
    // else in this run depends on
    await fetch(`${DEV}/api/patterns`, {
      method: "POST",
      body: await lxpBody("tile victim", "export function render(index) { rgb(0.77, 0.11, 0.22) }"),
    });
    await page.click('[data-role="patterns-source-device"]'); // re-reads /api/patterns
    await page.waitForFunction(
      (sel) => document.querySelectorAll(sel).length === 2,
      { timeout: 6000 },
      DTILE,
    );
    const victimId = ((await (await fetch(`${DEV}/api/patterns`)).json()).patterns ?? []).find(
      (p) => p.name === "tile victim",
    ).id;
    const victim = `${DTILE}[data-key="${victimId}"]`;

    // hover strip: ▶ Play activates it on the device without opening the editor
    await tileAction(page, victim, "tile-play");
    await sleep(1200);
    check(
      "patterns: the tile's Play activates the pattern on the device",
      (await (await fetch(`${DEV}/api/pattern`)).text()).includes("0.77"),
    );
    check(
      "patterns: Play does not open the editor",
      (await page.$('[data-role="patterns-panel"]:not([hidden])')) !== null,
    );
    await page.waitForSelector(`${victim}.playing`, { timeout: 4000 });
    check("patterns: the ring follows the newly played pattern", true);
    await page.screenshot({ path: `${shotDir}/device-e2e-strip-tile-playing.png` });

    // ⋯ → Add to playlist appends an item through the playlist store
    await tileAction(page, victim, "tile-menu");
    await page.waitForSelector('[data-role="tile-menu-popup"]', { timeout: 3000 });
    await page.screenshot({ path: `${shotDir}/device-e2e-tile-menu.png` });
    await page.click('[data-role="tile-menu-playlist"]');
    await sleep(900); // the store's 400 ms save debounce
    const plAdded = await (await fetch(`${DEV}/api/playlist`)).json();
    check(
      "patterns: ⋯ → Add to playlist appends the pattern",
      (plAdded.items ?? []).some((i) => i.id === victimId),
      JSON.stringify(plAdded.items),
    );
    await page.click('[data-role="tab-playlist"]');
    await sleep(900);
    const plRows = await page.$$eval('[data-role="playlist-item"]', (els) =>
      els.map((e) => e.textContent ?? ""),
    );
    check(
      "patterns: the playlist tab shows the added row",
      plRows.some((t) => t.includes("tile victim")),
      plRows.join("|"),
    );
    // clean up, and let the open Playlist tab's 1 Hz poll reconcile the
    // store to empty before leaving it (a stale item would show up in the
    // later "added twice with different params" check)
    await fetch(`${DEV}/api/playlist`, { method: "POST", body: "D 0" });
    await page
      .waitForFunction(() => document.querySelectorAll('[data-role="playlist-item"]').length === 0, {
        timeout: 6000,
      })
      .catch(() => null);
    check(
      "patterns: the playlist is empty again",
      (await page.$$eval('[data-role="playlist-item"]', (els) => els.length)) === 0,
    );
    await page.click('[data-role="tab-patterns"]');
    await sleep(600);

    // ⋯ → Delete: a danger dialog, cancelled once, then accepted
    await tileAction(page, victim, "tile-menu");
    await page.click('[data-role="tile-menu-delete"]');
    await waitDialog(page);
    check(
      "patterns: ⋯ → Delete asks with a danger dialog",
      (await dialogTitle(page)) === "Delete pattern from the device?" &&
        (await page.$eval('[data-role="dialog-confirm"]', (el) => el.className)).includes("danger"),
    );
    await cancelDialog(page);
    await sleep(600);
    check(
      "patterns: a cancelled tile delete keeps the pattern",
      ((await (await fetch(`${DEV}/api/patterns`)).json()).patterns ?? []).length === 2,
    );
    await tileAction(page, victim, "tile-menu");
    await page.click('[data-role="tile-menu-delete"]');
    await acceptDialog(page);
    await sleep(900);
    const afterTileDelete = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns ?? [];
    check(
      "patterns: an accepted tile delete removes it from the device",
      afterTileDelete.length === 1 && afterTileDelete[0].name === "device kept",
      JSON.stringify(afterTileDelete),
    );
    check("patterns: the deleted tile is gone", (await page.$(victim)) === null);
  }

  // ---- device output palette (Gitea #139) ----
  // Driven here rather than beside the other Output checks because the
  // editor hides the settings panel: real clicks need the Settings tab open.
  await page.click('[data-role="tab-settings"]');
  await sleep(400);
  // device output palette (Gitea #139): the Output card's editor drives
  // POST/DELETE /api/output/palette, and GET /api/output echoes it back
  {
    const p0 = await (await fetch(`${DEV}/api/output`)).json();
    check(
      "palette: absent by default",
      Array.isArray(p0.palette) && p0.palette.length === 0 && p0.paletteAmount === 0,
      JSON.stringify(p0.palette),
    );
    // "add stop" twice, then recolor the second one — real clicks on real
    // controls, the way a user builds a ramp
    await clickRole(page, "out-palette-add");
    await sleep(300);
    await clickRole(page, "out-palette-add");
    await sleep(400);
    const p1 = await (await fetch(`${DEV}/api/output`)).json();
    check(
      "palette: two stops installed from the editor",
      p1.palette.length === 8 && p1.palette[0] === 0 && p1.palette[4] === 64,
      JSON.stringify(p1.palette),
    );
    await page.$$eval('[data-role="out-palette-color"]', (els) => {
      const el = els[0];
      el.value = "#ff0000";
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await sleep(400);
    const p2 = await (await fetch(`${DEV}/api/output`)).json();
    check(
      "palette: color picker writes the stop",
      p2.palette[1] === 255 && p2.palette[2] === 0 && p2.palette[3] === 0,
      JSON.stringify(p2.palette),
    );
    await page.$eval('[data-role="out-palette-amount"]', (el) => {
      el.value = "60";
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await sleep(400);
    const p3 = await (await fetch(`${DEV}/api/output`)).json();
    check("palette: amount field applies", p3.paletteAmount === 60, JSON.stringify(p3));
    await page.screenshot({ path: `${shotDir}/device-e2e-output-palette.png` });
    // the API rejects the shapes the record can't hold
    for (const [name, body] of [
      ["ragged group", "50 0 1 2"],
      ["unsorted stops", "50 200 1 2 3 10 4 5 6"],
      ["bad amount", "101 0 1 2 3"],
      ["empty stop list", "50"],
    ]) {
      const bad = await (
        await fetch(`${DEV}/api/output/palette`, { method: "POST", body })
      ).json();
      check(`palette: rejects ${name}`, bad.ok === false, JSON.stringify(bad));
    }
    // clear button → DELETE
    await clickRole(page, "out-palette-clear");
    await sleep(400);
    const p4 = await (await fetch(`${DEV}/api/output`)).json();
    check(
      "palette: clear removes it",
      p4.palette.length === 0 && p4.paletteAmount === 0,
      JSON.stringify(p4),
    );
  }
  // ---- the map program's screen (A10, Gitea #471) ----
  // The console reaches it from Settings → LED layout → "Custom map program →"
  // (A8/#469 built that card; the interim link in the Device card is gone).
  // Install on device is the screen's ONE primary action; Clear and the
  // program's export/import live in its ⋯ menu; the debugger came with it.
  check(
    "settings: the LED layout card links to the map program",
    (await page.$('[data-role="layout-map-link"]')) !== null &&
      (await page.$('[data-role="map-program-link"]')) === null,
  );
  await page.$eval('[data-role="layout-map-link"]', (el) => el.click());
  await page.waitForSelector('[data-role="map-editor-view"]:not([hidden])', { timeout: 4000 });
  await sleep(900);
  check(
    "map screen: the console's primary action is Install on device",
    (await page.$('[data-role="map-install"]')) !== null &&
      (await page.$('[data-role="map-use"]')) === null,
  );
  const mapBadge = await page.$eval('[data-role="map-badge"]', (el) => (el.textContent ?? "").trim());
  check("map screen: it ran the program on arrival", /^\d+ points · 2D$/.test(mapBadge), mapBadge);
  check(
    "map screen: no compile error",
    (await page.$('[data-role="map-compile-error"]')) === null,
  );
  await page.screenshot({ path: `${shotDir}/device-map-editor.png` });
  await page.click('[data-role="map-install"]');
  await sleep(700);
  const mapGet = await (await fetch(`${DEV}/api/map`)).json();
  check(
    "map: Install on device uploads the computed map",
    mapGet.installed === true && mapGet.count > 0,
    JSON.stringify(mapGet),
  );
  const installedText = await page
    .$eval('[data-role="map-installed"]', (el) => (el.textContent ?? "").trim())
    .catch(() => "");
  check(
    "map screen: the header states what is on the device",
    installedText.includes(`${mapGet.count}px`),
    installedText,
  );
  await page.screenshot({ path: `${shotDir}/device-map-installed.png` });
  // a render2D pattern now uses the installed geometry
  await fetch(`${DEV}/api/code`, {
    method: "POST",
    body: await lxpBody("", "export function render2D(index, x, y) { rgb(x, y, 0) }"),
  });
  await sleep(500);
  const mpx = new Uint8Array(await (await fetch(`${DEV}/api/pixels`)).arrayBuffer());
  let varied = false;
  for (let i = 0; i < mpx.length; i += 3) if (mpx[i] !== mpx[0] || mpx[i + 1] !== mpx[1]) varied = true;
  check("map: render2D uses the geometry (pixels vary by x/y)", varied);
  // the map program is still debuggable on its own screen (ui-audit §7.6)
  const plotLine = await page.$$eval('[data-role="map-editor"] .cm-line', (els) => {
    const i = els.findIndex((el) => el.textContent?.includes("plot("));
    if (i < 0) return null;
    const r = els[i].getBoundingClientRect();
    return { y: r.y, h: r.height };
  });
  check("map screen: the program's source is in the code pane", plotLine !== null);
  if (plotLine) {
    const gut = await page.$eval('[data-role="map-editor"] .cm-bp-gutter', (el) => {
      const r = el.getBoundingClientRect();
      return { x: r.x, w: r.width };
    });
    await page.mouse.click(gut.x + gut.w / 2, plotLine.y + plotLine.h / 2);
    await sleep(250);
    await page.click('[data-role="map-run"]');
    await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 4000 }).catch(() => null);
    check("map screen: a breakpoint pauses the map run", (await page.$('.debugger[data-paused="true"]')) !== null);
    await page.click(".debugger .db-over");
    await sleep(300);
    check("map screen: the debugger steps", (await page.$('.debugger[data-paused="true"]')) !== null);
    await page.click('[data-role="map-debug"]'); // disarm
    await sleep(400);
  }
  // Clear lives in the ⋯ menu, behind the danger confirmation (#472)
  await page.click('[data-role="map-overflow"]');
  await sleep(200);
  await page.click('[data-role="map-clear"]');
  await waitDialog(page);
  await acceptDialog(page);
  await sleep(500);
  check(
    "map: Clear removes it from the device",
    (await (await fetch(`${DEV}/api/map`)).json()).installed === false,
  );
  await page.click('[data-role="map-editor-back"]');
  await sleep(400);
  check(
    "map screen: back returns to where it was opened from",
    (await page.$('[data-role="settings-panel"]:not([hidden])')) !== null,
  );

  await page.click('[data-role="tab-patterns"]');
  await sleep(400);

  // the tile's `Edit` verb opens the editor on it and activates it on the
  // device (a bare tile click PLAYS it, checked above)
  const seenReqs = [];
  page.on("request", (r) => {
    if (r.url().includes("/api/patterns") && r.method() === "DELETE") seenReqs.push(r.url());
  });
  await tileAction(page, DTILE, "tile-edit");
  await sleep(1300);
  check("library: a tile's Edit opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
  const activated = await (await fetch(`${DEV}/api/pattern`)).text();
  check("library: selecting a device pattern activates it", activated.includes("0.4"));
  check("library: editor shows the stored source", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.4"));
  // delete it from the editor — an in-app danger confirmation (Gitea #472),
  // cancelled once (nothing happens) before it is accepted
  check("library: Delete is in the editor's ⋯ menu", await menuHas(page, "delete"));
  await menuClick(page, "delete");
  await waitDialog(page);
  check(
    "library: delete asks with a danger dialog",
    (await dialogTitle(page)) === "Delete pattern from the device?" &&
      (await page.$eval('[data-role="dialog-confirm"]', (el) => el.className)).includes("danger"),
  );
  await page.screenshot({ path: `${shotDir}/device-e2e-dialog-delete.png` });
  await cancelDialog(page);
  await sleep(600);
  check("library: a cancelled delete sends no request", seenReqs.length === 0, seenReqs.join(","));
  check(
    "library: a cancelled delete keeps the pattern",
    ((await (await fetch(`${DEV}/api/patterns`)).json()).patterns ?? []).length === 1,
  );
  await menuClick(page, "delete");
  await acceptDialog(page);
  await sleep(900);
  check("library: DELETE request was sent", seenReqs.length > 0, seenReqs.join(","));
  const apiAfter = await (await fetch(`${DEV}/api/patterns`)).json();
  check(
    "library: delete removes it on the device",
    (apiAfter.patterns ?? []).length === 0,
    JSON.stringify(apiAfter),
  );

  // ---- dirty-aware resume across reload (#4) ----
  // An unsaved edit must (a) survive a reload and (b) be re-pushed so the
  // device runs it — even when the device was changed out-of-band meanwhile.
  await setEditor(page, "export function render(index) { rgb(0.111, 0.222, 0.333) }");
  await sleep(1500); // push debounce (500) + working-copy autosave (800) + margin
  check(
    "resume: unsaved edit was pushed to the device",
    (await (await fetch(`${DEV}/api/pattern`)).text()).includes("0.111"),
  );
  // change what the device runs out from under the editor
  await fetch(`${DEV}/api/code`, {
    method: "POST",
    body: await lxpBody("", "export function render(index) { rgb(0.9, 0.8, 0.7) }"),
  });
  await sleep(300);
  check(
    "resume: device changed out-of-band",
    (await (await fetch(`${DEV}/api/pattern`)).text()).includes("0.9"),
  );
  // reload — the dirty edit must win over the out-of-band device pattern
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle0",
  });
  await page.waitForSelector(".cm-content");
  await sleep(500); // let the device handshake settle after reload
  await sleep(1800); // let the resume push land on the device
  check(
    "resume: editor restores the unsaved edit",
    (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.111"),
  );
  check(
    "resume: device re-runs the resumed edit (not the out-of-band one)",
    (await (await fetch(`${DEV}/api/pattern`)).text()).includes("0.111"),
  );

  // clean copy → defer to the device. Save (clean), change the device
  // out-of-band, reload: the editor must open the RUNNING pattern, not resume.
  await renameTo(page, "device kept");
  await page.click('[data-role="save"]');
  await sleep(1000);
  await fetch(`${DEV}/api/code`, {
    method: "POST",
    body: await lxpBody("", "export function render(index) { rgb(0.44, 0.55, 0.66) }"),
  });
  await sleep(300);
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle0",
  });
  await page.waitForSelector(".cm-content");
  await sleep(500); // let the device handshake settle after reload
  await sleep(1200);
  check(
    "resume(clean): opens the device's running pattern, not the saved editor",
    (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.44"),
  );
  // ---- playlist (Phase 4) ----
  // save a pattern with a slider, add it to the playlist TWICE with different
  // params, set durations, play, advance
  await setEditor(page, "export function sliderHue(h) { g = h } export function render(index) { hsv(g + index / pixelCount, 1, 1) }");
  await sleep(900);
  await renameTo(page, "device kept");
  await page.click('[data-role="save"]');
  await sleep(900);
  check("playlist: Add to playlist appears for a saved pattern", await menuHas(page, "add-to-playlist"));
  // first add with hue=0.2
  await page.$eval('input[type="range"]', (el) => {
    el.value = "0.2";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  await menuClick(page, "add-to-playlist");
  await sleep(300);
  // second add with hue=0.8 (same pattern, different params)
  await page.$eval('input[type="range"]', (el) => {
    el.value = "0.8";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  await menuClick(page, "add-to-playlist");
  await sleep(500);
  const plAfterAdd = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: same pattern added twice with different params",
    plAfterAdd.items.length === 2 &&
      plAfterAdd.items[0].id === plAfterAdd.items[1].id &&
      Math.abs(plAfterAdd.items[0].controls.sliderHue[0] - 0.2) < 0.01 &&
      Math.abs(plAfterAdd.items[1].controls.sliderHue[0] - 0.8) < 0.01,
    JSON.stringify(plAfterAdd.items.map((i) => i.controls)),
  );
  // go to the Playlist tab
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  await page.click('[data-role="tab-playlist"]');
  await sleep(500);
  const rows = await page.$$('[data-role="playlist-item"]');
  check("playlist: tab shows both items", rows.length === 2);
  // set default duration
  await page.$eval('[data-role="pl-default-sec"]', (el) => {
    el.value = "5";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(500);
  check(
    "playlist: default duration persisted",
    (await (await fetch(`${DEV}/api/playlist`)).json()).defaultSec === 5,
  );
  // crossfade field (seconds in the UI → crossfadeMs on the wire)
  await page.$eval('[data-role="pl-crossfade"]', (el) => {
    el.value = "0.5";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(500);
  check(
    "playlist: crossfade persisted as ms",
    (await (await fetch(`${DEV}/api/playlist`)).json()).crossfadeMs === 500,
  );
  // per-item override: the duration CHIP opens the editor in place (#470)
  await page.$$eval('[data-role="pl-duration"]', (els) => els[0].click());
  await sleep(200);
  await page.$$eval('[data-role="pl-override"]', (els) => {
    els[0].click();
  });
  await sleep(200);
  await page.$$eval('[data-role="pl-sec"]', (els) => {
    els[0].value = "2";
    els[0].dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(500);
  check(
    "playlist: per-item override persisted",
    (await (await fetch(`${DEV}/api/playlist`)).json()).items[0].sec === 2,
  );
  const durChip = await page.$$eval('[data-role="pl-duration"]', (els) =>
    (els[0].textContent ?? "").trim(),
  );
  check("playlist: the duration chip shows the override", /^2 s/.test(durChip), durChip);
  await page.$$eval('[data-role="pl-duration"]', (els) => els[0].click()); // close again
  await sleep(150);

  // values edited INLINE on the row (#470, D6): the chip expands the item's
  // own sliders and the edit lands on the device as that item's `C` line
  await page.$$eval('[data-role="pl-values-toggle"]', (els) => els[0].click());
  await sleep(300);
  await page.$eval('[data-role="pl-values"] input[type="range"]', (el) => {
    el.value = "0.33";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(700);
  const plInline = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: a value edited inline lands on the item, not the other copy",
    Math.abs(plInline.items[0].controls.sliderHue[0] - 0.33) < 0.01 &&
      Math.abs(plInline.items[1].controls.sliderHue[0] - 0.8) < 0.01,
    JSON.stringify(plInline.items.map((i) => i.controls)),
  );
  await page.$$eval('[data-role="pl-values-toggle"]', (els) => els[0].click());
  await sleep(150);
  // drag-to-reorder: move item 0 (hue 0.2) below item 1 (hue 0.8)
  await page.evaluate(() => {
    const rows = document.querySelectorAll('[data-role="playlist-item"]');
    const grip = rows[0].querySelector('[data-role="pl-grip"]');
    const dt = new DataTransfer();
    grip.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }));
    rows[1].dispatchEvent(new DragEvent("dragover", { bubbles: true, dataTransfer: dt }));
    rows[1].dispatchEvent(new DragEvent("drop", { bubbles: true, dataTransfer: dt }));
  });
  await sleep(600);
  check(
    "playlist: drag reorders items",
    Math.abs(
      (await (await fetch(`${DEV}/api/playlist`)).json()).items[0].controls.sliderHue[0] - 0.8,
    ) < 0.01,
    JSON.stringify((await (await fetch(`${DEV}/api/playlist`)).json()).items.map((i) => i.controls)),
  );
  // play + advance
  await page.click('[data-role="pl-play"]');
  await sleep(500);
  const playing = await (await fetch(`${DEV}/api/playlist`)).json();
  check("playlist: play starts at index 0", playing.playing === true && playing.index === 0, JSON.stringify({ p: playing.playing, i: playing.index }));
  // …and the transport follows the device rather than latching on "play" when
  // the read-back beats the render loop (Gitea #431).
  const transportShown = await page
    .waitForSelector('[data-role="pl-next"]', { timeout: 2000 })
    .then(() => true)
    .catch(() => false);
  check("playlist: transport switches to the playing controls", transportShown);
  await page.click('[data-role="pl-next"]');
  await sleep(400);
  check(
    "playlist: next advances the device",
    (await (await fetch(`${DEV}/api/playlist`)).json()).index === 1,
  );
  await page.click('[data-role="pl-stop"]');
  await sleep(400);
  check(
    "playlist: stop halts auto-advance",
    (await (await fetch(`${DEV}/api/playlist`)).json()).playing === false,
  );
  check(
    "playlist: transport returns to the play button after stop",
    (await page.$('[data-role="pl-play"]')) !== null,
  );
  // total run-time summary (item0 default 5s + item1 override 2s = 7s)
  const total = await page.$eval('[data-role="pl-total"]', (el) => el.textContent ?? "");
  check("playlist: total run-time shown", /2 items/.test(total) && /7s/.test(total), total.trim());

  // ---- `+ Add` opens THE picker and appends what you choose (#470) ----
  await page.click('[data-role="pl-add"]');
  await page.waitForSelector('[data-role="pattern-picker"]', { timeout: 4000 });
  const pickCount = await page.$$eval('[data-role="picker-item"]', (els) => els.length);
  check("playlist: the picker lists the device's patterns", pickCount >= 1, String(pickCount));
  await page.$eval('[data-role="picker-search"]', (el) => {
    el.value = "zzzz-no-such-pattern";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(250);
  check(
    "playlist: the picker search narrows to nothing when nothing matches",
    (await page.$('[data-role="picker-empty"]')) !== null &&
      (await page.$$eval('[data-role="picker-item"]', (els) => els.length)) === 0,
  );
  await page.$eval('[data-role="picker-search"]', (el) => {
    el.value = "";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(250);
  await page.$$eval('[data-role="picker-item"]', (els) => els[0].click());
  await sleep(700);
  check(
    "playlist: picking from + Add appends a row",
    (await page.$$('[data-role="playlist-item"]')).length === 3 &&
      (await (await fetch(`${DEV}/api/playlist`)).json()).items.length === 3,
  );
  check(
    "playlist: the picker closes after a pick",
    (await page.$('[data-role="pattern-picker"]')) === null,
  );

  // ---- ✕ removes one row ----
  await page.$$eval('[data-role="pl-remove"]', (els) => els[2].click());
  await sleep(700);
  check(
    "playlist: ✕ removes that item only",
    (await (await fetch(`${DEV}/api/playlist`)).json()).items.length === 2,
  );

  // ---- Clear lives in ⋯ and is a danger confirm: cancel keeps it ----
  await page.click('[data-role="pl-more"]');
  await page.waitForSelector('[data-role="pl-clear"]', { timeout: 3000 });
  await page.click('[data-role="pl-clear"]');
  await cancelDialog(page);
  await sleep(500);
  check(
    "playlist: cancelling Clear keeps the items",
    (await (await fetch(`${DEV}/api/playlist`)).json()).items.length === 2,
  );
  await page.click('[data-role="pl-more"]');
  await page.waitForSelector('[data-role="pl-clear"]', { timeout: 3000 });
  await page.click('[data-role="pl-clear"]');
  check("playlist: Clear asks before emptying", (await dialogTitle(page)).includes("Clear"));
  await acceptDialog(page);
  await sleep(500);
  check(
    "playlist: clear empties it",
    (await (await fetch(`${DEV}/api/playlist`)).json()).items.length === 0,
  );

  // ---- "Add to playlist" captures the PROJECTION too (#470 + #468) ----
  // This console fabricates an 11×11 grid for a non-1D pattern, so a 3D one
  // is not native to it and the editor's quiet Projection row is live. The
  // choice made there is a VALUE, and the playlist item it is added to is
  // where that value gets its durable home (stores/pattern.ts).
  await page.click('[data-role="tab-patterns"]');
  await sleep(400);
  await tileAction(page, DTILE, "tile-edit");
  await sleep(1200);
  await setEditor(page, "export function render3D(index, x, y, z) { hsv(x, 1, z) }");
  await sleep(1200);
  await renameTo(page, "proj rider");
  await page.click('[data-role="save"]');
  await sleep(900);
  await page.click('[data-role="projection-change"]');
  await page.waitForSelector('[data-role="projection-options"]', { timeout: 2000 });
  await page.click('[data-role="projection-opt-yz"]');
  await sleep(400);
  await menuClick(page, "add-to-playlist");
  await sleep(700);
  const plWithProj = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: Add to playlist carries the editor's projection onto the item",
    plWithProj.items.length === 1 && plWithProj.items[0].proj === "yz",
    JSON.stringify(plWithProj.items),
  );
  await fetch(`${DEV}/api/playlist`, { method: "POST", body: "D 5" }); // clean up
  await page.click('[data-role="editor-back"]'); // back to the tabs
  await sleep(400);

  // ---- playlist pre-flight: an item whose assert() fails at the current
  // pixel count is reported per-item ("invalid") and badged in the UI ----
  const picky =
    'assert(pixelCount == 123456, "needs exactly 123456 pixels")\n' +
    "export function render(index) { hsv(0, 0, 1) }";
  const savedPicky = await (
    await fetch(`${DEV}/api/patterns`, { method: "POST", body: await lxpBody("Picky", picky) })
  ).json();
  await fetch(`${DEV}/api/playlist`, {
    method: "POST",
    body: `D 5\nI ${savedPicky.id} -1`,
  });
  await sleep(600); // give the device's pre-flight a beat
  const plPicky = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: pre-flight flags an assert violation per item",
    typeof plPicky.items[0].invalid === "string" &&
      plPicky.items[0].invalid.includes("123456"),
    JSON.stringify(plPicky.items[0].invalid),
  );
  await page.click('[data-role="tab-patterns"]');
  await sleep(200);
  await page.click('[data-role="tab-playlist"]');
  await sleep(800); // poll picks up the fresh playlist
  const badge = await page.$('[data-role="pl-invalid"]');
  check(
    "playlist: UI badges the invalid item",
    badge !== null &&
      (await page.$eval('[data-role="pl-invalid"]', (el) => el.title)).includes("123456"),
  );
  await fetch(`${DEV}/api/playlist`, { method: "POST", body: "D 5" }); // clean up
  await fetch(`${DEV}/api/patterns/${savedPicky.id}`, { method: "DELETE" });
  await sleep(300);

  // ---- "untitled" fix: a running pattern that matches a saved one shows its
  // name (the device streams only source, not which library entry it is) ----
  await fetch(`${DEV}/api/playlist/stop`, { method: "POST" });
  const uniq = "export function render(index) { rgb(0.13, 0.26, 0.39) }";
  await fetch(`${DEV}/api/code`, { method: "POST", body: await lxpBody("", uniq) }); // run it
  await fetch(`${DEV}/api/patterns`, {
    method: "POST",
    body: await lxpBody("Named Thing", uniq),
  }); // save same source
  await sleep(400);
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle0",
  });
  await page.waitForSelector(".cm-content");
  await sleep(1800); // connect + device pattern sources stream in, then match
  // the device streams each stored pattern's source in the background, so the
  // adoption lands once the matching one has arrived — poll rather than guess
  await page
    .waitForFunction(
      () =>
        (document.querySelector('[data-role="pattern-name"]')?.textContent ?? "").includes(
          "Named Thing",
        ),
      { timeout: 8000 },
    )
    .catch(() => null);
  const nm = await page.$eval('[data-role="pattern-name"]', (el) => el.textContent ?? "");
  check("untitled: running pattern adopts its saved name", nm.includes("Named Thing"), nm.trim());

  // clean up the saved pattern so a re-run starts fresh
  await fetch(`${DEV}/api/patterns`)
    .then((r) => r.json())
    .then((j) =>
      Promise.all(
        (j.patterns ?? []).map((p) =>
          fetch(`${DEV}/api/patterns/${p.id}`, { method: "DELETE" }),
        ),
      ),
    )
    .catch(() => {});

  // ---- Settings, ranked (A8, Gitea #469) ----
  //
  // The page's own shape, driven on the Settings TAB (the sections above
  // reach the same fields while the editor is open, which is fine for a form
  // but proves nothing about order or about what is on screen). Three
  // fixtures, because the whole point of the page is that it is different on
  // each: this strip mirror, a two-output board, and a HUB75 panel.
  {
    await page.click('[data-role="editor-back"]');
    await page.click('[data-role="tab-settings"]');
    await page.waitForSelector('[data-role="settings-panel"]:not([hidden])', { timeout: 8000 });
    await sleep(800);

    // Order (proposal §5.3): Device first, and BRIGHTNESS is the first
    // control on the page — ahead of every field in LED layout.
    const order = await page.$$eval(
      '[data-role="settings-panel"] .slabel, [data-role="settings-panel"] input, [data-role="settings-panel"] select',
      (els) =>
        els
          .map((e) =>
            e.tagName === "DIV" ? `#${e.textContent?.trim()}` : e.getAttribute("data-role") ?? "",
          )
          .filter(Boolean),
    );
    check(
      "settings: Device · LED layout · WiFi · Advanced, in that order",
      order.filter((x) => x.startsWith("#")).join(" ") ===
        "#Device #LED layout #Projection #WiFi #Advanced",
      order.filter((x) => x.startsWith("#")).join(" "),
    );
    check(
      "settings: brightness is the first control on the page",
      order.find((x) => !x.startsWith("#")) === "brightness",
      order.join(","),
    );
    check(
      "settings: …and it is the Device section's",
      (await page.$('[data-role="sect-device"] [data-role="brightness"]')) !== null,
    );

    // Every Advanced row is collapsed, and every one states its value.
    const rows = await page.$$eval('[data-role="advanced"] .drow', (els) =>
      els.map((e) => ({
        open: e.classList.contains("open"),
        status: e.querySelector(".st2")?.textContent?.trim() ?? "",
      })),
    );
    // seven unconditional rows here; Panel driver is the caps-gated eighth
    // and belongs to the HUB75 fixture below
    check("settings: the Advanced list carries every unconditional row", rows.length === 7, `${rows.length} rows`);
    check(
      "settings: every Advanced row starts collapsed",
      rows.every((r) => !r.open),
    );
    check(
      "settings: every collapsed row carries a one-line status",
      rows.every((r) => r.status.length > 0),
      JSON.stringify(rows.map((r) => r.status)),
    );
    check(
      "settings: a collapsed body is not in the DOM at all",
      (await page.$('[data-role="adv-clock-body"]')) === null,
    );
    await shotSettings(page, `${shotDir}/settings-strip-top.png`);

    // …and expanding one reveals its form.
    await openAdv(page, "adv-output");
    await openAdv(page, "adv-clock");
    await openAdv(page, "adv-storage");
    await sleep(300);
    check(
      "settings: an expanded row mounts its form",
      (await page.$('[data-role="out-gamma"]')) !== null &&
        (await page.$('[data-role="clock-tz"]')) !== null,
    );
    await shotSettings(page, `${shotDir}/settings-strip-advanced.png`, -1);

    // Mobile (390 px) — the primary phone surface for this page (D9).
    await page.setViewport({ width: 390, height: 780 });
    await sleep(400);
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth + 1,
    );
    check("settings: no horizontal overflow at 390 px", overflow);
    await shotSettings(page, `${shotDir}/settings-390.png`);
    await shotSettings(page, `${shotDir}/settings-390-advanced.png`, -1);
    await page.setViewport({ width: 1400, height: 900 });
    await sleep(300);

    // A reboot-requiring action is labelled AND confirmed — nothing reboots
    // from a bare button (§5.3). The data pin is absent on the mirror (no
    // `data_pins`), so WiFi's save is the one this host can prove.
    await page.$eval('[data-role="wifi-change"]', (el) => el.click());
    await page.waitForSelector('[data-role="wifi-save"]', { timeout: 4000 });
    await page.$eval('[data-role="wifi-save"]', (el) => el.click());
    await waitDialog(page);
    check(
      "settings: a reboot action opens the reboot-labelled dialog",
      (await page.$('[data-role="dialog-reboot"]')) !== null,
    );
    await cancelDialog(page);
    await sleep(200);
  }

  // Two outputs: the Outputs table exists ONLY when the board advertises more
  // than one, and an edit POSTs the whole table as `out` lines (all-or-
  // nothing) whose runs must partition the one pixel space (D11).
  {
    const OUT_PORT = E2E.mirror.outputs; // E2E_PORT + 27
    const OUT = `http://127.0.0.1:${OUT_PORT}`;
    const outDev = spawn(
      "../target/debug/luxel",
      ["serve", ...NO_NETIN, "--port", String(OUT_PORT), "--pixels", "120", "--outputs", "2"],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      outDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      outDev.on("exit", () => reject(new Error("two-output mirror died")));
      setTimeout(() => reject(new Error("two-output mirror start timeout")), 30000);
    });
    process.on("exit", () => outDev.kill());
    const outPage = await browser.newPage();
    try {
      await outPage.setViewport({ width: 1400, height: 900 });
      await outPage.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(OUT)}`, {
        waitUntil: "networkidle0",
      });
      await outPage.click('[data-role="editor-back"]');
      await outPage.click('[data-role="tab-settings"]');
      await outPage.waitForSelector('[data-role="outputs"]', { timeout: 8000 });
      check("outputs: the table appears when caps.outputs > 1", true);
      check(
        "outputs: the single implicit output is the only row until one is added",
        (await outPage.$$('[data-role="output-row"]')).length === 1,
      );
      // `+ Add output` exists because a spare physical output does
      await outPage.$eval('[data-role="output-add"]', (el) => el.click());
      await outPage.waitForFunction(
        () => document.querySelectorAll('[data-role="output-row"]').length === 2,
        { timeout: 6000 },
      );
      // split it evenly and push the whole table
      await outPage.$$eval('[data-role="output-count"]', (els) => {
        els.forEach((el) => {
          el.value = "60";
          el.dispatchEvent(new Event("change", { bubbles: true }));
        });
      });
      await sleep(900);
      const lay = await (await fetch(`${OUT}/api/layout`)).json();
      check(
        "outputs: the table round-trips through POST /api/layout",
        lay.outputs.length === 2 && lay.outputs.every((o) => o.count === 60),
        JSON.stringify(lay.outputs),
      );
      const ranges = await outPage.$$eval('[data-role="output-range"]', (els) =>
        els.map((e) => (e.textContent ?? "").replace(/\s+/g, " ").trim()),
      );
      check(
        "outputs: each row computes the run it owns",
        ranges.join(" | ") === "pixels 0–59 | pixels 60–119",
        ranges.join(" | "),
      );
      check(
        "outputs: a reversed run is a per-row checkbox, not a second wiring model",
        (await outPage.$('[data-role="output-rev"]')) !== null,
      );
      await shotSettings(outPage, `${shotDir}/settings-outputs.png`, 120);
    } finally {
      await outPage.close();
      outDev.kill();
    }
  }

  // A HUB75 panel: no kind picker at all (the board offers no choice), no
  // strip fields, and the arrangement widget + refresh estimate instead.
  {
    const HUB_PORT = E2E.mirror.hub75; // E2E_PORT + 28
    const HUB = `http://127.0.0.1:${HUB_PORT}`;
    const hubDev = spawn(
      "../target/debug/luxel",
      // --max-pixels so a 2x2 chain of 64x64 panels (16384 px) is a legal Layout;
      // the board default stays the single 64x64 panel it comes up as
      ["serve", ...NO_NETIN, "--port", String(HUB_PORT), "--board", "panel", "--max-pixels", "16384", "--rescan-hz", "115"],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      hubDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      hubDev.on("exit", () => reject(new Error("hub75 mirror died")));
      setTimeout(() => reject(new Error("hub75 mirror start timeout")), 30000);
    });
    process.on("exit", () => hubDev.kill());
    const hubPage = await browser.newPage();
    try {
      await hubPage.setViewport({ width: 1400, height: 900 });
      await hubPage.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(HUB)}`, {
        waitUntil: "networkidle0",
      });
      await hubPage.click('[data-role="editor-back"]');
      await hubPage.click('[data-role="tab-settings"]');
      await hubPage.waitForSelector('[data-role="layout-pw"]', { timeout: 8000 });
      check(
        "panel: no Layout picker — the board offers no choice (§5.7)",
        (await hubPage.$('[data-role="layout-kind"]')) === null,
      );
      check(
        "panel: no LED type / colour order / data pin",
        (await hubPage.$('[data-role="layout-proto"]')) === null &&
          (await hubPage.$('[data-role="layout-datapin"]')) === null,
      );
      check(
        "panel: no power cap and no blur/glow in Output processing",
        (await hubPage.$('[data-role="out-cap"]')) === null &&
          (await hubPage.$('[data-role="out-blur"]')) === null,
      );
      const head = await hubPage.$eval('[data-role="layout-headline"]', (e) => e.textContent.trim());
      check("panel: the summary line carries the kind", head === "64×64 matrix", head);
      const scan = await hubPage.$('[data-role="layout-scan"]');
      check("panel: the HUB75 scan divisor is a real field here", scan !== null);
      // one panel: the refresh estimate, no chain picture yet
      const hz = await hubPage.$eval('[data-role="refresh-hz"]', (e) => e.textContent.trim());
      check("panel: the estimated refresh is computed from the arrangement", hz === "115 Hz", hz);
      check(
        "panel: a single tile draws no chain picture (mockup S3)",
        (await hubPage.$('[data-role="arrangement"]')) === null,
      );
      // tile it 2×2 and the chain SVG appears, amber because the estimate drops
      await hubPage.$eval('[data-role="layout-cols"]', (el) => {
        el.value = "2";
        el.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await sleep(700);
      await hubPage.$eval('[data-role="layout-rows"]', (el) => {
        el.value = "2";
        el.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await hubPage.waitForSelector('[data-role="arrangement"][data-mode="chain"]', { timeout: 8000 });
      const amber = await hubPage.$eval('[data-role="refresh"]', (e) =>
        e.classList.contains("amber"),
      );
      const hz4 = await hubPage.$eval('[data-role="refresh-hz"]', (e) => e.textContent.trim());
      // the DEVICE reports `est_hz` since #475 and the browser prefers it,
      // so this is the firmware's own number, not the browser model's round
      check("panel: four chained panels go amber under 100 Hz", amber && hz4 === "28 Hz", hz4);
      const dark = await hubPage.$('[data-role="layout-dark"]');
      check("panel: the mirror drives the whole chain, so nothing is dark", dark === null);
      const note = await hubPage
        .waitForFunction(
          () => document.querySelector('[data-role="layout-note"]')?.textContent?.trim() ?? false,
          { timeout: 6000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check("panel: the arrangement change reports reboot_required", /reboot/i.test(note), note);
      // …and the action that applies it is caps-gated like everything else:
      // the mirror advertises `reboot:false` and has no /api/reboot route.
      check(
        "panel: no Reboot-to-apply button without caps.reboot (§5.7)",
        (await hubPage.$('[data-role="layout-reboot"]')) === null,
      );
      check(
        "panel: Advanced gains the Panel driver row",
        (await hubPage.$('[data-role="adv-panel-row"]')) !== null,
      );
      await shotSettings(hubPage, `${shotDir}/settings-panel.png`, 200);
    } finally {
      await hubPage.close();
      hubDev.kill();
    }
  }

  // ---- §5.7 sweep: absent, never disabled (Gitea #529) ------------------
  //
  // The rule the console is built on: a control is ABSENT unless the thing it
  // acts on exists. The single deliberate exception is a control disabled
  // because the user has to learn a BUDGET, and such a control carries
  // `data-reason` (and shows the same words on screen). That makes the rule
  // MECHANICAL, so assert it mechanically instead of one screenshot at a
  // time: with every Advanced body mounted, the playlist in both states and
  // the editor open, no `[disabled]` element may lack a `data-reason`.
  {
    // `adv-panel` is a HUB75 row and is absent here by design.
    const ADV = [
      "adv-output",
      "adv-clock",
      "adv-sync",
      "adv-mqtt",
      "adv-netin",
      "adv-storage",
      "adv-firmware",
    ];
    /** Settings tab with every disclosure — and the WiFi form — mounted. */
    const mountSettings = async () => {
      await page.click('[data-role="tab-settings"]');
      await page.waitForSelector('[data-role="settings-panel"]:not([hidden])', { timeout: 8000 });
      await page.$eval('[data-role="wifi-change"]', (el) => {
        if (el.getAttribute("aria-expanded") !== "true") el.click();
      });
      for (const role of ADV) {
        if (await page.$(`[data-role="${role}-toggle"]`)) await openAdv(page, role);
      }
      await sleep(400);
    };
    const sweep = async (what) => {
      const bad = await disabledSweep(page);
      check(`§5.7: nothing is disabled-without-a-reason (${what})`, bad.length === 0, JSON.stringify(bad));
    };

    // ---- state 1: nothing exists yet — an empty playlist, no palette ----
    await fetch(`${DEV}/api/playlist`, { method: "POST", body: "D 0" });
    await fetch(`${DEV}/api/output/palette`, { method: "DELETE" });
    await page.click('[data-role="tab-playlist"]');
    await sleep(900);
    check(
      "§5.7: an empty playlist has no Play button at all, just the empty state",
      (await page.$('[data-role="pl-play"]')) === null &&
        (await page.$('[data-role="pl-transport-empty"]')) !== null,
    );
    check(
      "§5.7: …and no ⋯ chip either, since Clear would be its only entry",
      (await page.$('[data-role="pl-more"]')) === null,
    );
    await page.screenshot({ path: `${shotDir}/no-disabled-playlist-empty.png` });
    // #530: the Default-seconds placeholder must READ, not clip to "mar".
    for (const w of [1400, 390]) {
      await page.setViewport({ width: w, height: w === 390 ? 780 : 900 });
      await sleep(350);
      const fits = await page.$eval(
        '[data-role="pl-default-sec"]',
        (el) => el.scrollWidth <= el.clientWidth + 1,
      );
      check(`playlist: the "manual" placeholder is not clipped at ${w} px (#530)`, fits);
      await page.screenshot({ path: `${shotDir}/playlist-default-sec-${w}.png` });
    }
    await page.setViewport({ width: 1400, height: 900 });
    await sleep(300);

    await mountSettings();
    check(
      "§5.7: a palette with no stops has no clear button",
      (await page.$('[data-role="out-palette-clear"]')) === null,
    );
    await page.$eval('[data-role="out-palette-preview"]', (el) =>
      el.scrollIntoView({ block: "center" }),
    );
    await sleep(250);
    await page.screenshot({ path: `${shotDir}/no-disabled-palette-empty.png` });
    await sweep("empty playlist · no palette · settings");

    // ---- state 2: the objects exist, and the budget is at its cap ----
    const swept = await (
      await fetch(`${DEV}/api/patterns`, {
        method: "POST",
        body: await lxpBody("Sweep", "export function render(index) { hsv(0.3, 1, 1) }"),
      })
    ).json();
    await fetch(`${DEV}/api/playlist`, {
      method: "POST",
      body: `D 5\nI ${swept.id} -1\nI ${swept.id} -1\nI ${swept.id} -1`,
    });
    // 32 stops = MAX_PALETTE_STOPS: the ONE budget the design says may be a
    // disabled control, and it must carry its reason.
    await fetch(`${DEV}/api/output/palette`, {
      method: "POST",
      body: `50 ${Array.from({ length: 32 }, (_, i) => `${i * 8} ${i * 8} 128 255`).join(" ")}`,
    });
    await page.click('[data-role="tab-playlist"]');
    await sleep(1200);
    check(
      "§5.7: Play is back once the playlist has an item",
      (await page.$('[data-role="pl-play"]')) !== null &&
        (await page.$('[data-role="pl-more"]')) !== null,
    );
    check(
      "§5.7: the first row has no dimmed ↑ — the control is simply absent",
      (await page.$$('[data-role="playlist-item"]')).length === 3 &&
        (await page.$$eval('[data-role="playlist-item"]', (els) =>
          [...els[0].querySelectorAll("button")].every((b) => b.title !== "move up"),
        )),
    );
    await mountSettings();
    const capped = await page.$eval('[data-role="out-palette-add"]', (el) => ({
      disabled: el.hasAttribute("disabled"),
      reason: el.getAttribute("data-reason") ?? "",
    }));
    const capText = await page
      .$eval('[data-role="out-palette-cap"]', (el) => (el.textContent ?? "").trim())
      .catch(() => "");
    check(
      "§5.7: the ONE exception — the palette-stop budget — is disabled WITH a reason",
      capped.disabled && /32/.test(capped.reason) && /32/.test(capText),
      JSON.stringify({ ...capped, capText }),
    );
    await page.$eval('[data-role="out-palette-cap"]', (el) =>
      el.scrollIntoView({ block: "center" }),
    );
    await sleep(250);
    await page.screenshot({ path: `${shotDir}/no-disabled-palette-at-cap.png` });
    await sweep("3 playlist items · palette at its 32-stop cap · settings");

    // ---- state 3: the editor, where most of the conditional UI lives ----
    await page.click('[data-role="tab-patterns"]');
    await page.click('[data-role="new-pattern"]');
    await page.waitForSelector('[data-role="editor-header"]', { timeout: 8000 });
    await sleep(800);
    await sweep("editor, a fresh pattern");
    await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
    await sleep(1200);
    await sweep("editor, a compiled pattern");
    await page.click('[data-role="editor-back"]');
    await sleep(400);

    // leave the device the way the sections below expect it
    await fetch(`${DEV}/api/output/palette`, { method: "DELETE" });
    await fetch(`${DEV}/api/playlist`, { method: "POST", body: "D 5" });
    await fetch(`${DEV}/api/patterns/${swept.id}`, { method: "DELETE" });
    await sleep(300);
  }

  // The three blocks above left the app on the Settings tab; the capacity
  // section below drives the code pane, so put the editor back on screen.
  await page.click('[data-role="tab-patterns"]');
  await page.click('[data-role="new-pattern"]');
  await page.waitForSelector('[data-role="editor-header"]', { timeout: 8000 });
  await sleep(600);

  // ---- capacity warning (Gitea #15) ----
  // The editor models the firmware's own pattern-load sequence (decode →
  // budgeted engine → frames, under a counting allocator in wasm) against the
  // free heap the device reports, and warns before the push lands.
  //
  // Silence first: this mirror reports heap_free 0 ("I can't tell you"), and
  // an unknown budget must never be treated as a small one.
  await setEditor(page, ARRAY_OVER);
  await sleep(1200);
  check(
    "capacity: silent when the device can't report free heap",
    (await page.$('[data-role="capacity-warning"]')) === null,
  );

  // A second mirror impersonating a starved device. 30 KB free leaves a
  // 10 KB load headroom (20 KB runtime floor) with the array arena clamped at
  // its 16 KB minimum — which puts all four verdicts within reach of a
  // one-line pattern. See crates/luxel-core/src/budget.rs.
  const TIGHT_PORT = E2E.mirror.tight; // E2E_PORT + 21
  const TIGHT = `http://127.0.0.1:${TIGHT_PORT}`;
  const tightDev = spawn(
    "../target/debug/luxel",
    ["serve", ...NO_NETIN, "--port", String(TIGHT_PORT), "--pixels", "120", "--heap-free", "30720"],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
  await new Promise((resolve, reject) => {
    tightDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
    tightDev.on("exit", () => reject(new Error("starved mirror died")));
    setTimeout(() => reject(new Error("starved mirror start timeout")), 30000);
  });
  process.on("exit", () => tightDev.kill());

  try {
    check(
      "capacity: mirror reports the impersonated free heap",
      (await fetch(`${TIGHT}/api/status`).then((r) => r.json())).heap_free === 30720,
    );

    await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(TIGHT)}`, {
      waitUntil: "networkidle0",
    });
    await page.waitForSelector(".cm-content");
    await sleep(1500);

    /** The warning banner's level, or "" when there is no warning. */
    const level = async () =>
      await page
        .$eval('[data-role="capacity-warning"]', (el) => el.getAttribute("data-level") ?? "")
        .catch(() => "");

    await setEditor(page, SMALL);
    await sleep(1200);
    check("capacity: no warning for a pattern that fits", (await level()) === "", await level());

    await setEditor(page, ARRAY_TIGHT);
    await sleep(1200);
    check("capacity: 'close to the limit' warning appears", (await level()) === "tight");
    await page.screenshot({
      path: `${shotDir}/device-e2e-capacity-tight.png`,
    });

    await setEditor(page, ARRAY_OVER);
    await sleep(1200);
    check("capacity: 'too large' warning appears", (await level()) === "over");
    const overText = await page
      .$eval('[data-role="capacity-warning"]', (el) => el.textContent ?? "")
      .catch(() => "");
    check(
      "capacity: warning names both figures",
      /\d+ KB needed, \d+ KB free/.test(overText),
      overText.trim(),
    );
    await page.screenshot({ path: `${shotDir}/device-e2e-capacity-over.png` });

    // Non-blocking: an over-budget pattern is still pushed. The device is the
    // authority on what it can run; the editor only says what it expects.
    const pushed = await fetch(`${TIGHT}/api/pattern`).then((r) => r.text());
    check("capacity: warning does not block the push", pushed.includes("array(1400)"), pushed.slice(0, 60));

    // The array arena is the OTHER rejection path: the pattern never gets far
    // enough for the floor check because its arrays don't fit the budget.
    await setEditor(page, ARRAY_ARENA);
    await sleep(1200);
    check("capacity: array-budget overrun also warns", (await level()) === "over");
    const arenaText = await page
      .$eval('[data-role="capacity-warning"]', (el) => el.textContent ?? "")
      .catch(() => "");
    check(
      "capacity: array-budget warning names the arena, not the floor",
      /arrays exceed this device's array memory budget/.test(arenaText),
      arenaText.trim(),
    );

    await setEditor(page, SMALL);
    await sleep(1200);
    check("capacity: warning clears when the pattern shrinks", (await level()) === "");
    await page.screenshot({ path: `${shotDir}/device-e2e-capacity-clear.png` });

    // ---- the outgoing engine's heap counts (Gitea #287) ----
    // Same 30 KB of free heap, but this mirror also reports a 30 KB pattern
    // RESIDENT. The firmware drops that engine before it decodes an incoming
    // one, so the real load base is 60 KB, not 30 KB — and ARRAY_OVER, which
    // the starved mirror above correctly calls "too large", fits here. This
    // is the regression Jeremy reported: with a fat pattern loaded, heap_free
    // reads low and the editor warned about patterns that load fine.
    const LOADED_PORT = E2E.mirror.loaded; // E2E_PORT + 22
    const LOADED = `http://127.0.0.1:${LOADED_PORT}`;
    const loadedDev = spawn(
      "../target/debug/luxel",
      [
        "serve",
        ...NO_NETIN,
        "--port",
        String(LOADED_PORT),
        "--pixels",
        "120",
        "--heap-free",
        "30720",
        "--engine-heap",
        "30720",
      ],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      loadedDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      loadedDev.on("exit", () => reject(new Error("loaded mirror died")));
      setTimeout(() => reject(new Error("loaded mirror start timeout")), 30000);
    });
    process.on("exit", () => loadedDev.kill());
    try {
      check(
        "capacity: mirror reports engine_heap",
        (await fetch(`${LOADED}/api/status`).then((r) => r.json())).engine_heap === 30720,
      );
      await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(LOADED)}`, {
        waitUntil: "networkidle0",
      });
      await page.waitForSelector(".cm-content");
      await sleep(1500);
      await setEditor(page, ARRAY_OVER);
      await sleep(1200);
      check(
        "capacity: the outgoing engine's heap is credited to the incoming pattern",
        (await level()) === "",
        await level(),
      );
      await page.screenshot({ path: `${shotDir}/device-e2e-capacity-engine-heap.png` });
      // ...and the same device still warns about something genuinely too big.
      await setEditor(page, arrayPattern(5200));
      await sleep(1200);
      check("capacity: still warns past the credited headroom", (await level()) === "over");
    } finally {
      loadedDev.kill();
    }

    // ---- the element ledger, at a panel's pixel count (Gitea #420) --------
    // A healthy-heap mirror driving 4096 px: nothing about SIZE stops this
    // pattern, only the count of array elements. It used to load and render
    // black with the editor saying nothing, because the capacity model
    // discarded the element-ledger vmerr and the device's own vmerr was
    // overwritten by the "indexing a non-array value" cascade one frame later.
    // `--max-pixels` is load-bearing: a strip mirror's ceiling is 2048, and
    // `--pixels 4096` used to be clamped to it in silence, which disarmed
    // this whole check (Gitea #495 — it is a hard error now).
    const PANEL_PORT = E2E.mirror.loadedPanel; // E2E_PORT + 23
    const PANEL = `http://127.0.0.1:${PANEL_PORT}`;
    const panelDev = spawn(
      "../target/debug/luxel",
      // `--max-pixels` is the #495 route: it raises this run's ceiling
      // without impersonating a panel, so the rig stays the honest one — a
      // STRIP bigger than the editor's preview.
      [
        "serve",
        ...NO_NETIN,
        "--port",
        String(PANEL_PORT),
        "--pixels",
        "4096",
        "--max-pixels",
        "4096",
        "--heap-free",
        "200000",
      ],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      panelDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      panelDev.on("exit", () => reject(new Error("panel mirror died")));
      setTimeout(() => reject(new Error("panel mirror start timeout")), 30000);
    });
    process.on("exit", () => panelDev.kill());
    try {
      await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(PANEL)}`, {
        waitUntil: "networkidle0",
      });
      await page.waitForSelector(".cm-content");
      await sleep(1500);
      await setEditor(page, THREE_CHANNELS);
      await sleep(2500);
      // Either banner is a pass — the local model warns first and the
      // device's own verdict replaces it a poll later — but it must NAME the
      // element ledger rather than leave a black strip unexplained.
      const banner = await page
        .$eval(
          '[data-role="capacity-rejected"], [data-role="capacity-warning"]',
          (el) => el.textContent ?? "",
        )
        .catch(() => "");
      check(
        "capacity: the element ledger is named, not swallowed",
        /array element budget exceeded|more array elements than this device allows/.test(banner),
        banner.trim(),
      );
      // ...and the device's own vmerr, once it lands, carries the figures and
      // does not decay into the missing-buffer cascade it causes.
      await page.waitForSelector('[data-role="capacity-rejected"]', { timeout: 15000 });
      await sleep(2000);
      const rejected = await page.$eval(
        '[data-role="capacity-rejected"]',
        (el) => el.textContent ?? "",
      );
      check(
        "capacity: the device's verdict names the numbers",
        /4096-element array needs 4100 more of the 10236-element budget/.test(rejected),
        rejected.trim(),
      );
      check(
        "capacity: the verdict is not the missing-buffer cascade",
        !/non-array/.test(rejected),
        rejected.trim(),
      );
      await page.screenshot({ path: `${shotDir}/device-e2e-capacity-elements.png` });
    } finally {
      panelDev.kill();
    }
  } finally {
    tightDev.kill();
  }
} finally {
  await browser.close();
  device.kill();
  web.kill();
}

console.log(fails.length === 0 ? "\nall device-mode checks passed" : `\n${fails.length} FAILURES`);
process.exit(fails.length === 0 ? 0 : 1);
