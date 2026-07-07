// Device-mode e2e: the built playground in real chromium, connected to
// `luxel serve` (the native mirror of the firmware API). Verifies connect,
// editor sync from the device, live-code push, preview streaming, controls,
// vars, compile errors, and disconnect.
//
// Usage (from web/): npm run build && node tools/device-e2e.mjs

import { execSync, spawn } from "node:child_process";
import dgram from "node:dgram";
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
  check("device: no share button", (await page.$('[data-role="share"]')) === null);
  check("device: no reconnect button", (await page.$('[data-role="reconnect"]')) === null);
  const hasDisconnect = await page.$$eval("header button", (btns) =>
    btns.some((b) => /disconnect/i.test(b.textContent ?? "")),
  );
  check("device: no disconnect button", hasDisconnect === false);

  const px = await page.$eval('[data-role="cfg-pixels"]', (el) => el.value);
  check("connect: pixel count from device", px === "120", `got ${px}`);

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

  // device map upload (Phase 4): compute a 2D map and install it on the device
  await page.select('[data-role="layout-kind"]', "map");
  await sleep(500);
  await page.click('[data-role="map-install"]');
  await sleep(500);
  const mapGet = await (await fetch(`${DEV}/api/map`)).json();
  check(
    "map: install uploads the computed map to the device",
    mapGet.installed === true && mapGet.count > 0,
    JSON.stringify(mapGet),
  );
  // a render2D pattern now uses the installed geometry
  await fetch(`${DEV}/api/code`, {
    method: "POST",
    body: "export function render2D(index, x, y) { rgb(x, y, 0) }",
  });
  await sleep(500);
  const mpx = new Uint8Array(await (await fetch(`${DEV}/api/pixels`)).arrayBuffer());
  let varied = false;
  for (let i = 0; i < mpx.length; i += 3) if (mpx[i] !== mpx[0] || mpx[i + 1] !== mpx[1]) varied = true;
  check("map: render2D uses the geometry (pixels vary by x/y)", varied);
  await page.click('[data-role="map-clear"]');
  await sleep(400);
  check(
    "map: clear removes it from the device",
    (await (await fetch(`${DEV}/api/map`)).json()).installed === false,
  );
  await page.select('[data-role="layout-kind"]', "strip");
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

  // LED protocol (Phase 3): the Settings dropdown switches the driver live
  const p0 = await (await fetch(`${DEV}/api/protocol`)).json();
  check(
    "protocol: GET returns current + options",
    p0.protocol === "sk9822" && p0.options.includes("ws2812"),
    JSON.stringify(p0),
  );
  await page.select('[data-role="cfg-protocol"]', "ws2812");
  await sleep(500);
  const p1 = await (await fetch(`${DEV}/api/protocol`)).json();
  check("protocol: dropdown switches the device", p1.protocol === "ws2812", JSON.stringify(p1));
  const cfgP = await (await fetch(`${DEV}/api/config`)).json();
  check("protocol: config GET reflects it", cfgP.protocol === "ws2812", JSON.stringify(cfgP));
  await page.select('[data-role="cfg-protocol"]', "sk9822");
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
  const banner = await page.$eval(".banner.error", (el) => el.textContent ?? "").catch(() => "");
  check("errors: local diagnostics shown", /line \d+:\d+/.test(banner), banner.trim());
  const still = await fetch(`${DEV}/api/pattern`);
  check("errors: broken pattern not pushed (device keeps old)", (await still.text()).includes("sliderBlue"));

  // ---- device pattern library (CRUD via the Device Patterns tab) ----
  // restore a valid pattern in the editor first (the error test left junk)
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 0.4) }");
  await sleep(900);
  page.on("dialog", (d) => {
    if (d.message().includes("save pattern")) return void d.accept("device kept");
    if (d.message().includes("delete")) return void d.accept();
    if (d.message().includes("clear")) return void d.accept();
    if (d.message().includes("WiFi")) return void d.accept();
    void d.dismiss();
  });

  // WiFi settings form (the panel is in the DOM even while the editor is open)
  const w0 = await (await fetch(`${DEV}/api/wifi`)).json();
  check("wifi: GET returns {ssid,source}", "source" in w0, JSON.stringify(w0));
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
  await sleep(500);
  check(
    "wifi: save stores the SSID on the device",
    (await (await fetch(`${DEV}/api/wifi`)).json()).ssid === "TestNet",
  );

  // MQTT settings: the Settings form stores the broker config (no broker is
  // running here, so it stays "not connected" — the announce/command contract
  // is covered by luxel-core::hamqtt unit tests + a live mosquitto check)
  {
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

  // sensor injection: POST /api/sensors takes a raw sensor-board frame
  // ("SB1.0\0"…"END\0", the serial wire format) and feeds exported sensor
  // vars — the render path a physical PB sensor board will use
  {
    await fetch(`${DEV}/api/code`, {
      method: "POST",
      body:
        "export var energyAverage\nexport var frequencyData\n" +
        "export function render(index) { hsv(0, 1, energyAverage) }",
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
      await new Promise((r) => udp.send(ddp, 4048, "127.0.0.1", r));
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
    check("netin: settings shows a status row", (await page.$('[data-role="netin-status"]')) !== null);
    await sleep(2700); // live timeout
    const stIdle = await (await fetch(`${DEV}/api/status`)).json();
    check("netin: pattern resumes after timeout", stIdle.live === null, JSON.stringify(stIdle));
  }

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
  await page.click('[data-role="save"]'); // dialog handler saves as "device kept"
  await sleep(900);
  const addBtn = await page.$('[data-role="add-to-playlist"]');
  check("playlist: add-to-playlist appears for a saved pattern", addBtn !== null);
  // first add with hue=0.2
  await page.$eval('input[type="range"]', (el) => {
    el.value = "0.2";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  await addBtn?.click();
  await sleep(300);
  // second add with hue=0.8 (same pattern, different params)
  await page.$eval('input[type="range"]', (el) => {
    el.value = "0.8";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  await addBtn?.click();
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
  // per-item override on the first row
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
  // total run-time summary (item0 override 2s + item1 default 5s = 7s)
  const total = await page.$eval('[data-role="pl-total"]', (el) => el.textContent ?? "");
  check("playlist: total run-time shown", /2 items/.test(total) && /7s/.test(total), total.trim());
  // clear empties the playlist
  await page.click('[data-role="pl-clear"]'); // dialog handler accepts
  await sleep(500);
  check(
    "playlist: clear empties it",
    (await (await fetch(`${DEV}/api/playlist`)).json()).items.length === 0,
  );

  // ---- "untitled" fix: a running pattern that matches a saved one shows its
  // name (the device streams only source, not which library entry it is) ----
  await fetch(`${DEV}/api/playlist/stop`, { method: "POST" });
  const uniq = "export function render(index) { rgb(0.13, 0.26, 0.39) }";
  await fetch(`${DEV}/api/code`, { method: "POST", body: uniq }); // run it
  await fetch(`${DEV}/api/patterns`, { method: "POST", body: `Named Thing\n${uniq}` }); // save same source
  await sleep(400);
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle0",
  });
  await page.waitForSelector(".cm-content");
  await sleep(1800); // connect + device pattern sources stream in, then match
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
} finally {
  await browser.close();
  device.kill();
  web.kill();
}

console.log(fails.length === 0 ? "\nall device-mode checks passed" : `\n${fails.length} FAILURES`);
process.exit(fails.length === 0 ? 0 : 1);
