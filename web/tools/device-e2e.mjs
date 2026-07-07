// Device-mode e2e: the built playground in real chromium, connected to
// `luxel serve` (the native mirror of the firmware API). Verifies connect,
// editor sync from the device, live-code push, preview streaming, controls,
// vars, compile errors, and disconnect.
//
// Usage (from web/): npm run build && node tools/device-e2e.mjs

import { execSync, spawn } from "node:child_process";
import puppeteer from "puppeteer-core";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

const PORT = 4181;
const DEV_PORT = 8723;
const DEV = `http://127.0.0.1:${DEV_PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

execSync("cargo build -q -p luxel-cli", { stdio: "inherit", cwd: ".." });
const device = spawn(
  "../target/debug/luxel",
  ["serve", "--port", String(DEV_PORT), "--pixels", "120"],
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
  args: ["--no-sandbox", "--disable-gpu", "--window-size=1400,900"],
});

const fails = [];
const check = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) fails.push(name);
};

async function setEditor(page, text) {
  await page.click(".cm-content");
  await page.keyboard.down("Control");
  await page.keyboard.press("a");
  await page.keyboard.up("Control");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(text, { delay: 0 });
}

try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  // No device-URL field any more: a real device serves the UI from its own
  // flash (auto-connect to same origin); here we use the `?device=` dev
  // override to point the built playground at the mirror, and it auto-connects.
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle0",
  });
  await page.waitForSelector(".cm-content");
  await page.waitForSelector(".device-badge", { timeout: 8000 });
  check("connect: badge shown (auto-connect, no URL typed)", true);

  // the device-URL field is gone entirely (the address is always known)
  check("device: no device-url field", (await page.$(".device-url")) === null);
  // share makes no sense on a device (the link is the device's LAN address) —
  // it must not appear in device mode
  check("device: no share button", (await page.$('[data-role="share"]')) === null);

  // connect-on-load race (#9): the async handshake is held cleanly — a
  // "connecting…" state shows and the waterfall is kept blank so no
  // pre-stream frames (the playground was running before we connected) leak
  // into it. The ws dial is delayed ~1.6 s, so we're still connecting here.
  const sawConnecting = await page
    .waitForFunction(
      () => document.querySelector('[data-role="conn-state"]')?.textContent?.includes("connecting"),
      { timeout: 2500 },
    )
    .then(() => true)
    .catch(() => false);
  check("connect: shows a connecting state", sawConnecting);
  const blankWhileConnecting = await page.$eval(".waterfall", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    let lit = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 0) lit++;
    return lit;
  });
  check(
    "connect: waterfall held blank while connecting (no stale frames)",
    blankWhileConnecting === 0,
    `${blankWhileConnecting} lit`,
  );

  const px = await page.$eval('[data-role="cfg-pixels"]', (el) => el.value);
  check("connect: pixel count from device", px === "120", `got ${px}`);
  // CodeMirror virtualizes: textContent only holds visible lines — check
  // the top of the device's default pattern
  const doc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check(
    "connect: editor synced to device pattern",
    doc.includes("canonical default pattern"),
    doc.slice(0, 40),
  );

  // the playground deliberately delays the ws dial (~1.6 s grace after
  // connect) — wait for the badge rather than racing a fixed sleep
  const wsBadge = await page
    .waitForFunction(() => document.body.innerText.includes("streaming"), { timeout: 10000 })
    .then(() => true)
    .catch(() => false);
  check("preview: websocket push active", wsBadge);
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
  check("preview streams from device", lit > 60, `lit=${lit}`);

  // layout dropdown is back in device mode (#3): strip/grid/2D map arrange the
  // live stream in the local preview (the device's pixel count stays fixed)
  const layoutSel = await page.$('[data-role="layout-kind"]');
  check("layout: dropdown present on device", layoutSel !== null);
  const layoutOpts = await page.$$eval('[data-role="layout-kind"] option', (os) =>
    os.map((o) => o.value),
  );
  check(
    "layout: offers strip/grid/2D map",
    ["strip", "grid", "map"].every((k) => layoutOpts.includes(k)),
    layoutOpts.join(","),
  );
  // the strip pixel count is fixed by hardware — the field is read-only
  const pxDisabled = await page.$eval('[data-role="layout-px"]', (el) => el.disabled);
  check("layout: strip pixel count is read-only on device", pxDisabled === true);
  // switching to grid rearranges the preview and reveals the grid inputs
  await page.select('[data-role="layout-kind"]', "grid");
  await sleep(300);
  check("layout: grid reveals w×h inputs", (await page.$('[data-role="layout-w"]')) !== null);
  // a preview-only layout change must NOT re-push the pattern to the device
  const stillRunning = await (await fetch(`${DEV}/api/pattern`)).text();
  check(
    "layout: grid switch doesn't disturb the device pattern",
    stillRunning.includes("canonical default pattern"),
  );
  await page.select('[data-role="layout-kind"]', "strip"); // restore
  await sleep(200);

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

  // pixel count (Phase 3): the Settings Pixels field resizes the strip LIVE
  // via /api/config (no reboot)
  const cfg0 = await (await fetch(`${DEV}/api/config`)).json();
  check("config: GET returns {pixels,max}", cfg0.pixels === 120 && cfg0.max >= 120, JSON.stringify(cfg0));
  await page.$eval('[data-role="cfg-pixels"]', (el) => {
    el.value = "48";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(700);
  const cfg1 = await (await fetch(`${DEV}/api/config`)).json();
  check("config: field resizes the device live", cfg1.pixels === 48, JSON.stringify(cfg1));
  const stAfter = await (await fetch(`${DEV}/api/status`)).json();
  check("config: status reports the new count", stAfter.pixels === 48, JSON.stringify(stAfter));
  // the device now streams 48 px — verify the pixel buffer resized
  const pxLen = (await (await fetch(`${DEV}/api/pixels`)).arrayBuffer()).byteLength;
  check("config: pixel buffer resized (48×3)", pxLen === 48 * 3, `${pxLen} bytes`);
  await page.$eval('[data-role="cfg-pixels"]', (el) => {
    el.value = "120";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(700); // restore for the rest of the suite

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
  check("controls: slider appeared from device", true);

  // move the slider to full; device pixels should go bright blue
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

  // compile error path: line/col + squiggle source range
  await setEditor(page, "export function render(index) { hsv( }");
  await sleep(1000);
  const banner = await page.$eval(".banner.error", (el) => el.textContent ?? "").catch(() => "");
  check("errors: device diagnostics shown", /line \d+:\d+/.test(banner), banner.trim());
  // device keeps the previous pattern running
  const still = await fetch(`${DEV}/api/pattern`);
  check("errors: device keeps old pattern", (await still.text()).includes("sliderBlue"));

  // ---- device pattern library (CRUD via the Device Patterns tab) ----
  // restore a valid pattern in the editor first (the error test left junk)
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 0.4) }");
  await sleep(900);
  page.on("dialog", (d) => {
    if (d.message().includes("save pattern")) return void d.accept("device kept");
    if (d.message().includes("delete")) return void d.accept();
    void d.dismiss();
  });
  await page.click('[data-role="save"]'); // save (editor header) → stores on the device
  await sleep(900);
  const apiList = await (await fetch(`${DEV}/api/patterns`)).json();
  check(
    "library: save-to-device stores it",
    apiList.patterns?.some((p) => p.name === "device kept"),
    JSON.stringify(apiList),
  );
  // the Device Patterns tab lists it (back out of the editor)
  await page.click('[data-role="editor-back"]');
  await sleep(400);
  check("library: back lands on Device Patterns", (await page.$('[data-role="device-panel"]:not([hidden])')) !== null);
  const listed = await page.$$eval('[data-role="device-pattern"]', (els) => els.map((e) => e.textContent ?? ""));
  check("library: Device Patterns tab lists it", listed.some((t) => t.includes("device kept")), listed.join("|"));
  // each device pattern renders a live preview thumbnail (#2) — its source is
  // fetched in the background, so wait for the canvas to appear
  const hasThumb = await page
    .waitForSelector('[data-role="device-pattern"] canvas', { timeout: 4000 })
    .then(() => true)
    .catch(() => false);
  check("library: device pattern shows a preview thumbnail", hasThumb);
  // clicking it opens the editor and activates it on the device
  const seenReqs = [];
  page.on("request", (r) => {
    if (r.url().includes("/api/patterns") && r.method() === "DELETE") seenReqs.push(r.url());
  });
  await page.click('[data-role="device-pattern"]');
  await sleep(1300);
  check("library: opening a device pattern opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
  const activated = await (await fetch(`${DEV}/api/pattern`)).text();
  check("library: selecting a device pattern activates it", activated.includes("0.4"));
  check("library: editor shows the stored source", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.4"));
  // delete it from the editor
  const delBtn = await page.$('[data-role="delete"]');
  check("library: delete button present", delBtn !== null);
  await delBtn?.click();
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
    body: "export function render(index) { rgb(0.9, 0.8, 0.7) }",
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
  await page.waitForSelector(".device-badge", { timeout: 8000 });
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
  await page.click('[data-role="save"]'); // dialog handler accepts as "device kept"
  await sleep(1000);
  await fetch(`${DEV}/api/code`, {
    method: "POST",
    body: "export function render(index) { rgb(0.44, 0.55, 0.66) }",
  });
  await sleep(300);
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle0",
  });
  await page.waitForSelector(".cm-content");
  await page.waitForSelector(".device-badge", { timeout: 8000 });
  await sleep(1200);
  check(
    "resume(clean): opens the device's running pattern, not the saved editor",
    (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.44"),
  );
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

  // disconnect returns to the local wasm engine
  await page.$$eval("header button", (btns) => {
    btns.find((b) => b.textContent?.trim() === "disconnect")?.click();
  });
  await sleep(1200);
  const badge = await page.$(".device-badge");
  check("disconnect: badge gone", badge === null);
  // reconnecting needs no URL — a plain reconnect button (the base is known)
  const reconnect = await page.$('[data-role="reconnect"]');
  check("disconnect: reconnect button shown (no URL field)", reconnect !== null);
  check("disconnect: still no device-url field", (await page.$(".device-url")) === null);
  await reconnect?.click();
  await page.waitForSelector(".device-badge", { timeout: 8000 }).catch(() => null);
  check("reconnect: reconnects without a URL", (await page.$(".device-badge")) !== null);
} finally {
  await browser.close();
  device.kill();
  web.kill();
}

console.log(fails.length === 0 ? "\nall device-mode checks passed" : `\n${fails.length} FAILURES`);
process.exit(fails.length === 0 ? 0 : 1);
