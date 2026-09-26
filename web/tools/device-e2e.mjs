// Device-mode e2e: the built playground in real chromium, connected to
// `luxel serve` (the native mirror of the firmware API). Verifies connect,
// editor sync from the device, live-code push, preview streaming, controls,
// vars, compile errors, and disconnect.
//
// Usage (from web/): npm run build && node tools/device-e2e.mjs

import { execSync, spawn } from "node:child_process";
import dgram from "node:dgram";
import fs from "node:fs";
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

// Since #538 a console OPENS ON THE PATTERNS PAGE, not in the editor (that is
// what the "default page" checks at the end of this file assert). Every flow
// here that starts inside the editor therefore asks for it by route — which
// is also the round-trip test that `lib/router.ts` honours a fragment on a
// cold load.
const EDIT = "#/editor";

/**
 * Reload INTO a screen. Two traps now that the route lives in the hash
 * (#538): `goto` to the URL we are already on is a same-document fragment
 * navigation (no boot at all), and a plain `reload` reopens whatever screen
 * the URL last named — which is the point of the feature, but not what a
 * test that wants the editor means. Set the fragment, then reload for real.
 */
/** Leave the editor IF it is open: a fresh console boot lands on the Patterns
 *  page since #538, so a "back out first" setup step is now conditional. */
async function leaveEditor(pg) {
  if (await pg.$('[data-role="editor-view"]:not([hidden])')) {
    await pg.click('[data-role="editor-back"]');
  }
}

async function reloadInto(pg, route = EDIT) {
  await pg.evaluate((r) => {
    location.hash = r;
  }, route);
  await pg.reload({ waitUntil: "networkidle0" });
}

// The Patterns page (#467) replaced the Device Patterns tab: one page, one
// grid per SOURCE, all mounted with the inactive ones hidden — so a tile
// selector always names its source's grid.
const DGRID = '[data-role="patterns-grid"][data-source="device"]';
const DTILE = `${DGRID} .tile`;

/**
 * What a projection looks like in a FRAME, on a 64x64 fixture (Gitea #538,
 * #598). A 1D pattern whose pixel is a pure function of `index` draws a hue
 * ramp, so along x every ROW is identical (the strip runs along x and is
 * replicated down y), along y every COLUMN is, and by index neither. These
 * two read the same thing off the two ends of the console: the device's own
 * engine frame, and the local preview canvas the user is looking at.
 */
