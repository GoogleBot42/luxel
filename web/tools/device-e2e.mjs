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
  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle0" });
  await page.waitForSelector(".cm-content");
  await sleep(500);

  // connect to the device mirror
  await page.type(".device-url", DEV);
  await page.click(".device-url + button");
  await page.waitForSelector(".device-badge", { timeout: 5000 });
  check("connect: badge shown", true);

  const px = await page.$eval("header input.num", (el) => el.value);
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
    .waitForFunction(() => document.body.innerText.includes("ws push"), { timeout: 10000 })
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

  // ---- device pattern library (CRUD against the mirror) ----
  // restore a valid pattern in the editor first (the error test left junk)
  await setEditor(
    page,
    "export function render(index) { hsv(index / pixelCount, 1, 0.4) }",
  );
  await sleep(900);
  page.on("dialog", (d) => {
    if (d.message().includes("save pattern")) return void d.accept("device kept");
    if (d.message().includes("delete")) return void d.accept();
    void d.dismiss();
  });
  await page.click('[data-role="save"]');
  await sleep(900);
  const devOpt = await page.$$eval("header select option", (els) =>
    els.some((o) => o.value.startsWith("device:") && o.textContent === "device kept"),
  );
  check("library: save-to-device adds an 'on device' entry", devOpt);
  const apiList = await (await fetch(`${DEV}/api/patterns`)).json();
  check(
    "library: device API lists it",
    apiList.patterns?.some((p) => p.name === "device kept"),
    JSON.stringify(apiList),
  );
  // switch to an example, then load the stored pattern back — it activates
  await page.select("header select", "Rainbow");
  await sleep(900);
  const devValue = await page.$$eval(
    "header select option",
    (els) => els.find((o) => o.value.startsWith("device:"))?.value ?? "",
  );
  await page.select("header select", devValue);
  await sleep(1200);
  const activated = await (await fetch(`${DEV}/api/pattern`)).text();
  check("library: selecting a device pattern activates it", activated.includes("0.4"));
  const editorNow = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("library: editor shows the stored source", editorNow.includes("0.4"));
  // delete it from the device
  const seenReqs = [];
  page.on("request", (r) => {
    if (r.url().includes("/api/patterns") && r.method() === "DELETE") seenReqs.push(r.url());
  });
  const delBtn = await page.$('[data-role="delete"]');
  check("library: delete button present after loading device pattern", delBtn !== null);
  await delBtn?.click();
  await sleep(900);
  check("library: DELETE request was sent", seenReqs.length > 0, seenReqs.join(","));
  const apiAfter = await (await fetch(`${DEV}/api/patterns`)).json();
  const note = await page
    .$eval('[data-role="save-note"]', (el) => el.textContent ?? "")
    .catch(() => "(no note)");
  check(
    "library: delete removes it on the device",
    (apiAfter.patterns ?? []).length === 0,
    `note=${note} api=${JSON.stringify(apiAfter)}`,
  );

  // disconnect returns to the local wasm engine
  await page.$$eval("header button", (btns) => {
    btns.find((b) => b.textContent?.trim() === "disconnect")?.click();
  });
  await sleep(1200);
  const badge = await page.$(".device-badge");
  check("disconnect: badge gone", badge === null);
} finally {
  await browser.close();
  device.kill();
  web.kill();
}

console.log(fails.length === 0 ? "\nall device-mode checks passed" : `\n${fails.length} FAILURES`);
process.exit(fails.length === 0 ? 0 : 1);