async function mirrorProjShape(base) {
  const buf = Buffer.from(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
  const px = [];
  for (let i = 0; i < buf.length / 3; i++) px.push(`${buf[i * 3]},${buf[i * 3 + 1]},${buf[i * 3 + 2]}`);
  const row = (y) => px.slice(y * 64, (y + 1) * 64).join("|");
  const col = (x) => Array.from({ length: 64 }, (_, y) => px[y * 64 + x]).join("|");
  const rows = row(0) === row(1) && row(0) === row(63);
  const cols = col(0) === col(1) && col(0) === col(63);
  return rows && !cols ? "along-x" : cols && !rows ? "along-y" : "index";
}

/** The same shape, read off the editor's own preview canvas (sampled on a
 *  16x16 lattice, so it does not care how the cells are laid out). */
async function previewProjShape(pg) {
  return pg.evaluate(() => {
    const c = document.querySelector('[data-role="editor-view"] [data-role="preview"] canvas');
    if (!c) return "no-canvas";
    const g = c.getContext("2d", { willReadFrequently: true });
    const w = c.width;
    const h = c.height;
    if (!g || w < 16 || h < 16) return `small ${w}x${h}`;
    const img = g.getImageData(0, 0, w, h).data;
    const at = (x, y) => {
      const i = (y * w + x) * 4;
      return `${img[i]},${img[i + 1]},${img[i + 2]}`;
    };
    const rowAt = (y) => Array.from({ length: 16 }, (_, k) => at(Math.floor(((k + 0.5) * w) / 16), y)).join("|");
    const colAt = (x) => Array.from({ length: 16 }, (_, k) => at(x, Math.floor(((k + 0.5) * h) / 16))).join("|");
    const rows = rowAt(Math.floor(h * 0.2)) === rowAt(Math.floor(h * 0.8));
    const cols = colAt(Math.floor(w * 0.2)) === colAt(Math.floor(w * 0.8));
    return rows && !cols ? "along-x" : cols && !rows ? "along-y" : "index";
  });
}

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

// The generated clean-room library the console browses (`gallery.json`, from
// `library/`). Two fixtures off it, by the generator's OWN `kind`, so the
// layout-filter checks (#562/#563) never hard-code a pattern name: one the
// 120 px strip mirror can show, and one it cannot.
const galleryJson = JSON.parse(fs.readFileSync("public/gallery.json", "utf8"));
const grid2d = galleryJson.find((p) => p.kind === "grid");
const strip1d = galleryJson.find((p) => p.kind === "strip");
if (!grid2d || !strip1d) {
  // Fail loudly here rather than as a confusing TypeError 3000 lines down.
  throw new Error("public/gallery.json has no grid/strip pattern — run `npm run build` first");
}

/** A real mouse click on a Settings control, scrolled into view first — the
 * Output card sits far down a scrolling panel, where a bare `page.click`
 * fails with "Node is either not clickable or not an HTMLElement". */
async function clickRole(page, role) {
  const sel = `[data-role="${role}"]`;
  await page.$eval(sel, (el) => el.scrollIntoView({ block: "center" }));
  await page.click(sel);
}

/**
 * Open a console on `base` with NO resumed working copy in the way (#585).
 *
 * Since #585 a boot only live-pushes the autosaved copy when it is an unsaved
 * edit of the program the device is ALREADY RUNNING; anything else resumes in
 * local preview and writes nothing. A copy left behind by an earlier section
 * is never that for a mirror this run has just started, so a section that is
 * about the push (the capacity model, below) has to arrive without one. The
 * `luxel.current` key is per-origin and every page here shares it, hence the
 * load-clear-reload rather than a plain `goto`.
 */
async function gotoConsole(pg, base, route = "") {
  const url = `http://localhost:${PORT}/?device=${encodeURIComponent(base)}${route}`;
  await pg.goto(url, { waitUntil: "networkidle0" });
  const had = await pg.evaluate(() => {
    const was = localStorage.getItem("luxel.current") !== null;
    localStorage.removeItem("luxel.current");
    return was;
  });
  if (had) await pg.reload({ waitUntil: "networkidle0" });
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

  // ---- the shell: default page, header, routes (#538) ----
  // Its own browser context (so the working copy is this page's alone) and
  // its own stored pattern, removed again at the end: everything after this
  // block expects DEV's library empty.
  {
    const shellCtx = await browser.createBrowserContext();
    const pg = await shellCtx.newPage();
    const shellId = (
      await (
        await fetch(`${DEV}/api/patterns`, {
          method: "POST",
          body: await lxpBody("Shell Check", "export function render(index) { hsv(0.3, 1, 1) }"),
        })
      ).json()
    ).id;
    const ranBefore = await (await fetch(`${DEV}/api/pattern`)).text();
    await fetch(`${DEV}/api/patterns/${shellId}/activate`, { method: "POST" });
    const brightBefore = (await (await fetch(`${DEV}/api/brightness`)).json()).brightness;
    // Every request a device-mode cold load makes, so the sweep below can say
    // that none of them failed (Gitea #564: the corpus gallery was probed
    // twice on every load and 404'd twice, against a 3-socket pool).
    const badReqs = [];
    pg.on("response", (r) => {
      if (r.status() >= 400) badReqs.push(`${r.status()} ${r.url()}`);
    });
    try {
      await pg.setViewport({ width: 1200, height: 800 });
      await pg.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
        waitUntil: "networkidle0",
      });
      await pg.waitForSelector('[data-role="patterns-panel"]:not([hidden])', { timeout: 8000 });
      check(
        "shell: a device-mode load makes no failing request (#564)",
        badReqs.length === 0,
        badReqs.join(", "),
      );
      check(
        "shell: …and never asks the device for the corpus gallery",
        !badReqs.some((u) => u.includes("pixelblaze-library.json")),
      );
      check(
        "shell: the corpus tab is a playground affordance — absent on a console",
        (await pg.$('[data-role="patterns-source-pixelblaze"]')) === null,
      );

      // Jeremy, 2026-09-19: a console opens on Patterns → On device with the
      // running pattern lit, NOT in the editor.
      check(
        "shell: a console opens on the Patterns page, not in the editor",
        (await pg.$('[data-role="editor-view"]:not([hidden])')) === null,
      );
      check(
        "shell: …on the On device source",
        await pg.$eval(
          '[data-role="patterns-grid"][data-source="device"]',
          (el) => !el.hasAttribute("hidden"),
        ),
      );
      await pg
        .waitForFunction(() => document.querySelector('[data-role="tile-playing"]') !== null, {
          timeout: 8000,
        })
        .catch(() => null);
      check(
        "shell: …with the running pattern's tile lit",
        (await pg.$('[data-role="tile-playing"]')) !== null,
      );

      // header geometry + colours (mockup S1)
      const hdr = await pg.$eval("header", (el) => {
        const cs = getComputedStyle(el);
        return { h: el.getBoundingClientRect().height, pad: cs.paddingLeft };
      });
      check("shell: the header is the mock's 44px bar", hdr.h === 44 && hdr.pad === "16px", JSON.stringify(hdr));
      const tabs = await pg.$$eval('[data-role="tabs"] button', (els) =>
        els.map((e) => {
          const cs = getComputedStyle(e);
          return { on: e.classList.contains("active"), color: cs.color, rule: cs.borderBottomColor };
        }),
      );
      const on = tabs.find((t) => t.on);
      const off = tabs.find((t) => !t.on);
      check(
        "shell: the active tab is bright text with an amber underline, not amber text",
        on?.color === "rgb(215, 218, 224)" &&
          on?.rule === "rgb(232, 163, 61)" &&
          off?.color === "rgb(138, 144, 160)",
        JSON.stringify(tabs),
      );

      // the device chip names the DEVICE — never the literal word "device"
      const st = await (await fetch(`${DEV}/api/status`)).json();
      const wantName = st.name ?? `127.0.0.1:${DEV_PORT}`;
      const chipName = await pg.$eval('[data-role="device-chip-name"]', (e) =>
        (e.textContent ?? "").trim(),
      );
      check("shell: the header chip names the device", chipName === wantName, chipName);
      check(
        "shell: …followed by its layout",
        (await pg.$eval('[data-role="layout-label"]', (e) => (e.textContent ?? "").trim())) ===
          "120 px strip",
      );

      // brightness in the header (Jeremy): one write per drag, not per step
      let brightPosts = 0;
      pg.on(
        "request",
        (r) => r.method() === "POST" && r.url().includes("/api/brightness") && brightPosts++,
      );
      await pg.$eval('[data-role="hdr-brightness"]', (el) => {
        for (const v of [1, 2, 3, 4, 5]) {
          el.value = String(v);
          el.dispatchEvent(new Event("input", { bubbles: true }));
        }
        el.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await sleep(700);
      check("shell: the header brightness writes once per drag", brightPosts === 1, String(brightPosts));
      check(
        "shell: …and the device took the value",
        (await (await fetch(`${DEV}/api/brightness`)).json()).brightness === 5,
      );
      check(
        "shell: …and the readout tracks it",
        (await pg.$eval('[data-role="hdr-brightness-val"]', (e) => e.textContent.trim())) === "5",
      );

      // a route per page; a refresh reopens it (Jeremy asked for Settings)
      await pg.click('[data-role="tab-settings"]');
      await pg.waitForSelector('[data-role="settings-panel"]:not([hidden])', { timeout: 8000 });
      check(
        "route: opening a tab names it in the URL",
        (await pg.evaluate(() => location.hash)) === "#/settings",
        await pg.evaluate(() => location.hash),
      );
      await pg.reload({ waitUntil: "networkidle0" });
      await pg.waitForSelector('[data-role="settings-panel"]:not([hidden])', { timeout: 8000 });
      check(
        "route: a refresh reopens Settings",
        (await pg.$('[data-role="settings-panel"]:not([hidden])')) !== null &&
          (await pg.evaluate(() => location.hash)) === "#/settings",
      );
      await pg.goBack({ waitUntil: "domcontentloaded" });
      await sleep(600);
      check(
        "route: back returns to the Patterns page",
        (await pg.$('[data-role="patterns-panel"]:not([hidden])')) !== null,
      );

      // the editor is a screen of its own — the shell header goes away
      await pg.$$eval('[data-role="patterns-grid"]:not([hidden]) [data-role="tile-edit"]', (els) =>
        els[0].click(),
      );
      await pg.waitForSelector('[data-role="editor-view"]:not([hidden])', { timeout: 8000 });
      check(
        "shell: the shell header is gone while editing",
        (await pg.$('[data-role="tabs"]')) === null && (await pg.$('[data-role="fps"]')) === null,
      );
      check(
        "shell: …and the editor carries the device chip instead",
        (await pg.$('[data-role="editor-view"] [data-role="layout-chip"]')) !== null,
      );
      check("route: the editor has its own path", (await pg.evaluate(() => location.hash)) === "#/editor");

      // Popovers dodge the viewport instead of hanging off it (Jeremy). The
      // editor's ⋯ is the worst case: hard against the right edge, and on a
      // short viewport there is no room BELOW it either.
      await pg.setViewport({ width: 420, height: 380 });
      await sleep(400);
      await pg.click('[data-role="overflow"]');
      await pg.waitForSelector('[data-role="editor-menu"]', { timeout: 4000 });
      const box = await pg.$eval('[data-role="editor-menu"]', (e) => {
        const r = e.getBoundingClientRect();
        const a = document.querySelector('[data-role="overflow"]').getBoundingClientRect();
        return { l: r.left, r: r.right, t: r.top, b: r.bottom, w: innerWidth, h: innerHeight, ar: a.right };
      });
      check(
        "popover: it dodges every viewport edge",
        box.l >= 0 && box.r <= box.w && box.t >= 0 && box.b <= box.h,
        JSON.stringify(box),
      );
      check(
        "popover: it is the mock's 214px menu, hung off its anchor",
        Math.round(box.r - box.l) === 214 && Math.abs(box.r - Math.min(box.ar, box.w - 8)) < 2,
        JSON.stringify(box),
      );
      await pg.screenshot({ path: `${shotDir}/device-e2e-shell-menu-420.png` });
      await pg.keyboard.press("Escape");
      await sleep(200);
      check("popover: Escape closes it", (await pg.$('[data-role="editor-menu"]')) === null);
      await pg.setViewport({ width: 1200, height: 800 });
      await sleep(300);
      await pg.click('[data-role="editor-back"]');
      await sleep(300);
    } finally {
      // put DEV back exactly as it was found: same brightness, same running
      // pattern, empty library
      await fetch(`${DEV}/api/brightness`, { method: "POST", body: String(brightBefore) }).catch(
        () => {},
      );
      await fetch(`${DEV}/api/patterns/${shellId}`, { method: "DELETE" }).catch(() => {});
      await fetch(`${DEV}/api/code`, { method: "POST", body: await lxpBody("", ranBefore) }).catch(
        () => {},
      );
      await pg.close();
      await shellCtx.close();
    }
  }

  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  // No device-URL field any more: a real device serves the UI from its own
  // flash (auto-connect to same origin); here we use the `?device=` dev
  // override to point the built playground at the mirror, and it auto-connects.
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}${EDIT}`, {
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
  // audit E2: one word in both modes — WHERE it lands is the save state's
  // job, not the button's ("Save to device" was the wrong text, #538)
  // …or `Saved`, once there is nothing to save (#738). Still ONE word.
  check(
    "device: the primary action still reads Save",
    /^Saved?$/.test(await page.$eval('[data-role="save"]', (el) => el.textContent.trim())),
  );
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
      await mappedPage.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(MAPPED)}${EDIT}`, {
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
      // Poison the persisted "Preview as" choice before the reload (Gitea
      // #539). It is the PLAYGROUND's control, but it lives in localStorage
      // and the pre-v2 editor's layout select wrote the same key, so a real
      // console carries stale values. A `map` choice is the sharp one: its
      // coordinates are never persisted, so the reconciler used to fall
      // through to a bare strip and present this 64x64 panel as 4096 px in 1D
      // — the header chip, every tile and Settings' projection block with it.
      // Everything below this line now runs with that value in place.
      await mappedPage.evaluate(() =>
        localStorage.setItem("luxel.previewAs", JSON.stringify({ mode: "map", pixels: 4096 })),
      );
      await mappedPage.setViewport({ width: 1400, height: 900 });
      await reloadInto(mappedPage);
      check(
        "layout: the stale Preview-as choice is in place for the checks below",
        (await mappedPage.evaluate(() => localStorage.getItem("luxel.previewAs")))?.includes("map"),
        "precondition",
      );
      // Settings → LED layout states the fixture (A8, #469): the installed
      // 64×64 grid makes this Layout a `map`, and the headline is the shape.
      const r = await mappedPage
        .waitForFunction(
          () => {
            const k = document.querySelector('[data-role="layout-kind"]')?.value;
            const head = document.querySelector('[data-role="layout-headline"]')?.textContent;
            return head?.includes("64 × 64") ? `${k} ${head.trim()}` : false;
          },
          { timeout: 10000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(
        "layout: a 64x64 panel console opens on a 64x64 grid",
        r === "map 64 × 64 matrix",
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
      await reloadInto(mappedPage);
      await sleep(1500);
      const shape1d = await mappedPage.$eval('[data-role="editor-view"] [data-role="preview"]', (el) => el.dataset.shape);
      check("layout: a 1D pattern on a panel previews as the panel, not a bar", shape1d === "grid");

      // audit E9: a console runs two loops and states both — the device's own
      // rate was already in the shell header, so showing only that here told
      // you nothing new (Jeremy, 2026-09-19).
      const dims = await mappedPage
        .$eval('[data-role="preview-dims"]', (el) => (el.textContent ?? "").trim())
        .catch(() => "");
      // `60/92 fps · 64×64 matrix` — RATES first, layout after, so the 360px
      // rail's ellipsis takes the layout and not the numbers (mockup S2).
      check(
        "E9: the console preview header states the local AND the device rate",
        /^\d+\/\d+ fps · 64×64 matrix/.test(dims),
        dims,
      );

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
      // audit E10 (Jeremy: "it should be using the same widget as from
      // settings in a popup") — the collapsed row stays, the CHANGE opens
      // settings/ProjectionCard.svelte, the same component and the same
      // engine-supplied labels.
      await mappedPage.click('[data-role="projection-change"]');
      await mappedPage.waitForSelector('[data-role="projection-options"]', { timeout: 2000 });
      const projCards = await mappedPage.$$eval(
        '[data-role="projection-options"] [data-role="projection-card"]',
        (els) => els.map((e) => e.dataset.mode),
      );
      check(
        "E10: change opens the Settings projection cards, one per engine option",
        projCards.length === 3 && projCards.includes("index") && projCards.includes("x"),
        projCards.join(","),
      );
      check(
        "E10: the popup carries its own reset to the device default",
        (await mappedPage.$('[data-role="projection-use-default"]')) !== null,
      );
      check(
        "E10: each card previews this pattern on this fixture (a live canvas)",
        (await mappedPage.$$eval('[data-role="projection-options"] canvas', (els) => els.length)) === 3,
      );
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-projection-popup.png` });
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
      // …and it REACHES THE DEVICE (Gitea #598). Before this, the pick only
      // reconfigured the local preview engine, so the row said "along x ·
      // override" while the LEDs stayed on the device default — which is what
      // #538's "per pattern override … definitely isn't applied live" was.
      // It is live-only and per-working-copy, so the Layout's stored default
      // must NOT move with it.
      await sleep(600);
      const ovrDev = await mirrorProjShape(MAPPED);
      const ovrLayout = (await (await fetch(`${MAPPED}/api/layout`)).json()).proj.proj1d;
      check(
        "projection: an editor override is applied on the device, live",
        ovrDev === "along-x" && ovrLayout === "index",
        `${ovrDev} / device default ${ovrLayout}`,
      );
      check(
        "projection: …and the preview shows the same thing",
        (await previewProjShape(mappedPage)) === "along-x",
      );
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-projection.png` });
      await mappedPage.click('[data-role="projection-reset"]');
      await sleep(800);
      check(
        "projection: reset goes back to the device default",
        (await mappedPage.$('[data-role="projection-change"]')) !== null,
      );
      const resetDev = await mirrorProjShape(MAPPED);
      check(
        "projection: reset puts the DEVICE back on its default too",
        resetDev === "index",
        resetDev,
      );
      // and the same reset lives INSIDE the popup, for the trip that starts
      // by opening it rather than by noticing the override
      await mappedPage.click('[data-role="projection-change"]');
      await mappedPage.waitForSelector('[data-role="projection-options"]', { timeout: 2000 });
      await mappedPage.click('[data-role="projection-opt-y"]');
      await sleep(500);
      await mappedPage.click('[data-role="projection-value"]');
      await mappedPage.waitForSelector('[data-role="projection-use-default"]', { timeout: 2000 });
      await mappedPage.click('[data-role="projection-use-default"]');
      await sleep(500);
      check(
        "E10: 'use device default' in the popup clears the override",
        (await mappedPage.$('[data-role="projection-change"]')) !== null &&
          (await mappedPage.$('[data-role="projection-options"]')) === null,
      );
      // ---- the Settings projection cards, the DEVICE DEFAULT half (#538) ----
      // A card is one `POST /api/layout proj1d …`; the device re-reads it on
      // the next frame and the console's whole Layout follows, preview
      // included. Both ends are read as a frame, not as an echo.
      await reloadInto(mappedPage, "#/settings");
      await mappedPage.waitForSelector('[data-role="projection-block"]', { timeout: 8000 });
      await mappedPage.screenshot({ path: `${shotDir}/device-e2e-panel-projection-settings.png` });
      await mappedPage.click('[data-role="projection-block"] [data-role="projection-card"][data-mode="y"]');
      await sleep(1200);
      const defDev = await mirrorProjShape(MAPPED);
      const defStored = (await (await fetch(`${MAPPED}/api/layout`)).json()).proj.proj1d;
      check(
        "projection: a Settings card applies on the device, live and stored",
        defDev === "along-y" && defStored === "y",
        `${defDev} / ${defStored}`,
      );
      await reloadInto(mappedPage, EDIT);
      await sleep(1500);
      const defPrev = await previewProjShape(mappedPage);
      const defRow = await mappedPage
        .$eval('[data-role="projection-value"]', (el) => (el.textContent ?? "").trim())
        .catch(() => "");
      check(
        "projection: the console preview follows the new device default",
        defPrev === "along-y" && defRow === "device default · along y",
        `${defPrev} / ${defRow}`,
      );
      await fetch(`${MAPPED}/api/layout`, { method: "POST", body: "proj1d index" }); // as found
      await sleep(600);

      // back to the 2D pattern: native on this Layout, so no row at all
      await fetch(`${MAPPED}/api/code`, {
        method: "POST",
        body: await lxpBody("", "export function render2D(index, x, y) { hsv(x, 1, y) }"),
      });
      await reloadInto(mappedPage);
      await sleep(1500);
      check(
        "projection: no row for a 2D pattern on a matrix",
        (await mappedPage.$('[data-role="projection-row"]')) === null,
      );

      // Patterns tiles + Playlist rows take the device's shape (#463: the
      // thumbnail used to be a 64-px bar on every board)
      await mappedPage.click('[data-role="editor-back"]');
      await mappedPage.click('[data-role="tab-patterns"]');
      await mappedPage.waitForSelector(`${DTILE} canvas`, { timeout: 8000 });
      await sleep(1500);
      const thumbShapes = await mappedPage.$$eval(`${DTILE}:not([hidden])`, (els) =>
        els.map((e) => e.dataset.kind),
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

      // the other half of the projection-rule pair (#538): what a strip hides
      // a 64x64 matrix shows, and only the 3D patterns are filtered here
      await mappedPage.click('[data-role="patterns-source-library"]');
      await sleep(2500);
      const libOnPanel = await mappedPage.evaluate(() => {
        const name = (e) => e.querySelector('[data-role="tile-name"]')?.textContent ?? "";
        const tiles = [...document.querySelectorAll('[data-source="library"] .tile')];
        const ct = document.querySelector('[data-role="patterns-source-library"] .ct');
        return {
          shown: tiles.length,
          chip: Number((ct?.textContent ?? "").trim()),
          plane: tiles.some((e) => /2D Fireworks Fade/.test(name(e))),
          cloud: tiles.some((e) => /3D Rotation/.test(name(e))),
        };
      });
      check(
        "filter: a 2D library pattern IS offered on a 64x64 panel console",
        libOnPanel.plane === true && libOnPanel.chip === libOnPanel.shown,
        JSON.stringify(libOnPanel),
      );
      check(
        "filter: …and only the 3D ones are dropped there",
        libOnPanel.cloud === false && libOnPanel.shown > 250,
        JSON.stringify(libOnPanel),
      );
      await mappedPage.click('[data-role="patterns-source-device"]');
      await sleep(600);

      // Settings names the projections a 64x64 MATRIX offers. Since #545 a
      // Layout never shows a pattern BIGGER than itself, so the rows are the
      // kinds SMALLER than this one: 1D alone. (A 3D pattern is no longer
      // offered on a matrix at all, and a strip's 2D/3D rows never were this
      // page's — #539: the stale choice made the whole page a strip's.)
      await mappedPage.click('[data-role="tab-settings"]');
      await sleep(800);
      const projRows = await mappedPage.$$eval('[data-role="projection-kind"]', (els) =>
        els.map((e) => e.dataset.dims).join(","),
      );
      // Since #545 a Layout never shows a pattern of HIGHER dimensionality,
      // so a matrix's only non-native kind is 1D.
      check(
        "settings: a panel console offers the projections of a matrix",
        projRows === "1",
        projRows,
      );
      await mappedPage.click('[data-role="tab-patterns"]');
      await sleep(300);

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
        "thumbs: a desktop row thumbnail is the mock's 44 px square (S4)",
        deskThumb.length > 0 && deskThumb.every((w) => w === 44),
        deskThumb.join(","),
      );
      // S4 has no ↑/↓ movers: the handle is the reorder affordance, with
      // arrow keys on it as the keyboard path (#538 §F)
      const movers = await mappedPage.$$eval(
        '[data-role="playlist-item"] button',
        (els) => els.filter((e) => /move (up|down)/i.test(e.getAttribute("aria-label") ?? "")).length,
      );
      check("playlist: the row has no ↑/↓ mover buttons (S4)", movers === 0, String(movers));

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
        '[data-role="pl-values-toggle"], [data-role="pl-remove"]',
        (els) => els.map((e) => Math.round(e.getBoundingClientRect().height)),
      );
      check(
        // S4b draws `.chip{height:26px}` — above the 24px floor, below the
        // 32px the row used to force
        "mobile: chips and ✕ are thumb-sized targets",
        targets.length > 0 && targets.every((h) => h >= 26),
        targets.join(","),
      );
      // S4b folds the duration off the chip row and onto the subtitle line
      const mobileDur = await mappedPage.evaluate(() => {
        const row = document.querySelector('[data-role="playlist-item"]');
        const chip = row.querySelector('[data-role="pl-duration"]');
        const line = row.querySelector('[data-role="pl-duration-inline"]');
        return {
          chip: chip ? getComputedStyle(chip).display : "absent",
          line: line ? getComputedStyle(line).display : "absent",
          text: line ? (line.textContent ?? "").trim() : "",
        };
      });
      check(
        "mobile: the duration moves onto the subtitle line (S4b)",
        mobileDur.chip === "none" && mobileDur.line !== "none" && /\d/.test(mobileDur.text),
        JSON.stringify(mobileDur),
      );
      // the defaults field has to be READABLE at 390 as well as at 1400
      // (Jeremy: "the text box is so small that … i cannot read it")
      const defaults390 = await mappedPage.$eval('[data-role="pl-default-sec"]', (el) => ({
        w: Math.round(el.getBoundingClientRect().width),
        clipped: el.scrollWidth > el.clientWidth + 1,
      }));
      check(
        "mobile: the default-duration field is readable at 390 px",
        defaults390.w >= 64 && !defaults390.clipped,
        JSON.stringify(defaults390),
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
            const m = /^(\d+) fps$/.exec(t);
            return m && Number(m[1]) > 0 ? t : false;
          },
          { timeout: 8000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(
        "fps: status bar reads the device's rate, bare (mockup S1 `27 fps`)",
        /^\d+ fps$/.test(shown),
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

  // ── The on-device JIT (#658, docs/jit-design.md §4a) ────────────────
  // Two surfaces, and the rule between them: `native` is a quiet marker
  // beside the frame rate (it explains the number, it is not news), while
  // a refusal has a REASON worth reading and gets the amber strip under
  // the preview — the same row #627's compile-time prediction uses. A
  // board with no backend (`off`, most of the fleet) says nothing at all.
  // The mirror impersonates all three with `--jit`; it has no JIT and the
  // emitter is a device backend, so impersonation is the only way to drive
  // this without an S3 on the bench.
  for (const [name, flag, role, want] of [
    ["native", "native", "jit-native", true],
    ["interp", "interp:too-large", "jit-device", true],
    ["off", "off", "jit-native", false],
  ]) {
    const port = name === "interp" ? E2E.mirror.jitInterp : E2E.mirror.jitNative;
    const base = `http://127.0.0.1:${port}`;
    const dev = spawn(
      "../target/debug/luxel",
      ["serve", ...NO_NETIN, "--port", String(port), "--pixels", "120", "--fps", "24",
       "--jit", flag],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      dev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      dev.on("exit", () => reject(new Error(`jit mirror (${flag}) died`)));
      setTimeout(() => reject(new Error(`jit mirror (${flag}) start timeout`)), 30000);
    });
    process.on("exit", () => dev.kill());
    const page = await browser.newPage();
    try {
      await page.setViewport({ width: 1400, height: 900 });
      // the refusal strip lives under the editor's preview; the marker is
      // in the shell header and shows on any screen
      const hash = name === "interp" ? "#/editor" : "";
      await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(base)}${hash}`, {
        waitUntil: "networkidle0",
      });
      // wait for the poll to have landed at all, so "absent" means absent
      // rather than "not yet"
      await page
        .waitForFunction(
          () => /\d/.test(document.querySelector('[data-role="fps"]')?.textContent ?? ""),
          { timeout: 15000 },
        )
        .catch(() => {});
      const seen = await page
        .waitForFunction(
          (sel) => document.querySelector(sel)?.textContent?.trim() ?? false,
          { timeout: want ? 8000 : 2500 },
          `[data-role="${role}"]`,
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(
        `jit: --jit ${flag} ${want ? "shows" : "shows no"} [data-role=${role}]`,
        want ? Boolean(seen) : !seen,
        seen || "(absent)",
      );
      if (name === "native") {
        const st = (await fetch(`${base}/api/status`).then((r) => r.json())).jit;
        check(
          "jit: the marker agrees with /api/status.jit.state",
          st?.state === "native" && seen === "native",
          `${JSON.stringify(st)} / readout ${seen}`,
        );
      }
      if (name === "interp") {
        check(
          "jit: the device's reason is spelled out, not left as an id",
          /interpreter/.test(seen) && !/too-large/.test(seen),
          seen,
        );
      }
      await page.screenshot({ path: `${shotDir}/device-e2e-jit-${name}.png` });
    } finally {
      await page.close();
      dev.kill();
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
            return /^\d+ fps$/.test(t) ? t : false;
          },
          { timeout: 8000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(
        "fps: a panel board's displayed rate (out_fps) wins over the render rate",
        shown === "112 fps",
        shown,
      );
      const title = await panelPage
        .$eval('[data-role="fps"]', (el) => el.getAttribute("title") ?? "")
        .catch(() => "");
      check(
        "fps: tooltip names the panel, the rescan ceiling and the local preview",
        /out_fps/.test(title) && /rescan 115 Hz/.test(title) && /local preview/.test(title),
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
  check(
    "reboot bar: absent until the device asks for one (#538)",
    (await page.$('[data-role="reboot-bar"]')) === null,
  );
  const layoutOpts = await page.$$eval('[data-role="layout-kind"] option', (os) =>
    os.map((o) => o.value),
  );
  check(
    "layout: offers strip/matrix/3D/custom map",
    ["strip", "matrix", "lattice", "map"].every((k) => layoutOpts.includes(k)),
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
  // An arrangement change is stored-and-reported until a reboot builds it
  // (#475). Since #538 that is a STICKY BAR pinned to the viewport, not a
  // line of dim text at the bottom of the form — and it names the field.
  await page.$eval('[data-role="layout-snake"]', (el) => el.click());
  const rebootNote = await page
    .waitForFunction(
      () => {
        const t = document.querySelector('[data-role="reboot-bar-text"]')?.textContent?.trim();
        // the kind switch above already raised the bar, so wait for the bar
        // to LIST this change rather than for it to exist
        return t && /panel arrangement/i.test(t) ? t : false;
      },
      { timeout: 6000 },
    )
    .then((h) => h.jsonValue())
    .catch(() => "");
  check(
    "reboot bar: an arrangement change raises it, naming the field",
    /apply after a reboot/i.test(rebootNote) && /panel arrangement/i.test(rebootNote),
    rebootNote,
  );
  check(
    "reboot bar: it is pinned to the bottom of the VIEWPORT",
    await page
      .$eval('[data-role="reboot-bar"]', (el) => {
        const cs = getComputedStyle(el);
        // inset, not flush: it wears the mockups' `.capstrip` — a 1px
        // rgba(217,163,67,.32) border and a 6px radius — which only reads as
        // a strip when the bottom edge is on screen (#538)
        return cs.position === "fixed" && parseFloat(cs.bottom) <= 24;
      })
      .catch(() => false),
  );
  // …and it is on screen wherever the user is, because the device is still
  // running the old wiring wherever they are. This flow has the EDITOR open
  // (the Settings panel stays mounted behind it), which is the screen that
  // hides the shell header entirely — so if the bar is up here, it is up
  // everywhere.
  check(
    "reboot bar: it is on screen even over the editor",
    (await page.$('[data-role="editor-view"]:not([hidden])')) !== null &&
      (await page.$('[data-role="reboot-bar"]')) !== null,
  );
  const laySnake = await (await fetch(`${DEV}/api/layout`)).json();
  check("layout: the snake reached the device", laySnake.matrix?.snake === 1, JSON.stringify(laySnake.matrix));

  // Projection is its own SECTION (mockup S3e, #538) and it is ABSENT on a
  // strip: a 1D Layout shows 1D patterns and nothing else, so there is
  // nothing to choose — and no copy anywhere saying so.
  await page.select('[data-role="layout-kind"]', "strip");
  await sleep(700);
  check(
    "projection: a strip layout has no Projection section at all",
    (await page.$('[data-role="sect-projection"]')) === null &&
      (await page.$('[data-role="projection-block"]')) === null,
  );

  // On a matrix it exists, in its own section with its own rule, and picking
  // a card POSTs one `proj1d` line.
  await page.select('[data-role="layout-kind"]', "matrix");
  await sleep(700);
  {
    check(
      "projection: a 2D layout gets the section, with the fixture in its head",
      (await page.$('[data-role="sect-projection"]')) !== null,
    );
    const note = await page
      .$eval('[data-role="sect-projection-note"]', (e) => e.textContent.trim())
      .catch(() => "");
    check("projection: the section head names the fixture", /matrix$/.test(note), note);
    const projRows = await page.$$eval('[data-role="projection-kind"]', (els) =>
      els.map((e) => e.dataset.dims).join(","),
    );
    check("projection: a 2D layout offers the 1D row only (#538)", projRows === "1", projRows);
    const cards = await page.$$('[data-role="projection-kind"][data-dims="1"] [data-role="projection-card"]');
    check("projection: three 1D options on a matrix", cards.length === 3, `${cards.length} cards`);
    await page.$eval(
      '[data-role="projection-kind"][data-dims="1"] [data-role="projection-card"][data-mode="x"]',
      (el) => el.click(),
    );
    await sleep(700);
    const proj = await (await fetch(`${DEV}/api/layout`)).json();
    check(
      "projection: picking a card sets the device default",
      proj.proj?.proj1d === "x",
      JSON.stringify(proj.proj),
    );
    await page.$eval(
      '[data-role="projection-kind"][data-dims="1"] [data-role="projection-card"][data-mode="index"]',
      (el) => el.click(),
    );
    await sleep(600);
  }
  await page.select('[data-role="layout-kind"]', "strip"); // restore
  await sleep(600);
  check(
    "projection: a strip console offers no projection cards — nothing is smaller than 1D (#545)",
    (await page.$$('[data-role="projection-card"]')).length === 0,
  );

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

  // ---- the colour picker pushes like any other control (#538) ----
  // The widget is new; the WIRE is not. What it emits is still the control's
  // own 16.16 triple, so the device's pixels are the proof.
  await setEditor(
    page,
    [
      "export var h = 0, s = 1, v = 1",
      "export function hsvPickerTint(a, b, c) { h = a; s = b; v = c }",
      "export function render(index) { hsv(h, s, v) }",
    ].join("\n"),
  );
  await sleep(1400);
  await page.waitForSelector('[data-role="color-swatch"]', { timeout: 5000 });
  check(
    "colour: a console control is a swatch, not three raw channels",
    (await page.$('[data-role="editor-view"] input[type="color"]')) === null,
  );
  await page.click('[data-role="color-swatch"]');
  await page.waitForSelector('[data-role="color-hex"]', { timeout: 3000 });
  await page.$eval('[data-role="color-hex"]', (el) => {
    el.value = "#0000ff";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(900);
  const pxHsv = new Uint8Array(await (await fetch(`${DEV}/api/pixels`)).arrayBuffer());
  check(
    "colour: the picker pushes the control to the device (blue on the wire)",
    pxHsv[2] === 255 && pxHsv[0] === 0 && pxHsv[1] === 0,
    `rgb=${pxHsv[0]},${pxHsv[1]},${pxHsv[2]}`,
  );
  await page.keyboard.press("Escape");
  await sleep(200);

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


  // ---- Settings → Device → Name (Gitea #538) ----
  //
  // `POST /api/name` is the route Jeremy's "Cannot set the device's name like
  // in the mocks" asked for. Two things have to be true at once: the device
  // stores it, and every surface that says WHICH board this is follows
  // immediately — the header chip, the Settings title row.
  {
    const before = await (await fetch(`${DEV}/api/name`)).json();
    check(
      "name: the mirror answers with its own default",
      before.name === "luxel-serve" && before.source === "default",
      JSON.stringify(before),
    );
    const field = await page.$('[data-role="device-name"]');
    check("name: Settings → Device carries a text field, not a sentence", field !== null);
    check(
      "name: it shows what the device calls itself",
      (await page.$eval('[data-role="device-name"]', (el) => el.value)) === "luxel-serve",
    );
    // the hint the old build carried ("no rename endpoint") is gone
    const deviceForm = await page.$eval('[data-role="sect-device"]', (el) => el.textContent);
    check(
      "name: no `no rename endpoint` hint, and no `served from this device`",
      !/rename endpoint/i.test(deviceForm) && !/served from this device/i.test(deviceForm),
    );
    await page.$eval('[data-role="device-name"]', (el) => {
      el.value = "Kitchen Strip";
      el.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await sleep(900);
    const after = await (await fetch(`${DEV}/api/name`)).json();
    check(
      "name: the field POSTs it and the device stores it",
      after.name === "Kitchen Strip" && after.source === "stored",
      JSON.stringify(after),
    );
    const chip = await page
      .$eval('[data-role="device-chip-name"]', (el) => el.textContent.trim())
      .catch(() => "");
    check("name: the header chip follows immediately", chip === "Kitchen Strip", chip);
    const sub = await page
      .$eval('[data-role="settings-subtitle"]', (el) => el.textContent.trim())
      .catch(() => "");
    check(
      "name: the Settings title row reads `<name> · vX.Y.Z`",
      /^Kitchen Strip · v\d/.test(sub),
      sub,
    );
    // a name the device refuses leaves both the device and the field alone
    await page.$eval('[data-role="device-name"]', (el) => {
      el.value = 'a"b';
      el.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await sleep(900);
    const kept = await (await fetch(`${DEV}/api/name`)).json();
    check("name: a rejected name does not stick on the device", kept.name === "Kitchen Strip");
    check(
      "name: …nor in the field",
      (await page.$eval('[data-role="device-name"]', (el) => el.value)) === "Kitchen Strip",
    );
    // …and renaming asks for a reboot, because the hostname binds at boot
    check(
      "name: a rename raises the reboot bar (the hostname binds at boot)",
      await page
        .$eval('[data-role="reboot-bar-text"]', (el) => /device name/i.test(el.textContent))
        .catch(() => false),
    );
    // restore: an empty body clears the name back to the board default
    await page.$eval('[data-role="device-name"]', (el) => {
      el.value = "";
      el.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await sleep(900);
    const cleared = await (await fetch(`${DEV}/api/name`)).json();
    check("name: clearing it restores the default", cleared.source === "default", JSON.stringify(cleared));
    await page.evaluate(() => {
      // the bar is honest about a real pending reboot; the mirror never
      // reboots, so clear it here rather than leaving it over every later shot
      const bar = document.querySelector('[data-role="reboot-bar"]');
      if (bar) bar.remove();
    });
  }

  // ---- Settings → LED layout → 3D (Gitea #538) ----
  //
  // "I cannot use, try out, inspect 3D layout mode at all! The option isn't
  // there." It is a lattice installed as the device's coordinate map: the
  // pixel space is sized, the coordinates go out in ONE POST, and every
  // surface follows — `geom.dims` 3, cloud tiles, the 1D+2D projection rows.
  {
    await page.select('[data-role="layout-kind"]', "lattice");
    await sleep(500);
    check(
      "3D: picking it reveals w × h × d rather than POSTing anything",
      (await page.$('[data-role="layout-lat-w"]')) !== null &&
        (await (await fetch(`${DEV}/api/layout`)).json()).dims === 1,
    );
    const latCount = await page
      .$eval('[data-role="layout-lat-count"]', (el) => el.textContent.trim())
      .catch(() => "");
    check("3D: the fields state the pixel count", latCount === "512 pixels", latCount);
    await page.$eval('[data-role="layout-lat-install"]', (el) => el.click());
    await sleep(2500);
    const lay3d = await (await fetch(`${DEV}/api/layout`)).json();
    check(
      "3D: installing it makes the device a 3D map of w·h·d pixels",
      lay3d.dims === 3 && lay3d.kind === "map" && lay3d.pixels === 512 && lay3d.map?.count === 512,
      JSON.stringify({ dims: lay3d.dims, kind: lay3d.kind, pixels: lay3d.pixels, map: lay3d.map }),
    );
    const head = await page.$eval('[data-role="layout-headline"]', (el) => el.textContent.trim());
    check("3D: the summary reads `8×8×8 lattice`", head === "8×8×8 lattice", head);
    const sub3d = await page.$eval('[data-role="layout-subhead"]', (el) => el.textContent.trim());
    check("3D: …with `512 pixels` under it", /^512 pixels/.test(sub3d), sub3d);
    // the whole app follows the Layout, not just this form
    const chipShape = await page
      .$eval('[data-role="layout-label"]', (el) => el.textContent.trim())
      .catch(() => "");
    check("3D: the header chip says what shape the fixture is", chipShape === "8×8×8 lattice", chipShape);
    const rows3d = await page.$$eval('[data-role="projection-kind"]', (els) =>
      els.map((e) => e.dataset.dims).join(","),
    );
    check("3D: the Projection section offers the 1D and 2D rows (#538)", rows3d === "1,2", rows3d);
    const opts1d = await page.$$eval(
      '[data-role="projection-kind"][data-dims="1"] [data-role="projection-card"]',
      (els) => els.map((e) => e.dataset.mode).join(","),
    );
    check("3D: a 1D pattern can go by index or along any axis", opts1d === "index,x,y,z", opts1d);
    // every thumbnail follows the Layout, so a lattice draws as a CLOUD —
    // the tiles on the Patterns tab are the same component
    const thumbShapes = await page.$$eval('[data-role="sect-layout"] [data-shape]', (els) =>
      els.map((e) => e.dataset.shape).join(","),
    );
    check(
      "3D: the live thumbnails draw a cloud, not a bar",
      thumbShapes.length > 0 && thumbShapes.split(",").every((x) => x === "cloud"),
      thumbShapes,
    );
    await shotSettings(page, `${shotDir}/settings-lattice.png`);
    // the Patterns tab draws it as a cloud, because that is what it is
    // Restore the strip the rest of this file expects — through the UI, not
    // a raw fetch: `applyLayout` is what teaches the browser the map is gone
    // (a bare `fetch` leaves `deviceMapCoords` holding the lattice, and every
    // thumbnail keeps drawing a cloud).
    await fetch(`${DEV}/api/layout`, { method: "POST", body: "map" }); // clear the map
    await page.select('[data-role="layout-kind"]', "strip");
    await sleep(900);
    await page.$eval('[data-role="layout-pixels"]', (el) => {
      el.value = "120";
      el.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await sleep(900);
    const restored3d = await (await fetch(`${DEV}/api/layout`)).json();
    check(
      "3D: …and switching back to Strip leaves the fixture as it was",
      restored3d.kind === "strip" && restored3d.dims === 1 && restored3d.pixels === 120,
      JSON.stringify({ kind: restored3d.kind, dims: restored3d.dims, pixels: restored3d.pixels }),
    );
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

  // Clock & time zone, through the FORM (Gitea #538): a zone is a place, not
  // a number; there is a way to ask for a sync; and the device time is
  // printed in the looking user's locale.
  {
    await openAdv(page, "adv-clock");
    await sleep(500);
    const tag = await page.$eval('[data-role="clock-tz"]', (el) => el.tagName);
    check("clock: the time zone is a select of real zone names", tag === "SELECT", tag);
    const groups = await page.$$eval('[data-role="clock-tz"] optgroup', (els) =>
      els.map((e) => e.label),
    );
    check(
      "clock: zones are grouped by region",
      groups.includes("America") && groups.includes("Europe") && groups.length > 4,
      groups.slice(0, 6).join(","),
    );
    const hasDenver = await page.$$eval('[data-role="clock-tz"] option', (els) =>
      els.some((e) => e.value === "America/Denver"),
    );
    check("clock: …and they are IANA names, not offsets", hasDenver);
    // picking one sends the zone's CURRENT offset as tzMinutes
    // …a zone with a HALF-hour offset and no DST, so the expected number is
    // stable and proves the wire is minutes rather than hours. (Chromium's
    // list carries the legacy spelling, which is why this is not
    // `Asia/Kolkata` — the select is built from that list.)
    const ZONE = "Asia/Calcutta";
    await page.select('[data-role="clock-tz"]', ZONE);
    await sleep(900);
    const tz = await (await fetch(`${DEV}/api/clock`)).json();
    check(
      "clock: picking a zone sends its offset in minutes",
      tz.tzMinutes === 330,
      JSON.stringify(tz),
    );
    const offLine = await page.$eval('[data-role="clock-offset"]', (el) => el.textContent.trim());
    check("clock: the status line states `UTC+5:30 · synced`", /UTC\+5:30 · synced/.test(offLine), offLine);
    // the device time is the user's locale format, with a DATE — never the
    // old `en-US` 24-hour string
    const shown = await page.$eval('[data-role="clock-status"]', (el) => el.textContent.trim());
    const want = new Date(Date.now()).toLocaleString(undefined, {
      timeZone: ZONE,
      dateStyle: "medium",
      timeStyle: "medium",
    });
    check(
      "clock: device time is the browser locale's date + time in the chosen zone",
      shown.slice(0, 12) === want.slice(0, 12),
      `${shown} vs ${want}`,
    );
    // the zone NAME is the browser's memory; the device only ever knew minutes
    const stored = await page.evaluate(() => localStorage.getItem("luxel.clock.zone"));
    check("clock: the chosen zone name is remembered locally", stored === ZONE, stored);
    // `Sync now` POSTs /api/clock/sync
    check("clock: there is a Sync now button", (await page.$('[data-role="clock-sync"]')) !== null);
    await page.$eval('[data-role="clock-sync"]', (el) => el.click());
    const note = await page
      .waitForFunction(
        () => document.querySelector('[data-role="clock-note"]')?.textContent?.trim() ?? false,
        { timeout: 6000 },
      )
      .then((h) => h.jsonValue())
      .catch(() => "");
    check("clock: Sync now reports what the device answered", /sync/i.test(note), note);
    // (no screenshot here: this flow has the editor open over the Settings
    // panel, so `shotSettings` would photograph the editor — the page's own
    // shots are taken in the "Settings, ranked" block further down)
    await page.select('[data-role="clock-tz"]', "UTC");
    await sleep(700);
    await fetch(`${DEV}/api/clock`, { method: "POST", body: "0" });
    await page.$eval('[data-role="adv-clock-toggle"]', (el) => el.click()); // collapse again
    await sleep(300);
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
  // audit E6 (Jeremy 2026-09-19): "saving a pattern from the library doesn't
  // automatically change the UI so that the pattern has playlist, delete, etc.
  // options in the overflow menu". The on-device verbs key off the stored id,
  // so the save must END with one — from the reply, or from the freshly-read
  // list matched by name.
  await page.click('[data-role="overflow"]');
  await page.waitForSelector('[data-role="editor-menu"]', { timeout: 4000 });
  const afterSaveMenu = await page.$$eval('[data-role="editor-menu"] > *', (els) =>
    els.map((e) => (e.classList.contains("sepr") ? "—" : (e.dataset.role ?? "?"))),
  );
  check(
    "E6: after a save the ⋯ menu gains Add to playlist and Delete",
    afterSaveMenu.includes("add-to-playlist") && afterSaveMenu.includes("delete"),
    afterSaveMenu.join(","),
  );
  check(
    "E6: and the on-device group is ruled off first, as in the mock",
    afterSaveMenu.join(",") === "add-to-playlist,—,duplicate,epe-export,epe-import,—,delete",
    afterSaveMenu.join(","),
  );
  await page.keyboard.press("Escape");
  await sleep(200);
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
  const stripThumbs = await page.$$eval(`${DTILE}:not([hidden])`, (els) => els.map((e) => e.dataset.kind));
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

    // ── the projection rule (Jeremy, #538): a 120 px STRIP cannot show a
    //    2D pattern, so the library simply does not offer it — but a stored
    //    one is the user's own and drops into "Not for this layout (N)",
    //    drawn in its own shape with no Play verb.
    const planeId = (
      await (
        await fetch(`${DEV}/api/patterns`, {
          method: "POST",
          body: await lxpBody("tile plane 2D", "export function render2D(i, x, y) { hsv(x, 1, y) }"),
        })
      ).json()
    ).id;
    await page.click('[data-role="patterns-source-device"]'); // re-reads /api/patterns
    await sleep(1200);
    check(
      "filter: a stored 2D pattern is NOT in the strip console's grid",
      (await page.$(`${DGRID} > .tiles .tile[data-key="${planeId}"]`)) === null,
    );
    const groupBefore = await page.evaluate(() => {
      const g = document.querySelector('[data-role="patterns-incompatible"]');
      const t = document.querySelector('[data-role="patterns-incompatible-toggle"]');
      return {
        shown: g !== null && !g.hidden,
        label: (t?.textContent ?? "").replace(/\s+/g, " ").trim(),
        expanded: t?.getAttribute("aria-expanded") ?? "",
      };
    });
    check(
      "filter: it lands in a collapsed 'Not for this layout' group",
      groupBefore.shown && groupBefore.expanded === "false" && /\(1\)/.test(groupBefore.label),
      JSON.stringify(groupBefore),
    );
    await page.click('[data-role="patterns-incompatible-toggle"]');
    await sleep(1800);
    const grouped = await page.evaluate((id) => {
      const g = document.querySelector('[data-role="patterns-incompatible"]');
      const tile = g?.querySelector(`.tile[data-key="${id}"]`);
      return {
        present: tile !== null && tile !== undefined && !tile.hidden,
        kind: tile?.dataset.kind ?? "",
        play: tile?.querySelector('[data-role="tile-play"]') !== null,
        edit: tile?.querySelector('[data-role="tile-edit"]') !== null,
      };
    }, planeId);
    check(
      "filter: the group draws it in its OWN shape (Auto), with Edit but no Play",
      grouped.present && grouped.kind === "grid" && !grouped.play && grouped.edit,
      JSON.stringify(grouped),
    );
    await page.screenshot({ path: `${shotDir}/device-e2e-strip-incompatible.png` });
    await page.click('[data-role="patterns-incompatible-toggle"]');
    await sleep(300);
    await fetch(`${DEV}/api/patterns/${planeId}`, { method: "DELETE" });
    await page.click('[data-role="patterns-source-device"]');
    await sleep(800);

    // and the LIBRARY on a strip simply loses its 2D patterns — the chip
    // counts what is on screen (the panel console's twin check is above)
    await page.click('[data-role="patterns-source-library"]');
    await sleep(2000);
    const libOnStrip = await page.evaluate(() => {
      const name = (e) => e.querySelector('[data-role="tile-name"]')?.textContent ?? "";
      const tiles = [...document.querySelectorAll('[data-source="library"] .tile')];
      const ct = document.querySelector('[data-role="patterns-source-library"] .ct');
      return {
        shown: tiles.length,
        chip: Number((ct?.textContent ?? "").trim()),
        plane: tiles.some((e) => /2D Fireworks Fade/.test(name(e))),
      };
    });
    check(
      "filter: a 2D library pattern is absent on a strip console",
      libOnStrip.plane === false &&
        libOnStrip.shown > 0 &&
        libOnStrip.shown < 250 &&
        libOnStrip.chip === libOnStrip.shown,
      JSON.stringify(libOnStrip),
    );
    await page.click('[data-role="patterns-source-device"]');
    await sleep(600);

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

    // mockups S1/S1b/S1c all draw the playing tile FIRST — the console opens
    // on this page to see what the LEDs are doing (#538 fidelity closure)
    const firstIsPlaying = await page.evaluate(
      (grid) => document.querySelector(`${grid} > .tiles .tile`)?.classList.contains("playing") ?? false,
      DGRID,
    );
    check("patterns: the playing tile sorts to the front of the grid", firstIsPlaying);

    // Jeremy, #555: the tile that is already playing wears the ▶ pill
    // top-left and keeps its strip, MINUS the one verb that makes no sense
    // there — no "Play" for what is already playing, but Edit and ⋯ stay, or
    // the running pattern would be the only one you cannot open or delete.
    const playingTile = await page.evaluate((sel) => {
      const el = document.querySelector(sel);
      if (!el) return null;
      const pill = el.querySelector('[data-role="tile-playing"]');
      return {
        pill: pill ? (pill.textContent ?? "").trim() : "",
        pillTop: pill ? pill.getBoundingClientRect().top - el.getBoundingClientRect().top : -1,
        play: el.querySelector('[data-role="tile-play"]') !== null,
        edit: el.querySelector('[data-role="tile-edit"]') !== null,
        menu: el.querySelector('[data-role="tile-menu"]') !== null,
        ring: getComputedStyle(el).boxShadow,
      };
    }, victim);
    check(
      "patterns: the playing tile has the ▶ pill top-left and no Play verb",
      playingTile !== null &&
        /playing/.test(playingTile.pill) &&
        playingTile.pillTop < 20 &&
        !playingTile.play,
      JSON.stringify(playingTile),
    );
    check(
      "patterns: …but keeps Edit and ⋯ (#555)",
      playingTile !== null && playingTile.edit && playingTile.menu,
      JSON.stringify(playingTile),
    );
    check(
      "patterns: the playing tile wears the 2 px --ok ring",
      playingTile !== null && playingTile.ring === "rgb(95, 191, 122) 0px 0px 0px 2px",
      playingTile?.ring,
    );
    // a tile the device is running is still a full citizen of the grid: the
    // ⋯ checks below drive it WHILE it plays
    // ⋯ → Add to playlist appends an item through the playlist store
    await tileAction(page, victim, "tile-menu");
    await page.waitForSelector('[data-role="tile-menu-popup"]', { timeout: 3000 });
    await page.screenshot({ path: `${shotDir}/device-e2e-tile-menu.png` });
    // the menu is mockup S2's `.menu`: three separated groups, the document
    // verbs in the middle, the destructive one last (#538 fidelity closure)
    const menuShape = await page.$eval('[data-role="tile-menu-popup"]', (el) => ({
      items: [...el.querySelectorAll(".mi")].map((b) => (b.textContent ?? "").trim()),
      seprs: el.querySelectorAll(".sepr").length,
      lastIsDelete: el.lastElementChild?.getAttribute("data-role") === "tile-menu-delete",
    }));
    check(
      "patterns: the tile ⋯ menu carries the mock's item list, separated",
      menuShape.items.join("|") ===
        "Add to playlist|Duplicate|Export .epe|Import .epe…|Delete" &&
        menuShape.seprs === 2 &&
        menuShape.lastIsDelete,
      JSON.stringify(menuShape),
    );
    // ⋯ → Import .epe… puts the FILE in this device's library (#572). The
    // editor's import verb replaces the open document; on a tile it is the
    // library verb instead — parse, compile, `POST /api/patterns`, and the
    // editor is never opened.
    {
      const epePath = `${shotDir}/e2e-import.epe`;
      fs.writeFileSync(
        epePath,
        JSON.stringify({
          name: "Imported Tile",
          id: "e2eimportabcdefgh",
          sources: { main: SMALL },
        }),
      );
      const before = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns ?? [];
      // the menu is dismissed first: the real verb closes it and then opens
      // the OS file dialog, which puppeteer drives by feeding the input
      await page.keyboard.press("Escape");
      await sleep(200);
      const picker = await page.$('[data-role="tile-menu-import-file"]');
      check("#572: the tile ⋯ menu's import picker is on the page", picker !== null);
      await picker.uploadFile(epePath);
      await sleep(1500);
      const after = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns ?? [];
      const added = after.find((p) => p.name === "Imported Tile");
      check(
        "#572: ⋯ → Import .epe… stores the file in the device's library",
        added !== undefined && after.length === before.length + 1,
        `${before.length} → ${after.length}`,
      );
      check(
        "#572: …and it does NOT open the editor (importing is not a device action, #563)",
        (await page.$('[data-role="editor-view"]:not([hidden])')) === null,
      );
      if (added) await fetch(`${DEV}/api/patterns/${added.id}`, { method: "DELETE" });
      fs.unlinkSync(epePath);
      await sleep(800); // let the grid settle back to the pre-import tiles
      await tileAction(page, victim, "tile-menu");
      await page.waitForSelector('[data-role="tile-menu-popup"]', { timeout: 3000 });
    }
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
        (await page.$eval('[data-role="dialog"]', (el) => el.hasAttribute("data-danger"))),
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

  // ---- what the Settings tab costs the device (Gitea #540) ----
  // The device serves from three sockets and a closing one holds its slot for
  // up to two seconds, so the page's steady-state poll load has to leave one
  // free — on the Athom it did not, and a second client (or the page's own
  // next poll) was refused. Two properties keep it there: `/api/status` is
  // fetched ONCE per second by the session poll (the tab used to re-GET the
  // whole body at 0.5 Hz as well, just to read `live`), and the tab's three
  // reads go one at a time rather than as a burst that fills both gate slots.
  {
    const inflight = new Map();
    const counts = new Map();
    let maxConcurrent = 0;
    let trioOverlap = 0;
    const isTrio = (u) => /\/api\/(mqtt|sync|clock)$/.test(u);
    const onReq = (r) => {
      inflight.set(r, r.url());
      maxConcurrent = Math.max(maxConcurrent, inflight.size);
      const n = [...inflight.values()].filter(isTrio).length;
      if (n > 1) trioOverlap++;
      const key = new URL(r.url()).pathname;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    };
    const onDone = (r) => inflight.delete(r);
    page.on("request", onReq);
    page.on("requestfinished", onDone);
    page.on("requestfailed", onDone);
    await sleep(10000);
    page.off("request", onReq);
    page.off("requestfinished", onDone);
    page.off("requestfailed", onDone);
    const status = counts.get("/api/status") ?? 0;
    const mqtt = counts.get("/api/mqtt") ?? 0;
    check(
      "settings poll: /api/status is read once a second, not twice",
      status > 0 && status <= 12,
      `${status} in 10 s`,
    );
    check(
      "settings poll: the tab's own reads stay at 0.5 Hz",
      mqtt >= 3 && mqtt <= 7,
      `${mqtt} mqtt in 10 s`,
    );
    check(
      "settings poll: the tab's reads never overlap each other",
      trioOverlap === 0,
      `${trioOverlap} overlaps`,
    );
    check(
      "settings poll: never more than the gate's two requests in flight",
      maxConcurrent <= 2,
      `${maxConcurrent} concurrent`,
    );
  }

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
    // Recolour stop 0. Since Gitea #734 the palette is the shared
    // `GradientEditor`, and a stop's colour is the app's own `ColorPicker`,
    // not an `<input type="color">` — so this drives the real controls:
    // open the stop editor (stop 0 is `picked` by default), open the
    // picker's popover from its swatch, and type a hex. The editor commits
    // on a 250 ms trailing timer (a ColorPicker drag is a pointermove storm
    // and Settings must not POST the palette per move), hence the wait.
    // Select stop 0 EXPLICITLY, and note that CLICKING A STOP OPENS THE
    // EDITOR (`GradientEditor.svelte:461`) while `out-palette-edit` is a
    // TOGGLE (:488) — so doing both closes it again. Two traps in one line:
    // adding a stop selects the one just added, so the editor opens on stop 1
    // and a test that assumes 0 silently recolours the wrong one (it did —
    // the red landed at palette[5..7], not [1..3]).
    await page.$$eval('[data-role="out-palette-stop"]', (els) => els[0].click());
    await sleep(250);
    await page.click('[data-role="out-palette-color"] [data-role="color-swatch"]');
    await sleep(200);
    await page.$eval('[data-role="color-hex"]', (el) => {
      el.focus();
      el.value = "#ff0000";
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await sleep(700);
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

  // The tile's `Edit` verb opens the editor on the stored pattern and writes
  // NOTHING to the device (Gitea #563) — a bare tile click / `Play` is what
  // changes the LEDs, checked above. The device is running the render2D
  // pattern the map section pushed, and must still be running it after.
  const beforeEdit = await (await fetch(`${DEV}/api/pattern`)).text();
  const seenReqs = [];
  page.on("request", (r) => {
    if (r.url().includes("/api/patterns") && r.method() === "DELETE") seenReqs.push(r.url());
  });
  await tileAction(page, DTILE, "tile-edit");
  await sleep(1300);
  check("library: a tile's Edit opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
  const afterEdit = await (await fetch(`${DEV}/api/pattern`)).text();
  check(
    "library: Edit on a stored pattern does NOT activate it (#563)",
    afterEdit === beforeEdit,
    afterEdit.slice(0, 60),
  );
  check(
    "library: …and the header says the editor is not driving the device",
    (await saveState(page)) === "saved · on device · preview only",
    await saveState(page),
  );
  check(
    "library: …so ▶ Play on device is offered",
    (await page.$('[data-role="editor-play-device"]')) !== null,
  );
  check("library: editor shows the stored source", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.4"));
  // delete it from the editor — an in-app danger confirmation (Gitea #472),
  // cancelled once (nothing happens) before it is accepted
  check("library: Delete is in the editor's ⋯ menu", await menuHas(page, "delete"));
  await menuClick(page, "delete");
  await waitDialog(page);
  check(
    "library: delete asks with a danger dialog",
    (await dialogTitle(page)) === "Delete pattern from the device?" &&
      (await page.$eval('[data-role="dialog"]', (el) => el.hasAttribute("data-danger"))),
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
  // The delete above left the editor holding an orphaned document in local
  // preview (#563); a clean reload re-adopts whatever the device is running,
  // which is the live-push state the resume checks below are about.
  await reloadInto(page);
  await page.waitForSelector(".cm-content");
  await sleep(1200);
  check(
    "resume: a clean reload opens the running pattern, so live push is back on",
    !(await saveState(page)).includes("preview only"),
    await saveState(page),
  );
  // An unsaved edit must survive a reload — in the EDITOR. It must NOT be
  // re-pushed when the device has moved on meanwhile: a page load is not a
  // device action (#585).
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
  // reload — the edit wins in the EDITOR; the DEVICE keeps what it is running.
  // Both ids are empty here (an ad-hoc edit on one side, an out-of-band ad-hoc
  // push on the other) and two empty ids are not a match, so the copy resumes
  // in local preview and the out-of-band program keeps the LEDs (#585). This
  // used to assert the opposite — the boot push that cost Jeremy's playlist.
  await reloadInto(page);
  await page.waitForSelector(".cm-content");
  await sleep(500); // let the device handshake settle after reload
  await sleep(1800); // …and well past the push debounce, if there were a push
  check(
    "resume: editor restores the unsaved edit",
    (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.111"),
  );
  check(
    "#585 resume: the device keeps the out-of-band pattern — the boot pushes nothing",
    (await (await fetch(`${DEV}/api/pattern`)).text()).includes("0.9"),
  );
  check(
    "#585 resume: …and the resumed copy says it is preview only",
    (await saveState(page)) === "unsaved · preview only",
    await saveState(page),
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
  await reloadInto(page);
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
  // drag-to-reorder: move item 0 (hue 0.33) below item 1 (hue 0.8).
  // POINTER events since #538 round 2 — the reorder is live and the store
  // only changes on release, so the drag has to be a real pointer gesture
  // (the deep checks on the intermediate transforms are in the r2-3 section
  // at the foot of this file).
  {
    const at = async (sel, i) =>
      page.$$eval(
        sel,
        (els, n) => {
          const b = els[n].getBoundingClientRect();
          return { x: b.x + b.width / 2, y: b.y + b.height / 2, h: b.height };
        },
        i,
      );
    const grip = await at('[data-role="pl-grip"]', 0);
    const row1 = await at('[data-role="playlist-item"]', 1);
    await page.mouse.move(grip.x, grip.y);
    await page.mouse.down();
    await page.mouse.move(grip.x, grip.y + row1.h * 0.7, { steps: 6 });
    await sleep(150);
    await page.mouse.up();
  }
  await sleep(1000);
  check(
    "playlist: drag reorders items",
    Math.abs(
      (await (await fetch(`${DEV}/api/playlist`)).json()).items[0].controls.sliderHue[0] - 0.8,
    ) < 0.01,
    JSON.stringify((await (await fetch(`${DEV}/api/playlist`)).json()).items.map((i) => i.controls)),
  );
  // …and the same reorder from the KEYBOARD: S4 deletes the ↑/↓ movers, so
  // the handle itself has to carry the arrow keys (#538 §F). Move row 0
  // (hue 0.8 after the drag) back down with ArrowDown on its handle.
  await page.$$eval('[data-role="pl-grip"]', (els) => els[0].focus());
  await page.keyboard.press("ArrowDown");
  await sleep(700);
  check(
    "playlist: ↓ on the drag handle reorders from the keyboard",
    Math.abs(
      (await (await fetch(`${DEV}/api/playlist`)).json()).items[0].controls.sliderHue[0] - 0.33,
    ) < 0.01,
    JSON.stringify((await (await fetch(`${DEV}/api/playlist`)).json()).items.map((i) => i.controls)),
  );

  // ---- transport: the FOUR persistent controls of mockup S4 (#538 §F) ----
  //
  // A bed long enough that nothing auto-advances under the checks below: the
  // 2 s override set above would roll the index while we were asserting it.
  // One item inherits the (long) default, one overrides it, so the footer
  // still exercises both halves of the run-time sum.
  const bedIds = (await (await fetch(`${DEV}/api/playlist`)).json()).items.map((it) => it.id);
  await fetch(`${DEV}/api/playlist`, {
    method: "POST",
    body: `D 60\nX 0\nI ${bedIds[0]} -1\nI ${bedIds[1]} 20\n`,
  });
  await sleep(1400); // the 1 Hz follow poll picks the bed up

  /** The transport's roles, in DOM order. */
  const transport = () =>
    page.$$eval(
      '[data-role="pl-play"],[data-role="pl-pause"],[data-role="pl-stop"],[data-role="pl-prev"],[data-role="pl-next"]',
      (els) => els.map((e) => e.dataset.role).join(","),
    );
  check(
    "playlist: four transport controls while stopped (S4)",
    (await transport()) === "pl-play,pl-stop,pl-prev,pl-next",
    await transport(),
  );
  check(
    "playlist: the now-playing block stays mounted while stopped (S4)",
    (await page.$('[data-role="pl-now"]')) !== null,
  );
  // next/prev while stopped move the PARKED position (both devices ignore a
  // step while not playing), so the buttons are honest in every state
  // (the two items are the same pattern, so the item the readout is parked on
  // is told apart by its DURATION: item 0 inherits 60 s, item 1 overrides 20)
  const parkedMax = () =>
    page.$eval('[data-role="pl-progress"]', (el) => Number(el.getAttribute("aria-valuemax")));
  const parked0 = await parkedMax();
  await page.click('[data-role="pl-next"]');
  await sleep(400);
  const parked1 = await parkedMax();
  const deviceUntouched = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: next while stopped parks on the next item without waking the device",
    parked0 === 60 && parked1 === 20 && deviceUntouched.playing === false,
    `${parked0} → ${parked1}`,
  );
  await page.click('[data-role="pl-prev"]');
  await sleep(1400); // long enough that a follow poll could have stolen it back
  check("playlist: …and prev parks back", (await parkedMax()) === 60, String(await parkedMax()));

  await page.click('[data-role="pl-play"]');
  await sleep(600);
  const playing = await (await fetch(`${DEV}/api/playlist`)).json();
  check("playlist: play starts at index 0", playing.playing === true && playing.index === 0, JSON.stringify({ p: playing.playing, i: playing.index }));
  // …and the transport follows the device rather than latching on "play" when
  // the read-back beats the render loop (Gitea #431). The SAME four controls
  // are there; only the primary's verb changes.
  const pauseShown = await page
    .waitForSelector('[data-role="pl-pause"]', { timeout: 3000 })
    .then(() => true)
    .catch(() => false);
  check("playlist: the primary becomes Pause while playing", pauseShown);
  check(
    "playlist: the same four controls while playing (S4)",
    (await transport()) === "pl-pause,pl-stop,pl-prev,pl-next",
    await transport(),
  );
  const nowName = await page.$eval('[data-role="pl-now-name"]', (el) => (el.textContent ?? "").trim());
  check(
    "playlist: the now-playing block names the running item",
    nowName === playing.items[playing.index].name,
    `${nowName} vs ${playing.items[playing.index].name}`,
  );

  // ---- the S4 row treatments (mockdiff measures these; these are the
  //      behavioural half — that the right ROW wears them) ----
  const playMark = await page.evaluate(() => {
    const row = document.querySelector('[data-role="playlist-item"].playing');
    if (!row) return null;
    const cs = getComputedStyle(row);
    return {
      edge: `${cs.borderLeftWidth} ${cs.borderLeftColor}`,
      grip: (row.querySelector('[data-role="pl-grip"]')?.textContent ?? "").trim(),
      gripColor: getComputedStyle(row.querySelector('[data-role="pl-grip"]')).color,
    };
  });
  check(
    "playlist: the playing row is marked with the --ok left edge and a ▶ handle (S4)",
    playMark !== null &&
      playMark.edge === "3px rgb(95, 191, 122)" &&
      playMark.grip === "▶" &&
      playMark.gripColor === "rgb(95, 191, 122)",
    JSON.stringify(playMark),
  );
  // S4's `.prog` is a 3px track with a REAL fill element in it, not a
  // gradient painted on the track
  const fill = await page.evaluate(() => {
    const bar = document.querySelector('[data-role="pl-progress"]');
    const i = bar?.querySelector("i");
    if (!i) return null;
    return {
      h: Math.round(bar.getBoundingClientRect().height),
      bg: getComputedStyle(i).backgroundColor,
      frac: i.getBoundingClientRect().width / bar.getBoundingClientRect().width,
    };
  });
  check(
    "playlist: the progress bar is a 3px track with an --ok fill element (S4)",
    fill !== null && fill.h === 3 && fill.bg === "rgb(95, 191, 122)" && fill.frac >= 0 && fill.frac <= 1,
    JSON.stringify(fill),
  );
  // S4 puts the values band UNDER the row as its own block, and the footer
  // OUTSIDE the list with the page's own padding
  await page.$$eval('[data-role="pl-values-toggle"]', (els) => els[0].click());
  await sleep(400);
  const band = await page.evaluate(() => {
    const v = document.querySelector('[data-role="pl-values"]');
    if (!v) return null;
    const prev = v.previousElementSibling;
    return {
      sibling: prev?.getAttribute("data-role") ?? null,
      nested: v.closest('[data-role="playlist-item"]') !== null,
      display: getComputedStyle(v).display,
    };
  });
  check(
    "playlist: the values band is a sibling UNDER the row, not nested in it (S4 .plvals)",
    band !== null && band.sibling === "playlist-item" && band.nested === false && band.display === "block",
    JSON.stringify(band),
  );
  await page.$$eval('[data-role="pl-values-toggle"]', (els) => els[0].click());
  await sleep(200);
  const foot = await page.evaluate(() => {
    const f = document.querySelector('[data-role="pl-total"]');
    if (!f) return null;
    const cs = getComputedStyle(f);
    return {
      inList: f.closest(".pl-list") !== null,
      pad: `${cs.paddingTop} ${cs.paddingRight} ${cs.paddingBottom} ${cs.paddingLeft}`,
      text: (f.textContent ?? "").trim(),
    };
  });
  check(
    "playlist: the footer is a sibling of the list with S4's own padding",
    foot !== null && foot.inList === false && foot.pad === "0px 20px 22px 20px" && /^\d+ items? · loop ≈ /.test(foot.text),
    JSON.stringify(foot),
  );

  // ---- seek: drag the progress bar to mid-item ----
  // The wire has no seek, so this is `play <index>` + a local clock offset
  // (stores/device.ts): the readout jumps, the device re-enters the item.
  const progBox = await (await page.$('[data-role="pl-progress"]')).boundingBox();
  await page.mouse.move(progBox.x + progBox.width * 0.5, progBox.y + progBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(progBox.x + progBox.width * 0.5, progBox.y + progBox.height / 2);
  await page.mouse.up();
  await sleep(700);
  const seeked = await page.$eval('[data-role="pl-progress"]', (el) => ({
    at: Number(el.getAttribute("aria-valuenow")),
    max: Number(el.getAttribute("aria-valuemax")),
  }));
  check(
    "playlist: dragging the progress bar seeks the clock into the item",
    seeked.max === 60 && seeked.at >= 24 && seeked.at <= 36,
    JSON.stringify(seeked),
  );
  const afterSeek = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: a seek re-enters the same item rather than advancing",
    afterSeek.playing === true && afterSeek.index === 0,
    JSON.stringify({ p: afterSeek.playing, i: afterSeek.index }),
  );

  await page.click('[data-role="pl-next"]');
  await sleep(500);
  check(
    "playlist: next advances the device",
    (await (await fetch(`${DEV}/api/playlist`)).json()).index === 1,
  );

  // ---- Pause holds the place; Stop gives it up ----
  await page.click('[data-role="pl-pause"]');
  await sleep(600);
  const paused = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: Pause halts the auto-advance and keeps the index",
    paused.playing === false && paused.index === 1,
    JSON.stringify({ p: paused.playing, i: paused.index }),
  );
  await page.waitForSelector('[data-role="pl-play"]', { timeout: 3000 });
  await page.click('[data-role="pl-play"]');
  await sleep(700);
  const resumed = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: Play after a Pause resumes that item, not the top",
    resumed.playing === true && resumed.index === 1,
    JSON.stringify({ p: resumed.playing, i: resumed.index }),
  );
  await page.click('[data-role="pl-stop"]');
  await sleep(600);
  check(
    "playlist: stop halts auto-advance",
    (await (await fetch(`${DEV}/api/playlist`)).json()).playing === false,
  );
  check(
    "playlist: transport returns to the play button after stop",
    (await page.$('[data-role="pl-play"]')) !== null,
  );
  await page.click('[data-role="pl-play"]');
  await sleep(700);
  const afterStop = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: Play after a Stop starts the queue from the top",
    afterStop.playing === true && afterStop.index === 0,
    JSON.stringify({ p: afterStop.playing, i: afterStop.index }),
  );

  // ---- opening a pattern must not hijack the device (Gitea #563) ----
  // The playlist is PLAYING right now — the exact state that made this a bug
  // worth a ticket. Browsing to a Library pattern and opening it in the editor
  // must leave it playing, write nothing, and say so; `Save` must store the
  // pattern without activating it; `▶ Play on device` is the one click that
  // changes the LEDs, and live push resumes from there.
  {
    const wires = [];
    const logWire = (r) => {
      if (r.method() !== "POST") return;
      const u = r.url();
      if (/\/api\/(code|control|events|sensors)$/.test(u) || /\/activate$/.test(u)) wires.push(u);
    };
    page.on("request", logWire);
    try {
      await page.click('[data-role="tab-patterns"]');
      await sleep(400);
      await page.click('[data-role="patterns-source-library"]');
      await sleep(300);
      await page.$eval(
        '[data-role="gallery-search"]',
        (el, v) => {
          el.value = v;
          el.dispatchEvent(new Event("input", { bubbles: true }));
        },
        strip1d.name,
      );
      await sleep(900);
      // Scope to the tile the search left VISIBLE, by key. A filtered-out
      // tile stays in the DOM at 0x0, so "the first .tile in the grid" is
      // whatever sorts first in the whole library — it only ever worked
      // because that pattern happened to be the one this section searches
      // for, and `tileAction` aborts the whole run on a 0x0 hover
      // (.claude/rules/web.md, 2026-09-20).
      const LTILE = `[data-role="patterns-grid"][data-source="library"] .tile[data-key="${strip1d.name}"]`;
      await tileAction(page, LTILE, "tile-edit");
      await sleep(2000); // well past the 500 ms push debounce

      check("563: a Library tile's Edit opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
      check("563: …and sends the device NOTHING", wires.length === 0, wires.join(", "));
      const stillPlaying = await (await fetch(`${DEV}/api/playlist`)).json();
      check(
        "563: …the playlist keeps playing",
        stillPlaying.playing === true,
        JSON.stringify({ p: stillPlaying.playing, i: stillPlaying.index }),
      );
      check(
        "563: …the header says preview only",
        (await saveState(page)) === "preview only · not on device",
        await saveState(page),
      );
      // the rail preview still runs the LOCAL engine: two canvas reads apart
      const animates = await page.evaluate(async () => {
        const cv = document.querySelector('main.editor-frame:not([hidden]) [data-role="preview"] canvas');
        if (!cv) return "no canvas";
        const a = cv.toDataURL();
        await new Promise((r) => setTimeout(r, 600));
        return cv.toDataURL() === a ? "frozen" : "animating";
      });
      check("563: …and the local preview animates", animates === "animating", animates);
      await page.screenshot({ path: `${shotDir}/device-e2e-563-preview-only.png` });

      // Save stores it WITHOUT activating: a row appears, the LEDs do not move
      const runningBefore = await (await fetch(`${DEV}/api/pattern`)).text();
      const patsBeforeSave = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns.length;
      await renameTo(page, "563 preview only");
      await page.click('[data-role="save"]');
      await sleep(1200);
      const patsAfterSave = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns;
      check(
        "563: Save creates the pattern on the device",
        patsAfterSave.length === patsBeforeSave + 1 &&
          patsAfterSave.some((p) => p.name === "563 preview only"),
        `${patsBeforeSave} → ${patsAfterSave.length}`,
      );
      check(
        "563: …without activating it (the running pattern is unchanged)",
        (await (await fetch(`${DEV}/api/pattern`)).text()) === runningBefore,
      );
      check(
        "563: …and the header still says preview only",
        (await saveState(page)) === "saved · on device · preview only",
        await saveState(page),
      );
      check(
        "563: …and no activate/code request was sent by any of that",
        wires.length === 0,
        wires.join(", "),
      );
      await page.screenshot({ path: `${shotDir}/device-e2e-563-saved-preview-only.png` });

      // ▶ Play on device: the one explicit activation
      await page.click('[data-role="editor-play-device"]');
      await sleep(1500);
      check(
        "563: ▶ Play on device activates it",
        wires.some((u) => /\/activate$/.test(u)),
        wires.join(", "),
      );
      const nowRunning = await (await fetch(`${DEV}/api/pattern`)).text();
      check(
        "563: …so the device is running the opened pattern",
        nowRunning.trim() === strip1d.source.trim(),
        nowRunning.slice(0, 60),
      );
      check(
        "563: …the header drops the preview-only state",
        !(await saveState(page)).includes("preview only"),
        await saveState(page),
      );
      check(
        "563: …and ▶ Play on device is gone (nothing left to play)",
        (await page.$('[data-role="editor-play-device"]')) === null,
      );
      await page.screenshot({ path: `${shotDir}/device-e2e-563-playing-live.png` });

      // …and live push is back: a subsequent edit reaches /api/code
      wires.length = 0;
      await setEditor(page, "export function render(index) { rgb(0.5, 0.25, 0.125) }");
      await sleep(1500);
      check(
        "563: live push resumes once the editor IS the running pattern",
        wires.some((u) => u.endsWith("/api/code")),
        wires.join(", "),
      );
      check(
        "563: …and the device runs the edit",
        (await (await fetch(`${DEV}/api/pattern`)).text()).includes("0.125"),
      );
    } finally {
      page.off("request", logWire);
      // tidy: drop the pattern this section stored and leave the editor out
      const pats = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns ?? [];
      const mine = pats.find((p) => p.name === "563 preview only");
      if (mine) await fetch(`${DEV}/api/patterns/${mine.id}`, { method: "DELETE" }).catch(() => {});
      await leaveEditor(page);
      await sleep(300);
      // Put the Patterns page back the way the rest of the run expects it:
      // `On device`, with an empty search box (the search is the PAGE's, so a
      // needle left in it filters the device grid too and every later
      // `tileAction(DTILE, …)` then aborts on a hidden tile).
      await page.click('[data-role="patterns-source-device"]').catch(() => {});
      await page.$eval('[data-role="gallery-search"]', (el) => {
        el.value = "";
        el.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await sleep(400);
      await page.click('[data-role="tab-playlist"]');
      await sleep(500);
    }
  }

  await page.click('[data-role="pl-stop"]');
  await sleep(500);

  // the defaults field has to be READABLE (Jeremy: the "manual" placeholder
  // clipped to "mar" in a 56 px box). 72 px, and showing the number itself.
  const defaults1400 = await page.$eval('[data-role="pl-default-sec"]', (el) => ({
    w: Math.round(el.getBoundingClientRect().width),
    h: Math.round(el.getBoundingClientRect().height),
    value: el.value,
    placeholder: el.placeholder,
    clipped: el.scrollWidth > el.clientWidth + 1,
  }));
  check(
    "playlist: the default-duration field is readable at 1400 px and shows the number",
    defaults1400.w >= 70 && defaults1400.h <= 28 && defaults1400.value === "60" &&
      defaults1400.placeholder === "" && !defaults1400.clipped,
    JSON.stringify(defaults1400),
  );
  const labelled = await page.evaluate(() =>
    ["pl-default-sec", "pl-crossfade"].every((r) => {
      const el = document.querySelector(`[data-role="${r}"]`);
      return el && el.id && document.querySelector(`label[for="${el.id}"]`) !== null;
    }),
  );
  check("playlist: both settings fields have real <label for> elements", labelled);

  // total run-time summary (item0 inherits 60s + item1 override 20s = 1m 20s)
  const total = await page.$eval('[data-role="pl-total"]', (el) => el.textContent ?? "");
  check("playlist: total run-time shown", /2 items/.test(total) && /1m 20s/.test(total), total.trim());

  // ---- `+ Add` opens THE picker and appends what you choose (#470) ----
  await page.click('[data-role="pl-add"]');
  await page.waitForSelector('[data-role="pattern-picker"]', { timeout: 4000 });
  // the picker has TWO sections now: the device's patterns, and the library
  // (gallery.json, the same source the Patterns page browses) — #538 §F
  await page.waitForSelector('[data-role="picker-section-library"]', { timeout: 15000 });
  check(
    "playlist: the picker offers both a Patterns and a Library section",
    (await page.$('[data-role="picker-section-pattern"]')) !== null,
  );
  const pickCount = await page.$$eval('[data-role="picker-item"][data-kind="pattern"]', (els) => els.length);
  check("playlist: the picker lists the device's patterns", pickCount >= 1, String(pickCount));

  // ---- the picker is LAYOUT-FILTERED, like every other grid (#562) ----
  // The fixture here is a 120 px 1D strip, so the Library section must offer
  // the strip patterns and none of the grid ones — picking a 2D pattern would
  // write it to the device's store and queue an item the strip cannot show.
  // Driven off gallery.json's own `kind` rather than a hard-coded name.
  const pickerSearch = async (q) => {
    await page.$eval(
      '[data-role="picker-search"]',
      (el, v) => {
        el.value = v;
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      q,
    );
    await sleep(300);
    return page.$$eval('[data-role="picker-item"][data-kind="library"]', (els) =>
      els.map((e) => (e.querySelector(".pknm")?.textContent ?? "").trim()),
    );
  };
  const offered2d = await pickerSearch(grid2d.name);
  check(
    "playlist: the picker hides 2D library patterns on a 1D strip (#562)",
    offered2d.length === 0,
    offered2d.length ? `still offered: ${offered2d.join(", ")}` : `searched "${grid2d.name}"`,
  );
  check(
    "playlist: …and still offers the 1D ones",
    (await pickerSearch(strip1d.name)).includes(strip1d.name),
    strip1d.name,
  );
  await pickerSearch("");
  const offered = await page.$$eval('[data-role="picker-item"][data-kind="library"]', (els) =>
    els.map((e) => (e.querySelector(".pknm")?.textContent ?? "").trim()),
  );
  const kindOf = new Map(galleryJson.map((p) => [p.name, p.kind]));
  // "any" is the generator's DIMENSIONLESS kind — an index-space
  // `renderFrame` that declares no geometry and is native on EVERY Layout, so
  // a strip offers it exactly as it offers a "strip" one. Only "grid" and
  // "cloud" are the ones a 1D fixture must hide (#538/#629).
  check(
    "playlist: every library row the picker offers fits the layout",
    offered.every((n) => ["strip", "any", undefined].includes(kindOf.get(n))),
    offered.filter((n) => kindOf.get(n) === "grid" || kindOf.get(n) === "cloud").join(", "),
  );

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
  await page.$$eval('[data-role="picker-item"][data-kind="pattern"]', (els) => els[0].click());
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

  // ---- a LIBRARY pick saves to the device first, then queues it ----
  const patsBefore = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns.length;
  await page.click('[data-role="pl-add"]');
  await page.waitForSelector('[data-role="picker-section-library"]', { timeout: 15000 });
  const libName = await page.$eval(
    '[data-role="picker-item"][data-kind="library"]',
    (el) => (el.querySelector(".pknm")?.textContent ?? "").trim(),
  );
  await page.click('[data-role="picker-item"][data-kind="library"]');
  await page
    .waitForFunction(() => document.querySelector('[data-role="pattern-picker"]') === null, {
      timeout: 20000,
    })
    .catch(() => {});
  await sleep(1000);
  const patsAfter = (await (await fetch(`${DEV}/api/patterns`)).json()).patterns;
  check(
    "playlist: a Library pick saves the pattern to the device",
    patsAfter.length === patsBefore + 1 && patsAfter.some((p) => p.name === libName),
    `${patsBefore} → ${patsAfter.length}, wanted "${libName}"`,
  );
  const plWithLib = await (await fetch(`${DEV}/api/playlist`)).json();
  const libItem = plWithLib.items[plWithLib.items.length - 1];
  check(
    "playlist: …and appends it as the last item",
    plWithLib.items.length === 4 &&
      patsAfter.find((p) => p.id === libItem.id)?.name === libName,
    JSON.stringify({ n: plWithLib.items.length, last: libItem?.name }),
  );
  // tidy: drop the library row again so the counts below are unchanged
  await page.$$eval('[data-role="pl-remove"]', (els) => els[3].click());
  await sleep(700);

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
  // Jeremy: "clearing the playlist flickers, waits, and then deletes" (#538
  // §A). Two measurements pin the fix: the rows must leave in ONE DOM flush
  // (no clear-then-refetch repaint), and the write must be ONE POST that
  // leaves immediately instead of waiting out the 400 ms edit debounce.
  let clearPosts = 0;
  const countClearPost = (r) => {
    if (r.method() === "POST" && r.url() === `${DEV}/api/playlist`) clearPosts++;
  };
  page.on("request", countClearPost);
  await page.evaluate(() => {
    window.__plBatches = 0;
    const panel = document.querySelector('[data-role="playlist-panel"]');
    window.__plObs = new MutationObserver(() => {
      window.__plBatches++;
    });
    window.__plObs.observe(panel, { childList: true, subtree: true });
  });
  await acceptDialog(page);
  await sleep(2500); // past the old 400 ms debounce AND two follow polls
  page.off("request", countClearPost);
  const batches = await page.evaluate(() => {
    window.__plObs.disconnect();
    return window.__plBatches;
  });
  check(
    "playlist: clear empties it",
    (await (await fetch(`${DEV}/api/playlist`)).json()).items.length === 0,
  );
  check(
    "playlist: clear repaints the list exactly once (no flicker)",
    batches === 1,
    `${batches} mutation batches`,
  );
  check(
    "playlist: clear writes ONE POST, immediately (no debounce wait)",
    clearPosts === 1,
    `${clearPosts} POSTs`,
  );

  // ---- "Add to playlist" captures the PROJECTION too (#470 + #468) ----
  // A projection exists only where the Layout can show the pattern AND the
  // pattern is not native to it — since #545 that is a SMALLER pattern on a
  // bigger fixture, never the other way round. So the rig is a 12×10 matrix
  // with a 1D pattern in the editor; the quiet Projection row is live there,
  // the choice made in it is a VALUE, and the playlist item it is added to is
  // where that value gets its durable home (stores/pattern.ts).
  //
  // Through the Settings PICKER, not a raw fetch: `deviceLayoutWire` is what
  // the reconciler reads and only `applyLayout` writes it.
  await page.click('[data-role="tab-settings"]');
  await sleep(400);
  await page.select('[data-role="layout-kind"]', "matrix");
  await sleep(900);
  await page.click('[data-role="tab-patterns"]');
  await sleep(400);
  await tileAction(page, DTILE, "tile-edit");
  await sleep(1200);
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
  await sleep(1200);
  await renameTo(page, "proj rider");
  await page.click('[data-role="save"]');
  await sleep(900);
  await page.click('[data-role="projection-change"]');
  await page.waitForSelector('[data-role="projection-options"]', { timeout: 2000 });
  await page.click('[data-role="projection-opt-y"]');
  await sleep(400);
  await menuClick(page, "add-to-playlist");
  await sleep(700);
  const plWithProj = await (await fetch(`${DEV}/api/playlist`)).json();
  check(
    "playlist: Add to playlist carries the editor's projection onto the item",
    plWithProj.items.length === 1 && plWithProj.items[0].proj === "y",
    JSON.stringify(plWithProj.items),
  );
  await fetch(`${DEV}/api/playlist`, { method: "POST", body: "D 5" }); // clean up
  await page.select('[data-role="layout-kind"]', "strip"); // restore
  await sleep(900);
  await page.click('[data-role="editor-back"]'); // back to the tabs
  await sleep(400);
  await page.click('[data-role="tab-settings"]');
  await sleep(300);
  await page.select('[data-role="layout-kind"]', "strip"); // restore the fixture
  await sleep(700);

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
  await reloadInto(page);
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

    // Order (proposal §5.3): Device first, and the NAME and BRIGHTNESS are
    // the first controls on the page — ahead of every field in LED layout.
    // There is no `Projection` heading on this strip: a 1D Layout shows 1D
    // patterns and nothing else, so the section is absent (#538).
    const order = await page.$$eval(
      '[data-role="settings-panel"] .slabel, [data-role="settings-panel"] input, [data-role="settings-panel"] select',
      (els) =>
        els
          .map((e) =>
            e.tagName === "DIV" ? `#${e.textContent?.trim()}` : e.getAttribute("data-role") ?? "",
          )
          .filter(Boolean),
    );
    // Projection is CONDITIONAL since #545: a 1D fixture (this one) can show
    // no pattern of another dimensionality at all, so it has nothing to
    // configure and the section is absent rather than empty. Where it does
    // appear — the panel console above — it sits between LED layout and WiFi.
    const heads = order.filter((x) => x.startsWith("#"));
    check(
      "settings: Device · LED layout · [Projection ·] WiFi · Advanced, in that order",
      heads.filter((x) => x !== "#Projection").join(" ") ===
        "#Device #LED layout #WiFi #Advanced" &&
        (!heads.includes("#Projection") || heads.indexOf("#Projection") === 2),
      heads.join(" "),
    );
    check(
      "settings: Name then Brightness are the first controls (mockup S3)",
      order.filter((x) => !x.startsWith("#")).slice(0, 2).join(",") ===
        "device-name,brightness",
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
      await leaveEditor(outPage);
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
      await leaveEditor(hubPage);

      // ---- the preview enforces the PANEL's array ledger (#253/#420) ----
      // PB's 10,236-element count is what a board with no array arena
      // enforces; the panel's 8 MB arena raises it a hundredfold. A preview
      // engine stuck on PB's number failed `array(pixelCount)` during init and
      // drew a BLACK canvas for a pattern the LEDs were showing — which is
      // exactly what `library/fairies.js` (15,104 elements at 4096 px) did on
      // the bench. The canvas is the assertion: a compile error or a capacity
      // banner would be a different (and visible) failure.
      await hubPage.click('[data-role="patterns-source-library"]');
      await hubPage.waitForFunction(
        () => document.querySelectorAll('[data-source="library"] .tile').length > 50,
        { timeout: 15000 },
      );
      const tagged = await hubPage.evaluate(() => {
        const name = (e) => e.querySelector('[data-role="tile-name"]')?.textContent ?? "";
        const t = [...document.querySelectorAll('[data-source="library"] .tile')].find((e) =>
          /Fairies/i.test(name(e)),
        );
        if (t) {
          t.setAttribute("data-probe", "fairies");
          t.scrollIntoView({ block: "center" });
        }
        return t !== undefined;
      });
      check("panel: the library carries _Fairies (the #420 fixture)", tagged);
      if (tagged) {
        await tileAction(hubPage, '[data-probe="fairies"]', "tile-edit");
        await hubPage.waitForSelector('[data-role="editor-view"]:not([hidden])', { timeout: 8000 });
        const litPx = await hubPage
          .waitForFunction(
            () => {
              const c = document.querySelector(
                'main.editor-frame:not([hidden]) [data-role="preview"] canvas',
              );
              if (!c) return false;
              const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
              let n = 0;
              for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 8) n++;
              return n > 50 && n;
            },
            { timeout: 2000, polling: 100 },
          )
          .then((h) => h.jsonValue())
          .catch(() => 0);
        check("panel: _Fairies previews at 4096 px instead of rendering black", litPx > 50, `lit=${litPx}`);
        check(
          "panel: …and no capacity banner cries wolf about the panel's arena",
          (await hubPage.$('[data-role="capacity-rejected"], [data-role="capacity-warning"]')) === null,
        );
        await hubPage.screenshot({ path: `${shotDir}/panel-fairies-preview.png` });
        await leaveEditor(hubPage);
      }
      await hubPage.click('[data-role="patterns-source-device"]');
      await sleep(400);

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
      check("panel: the summary line carries the kind", head === "64 × 64 matrix", head);
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
      // Since #401/#525 the host reports its CONFIGURED driver (`/api/layout`
      // `driver`), so the browser computes this from the clock and bit depth
      // it was told — the same formula, rounded here rather than truncated in
      // the host's integer `est_hz` (115.2/4 = 28.8 → 29, where `est_hz` says
      // 28). A host that reports no driver block still hands over `est_hz`
      // and that number wins (#475); `lib/panelDriver.ts` picks.
      check("panel: four chained panels go amber under 100 Hz", amber && hz4 === "29 Hz", hz4);
      const dark = await hubPage.$('[data-role="layout-dark"]');
      check("panel: the mirror drives the whole chain, so nothing is dark", dark === null);
      const note = await hubPage
        .waitForFunction(
          () => document.querySelector('[data-role="reboot-bar-text"]')?.textContent?.trim() ?? false,
          { timeout: 6000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check("panel: the arrangement change raises the reboot bar", /reboot/i.test(note), note);
      // …and the action that applies it is caps-gated like everything else:
      // the mirror advertises `reboot:false` and has no /api/reboot route, so
      // the bar states the fact and leaves the power cycle to the human.
      check(
        "panel: no Reboot-now button without caps.reboot (§5.7)",
        (await hubPage.$('[data-role="reboot-now"]')) === null,
      );
      check(
        "panel: Advanced gains the Panel driver row",
        (await hubPage.$('[data-role="adv-panel-row"]')) !== null,
      );

      // ---- Advanced › Panel driver is a FORM (Gitea #401/#525) ----------
      // The four driver values are settings the host stores and applies at
      // boot, so each field POSTs one `panel <planes> <clock> <chip> <blank>`
      // line and the reply is adopted. The mirror has no HUB75 hardware, so
      // it synthesises a `driver.live` equal to what is stored (the "in sync"
      // reading); the pending / fallback / off states are unit-tested.
      await openAdv(hubPage, "adv-panel");
      check(
        "panel driver: the card is editable where the host reports a driver",
        (await hubPage.$eval('[data-role="panel-planes"]', (e) => e.tagName)) === "SELECT" &&
          (await hubPage.$('[data-role="panel-chip"]')) !== null &&
          (await hubPage.$('[data-role="panel-blank"]')) !== null,
      );
      check(
        "panel driver: the chip list is the DEVICE's, not the browser's",
        (await hubPage.$$eval('[data-role="panel-chip"] option', (o) => o.map((e) => e.value))).join(
          ",",
        ) === "shiftreg,fm6126a,icn2038s,dp3246",
      );
      // What the card says is the host's `driver.live` reading, never a
      // guess: `null` is "panel output is off", `fallback` is "it did not
      // fit", a mismatch is "reboot to apply", and only an exact match is
      // "running this". The mirror applies a `panel` line to its own live
      // reading immediately, so the state here follows from what it reports.
      const liveState = (l, cfg, m) => {
        if (!l) return "disabled";
        if (l.fallback) return "fallback";
        const chain = Math.max(1, m.cols * m.rows);
        const scan = m.scan > 0 ? m.scan : Math.floor(m.ph / 2);
        const same =
          l.planes === cfg.planes &&
          l.clock_mhz === cfg.clock_mhz &&
          l.chip === cfg.chip &&
          l.blank === cfg.blank &&
          l.w === m.pw * chain &&
          l.h === m.ph &&
          l.scan === scan;
        return same ? "live" : "pending";
      };
      const asFound = await (await fetch(`${HUB}/api/layout`)).json();
      check(
        "panel driver: the card states the host's own live reading",
        (await hubPage.$eval('[data-role="panel-state"]', (e) => e.dataset.state)) ===
          liveState(asFound.driver.live, asFound.driver, asFound.matrix),
        `${await hubPage.$eval('[data-role="panel-state"]', (e) => e.dataset.state)} vs ${JSON.stringify(asFound.driver.live)}`,
      );
      await hubPage.$eval('[data-role="panel-planes"]', (el) => {
        el.value = "6";
        el.dispatchEvent(new Event("input", { bubbles: true }));
        el.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await sleep(900);
      const drv = (await (await fetch(`${HUB}/api/layout`)).json()).driver;
      check(
        "panel driver: one field writes the whole `panel` line",
        drv.planes === 6 && drv.clock_mhz === 30 && drv.chip === "shiftreg" && drv.blank === 1,
        JSON.stringify(drv),
      );
      check(
        "panel driver: the collapsed row states the configured values",
        /^30 MHz · 6 planes/.test(
          await hubPage.$eval('[data-role="adv-panel-status"]', (e) => e.textContent.trim()),
        ),
        await hubPage.$eval('[data-role="adv-panel-status"]', (e) => e.textContent.trim()),
      );
      await shotSettings(hubPage, `${shotDir}/settings-panel.png`, 200);
    } finally {
      await hubPage.close();
      hubDev.kill();
    }
  }

  // ---- Scenes on a console (Gitea #480; mockups S6 · S6c · S7) ----------
  //
  // D10, the visibility rule: a matrix console ALWAYS has the Scenes tab, a
  // strip console never does. The main mirror here is a 120 px strip, so it
  // is the negative case; a `--board panel` mirror beside it is the positive
  // one.
  //
  // The rest is the #563 live-push discipline applied to a scene: editing the
  // scene the device is SHOWING reaches it as it happens, editing any other
  // one reaches it only on Save. That is the half a computed-style frame
  // cannot see, so it is asserted here.
  {
    check(
      "scenes: a strip console has no Scenes tab (D10)",
      (await page.$('[data-role="tab-scenes"]')) === null,
    );

    // The same Layout gate on the EDITOR (Gitea #486, mockup S2f): a strip
    // console must not offer a builtin that would silently do nothing on it.
    // The docs hover is gated with the list, so a name typed by hand
    // documents nothing either.
    {
      // the previous section left the page on Settings
      await page.click('[data-role="tab-patterns"]').catch(() => {});
      await page.waitForSelector('[data-role="new-pattern"]', { timeout: 10000 });
      await sleep(600);
      await page.$eval('[data-role="new-pattern"]', (el) => el.click());
      await page.waitForSelector('[data-role="editor-view"]:not([hidden]) .cm-content', {
        timeout: 10000,
      });
      await sleep(1200);
      const labels = [];
      for (const prefix of ["text", "draw", "fon"]) {
        // A FRESH line each time, not a cleared document: `completeFromList`
        // gives CodeMirror a result with a `validFor`, and a second prefix at
        // the same offset is filtered against the first one's options and
        // comes back empty — which reads exactly like "the builtin is gated"
        // (see web/tools/e2e.mjs).
        await page.$eval('[data-role="editor-view"]:not([hidden]) .cm-content', (el) => el.focus());
        await page.keyboard.down("Control");
        await page.keyboard.press("End");
        await page.keyboard.up("Control");
        await page.keyboard.press("Enter");
        for (const ch of prefix) await page.keyboard.press(ch);
        await sleep(900);
        labels.push(
          ...(await page.evaluate(() =>
            [...document.querySelectorAll(".cm-tooltip-autocomplete li .cm-completionLabel")].map(
              (e) => (e.textContent ?? "").trim(),
            ),
          )),
        );
        await page.keyboard.press("Escape");
        await sleep(150);
      }
      check(
        "completions: a strip console offers none of the five text builtins (S2f)",
        // `labels.length > 0` is the anti-vacuity half: an empty list would
        // pass the negative assertion while proving nothing
        labels.length > 0 &&
          ["drawText", "drawNumber", "textWidth", "font", "textSlot"].every(
            (b) => !labels.includes(b),
          ),
        JSON.stringify([...new Set(labels)]),
      );
      await page.click('[data-role="editor-back"]').catch(() => {});
      await sleep(600);
    }

    const SC_PORT = E2E.mirror.devScenes; // E2E_PORT + 51
    const SC = `http://127.0.0.1:${SC_PORT}`;
    const scDev = spawn(
      "../target/debug/luxel",
      ["serve", ...NO_NETIN, "--port", String(SC_PORT), "--board", "panel", "--pixels", "4096"],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      scDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      scDev.on("exit", () => reject(new Error("scenes mirror died")));
      setTimeout(() => reject(new Error("scenes mirror start timeout")), 30000);
    });
    process.on("exit", () => scDev.kill());
    const scPage = await browser.newPage();
    try {
      await scPage.setViewport({ width: 1400, height: 950 });
      await gotoConsole(scPage, SC);
      await scPage.waitForSelector('[data-role="tab-scenes"]', { timeout: 15000 });
      check("scenes: a matrix console always has the Scenes tab (D10)", true);

      await scPage.click('[data-role="tab-scenes"]');
      await scPage.waitForSelector('[data-role="scenes-empty"]', { timeout: 10000 });
      check("scenes: an empty matrix console shows the empty state, not a grid (S6c)", true);
      await scPage.screenshot({ path: `${shotDir}/device-e2e-scenes-empty.png` });

      // create one, give it a colour layer, and save it to the device
      await scPage.$eval('[data-role="new-scene"]', (el) => el.click());
      await scPage.waitForSelector('[data-role="scene-editor-view"]:not([hidden])', {
        timeout: 10000,
      });
      await sleep(800);
      await scPage.click('[data-role="scene-add-layer"]');
      await sleep(300);
      await scPage.click('[data-role="scene-add-color"]');
      await sleep(500);
      await scPage.click('[data-role="scene-save"]');
      await sleep(900);
      const stored = await fetch(`${SC}/api/scenes`).then((r) => r.json());
      check(
        "scenes: Save writes the record to the device",
        stored.scenes.length === 1 && stored.scenes[0].layers.length === 1,
        JSON.stringify(stored.scenes?.[0]?.layers?.length),
      );
      check(
        "scenes: the device reports the shared blob's budget",
        stored.max === 3840 && stored.used > 0,
        `${stored.used} of ${stored.max}`,
      );

      // NOT the running scene yet → an edit must not reach the device (#563)
      const sceneId = stored.scenes[0].id;
      await scPage.$eval('[data-role="scene-layer-name"]', (el) => {
        el.value = "Quiet edit";
        el.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await sleep(700);
      const quiet = await fetch(`${SC}/api/scenes/${sceneId}`).then((r) => r.json());
      check(
        "scenes: editing a scene the device is NOT showing touches nothing (#563)",
        quiet.layers[0].name !== "Quiet edit",
        quiet.layers[0].name,
      );
      const st = await scPage.$eval(
        '[data-role="scene-save-state"]',
        (el) => el.dataset.saveState ?? "",
      );
      check("scenes: an unsaved edit says so", st === "unsaved changes", st);
      await scPage.click('[data-role="scene-save"]');
      await sleep(800);

      // play it, then edit again: now the device gets it as it happens
      await scPage.click('[data-role="scene-overflow"]');
      await sleep(300);
      await scPage.click('[data-role="scene-play-device"]');
      await sleep(1200);
      const active = await fetch(`${SC}/api/scenes`).then((r) => r.json());
      check(
        "scenes: Play on device activates the scene",
        active.active === sceneId,
        String(active.active),
      );
      await scPage.$eval('[data-role="scene-layer-name"]', (el) => {
        el.value = "Live edit";
        el.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await sleep(900);
      const live = await fetch(`${SC}/api/scenes/${sceneId}`).then((r) => r.json());
      check(
        "scenes: editing the RUNNING scene live-pushes (#563)",
        live.layers[0].name === "Live edit",
        live.layers[0].name,
      );
      await scPage.screenshot({ path: `${shotDir}/device-e2e-scenes-editor.png` });
      // ---- the sprite layer and the text slot, on a real console ---------
      // (Gitea #481 / #486; mockups S7c · S7h). What the playground harness
      // cannot show: that painting a pixel rewrites the sprite's PATTERN in
      // the DEVICE's store, and that the slot row echoes what `POST
      // /api/text` put there.
      {
        // a sprite in the device's store for the layer to bind
        const spr =
          "// @sprite w=4 h=4 frames=1 fps=0\n" +
          "var sprH = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]\n" +
          "var sprS = [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]\n" +
          "var sprV = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]\n" +
          "\nvar sprW = 4\nvar sprHt = 4\n" +
          "\nexport function renderFrame() {\n  blit(sprH, sprS, sprV, sprW, sprHt, 0, 0, 3)\n}\n";
        const put = await fetch(`${SC}/api/patterns`, {
          method: "POST",
          body: await lxpBody("Dot", spr, 4096),
        }).then((r) => r.json());
        check("sprite: a sprite-tagged pattern stores like any other", put.ok === true, JSON.stringify(put));
        const sprId = put.id;
        await sleep(1200); // the console's pattern poll has to see it

        await scPage.click('[data-role="scene-add-layer"]');
        await sleep(300);
        await scPage.click('[data-role="scene-add-sprite"]');
        // the picker only opens once the console KNOWS what is a sprite, and
        // a device row s source streams in after its name
        await scPage.waitForSelector(
          '[data-role="pattern-picker"] [data-role="picker-item"][data-kind="pattern"]',
          { timeout: 15000 },
        );
        await sleep(400);
        // the store has one now, so the picker opens rather than a blank
        // being made — and it offers SPRITES only (#700)
        const offered = await scPage
          .$$eval('[data-role="pattern-picker"] [data-role="picker-item"][data-kind="pattern"]', (els) =>
            els.map((e) => (e.querySelector(".pknm")?.textContent ?? "").trim()),
          )
          .catch(() => []);
        if (offered.length > 0)
          check(
            "sprite: the picker offers sprite-tagged patterns only (#700)",
            offered.every((n) => n === "Dot"),
            JSON.stringify(offered),
          );
        await scPage
          .$eval('[data-role="pattern-picker"] [data-role="picker-item"][data-kind="pattern"]', (el) => el.click())
          .catch(() => {});
        await sleep(1200);
        check(
          "sprite: a bound sprite layer puts the tool row above the preview (S7c)",
          (await scPage.$('[data-role="sprite-tools"]')) !== null,
        );

        // paint one cell inside the layer's box
        await scPage.$eval('[data-role="scene-stage"]', (c) => {
          const r = c.getBoundingClientRect();
          c.dispatchEvent(
            new PointerEvent("pointerdown", {
              clientX: r.left + (1.5 / c.width) * r.width,
              clientY: r.top + (1.5 / c.height) * r.height,
              bubbles: true,
            }),
          );
        });
        // The store write is debounced 600 ms behind the stroke and then has
        // to compile and POST, so POLL for it rather than guess a sleep — a
        // loaded machine took longer than a fixed wait once.
        let back = {};
        let lit = 0;
        for (let i = 0; i < 16 && lit === 0; i++) {
          await sleep(500);
          // by NAME, not by the id captured above: a same-name save
          // overwrites, and the device is free to hand the row a new id
          const rows = await fetch(`${SC}/api/patterns`).then((r) => r.json());
          const dotId = (rows.patterns ?? []).find((p) => p.name === "Dot")?.id ?? sprId;
          back = await fetch(`${SC}/api/patterns/${dotId}`).then((r) => r.json());
          lit = (JSON.parse(/var sprV = (\[[^\]]*\])/.exec(back.source ?? "")?.[1] ?? "[]") ?? [])
            .filter((v) => v > 0).length;
        }
        check(
          "sprite: painting rewrites the sprite's PATTERN on the device",
          lit === 1 && (back.source ?? "").startsWith("// @sprite w=4 h=4"),
          `${lit} lit · ${(back.source ?? "").slice(0, 32)}`,
        );
        await scPage.screenshot({ path: `${shotDir}/device-e2e-sprite.png` });

        // ---- the text slot's echo (S7h) ----
        await fetch(`${SC}/api/text`, { method: "POST", body: "0 PARTY 21:00" });
        await scPage.click('[data-role="scene-add-layer"]');
        await sleep(300);
        await scPage.click('[data-role="scene-add-text"]');
        await sleep(700);
        await scPage.click('[data-role="scene-text-slot"]');
        await sleep(2600); // the slot table is polled
        const now = await scPage
          .$eval('[data-role="scene-text-slot-value"]', (el) => (el.textContent ?? "").trim())
          .catch(() => "");
        check(
          "text: the slot row echoes what POST /api/text wrote (S7h `Now`)",
          now === "PARTY 21:00",
          now,
        );
        // and it is a READOUT on a console — only the playground lets you type
        const editable = await scPage.$eval(
          '[data-role="scene-text-slot-value"]',
          (el) => el.tagName.toLowerCase(),
        );
        check("text: `Now` is a readout on a console, not an input", editable === "div", editable);
        await scPage.screenshot({ path: `${shotDir}/device-e2e-text-slot.png` });

        // put the record back to what the blob-budget check below expects
        await scPage.click('[data-role="scene-save"]');
        await sleep(900);
      }


      // the shared 3840 B blob is a budget the user has to be told about:
      // fill it from outside and let the next save be refused
      for (let i = 0; i < 40; i++) {
        const body =
          `S - Filler ${i}\n` +
          "L color 0 0 0 0 normal 100 none fill 1\nN A very long layer name here\nK 112233\n";
        const r = await fetch(`${SC}/api/scenes`, { method: "POST", body }).then((x) => x.json());
        if (!r.ok) break;
      }
      // GROW the record — replacing a scene with one the same size fits the
      // blob that already holds it, so the refusal needs another layer.
      await scPage.click('[data-role="scene-add-layer"]');
      await sleep(300);
      await scPage.click('[data-role="scene-add-color"]');
      await sleep(400);
      await scPage.click('[data-role="scene-save"]');
      await sleep(1000);
      const bar = await scPage
        .$eval('[data-role="api-error-bar"]', (el) => (el.textContent ?? "").trim())
        .catch(() => "");
      check(
        "scenes: a full store is refused in the ONE error strip, with the numbers",
        /shares 3,840 bytes of storage/.test(bar),
        bar.slice(0, 120),
      );
      await scPage.screenshot({ path: `${shotDir}/device-e2e-scenes-full.png` });
    } finally {
      await scPage.close();
      scDev.kill();
    }
  }


  // ── Scenes on the playlist, in the picker and in the ⋯ menus (#478/#482) ──
  //
  // Its own 64×64 mirror: scenes need a regular 2D grid (§5.4c), and the main
  // mirror here is a 120 px strip — which is exactly what the LAST check in
  // this block uses, to prove the menu item is ABSENT there rather than
  // disabled (mock S2e).
  {
    const SC_PORT = E2E.mirror.plScenes; // E2E_PORT + 53
    const SC = `http://127.0.0.1:${SC_PORT}`;
    // `--scenes FILE` preloads the scene store (docs/api.md): the flag exists
    // so a harness can bring a mirror up with scenes already in it. This one
    // needs no pattern ids — colour and text layers name nothing.
    const preload = `${shotDir}/e2e-playlist-scenes-preload.txt`;
    fs.writeFileSync(
      preload,
      [
        // an EXPLICIT id: a mirror started with `--scenes` does not assign one
        // to an `S -` block, so such a scene cannot be referenced at all (Gitea
        // #701) — the preload names its own.
        "S 5eed0001 Wall clock",
        "L color 0 0 0 0 normal 100 none fill 1",
        "K 101030",
        "L text 0 0 0 0 normal 100 none fill 1",
        "T clock HH:MM",
        "F regular ffffff c none 0",
        "",
      ].join("\n"),
    );
    const plDev = spawn(
      "../target/debug/luxel",
      [
        "serve",
        ...NO_NETIN,
        "--port",
        String(SC_PORT),
        "--board",
        "panel",
        "--pixels",
        "4096",
        "--name",
        "luxel-scenes",
        "--scenes",
        preload,
      ],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      plDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      plDev.on("exit", () => reject(new Error("playlist-scenes mirror died")));
      setTimeout(() => reject(new Error("playlist-scenes mirror start timeout")), 30000);
    });
    process.on("exit", () => plDev.kill());
    const plPage = await browser.newPage();
    try {
      const preloaded = await (await fetch(`${SC}/api/scenes`)).json();
      check(
        "scenes: `--scenes FILE` preloads the store",
        preloaded.scenes.length === 1 &&
          preloaded.scenes[0].name === "Wall clock" &&
          preloaded.scenes[0].id === "5eed0001",
        JSON.stringify(preloaded.scenes.map((s) => `${s.id}:${s.name}`)),
      );

      // two patterns, then a TWO-layer scene over the API (the shape a row
      // has to describe: `Scene ▤ · 2 layers`)
      const pid = {};
      for (const [name, src] of [
        ["Aurora 2D", "export function render2D(index, x, y) { hsv(.33, 1, y) }\n"],
        ["Blue Comet", "export function render2D(index, x, y) { hsv(.6, 1, wave(x + time(.05))) }\n"],
      ]) {
        const r = await fetch(`${SC}/api/patterns`, {
          method: "POST",
          body: await lxpBody(name, src, 4096),
        });
        pid[name] = (await r.json()).id;
      }
      const made = await (
        await fetch(`${SC}/api/scenes`, {
          method: "POST",
          body: [
            "S - Clock overlay",
            "L pat 0 0 0 0 normal 100 none fill 1",
            `I ${pid["Aurora 2D"]}`,
            "L text 0 0 0 0 normal 100 none fill 1",
            "T clock HH:MM",
            "F regular ffffff c none 0",
          ].join("\n"),
        })
      ).json();
      check(
        "scenes: POST /api/scenes answers with an assigned id",
        made.ok === true && /^[0-9a-f]{8}$/.test(made.id ?? ""),
        JSON.stringify(made),
      );

      // a playlist with a PATTERN item and a SCENE item — the wire's
      // `I S<id> <sec>` (the serializer's half is unit-tested in
      // web/tests/playlist.test.mjs)
      await fetch(`${SC}/api/playlist`, {
        method: "POST",
        body: `D 900\nX 0\nI ${pid["Aurora 2D"]} -1\nI S${made.id} -1`,
      });
      await fetch(`${SC}/api/playlist/play`, { method: "POST", body: "1" });
      await plPage.setViewport({ width: 1400, height: 900 });
      await gotoConsole(plPage, SC, "#/playlist");
      await plPage.waitForSelector('[data-role="pl-edit-scene"]', { timeout: 15000 });
      await sleep(2500); // the composite needs a few frames

      const row = await plPage.evaluate(() => {
        const link = document.querySelector('[data-role="pl-edit-scene"]');
        const li = link?.closest('[data-role="playlist-item"]');
        return {
          type: li?.querySelector(".who .t")?.innerText.replace(/\s+/g, " ").trim() ?? "",
          href: link?.getAttribute("href") ?? "",
          label: link?.innerText.replace(/\s+/g, " ").trim() ?? "",
          hasValues: li?.querySelector('[data-role="pl-values-toggle"]') !== null,
          hasDuration: li?.querySelector('[data-role="pl-duration"]') !== null,
          hasThumb: li?.querySelector('[data-role="scene-thumb"] canvas') !== null,
        };
      });
      check("scenes: a scene row says what it is (S4c)", row.type === "Scene ▤ · 2 layers", row.type);
      check("scenes: a scene row has NO values chip — its layers own their values", !row.hasValues);
      check("scenes: a scene row keeps the duration chip", row.hasDuration);
      check("scenes: a scene row carries a composite thumbnail (#482)", row.hasThumb);
      check(
        "scenes: `Edit scene ›` points at the scene editor's route",
        row.href === `#/scenes/${made.id}` && row.label === "Edit scene ›",
        `${row.href} / ${row.label}`,
      );

      // the composite is REAL: the thumbnail's canvas has lit pixels, which
      // it can only have from `luxel_core::compose` running in the wasm
      const lit = await plPage.evaluate(() => {
        const c = document.querySelector('[data-role="scene-thumb"] canvas');
        if (!c) return -1;
        const px = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        let n = 0;
        for (let i = 0; i < px.length; i += 4) if (px[i] + px[i + 1] + px[i + 2] > 12) n++;
        return n;
      });
      check("scenes: the composite thumbnail actually paints", lit > 0, `${lit} lit pixels`);
      await plPage.screenshot({ path: `${shotDir}/scenes-playlist-row.png` });

      // …and the link OPENS the scene editor (App's `openScene`), rather than
      // only pointing at it: a fragment the shell never applies is a dead link.
      await plPage.click('[data-role="pl-edit-scene"]');
      await plPage.waitForSelector('[data-role="scene-editor-view"]:not([hidden])', {
        timeout: 8000,
      });
      check(
        "scenes: Edit scene › opens the scene editor on that scene",
        (await plPage.evaluate(() => location.hash)) === `#/scenes/${made.id}`,
        await plPage.evaluate(() => location.hash),
      );
      // Back from a scene editor lands on the SCENES page (that is the screen
      // the editor belongs to, and its back button says so) — the playlist is
      // one tab away.
      await plPage.click('[data-role="scene-editor-back"]');
      await plPage.waitForSelector('[data-role="tab-playlist"]', { timeout: 8000 });
      await plPage.click('[data-role="tab-playlist"]');
      await plPage.waitForSelector('[data-role="playlist-panel"]:not([hidden])', { timeout: 8000 });

      // ---- the ONE picker gains a Scenes section (S4c) ----
      await plPage.click('[data-role="pl-add"]');
      await plPage.waitForSelector('[data-role="pattern-picker"]', { timeout: 4000 });
      await plPage.waitForSelector('[data-role="picker-section-scene"]', { timeout: 8000 });
      const sections = await plPage.$$eval("[data-role^='picker-section-']", (els) =>
        els.map((e) => e.getAttribute("data-role")),
      );
      check(
        "scenes: the picker offers Patterns and Scenes sections",
        sections.includes("picker-section-pattern") && sections.includes("picker-section-scene"),
        sections.join(","),
      );
      const facts = await plPage.$$eval('[data-role="picker-item"][data-kind="scene"]', (els) =>
        els.map((e) => `${e.querySelector(".pknm").textContent.trim()}|${e.querySelector(".mdim").textContent.trim()}`),
      );
      check(
        "scenes: a picker scene row carries the layer count as its one fact",
        facts.includes("Clock overlay|2 layers") && facts.includes("Wall clock|2 layers"),
        facts.join(" · "),
      );
      const sweepPicker = await disabledSweep(plPage);
      check(
        "scenes: nothing in the open picker is disabled without a reason (§5.7)",
        sweepPicker.length === 0,
        JSON.stringify(sweepPicker),
      );
      await plPage.screenshot({ path: `${shotDir}/scenes-picker.png` });

      const before = (await (await fetch(`${SC}/api/playlist`)).json()).items.length;
      await plPage.click('[data-role="picker-item"][data-kind="scene"]');
      await sleep(900);
      const pl = await (await fetch(`${SC}/api/playlist`)).json();
      const last = pl.items[pl.items.length - 1];
      check(
        "scenes: picking a scene queues it as a scene item (`I S<id>` on the wire)",
        pl.items.length === before + 1 && last.kind === "scene" && /^[0-9a-f]{8}$/.test(last.id),
        JSON.stringify(last),
      );

      // ---- `Add to scene ▸` in the editor's ⋯ (S2e) ----
      await gotoConsole(plPage, SC, "#/");
      await plPage.waitForSelector(`${DTILE}`, { timeout: 15000 });
      await tileAction(plPage, `${DGRID} [data-role="tile"][data-name="Blue Comet"]`, "tile-edit");
      await plPage.waitForSelector('[data-role="editor-view"]:not([hidden]) .cm-content', {
        timeout: 15000,
      });
      await sleep(1500);
      await plPage.click('[data-role="overflow"]');
      await plPage.waitForSelector('[data-role="add-to-scene"]', { timeout: 4000 });
      const label = await plPage.$eval('[data-role="add-to-scene"]', (el) =>
        el.childNodes[0].textContent.trim(),
      );
      check("scenes: the editor ⋯ carries `Add to scene`", label === "Add to scene", label);
      await (await plPage.$('[data-role="add-to-scene"]')).hover();
      await sleep(300);
      const subNames = await plPage.$$eval('[data-role="add-to-scene-item"]', (els) =>
        els.map((e) => e.textContent.trim()),
      );
      check(
        "scenes: the submenu lists every scene on the device",
        subNames.includes("Clock overlay") && subNames.includes("Wall clock"),
        subNames.join(","),
      );
      check(
        "scenes: the submenu ends with `New scene…`",
        (await plPage.$eval('[data-role="add-to-scene-new"]', (e) => e.textContent.trim())) ===
          "New scene…",
      );
      await plPage.screenshot({ path: `${shotDir}/scenes-editor-menu.png` });

      const layersBefore = (await (await fetch(`${SC}/api/scenes/${made.id}`)).json()).layers.length;
      await plPage.click(`[data-role="add-to-scene-item"][data-id="${made.id}"]`);
      await sleep(1200);
      const after = await (await fetch(`${SC}/api/scenes/${made.id}`)).json();
      const top = after.layers[after.layers.length - 1];
      check(
        "scenes: Add to scene puts the pattern on TOP of the scene",
        after.layers.length === layersBefore + 1 &&
          top.type === "pat" &&
          top.pat.id === pid["Blue Comet"],
        JSON.stringify({ n: after.layers.length, top: top.type, id: top.pat?.id }),
      );

      // ---- `New scene…` creates one and opens the scene editor ----
      const nBefore = (await (await fetch(`${SC}/api/scenes`)).json()).scenes.length;
      await plPage.click('[data-role="overflow"]');
      await plPage.waitForSelector('[data-role="add-to-scene"]', { timeout: 4000 });
      await (await plPage.$('[data-role="add-to-scene"]')).hover();
      await sleep(300);
      await plPage.click('[data-role="add-to-scene-new"]');
      await sleep(1200);
      const list = await (await fetch(`${SC}/api/scenes`)).json();
      const fresh = list.scenes[list.scenes.length - 1];
      const hash = await plPage.evaluate(() => location.hash);
      const inEditor =
        (await plPage.$('[data-role="scene-editor-view"]:not([hidden])')) !== null;
      check(
        "scenes: `New scene…` names the scene after the pattern and opens it",
        list.scenes.length === nBefore + 1 &&
          fresh.name === "Blue Comet" &&
          fresh.layers.length === 1 &&
          hash === `#/scenes/${fresh.id}` &&
          inEditor,
        `${list.scenes.length} scenes, ${fresh?.name}, ${hash}, editor ${inEditor}`,
      );
    } finally {
      await plPage.close();
      plDev.kill();
    }
  }

  // ---- …and on a STRIP console the item is ABSENT, not disabled (S2e) ----
  //
  // The main mirror is a 120 px strip, so this is the same menu on hardware
  // that cannot hold a scene: one item shorter, nothing greyed.
  {
    await leaveEditor(page);
    await reloadInto(page, EDIT);
    await page.waitForSelector('[data-role="editor-view"]:not([hidden]) .cm-content', {
      timeout: 15000,
    });
    await page.click('[data-role="overflow"]');
    await sleep(300);
    check(
      "scenes: a strip console's ⋯ has no `Add to scene` at all (absent, not disabled)",
      (await page.$('[data-role="add-to-scene"]')) === null,
    );
    const stripSweep = await disabledSweep(page);
    check(
      "scenes: and nothing in that menu is disabled without a reason",
      stripSweep.length === 0,
      JSON.stringify(stripSweep),
    );
    await page.keyboard.press("Escape");
    await leaveEditor(page);
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
    // S4's transport is a FIXED four-button group (#538 §F), so on an empty
    // queue it stays put and goes disabled — the one place §5.7's
    // absent-never-disabled rule is deliberately set aside, and therefore the
    // one place the buttons must carry `data-reason` (the sweep below proves
    // they do). The ⋯ chip still follows §5.7: Clear would be its only entry.
    check(
      "§5.7: an empty playlist keeps the four controls, disabled WITH a reason",
      (await page.$$eval(
        '[data-role="pl-play"],[data-role="pl-stop"],[data-role="pl-prev"],[data-role="pl-next"]',
        (els) =>
          els.length === 4 &&
          els.every((e) => e.hasAttribute("disabled") && (e.getAttribute("data-reason") ?? "") !== ""),
      )) && (await page.$('[data-role="pl-empty"]')) !== null,
    );
    check(
      "§5.7: …and no ⋯ chip either, since Clear would be its only entry",
      (await page.$('[data-role="pl-more"]')) === null,
    );
    await page.screenshot({ path: `${shotDir}/no-disabled-playlist-empty.png` });
    // #530 → #538 §F: the Default-seconds field must READ. The "manual"
    // placeholder that clipped to "mar" is gone entirely — the field shows the
    // default SECONDS — but the width requirement it produced still stands.
    for (const w of [1400, 390]) {
      await page.setViewport({ width: w, height: w === 390 ? 780 : 900 });
      await sleep(350);
      const fits = await page.$eval('[data-role="pl-default-sec"]', (el) => ({
        ok: el.scrollWidth <= el.clientWidth + 1 && el.getBoundingClientRect().width >= 64,
        placeholder: el.placeholder,
        w: Math.round(el.getBoundingClientRect().width),
      }));
      check(
        `playlist: the default-seconds field reads at ${w} px, with no "manual" placeholder (#530/#538)`,
        fits.ok && fits.placeholder === "",
        JSON.stringify(fits),
      );
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
      "§5.7: Play is live again — and the ⋯ chip is back — once there is an item",
      (await page.$eval('[data-role="pl-play"]', (el) => !el.hasAttribute("disabled"))) &&
        (await page.$('[data-role="pl-more"]')) !== null,
    );
    check(
      "playlist: no row carries a mover button — the handle reorders (S4)",
      (await page.$$('[data-role="playlist-item"]')).length === 3 &&
        (await page.$$eval('[data-role="playlist-item"]', (els) =>
          els.every((row) =>
            [...row.querySelectorAll("button")].every(
              (b) => !/move (up|down)/i.test(`${b.title} ${b.getAttribute("aria-label") ?? ""}`),
            ),
          ),
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

    await gotoConsole(page, TIGHT, EDIT);
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
      await gotoConsole(page, LOADED, EDIT);
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
      await gotoConsole(page, PANEL, EDIT);
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

  // ---- #573: a resumed working copy never reshapes the console ----
  //
  // A console's Layout is the FIXTURE's. Hand a 300 px strip a `render2D`
  // program and `/api/status` reports the grid the ENGINE fabricated for it
  // (`source:"default"`, ceil(√300) = 18×17); the console used to adopt THAT
  // as its geometry: the header chip read `18×17 matrix`, every tile drew as a
  // grid, Settings offered a matrix's projections and the #538 compatibility
  // filter stopped hiding the 2D pattern (Gitea #573). Same class as #539,
  // through a different input.
  //
  // The strip got handed that program by the console's own BOOT, which resumed
  // a dirty working copy and live-pushed it — the #585 half of the same
  // ticket, fixed here too: the push is gone (the copy resumes in local
  // preview), so the fabricated grid is produced out of band below and the
  // boot is checked to send nothing.
  //
  // Each half runs in its OWN browser context, so the seeded localStorage is
  // the only state it carries, and against its OWN mirror, so the numbers in
  // the assertions are the ticket's.
  {
    const WIP_STRIP = E2E.mirror.wipStrip;
    const WIP = `http://127.0.0.1:${WIP_STRIP}`;
    const REN2D = "export function render2D(index, x, y) { hsv(x, 1, y) }";
    const REN1D = "export function render(index) { hsv(index / pixelCount, 1, 1) }";
    /** Seed a mirror's pattern store; returns the new id. */
    const saveOn = async (base, name, src) =>
      (
        await (
          await fetch(`${base}/api/patterns`, { method: "POST", body: await lxpBody(name, src) })
        ).json()
      ).id;
    /** Boot a console in a fresh context with `wip` as the DIRTY working copy
     *  the browser is holding from an earlier session. `wipId` is the device
     *  pattern that copy is an edit OF (#585); `wires`, when given, collects
     *  the POST paths the page sends across the boot. */
    const bootWithWip = async (base, wip, { wipId = "", wires = null } = {}) => {
      const ctx = await browser.createBrowserContext();
      const pg = await ctx.newPage();
      await pg.setViewport({ width: 1400, height: 900 });
      const url = `http://localhost:${PORT}/?device=${encodeURIComponent(base)}`;
      // seed first: `luxel.current` is read during boot, so it must already be
      // on the origin for the load that matters
      await pg.goto(url, { waitUntil: "networkidle0" });
      await pg.evaluate(
        (src, id) => {
          localStorage.setItem(
            "luxel.current",
            JSON.stringify({
              source: src,
              patternName: "",
              exampleName: "",
              dirty: true,
              devicePatternId: id,
            }),
          );
        },
        wip,
        wipId,
      );
      // attached before the reload: the boot IS what we are measuring
      if (wires) pg.on("request", (r) => r.method() === "POST" && wires.push(new URL(r.url()).pathname));
      await pg.reload({ waitUntil: "networkidle0" });
      await sleep(3500);
      return { ctx, pg };
    };

    const wipDev = spawn(
      "../target/debug/luxel",
      ["serve", ...NO_NETIN, "--port", String(WIP_STRIP), "--pixels", "300"],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      wipDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      wipDev.on("exit", () => reject(new Error("#573 strip mirror died")));
      setTimeout(() => reject(new Error("#573 strip mirror start timeout")), 30000);
    });
    process.on("exit", () => wipDev.kill());
    let wipCtx = null;
    try {
      // the ticket's fixture: one pattern the strip can show, one it cannot
      await saveOn(WIP, "Strip 1D", REN1D);
      await saveOn(WIP, "Strip 2D", REN2D);
      // Put the strip on the 2D program OUT OF BAND. Until #585 the console's
      // own boot push did this, which was the bug; the fabricated-grid state
      // is still reachable (any `POST /api/code` of a 2D program makes it), so
      // it is produced here deliberately and the console is booted into it.
      await fetch(`${WIP}/api/code`, { method: "POST", body: await lxpBody("", REN2D) });
      await sleep(400);
      const wipWires = [];
      const booted = await bootWithWip(WIP, REN2D, { wires: wipWires });
      wipCtx = booted.ctx;
      const wipPage = booted.pg;

      const chip = await wipPage.$eval('[data-role="layout-label"]', (el) =>
        (el.textContent ?? "").trim(),
      );
      check(
        "#573: a strip console holding a dirty 2D working copy still reads `300 px strip`",
        chip === "300 px strip",
        chip,
      );

      // the device really IS running the 2D program — this is the fabricated
      // grid case, not a mirror that quietly refused the push
      const geom = (await (await fetch(`${WIP}/api/status`)).json()).geom ?? {};
      check(
        "#573: …while the device reports the engine's fabricated grid",
        geom.source === "default" && geom.dims === 2,
        JSON.stringify(geom),
      );

      // #585: and the boot itself wrote NOTHING — the dirty copy is the
      // editor's document, not a program handed to the LEDs
      check(
        "#585: the boot resume sends the device no code push",
        !wipWires.includes("/api/code"),
        wipWires.join(", ") || "(no POSTs)",
      );

      // the SHOWN tiles only: the `Not for this layout` group below deliberately
      // draws in the playground's Auto style (each pattern's own shape, #538),
      // so it is the one place on a strip console a grid is correct.
      const kinds = await wipPage.$$eval(`${DGRID} [data-role="tile"]`, (els) =>
        els
          .filter((e) => e.closest('[data-role="patterns-incompatible"]') === null)
          .map((e) => e.getAttribute("data-kind")),
      );
      check(
        "#573: every device tile draws as a bar",
        kinds.length > 0 && kinds.every((k) => k === "bar"),
        kinds.join(","),
      );

      // the #538 filter follows the Layout, so it is back to hiding the 2D one
      const incompat = await wipPage
        .$eval('[data-role="patterns-incompatible"]', (el) => ({
          hidden: el.hasAttribute("hidden"),
          text: (el.textContent ?? "").trim(),
        }))
        .catch(() => ({ hidden: true, text: "" }));
      check(
        "#573: the `Not for this layout` group is back, with the 2D pattern in it",
        !incompat.hidden && /Not for this layout \(1\)/.test(incompat.text),
        `${incompat.hidden ? "hidden" : "shown"} ${incompat.text.slice(0, 60)}`,
      );
      const autoKinds = await wipPage.$$eval(
        '[data-role="patterns-incompatible"] [data-role="tile"]',
        (els) => els.map((e) => e.getAttribute("data-kind")),
      );
      check(
        "#573: …and that group keeps the Auto style — the 2D pattern's OWN grid (#538)",
        autoKinds.length === 1 && autoKinds[0] === "grid",
        autoKinds.join(","),
      );
      await wipPage.screenshot({ path: `${shotDir}/device-e2e-573-strip-patterns.png` });

      await wipPage.evaluate(() => {
        location.hash = "#/settings";
      });
      await wipPage.reload({ waitUntil: "networkidle0" });
      await sleep(1500);
      check(
        "#573: Settings shows no Projection section on a strip",
        (await wipPage.$('[data-role="sect-projection"]')) === null,
      );
      await wipPage.screenshot({ path: `${shotDir}/device-e2e-573-strip-settings.png` });

      // ---- the reverse: a panel console holding a dirty 1D working copy ----
      const PANEL_PORT573 = E2E.mirror.wipPanel;
      const PURL = `http://127.0.0.1:${PANEL_PORT573}`;
      const panel573 = spawn(
        "../target/debug/luxel",
        [
          "serve",
          ...NO_NETIN,
          "--port",
          String(PANEL_PORT573),
          "--board",
          "panel",
          "--pixels",
          "4096",
          "--max-pixels",
          "4096",
        ],
        { stdio: ["ignore", "pipe", "inherit"] },
      );
      await new Promise((resolve, reject) => {
        panel573.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
        panel573.on("exit", () => reject(new Error("#573 panel mirror died")));
        setTimeout(() => reject(new Error("#573 panel mirror start timeout")), 30000);
      });
      process.on("exit", () => panel573.kill());
      let revCtx = null;
      try {
        await saveOn(PURL, "Panel 2D", REN2D);
        const rev = await bootWithWip(PURL, REN1D);
        revCtx = rev.ctx;
        const rchip = await rev.pg.$eval('[data-role="layout-label"]', (el) =>
          (el.textContent ?? "").trim(),
        );
        check(
          "#573 reverse: a panel console holding a dirty 1D working copy still reads `64×64 matrix`",
          rchip === "64×64 matrix",
          rchip,
        );
        const rkinds = await rev.pg.$$eval(`${DGRID} [data-role="tile"]`, (els) =>
          els
            .filter((e) => e.closest('[data-role="patterns-incompatible"]') === null)
            .map((e) => e.getAttribute("data-kind")),
        );
        check(
          "#573 reverse: its device tiles draw as grids",
          rkinds.length > 0 && rkinds.every((k) => k === "grid"),
          rkinds.join(","),
        );
      } finally {
        if (revCtx) await revCtx.close();
        panel573.kill();
      }
    } finally {
      if (wipCtx) await wipCtx.close();
      wipDev.kill();
    }

    // ---- #585: a boot never takes the device over ----
    //
    // The push rule (#563) applied to the one path that still broke it: a
    // console resuming the browser's autosaved working copy. The copy is the
    // user's and is never thrown away, but it only reaches the LEDs when it is
    // an unsaved edit OF the program the device is already running — which is
    // the ONE fact `lib/resume.ts` decides on (`tests/resume.test.mjs` pins the
    // table; this pins that the console really asks it).
    //
    // The negative case is the ticket's: a playing playlist, a dirty 2D copy
    // from an earlier session, and a reload that used to replace both.
    const RES_PORT = E2E.mirror.wipResume;
    const RES = `http://127.0.0.1:${RES_PORT}`;
    const resDev = spawn(
      "../target/debug/luxel",
      ["serve", ...NO_NETIN, "--port", String(RES_PORT), "--pixels", "300"],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      resDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      resDev.on("exit", () => reject(new Error("#585 strip mirror died")));
      setTimeout(() => reject(new Error("#585 strip mirror start timeout")), 30000);
    });
    process.on("exit", () => resDev.kill());
    let resCtx = null;
    let liveCtx = null;
    try {
      const id1d = await saveOn(RES, "Resume 1D", REN1D);
      await saveOn(RES, "Resume 2D", REN2D);
      // a one-item playlist, so what the device is running stays deterministic
      await fetch(`${RES}/api/playlist`, { method: "POST", body: `D 30\nX 0\nI ${id1d} -1\n` });
      await fetch(`${RES}/api/playlist/play`, { method: "POST" });
      await sleep(900);
      check(
        "#585 setup: the mirror's playlist is playing",
        (await (await fetch(`${RES}/api/playlist`)).json()).playing === true,
      );

      const resWires = [];
      const res = await bootWithWip(RES, REN2D, { wires: resWires });
      resCtx = res.ctx;
      check(
        "#585: a console resuming a dirty working copy pushes no code",
        !resWires.includes("/api/code"),
        resWires.join(", ") || "(no POSTs)",
      );
      const stillPlaying = await (await fetch(`${RES}/api/playlist`)).json();
      check(
        "#585: …so the playlist is still playing",
        stillPlaying.playing === true,
        JSON.stringify({ p: stillPlaying.playing, i: stillPlaying.index }),
      );
      check(
        "#585: …and the device is still running the playlist's pattern",
        (await (await fetch(`${RES}/api/pattern`)).text()).trim() === REN1D.trim(),
      );
      const resChip = await res.pg.$eval('[data-role="layout-label"]', (el) =>
        (el.textContent ?? "").trim(),
      );
      check("#585: …the chip is still the fixture's", resChip === "300 px strip", resChip);

      // the copy IS the editor's document — in local preview, with the one
      // verb that would hand it to the LEDs offered
      await reloadInto(res.pg, EDIT);
      await sleep(3000);
      await res.pg.waitForSelector(".cm-content");
      check(
        "#585: …the editor holds the resumed working copy",
        (await res.pg.$eval(".cm-content", (el) => el.textContent ?? "")).includes("render2D"),
      );
      check(
        "#585: …and says it is preview only",
        (await saveState(res.pg)) === "unsaved · preview only",
        await saveState(res.pg),
      );
      check(
        "#585: …with ▶ Play on device offered as the way to change that",
        (await res.pg.$('[data-role="editor-play-device"]')) !== null,
      );
      check(
        "#585: …and that second boot pushed nothing either",
        !resWires.includes("/api/code"),
        resWires.join(", ") || "(no POSTs)",
      );
      await res.pg.screenshot({ path: `${shotDir}/device-e2e-585-preview-only.png` });

      // ---- the positive case: a dirty edit OF the running pattern ----
      await fetch(`${RES}/api/playlist/stop`, { method: "POST" });
      await fetch(`${RES}/api/patterns/${id1d}/activate`, { method: "POST" });
      await sleep(900);
      const EDITED = `${REN1D}\n// edited585`;
      const liveWires = [];
      const live = await bootWithWip(RES, EDITED, { wipId: id1d, wires: liveWires });
      liveCtx = live.ctx;
      check(
        "#585: a dirty edit of the RUNNING pattern still live-pushes at boot",
        liveWires.includes("/api/code"),
        liveWires.join(", ") || "(no POSTs)",
      );
      check(
        "#585: …so the device runs the edit",
        (await (await fetch(`${RES}/api/pattern`)).text()).includes("edited585"),
      );
      await reloadInto(live.pg, EDIT);
      await sleep(3000);
      await live.pg.waitForSelector(".cm-content");
      check(
        "#585: …and the header does NOT say preview only",
        (await saveState(live.pg)) === "unsaved",
        await saveState(live.pg),
      );
      await live.pg.screenshot({ path: `${shotDir}/device-e2e-585-live-resume.png` });
    } finally {
      if (resCtx) await resCtx.close();
      if (liveCtx) await liveCtx.close();
      resDev.kill();
    }
  }

  // ---- Jeremy's round-2 items (Gitea #538, 2026-09-20) --------------------
  //
  // One dedicated `--board panel` mirror, because these five checks want a
  // state of their own — a known 3-row playlist to drag, a known library to
  // delete from, an arena to read, and (last of all) a device to KILL.
  {
    const R2_PORT = E2E.mirror.doomed;
    const R2 = `http://127.0.0.1:${R2_PORT}`;
    const r2Dev = spawn(
      "../target/debug/luxel",
      [
        "serve",
        ...NO_NETIN,
        "--port",
        String(R2_PORT),
        "--board",
        "panel",
        "--pixels",
        "4096",
        "--name",
        "luxel-r2",
      ],
      { stdio: ["ignore", "pipe", "inherit"] },
    );
    await new Promise((resolve, reject) => {
      r2Dev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
      r2Dev.on("exit", () => reject(new Error("#538 r2 panel mirror died")));
      setTimeout(() => reject(new Error("#538 r2 panel mirror start timeout")), 30000);
    });
    process.on("exit", () => r2Dev.kill());
    let r2Ctx = null;
    try {
      const saveR2 = async (name, src) =>
        (
          await (
            await fetch(`${R2}/api/patterns`, { method: "POST", body: await lxpBody(name, src, 4096) })
          ).json()
        ).id;
      const mk = (h) =>
        `export function render(index) { hsv(${h} + index / pixelCount, 1, 1) }`;
      const idA = await saveR2("R2 Alpha", mk(0.1));
      const idB = await saveR2("R2 Bravo", mk(0.4));
      const idC = await saveR2("R2 Charlie", mk(0.7));
      await fetch(`${R2}/api/playlist`, {
        method: "POST",
        body: `D 300\nX 0\nI ${idA} -1\nI ${idB} -1\nI ${idC} -1\n`,
      });
      await fetch(`${R2}/api/playlist/play`, { method: "POST" });
      await sleep(700);

      r2Ctx = await browser.createBrowserContext();
      const pg = await r2Ctx.newPage();
      await pg.setViewport({ width: 1400, height: 900 });
      await gotoConsole(pg, R2, "#/playlist");
      await pg.waitForSelector('[data-role="playlist-item"]', { timeout: 15000 });
      await sleep(1500);

      // ---- 1. PSRAM: the NUMBER, not the word "present" ----
      {
        await pg.click('[data-role="tab-settings"]');
        await pg.waitForSelector('[data-role="adv-storage-toggle"]', { timeout: 10000 });
        const collapsed = await pg.$eval(
          '[data-role="adv-storage-status"]',
          (el) => el.textContent.trim(),
        );
        check(
          "r2-1: the collapsed Storage row carries the arena figure",
          /PSRAM 8\.0 MB free of 8 MB/.test(collapsed),
          collapsed,
        );
        await pg.click('[data-role="adv-storage-toggle"]');
        await sleep(600);
        const psram = await pg.$eval('[data-role="storage-psram"]', (el) => el.textContent.trim());
        check("r2-1: Storage states `8.0 MB free of 8 MB`", psram === "8.0 MB free of 8 MB", psram);
        const after = await pg.$eval('[data-role="storage-psram"]', (el) =>
          (el.parentElement.textContent ?? "").trim(),
        );
        check(
          "r2-1: …and the one-line explanation comes AFTER the number",
          after.indexOf("8.0 MB") < after.indexOf("external arena"),
          after.slice(0, 120),
        );
        await pg.screenshot({ path: `${shotDir}/device-e2e-r2-psram.png` });
        await pg.click('[data-role="tab-playlist"]');
        await sleep(800);
      }

      // ---- 3. drag to reorder: LIVE, and only the release writes ----
      {
        const posts = [];
        const countPl = (r) => {
          if (r.method() === "POST" && r.url() === `${R2}/api/playlist`) posts.push(r.postData());
        };
        const rowBox = async (n) =>
          pg.$$eval(
            '[data-role="playlist-item"]',
            (els, i) => {
              const b = els[i].getBoundingClientRect();
              return { x: b.x + b.width / 2, y: b.y + b.height / 2, h: b.height };
            },
            n,
          );
        const gripBox = async (n) =>
          pg.$$eval(
            '[data-role="pl-grip"]',
            (els, i) => {
              const b = els[i].getBoundingClientRect();
              return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
            },
            n,
          );
        const names = () =>
          pg.$$eval('[data-role="pl-name"]', (els) => els.map((e) => e.textContent.trim()));
        const transforms = () =>
          pg.$$eval('[data-role="playlist-item"]', (els) =>
            els.map((e) => getComputedStyle(e).transform),
          );

        const before = await names();
        check(
          "r2-3 setup: three rows in the order they were queued",
          before.join("|") === "R2 Alpha|R2 Bravo|R2 Charlie",
          before.join("|"),
        );
        check(
          "r2-3: a resting row carries no transform (the mock's row is untouched)",
          (await transforms()).every((t) => t === "none"),
          (await transforms()).join(" · "),
        );

        pg.on("request", countPl);
        // grab row 0's handle and drag it down past row 1's middle
        const g0 = await gripBox(0);
        const r1 = await rowBox(1);
        await pg.mouse.move(g0.x, g0.y);
        await pg.mouse.down();
        await pg.mouse.move(g0.x, g0.y + r1.h * 0.6, { steps: 8 });
        await sleep(200);

        const mid = await transforms();
        const lifted = await pg.$$eval('[data-role="playlist-item"]', (els) =>
          els.map((e) => e.getAttribute("data-lifted")),
        );
        check(
          "r2-3: the grabbed row is LIFTED and follows the pointer",
          lifted[0] === "1" && mid[0] !== "none" && /matrix/.test(mid[0]),
          `${lifted[0]} / ${mid[0]}`,
        );
        check(
          "r2-3: …and the row it is passing has slid up to open the hole",
          mid[1] !== "none",
          mid[1],
        );
        check(
          "r2-3: …a row beyond the drop point has not moved",
          mid[2] === "none",
          mid[2],
        );
        const during = await names();
        check(
          "r2-3: the ORDER is unchanged while dragging",
          during.join("|") === before.join("|"),
          during.join("|"),
        );
        check("r2-3: …and nothing has been written yet", posts.length === 0, `${posts.length} POSTs`);
        await pg.screenshot({ path: `${shotDir}/device-e2e-r2-drag.png` });

        await pg.mouse.up();
        await sleep(1400); // past the 400 ms edit debounce
        pg.off("request", countPl);
        const after = await names();
        check(
          "r2-3: the release reorders the list",
          after.join("|") === "R2 Bravo|R2 Alpha|R2 Charlie",
          after.join("|"),
        );
        check(
          "r2-3: …with exactly ONE POST, carrying the new order",
          posts.length === 1 && posts[0].indexOf(idB) < posts[0].indexOf(idA),
          `${posts.length} POSTs`,
        );
        check(
          "r2-3: …and every row is back to no transform",
          (await transforms()).every((t) => t === "none"),
          (await transforms()).join(" · "),
        );
        const onDevice = await (await fetch(`${R2}/api/playlist`)).json();
        check(
          "r2-3: …which is what the device now holds",
          onDevice.items.map((i) => i.id).join("|") === [idB, idA, idC].join("|"),
          onDevice.items.map((i) => i.id).join("|"),
        );

        // ---- Escape cancels, and writes nothing ----
        const posts2 = [];
        const countPl2 = (r) => {
          if (r.method() === "POST" && r.url() === `${R2}/api/playlist`) posts2.push(1);
        };
        pg.on("request", countPl2);
        const g2 = await gripBox(2);
        const rr = await rowBox(0);
        await pg.mouse.move(g2.x, g2.y);
        await pg.mouse.down();
        await pg.mouse.move(g2.x, g2.y - rr.h * 1.4, { steps: 8 });
        await sleep(150);
        check(
          "r2-3: Escape case — the drag is live first",
          (await transforms())[2] !== "none",
        );
        await pg.keyboard.press("Escape");
        await sleep(400);
        await pg.mouse.up();
        await sleep(1200);
        pg.off("request", countPl2);
        check(
          "r2-3: Escape cancels the reorder",
          (await names()).join("|") === "R2 Bravo|R2 Alpha|R2 Charlie",
          (await names()).join("|"),
        );
        check("r2-3: …and writes nothing", posts2.length === 0, `${posts2.length} POSTs`);
        check(
          "r2-3: …and animates every row back to rest",
          (await transforms()).every((t) => t === "none"),
          (await transforms()).join(" · "),
        );

        // the keyboard path the handle has always carried still works
        await pg.$$eval('[data-role="pl-grip"]', (els) => els[0].focus());
        await pg.keyboard.press("ArrowDown");
        await sleep(1200);
        check(
          "r2-3: ↑/↓ on the handle still reorders (the a11y path)",
          (await names()).join("|") === "R2 Alpha|R2 Bravo|R2 Charlie",
          (await names()).join("|"),
        );
      }

      // ---- 2. a direct Play parks the playlist ----
      {
        check(
          "r2-2 setup: the playlist is playing",
          (await (await fetch(`${R2}/api/playlist`)).json()).playing === true,
        );
        const parkedAt = (await (await fetch(`${R2}/api/playlist`)).json()).index;
        await pg.click('[data-role="tab-patterns"]');
        await pg.waitForSelector('[data-role="patterns-grid"][data-source="device"] .tile', {
          timeout: 15000,
        });
        await sleep(1200);
        // Play the pattern that is NOT the one the playlist is on
        const target = "R2 Charlie";
        const sel = `[data-role="patterns-grid"][data-source="device"] .tile[data-name="${target}"]`;
        await tileAction(pg, sel, "tile-play");
        await sleep(1800);
        const pl = await (await fetch(`${R2}/api/playlist`)).json();
        check(
          "r2-2: Play on a tile STOPS the playlist first",
          pl.playing === false,
          JSON.stringify({ playing: pl.playing, index: pl.index }),
        );
        check(
          "r2-2: …and the device is running the pattern that was played",
          (await (await fetch(`${R2}/api/pattern`)).text()).includes("0.7"),
        );
        await pg.click('[data-role="tab-playlist"]');
        await sleep(1200);
        const nowName = await pg.$eval('[data-role="pl-now-name"]', (el) => el.textContent.trim());
        check(
          "r2-2: the transport parks on the item it left",
          nowName === "R2 Alpha" || nowName === "R2 Bravo" || nowName === "R2 Charlie",
          nowName,
        );
        const why = await pg.$eval('[data-role="pl-preempted"]', (el) =>
          el.textContent.replace(/\s+/g, " ").trim(),
        );
        check(
          "r2-2: …and the now-playing block SAYS why it is stopped",
          why === `stopped — playing ${target} directly`,
          why,
        );
        check(
          "r2-2: …the transport offers Play (the parked item resumes)",
          (await pg.$('[data-role="pl-play"]')) !== null,
        );
        await pg.screenshot({ path: `${shotDir}/device-e2e-r2-parked.png` });
        await pg.click('[data-role="pl-play"]');
        await sleep(1400);
        const resumed = await (await fetch(`${R2}/api/playlist`)).json();
        check(
          "r2-2: Play resumes the playlist from the parked item",
          resumed.playing === true && resumed.index === parkedAt,
          JSON.stringify({ playing: resumed.playing, index: resumed.index, parkedAt }),
        );
        check(
          "r2-2: …and the explanation is gone",
          (await pg.$('[data-role="pl-preempted"]')) === null,
        );
      }

      // ---- 6. deleting one pattern must not rebuild the other tiles ----
      {
        await pg.click('[data-role="tab-patterns"]');
        await pg.waitForSelector('[data-role="patterns-grid"][data-source="device"] .tile', {
          timeout: 15000,
        });
        await sleep(2500); // every tile has compiled and drawn
        const GRID = '[data-role="patterns-grid"][data-source="device"]';
        const stamp = async () =>
          pg.$$eval(`${GRID} .tile`, (els) =>
            els.map((e) => ({
              name: e.dataset.name,
              built: e.dataset.compiled,
              spinner: !!e.querySelector('[data-role="tile-spinner"]'),
            })),
          );
        // mark every surviving canvas so element identity can be checked after
        await pg.$$eval(`${GRID} .tile canvas`, (els) => els.forEach((c, i) => (c.__r2 = i)));
        const before = await stamp();
        check("r2-6 setup: three tiles, all compiled", before.length === 3, JSON.stringify(before));

        const fetched = [];
        const countSrc = (r) => {
          const m = /\/api\/patterns\/([0-9a-fx]+)$/.exec(r.url());
          if (r.method() === "GET" && m) fetched.push(m[1]);
        };
        pg.on("request", countSrc);
        await tileAction(pg, `${GRID} .tile[data-name="R2 Bravo"]`, "tile-menu");
        await pg.waitForSelector('[data-role="tile-menu-delete"]', { timeout: 5000 });
        await pg.click('[data-role="tile-menu-delete"]');
        await acceptDialog(pg);
        await sleep(2500);
        pg.off("request", countSrc);

        const after = await stamp();
        check(
          "r2-6: the deleted tile is gone and the others remain",
          after.length === 2 && !after.some((t) => t.name === "R2 Bravo"),
          JSON.stringify(after.map((t) => t.name)),
        );
        check(
          "r2-6: the survivors' engines were NOT rebuilt (data-compiled unchanged)",
          after.every(
            (t) => t.built === before.find((b) => b.name === t.name)?.built && t.built === "1",
          ),
          JSON.stringify(after.map((t) => `${t.name}=${t.built}`)),
        );
        check(
          "r2-6: …no survivor fell back to a spinner",
          after.every((t) => !t.spinner),
          JSON.stringify(after.map((t) => `${t.name}:${t.spinner}`)),
        );
        const sameCanvas = await pg.$$eval(`${GRID} .tile canvas`, (els) =>
          els.every((c) => typeof c.__r2 === "number"),
        );
        check("r2-6: …and kept their canvas elements", sameCanvas);
        check(
          "r2-6: …and no source was re-fetched for them",
          fetched.length === 0,
          fetched.join(", "),
        );
      }

      // ---- 4. a refused settings POST is a banner at the TOP, not a whisper ----
      {
        await pg.click('[data-role="tab-settings"]');
        await pg.waitForSelector('[data-role="layout-cols"]', { timeout: 10000 });
        await sleep(800);
        // 2 x 1 panels of 64x64 = 8192 on a 4096-px board (#600). The card
        // pre-checks it, so nothing is sent — and the banner still explains.
        const layoutPosts = [];
        const countLayout = (r) => {
          if (r.method() === "POST" && r.url() === `${R2}/api/layout`) layoutPosts.push(1);
        };
        pg.on("request", countLayout);
        await pg.$eval('[data-role="layout-cols"]', (el) => {
          el.value = "2";
          el.dispatchEvent(new Event("change", { bubbles: true }));
        });
        await sleep(1200);
        pg.off("request", countLayout);
        const bar = await pg.$('[data-role="api-error-bar"]');
        check("r2-4: an over-cap chain raises the error banner", bar !== null);
        const text = await pg.$eval('[data-role="api-error-text"]', (el) =>
          el.textContent.replace(/\s+/g, " ").trim(),
        );
        check(
          "r2-4: …stating both numbers",
          text.startsWith("8,192 px — this board tops out at 4,096."),
          text.slice(0, 80),
        );
        check("r2-4: …and the reason", /bitplane DMA frame buffers/.test(text), text.slice(0, 200));
        check(
          "r2-4: …and an arrangement that fits",
          // the banner renders the table's backticked wire line as real <code>,
          // so the delimiters are gone from textContent
          /two 32×64 tiles \(matrix 32 64 2 1 tr row 0 0\)/.test(text),
          text.slice(-120),
        );
        check(
          "r2-4: …with the device's own words kept in a details line",
          /pw\*ph\*cols\*rows out of range/.test(
            await pg.$eval('[data-role="api-error-details"]', (el) => el.textContent),
          ),
        );
        check("r2-4: …and nothing was POSTed (#600 pre-check)", layoutPosts.length === 0);
        const marked = await pg.$eval(
          '[data-role="layout-cols"]',
          (el) => el.hasAttribute("data-field-error"),
        );
        check("r2-4: …with the field it belongs to highlighted", marked);
        // it is at the TOP: above the settings panel, not inside the form
        const above = await pg.evaluate(() => {
          const b = document.querySelector('[data-role="api-error-bar"]').getBoundingClientRect();
          const p = document.querySelector('[data-role="settings-panel"]').getBoundingClientRect();
          return b.bottom <= p.top + 1;
        });
        check("r2-4: …at the TOP of the page, above the settings panel", above);
        await pg.screenshot({ path: `${shotDir}/device-e2e-r2-errorbar.png` });
        await pg.click('[data-role="api-error-dismiss"]');
        await sleep(400);
        check(
          "r2-4: …and it is dismissable",
          (await pg.$('[data-role="api-error-bar"]')) === null,
        );
        check(
          "r2-4: …which also clears the field highlight",
          !(await pg.$eval('[data-role="layout-cols"]', (el) =>
            el.hasAttribute("data-field-error"),
          )),
        );
      }

      // ---- 7. the device goes away, and the app SAYS so (last: it kills it) ----
      {
        r2Dev.kill();
        await sleep(6000); // the 1 Hz fast-fail probe needs two misses
        const bar = await pg.$('[data-role="device-down-bar"]');
        check("r2-7: a dead device raises the unreachable banner within ~5 s", bar !== null);
        const text = await pg.$eval('[data-role="device-down-bar"]', (el) =>
          el.textContent.replace(/\s+/g, " ").trim(),
        );
        check(
          "r2-7: …saying it is retrying, and when the device was last seen",
          /^Device unreachable — retrying…/.test(text) && /last seen \d+ s ago/.test(text),
          text,
        );
        await pg.screenshot({ path: `${shotDir}/device-e2e-r2-down.png` });

        // a WRITE while down fails FAST — no 30 s of silent retries
        await pg.click('[data-role="tab-patterns"]');
        await sleep(600);
        const t0 = Date.now();
        await tileAction(
          pg,
          '[data-role="patterns-grid"][data-source="device"] .tile',
          "tile-play",
        );
        await pg.waitForSelector('[data-role="api-error-bar"], [data-role="device-down-bar"]', {
          timeout: 8000,
        });
        const dt = Date.now() - t0;
        check("r2-7: a write while down reports at once, not after the retry ladder", dt < 8000, `${dt} ms`);

        // bring it back: the banner clears itself and says so
        const revived = spawn(
          "../target/debug/luxel",
          [
            "serve",
            ...NO_NETIN,
            "--port",
            String(R2_PORT),
            "--board",
            "panel",
            "--pixels",
            "4096",
            "--name",
            "luxel-r2",
          ],
          { stdio: ["ignore", "pipe", "inherit"] },
        );
        process.on("exit", () => revived.kill());
        await new Promise((resolve, reject) => {
          revived.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
          setTimeout(() => reject(new Error("#538 r2 mirror restart timeout")), 30000);
        });
        await sleep(4000);
        check(
          "r2-7: the banner clears itself when the device answers again",
          (await pg.$('[data-role="device-down-bar"]')) === null,
        );
        revived.kill();
      }
    } finally {
      if (r2Ctx) await r2Ctx.close();
      r2Dev.kill();
    }
  }

  // ---- release packages, format skew and the store self-heal (Gitea #643) --
  //
  // The Athom went dark on 2026-09-20 taking a firmware OTA across an LXBC
  // format bump: the new engine could not read one blob in its store, and the
  // console it served — the one that shipped with the OLD firmware — could
  // not compile anything the new engine would run. Three mechanisms came out
  // of that, and all three are driven here:
  //
  //   1. a `.luxr` release package that installs firmware AND the matching
  //      web assets as one action (the mirror takes both under --accept-ota);
  //   2. a banner when the two bytecode formats disagree, in either
  //      direction (--bc-format impersonates a device on either side);
  //   3. an automatic recompile of the stored patterns when only the BLOBS
  //      are behind (--stale-store ages a store the way the bump did).
  {
    const tmp = fs.mkdtempSync("/tmp/luxr-e2e-");
    const PKG_BOARD = "Pixelblaze v3 Standard";
    const appBytes = 2048;
    const assetBytes = 512;
    fs.writeFileSync(`${tmp}/app.bin`, Buffer.alloc(appBytes, 0xe9));
    fs.writeFileSync(`${tmp}/web.luxa`, Buffer.alloc(assetBytes, 0x4c));
    // Built by the SAME packer tools/deploy.sh and release.yml use, so the
    // container the console parses here is the one the bench produces.
    const packLuxr = (out, board) =>
      execSync(
        "node --experimental-strip-types --disable-warning=ExperimentalWarning " +
          `tools/pack-luxr.mjs --board '${board}' --version 9.9.9 ` +
          `--app ${tmp}/app.bin --assets ${tmp}/web.luxa ${tmp}/${out}`,
        { stdio: "pipe" },
      );
    packLuxr("right.luxr", PKG_BOARD);
    packLuxr("wrong.luxr", "Athom music-reactive WLED controller");
    check(
      "643: pack-luxr.mjs writes a container of exactly header + both payloads",
      fs.statSync(`${tmp}/right.luxr`).size === 80 + PKG_BOARD.length + 5 + appBytes + assetBytes,
      String(fs.statSync(`${tmp}/right.luxr`).size),
    );

    // ---- 2. the two skew banners ----
    for (const [role, fmt, needle] of [
      ["bc-bundle-older", 99, /compiles v\d+, device reads v99/],
      ["bc-bundle-newer", 1, /device reads v1/],
    ]) {
      const port = role.endsWith("older") ? E2E.mirror.bcOld : E2E.mirror.bcNew;
      const dev = spawn(
        "../target/debug/luxel",
        ["serve", ...NO_NETIN, "--port", String(port), "--pixels", "60", "--bc-format", String(fmt)],
        { stdio: ["ignore", "pipe", "inherit"] },
      );
      await new Promise((resolve, reject) => {
        dev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
        dev.on("exit", () => reject(new Error("#643 skew mirror died")));
        setTimeout(() => reject(new Error("#643 skew mirror start timeout")), 30000);
      });
      process.on("exit", () => dev.kill());
      const pg = await browser.newPage();
      try {
        await pg.setViewport({ width: 1400, height: 900 });
        await gotoConsole(pg, `http://127.0.0.1:${port}`);
        await pg.waitForSelector(`[data-role="${role}"]`, { timeout: 15000 });
        const text = await pg.$eval('[data-role="bc-banner-text"]', (el) => el.textContent.trim());
        check(`643: ${role} banner names both formats`, needle.test(text), text);
        // the OTHER banner must not be on screen at the same time
        const other = role.endsWith("older") ? "bc-bundle-newer" : "bc-bundle-older";
        check(
          `643: ${role} is the only skew banner`,
          (await pg.$(`[data-role="${other}"]`)) === null,
        );
        // the upload is offered only where the fix is a file the user has
        check(
          `643: ${role} offers the web-asset upload only when that is the fix`,
          ((await pg.$('[data-role="bc-assets-upload"]')) !== null) === role.endsWith("older"),
        );
        // ...and a skewed device is never healed: a recompile with the wrong
        // compiler would replace unreadable blobs with unreadable blobs
        check(
          `643: ${role} does not start a recompile`,
          (await pg.$('[data-role="bc-healing"]')) === null &&
            (await pg.$('[data-role="bc-healed"]')) === null,
        );
        await pg.screenshot({ path: `${shotDir}/device-e2e-643-${role}.png` });
      } finally {
        await pg.close();
        dev.kill();
      }
    }

    // ---- 1. Settings → Firmware & recovery → Update… installs a package ----
    {
      const OTA_PORT = E2E.mirror.otaAccept;
      const OTA = `http://127.0.0.1:${OTA_PORT}`;
      const otaDev = spawn(
        "../target/debug/luxel",
        [
          "serve",
          ...NO_NETIN,
          "--port",
          String(OTA_PORT),
          "--pixels",
          "60",
          "--accept-ota",
          "--board-name",
          PKG_BOARD,
        ],
        { stdio: ["ignore", "pipe", "inherit"] },
      );
      await new Promise((resolve, reject) => {
        otaDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
        otaDev.on("exit", () => reject(new Error("#643 OTA mirror died")));
        setTimeout(() => reject(new Error("#643 OTA mirror start timeout")), 30000);
      });
      process.on("exit", () => otaDev.kill());
      const pg = await browser.newPage();
      try {
        await pg.setViewport({ width: 1400, height: 900 });
        await gotoConsole(pg, OTA);
        await pg.click('[data-role="tab-settings"]');
        await openAdv(pg, "adv-firmware");
        check(
          "643: the Firmware row names the board the package must match",
          (await pg.$eval('[data-role="fw-board"]', (el) => el.textContent.trim())) === PKG_BOARD,
        );

        // A wrong-board package is refused BEFORE anything is streamed —
        // #389's lesson, one step earlier than ota-push.sh's image grep.
        const before = await (await fetch(`${OTA}/api/status`)).json();
        await (await pg.$('[data-role="fw-file"]')).uploadFile(`${tmp}/wrong.luxr`);
        await pg.waitForSelector('[data-role="api-error-bar"]', { timeout: 8000 });
        const err = await pg.$eval('[data-role="api-error-details"]', (el) => el.textContent);
        check("643: a wrong-board package is refused by name", /built for Athom/.test(err), err);
        const stillBefore = await (await fetch(`${OTA}/api/status`)).json();
        check(
          "643: the refused package never reached the OTA slot",
          stillBefore.version === before.version,
          stillBefore.version,
        );
        await pg.screenshot({ path: `${shotDir}/device-e2e-643-board-mismatch.png` });
        await pg.click('[data-role="api-error-dismiss"]');

        // The real thing: firmware, the reboot wait, then the assets.
        await (await pg.$('[data-role="fw-file"]')).uploadFile(`${tmp}/right.luxr`);
        await waitDialog(pg);
        const title = await dialogTitle(pg);
        check("643: the install is confirmed by file name", /right\.luxr/.test(title), title);
        const body = await pg.$eval('[data-role="dialog"]', (el) => el.textContent);
        check(
          "643: the confirm says both halves are installed",
          /web app from the same release/.test(body),
          body.slice(0, 160),
        );
        await acceptDialog(pg);
        await pg.waitForSelector('[data-role="fw-progress"]', { timeout: 8000 });
        await pg.screenshot({ path: `${shotDir}/device-e2e-643-installing.png` });
        // The mirror discards the bytes but reports a `+otaN` version, which
        // is the "it came back as something else" the flow waits for.
        let got = null;
        for (let i = 0; i < 60; i++) {
          const st = await (await fetch(`${OTA}/api/status`)).json();
          if (st.version !== before.version) {
            got = st;
            break;
          }
          await sleep(500);
        }
        check("643: the device came back as a different build", got !== null, got?.version ?? "never");
        // The console reloads itself once the assets land, so the assertion
        // that they DID land is made against the device, not the page.
        let landed = null;
        for (let i = 0; i < 40; i++) {
          const st = await (await fetch(`${OTA}/api/status`)).json();
          if (st.ota?.assets > 0) {
            landed = st.ota;
            break;
          }
          await sleep(500);
        }
        check(
          "643: the web assets followed the firmware without being asked for",
          landed !== null && landed.app === appBytes && landed.assets === assetBytes,
          JSON.stringify(landed),
        );

        // A bare .bin is still accepted — and says out loud what it did NOT do.
        await gotoConsole(pg, OTA);
        await pg.click('[data-role="tab-settings"]');
        await openAdv(pg, "adv-firmware");
        await (await pg.$('[data-role="fw-file"]')).uploadFile(`${tmp}/app.bin`);
        await waitDialog(pg);
        const bareBody = await pg.$eval('[data-role="dialog"]', (el) => el.textContent);
        check(
          "643: a bare image warns that the console is NOT updated",
          /firmware image ONLY/.test(bareBody),
          bareBody.slice(0, 200),
        );
        await cancelDialog(pg);
      } finally {
        await pg.close();
        otaDev.kill();
      }
    }

    // ---- 3. a stale store heals itself, and the playlist plays again ----
    {
      const ST_PORT = E2E.mirror.staleStore;
      const ST = `http://127.0.0.1:${ST_PORT}`;
      const stDev = spawn(
        "../target/debug/luxel",
        [
          "serve",
          ...NO_NETIN,
          "--port",
          String(ST_PORT),
          "--pixels",
          "60",
          "--fps",
          "24",
          "--stale-store",
        ],
        { stdio: ["ignore", "pipe", "inherit"] },
      );
      await new Promise((resolve, reject) => {
        stDev.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
        stDev.on("exit", () => reject(new Error("#643 stale mirror died")));
        setTimeout(() => reject(new Error("#643 stale mirror start timeout")), 30000);
      });
      process.on("exit", () => stDev.kill());
      const pg = await browser.newPage();
      try {
        // Seed the store the way an older console would have: every pattern
        // entering it for the first time is aged by one format version.
        const ids = [];
        for (const [name, hue] of [
          ["Stale Aurora", "0.1"],
          ["Stale Fairies", "0.6"],
        ]) {
          const r = await fetch(`${ST}/api/patterns`, {
            method: "POST",
            body: await lxpBody(
              name,
              `export function render(index) { hsv(${hue} + index / pixelCount, 1, 1) }`,
              60,
            ),
          });
          ids.push((await r.json()).id);
        }
        const listed = await (await fetch(`${ST}/api/patterns`)).json();
        check(
          "643: the device flags every blob it cannot read",
          listed.patterns.length === 2 && listed.patterns.every((p) => p.stale === true),
          JSON.stringify(listed),
        );
        await fetch(`${ST}/api/playlist`, {
          method: "POST",
          body: `D 3\nI ${ids[0]} -1\nI ${ids[1]} -1\n`,
        });
        const plBefore = await (await fetch(`${ST}/api/playlist`)).json();
        check(
          "643: every playlist item is invalid while the blobs are stale",
          plBefore.items.length === 2 &&
            plBefore.items.every((i) => /bytecode format/.test(i.invalid ?? "")),
          JSON.stringify(plBefore.items.map((i) => i.invalid)),
        );

        // Opening the console is the whole user action.
        await pg.setViewport({ width: 1400, height: 900 });
        await gotoConsole(pg, ST);
        await pg.waitForSelector('[data-role="bc-healed"]', { timeout: 25000 });
        const healed = await pg.$eval('[data-role="bc-healed-text"]', (el) => el.textContent.trim());
        check(
          "643: the console reports what it recompiled",
          /recompiled 2 stored patterns/.test(healed),
          healed,
        );
        check(
          "643: no skew banner — the formats agreed, only the blobs were behind",
          (await pg.$('[data-role="bc-bundle-older"]')) === null,
        );
        await pg.screenshot({ path: `${shotDir}/device-e2e-643-healed.png` });

        const after = await (await fetch(`${ST}/api/patterns`)).json();
        check(
          "643: nothing is stale afterwards",
          after.patterns.every((p) => p.stale !== true),
          JSON.stringify(after),
        );
        check(
          "643: the ids survived, so every playlist reference did too",
          after.patterns
            .map((p) => p.id)
            .sort()
            .join() === ids.slice().sort().join(),
          `${after.patterns.map((p) => p.id)} vs ${ids}`,
        );
        const plAfter = await (await fetch(`${ST}/api/playlist`)).json();
        check(
          "643: the playlist re-validates clean",
          plAfter.items.length === 2 && plAfter.items.every((i) => i.invalid === undefined),
          JSON.stringify(plAfter.items.map((i) => i.invalid)),
        );

        // ...and it actually plays: a repaired blob really does decode.
        await fetch(`${ST}/api/playlist/play`, { method: "POST", body: "0" });
        await sleep(2500);
        const st = await (await fetch(`${ST}/api/status`)).json();
        check("643: the strip is lit again", st.vmerr === null && st.fps > 0, `fps ${st.fps} vmerr ${st.vmerr}`);
        await fetch(`${ST}/api/playlist/stop`, { method: "POST", body: "" });

        // Idempotent: a reload over a healthy store repairs nothing.
        await gotoConsole(pg, ST);
        await sleep(3000);
        check(
          "643: a reload over a healthy store recompiles nothing",
          (await pg.$('[data-role="bc-healed"]')) === null &&
            (await pg.$('[data-role="bc-healing"]')) === null,
        );
      } finally {
        await pg.close();
        stDev.kill();
      }
    }
    fs.rmSync(tmp, { recursive: true, force: true });
  }


} finally {
  await browser.close();
  device.kill();
  web.kill();
}

console.log(fails.length === 0 ? "\nall device-mode checks passed" : `\n${fails.length} FAILURES`);
process.exit(fails.length === 0 ? 0 : 1);
