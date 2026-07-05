// End-to-end test of the device live-code page against `luxel serve` (the
// native mirror of the firmware server). Run from the repo root:
//   node tools/serve-e2e.mjs
// Requires the flake devshell (chromium) and web/node_modules (puppeteer-core).

import { execSync, spawn } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url + "/../../web/");
const puppeteer = require("puppeteer-core");

const PORT = 8721;
let failures = 0;
function check(name, ok, extra = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!ok) failures++;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- start the server ----
execSync("cargo build -q -p luxel-cli", { stdio: "inherit" });
const server = spawn("target/debug/luxel", ["serve", "--port", String(PORT), "--pixels", "120"], {
  stdio: ["ignore", "pipe", "inherit"],
});
process.on("exit", () => server.kill());
await new Promise((resolve, reject) => {
  server.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
  server.on("exit", () => reject(new Error("server died")));
  setTimeout(() => reject(new Error("server start timeout")), 30000);
});

// ---- API-level checks (mirror what the page does) ----
const base = `http://127.0.0.1:${PORT}`;
await sleep(1200); // let fps settle
const status = await (await fetch(`${base}/api/status`)).json();
check("status: pixels", status.pixels === 120, JSON.stringify(status));
check("status: fps > 0", status.fps > 0, `fps=${status.fps}`);
check("status: no vmerr", status.vmerr === null);

const px1 = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check("pixels: 3 bytes per pixel", px1.length === 360, `len=${px1.length}`);
check("pixels: not all black", px1.some((b) => b > 0));

const bad = await (await fetch(`${base}/api/code`, { method: "POST", body: "export function render(index) { hsv(" })).json();
check("code: syntax error rejected", bad.ok === false && bad.line >= 1, JSON.stringify(bad));

const good = await (await fetch(`${base}/api/code`, {
  method: "POST",
  body: "export function render(index) { rgb(0, 0, 1) }",
})).json();
check("code: upload accepted", good.ok === true, JSON.stringify(good));
await sleep(300);
const px2 = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check("code: pattern actually swapped (all blue)", px2.length === 360 && px2[0] === 0 && px2[1] === 0 && px2[2] === 255, `first px = ${px2[0]},${px2[1]},${px2[2]}`);

const vmerrSrc = "export var arr = array(4)\nexport function render(index) { arr[9] = 1\nhsv(0,0,0) }";
const ve = await (await fetch(`${base}/api/code`, { method: "POST", body: vmerrSrc })).json();
check("code: vmerr pattern accepted (compiles)", ve.ok === true);
await sleep(400);
const st2 = await (await fetch(`${base}/api/status`)).json();
check("status: vmerr surfaced with location", typeof st2.vmerr === "string" && st2.vmerr.includes("line 2"), String(st2.vmerr));

// restore a lit pattern (the vmerr pattern above correctly renders black:
// render aborts before hsv) so the preview check sees light
await fetch(`${base}/api/code`, {
  method: "POST",
  body: "export function render(index) { hsv(index / pixelCount, 1, 1) }",
});
await sleep(300);

// ---- browser-level checks ----
const executablePath = execSync("command -v chromium").toString().trim();
const browser = await puppeteer.launch({
  executablePath,
  args: ["--no-sandbox", "--headless=new"],
});
try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1100, height: 800 });
  await page.goto(base, { waitUntil: "networkidle2" });

  check("page: title", (await page.title()) === "Luxel");
  await sleep(1500);
  const statusText = await page.$eval("#status", (el) => el.textContent);
  check("page: status line shows fps + px", /\d+ fps · 120 px/.test(statusText), statusText);

  // preview canvases resized to the strip and painting non-black pixels
  const preview = await page.evaluate(() => {
    const strip = document.getElementById("strip");
    const wf = document.getElementById("waterfall");
    const d = strip.getContext("2d").getImageData(0, 0, strip.width, 1).data;
    let lit = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 0) lit++;
    return { stripW: strip.width, wfW: wf.width, lit };
  });
  check("page: strip canvas sized to pixel count", preview.stripW === 120, JSON.stringify(preview));
  check("page: preview shows lit pixels", preview.lit > 100, `lit=${preview.lit}`);

  // type a pattern and run it via the button
  await page.evaluate(() => {
    document.getElementById("src").value = "export function render(index) { rgb(1, 0, 0) }";
  });
  await page.click("#run");
  await page.waitForFunction(
    () => document.getElementById("result").textContent.includes("running"),
    { timeout: 5000 },
  );
  check("page: run reports success", true);
  await sleep(400);
  const red = await page.evaluate(() => {
    const strip = document.getElementById("strip");
    const d = strip.getContext("2d").getImageData(0, 0, 1, 1).data;
    return [d[0], d[1], d[2]];
  });
  check("page: preview went red after live-code", red[0] === 255 && red[1] === 0 && red[2] === 0, red.join(","));

  // compile error path
  await page.evaluate(() => {
    document.getElementById("src").value = "export function render(index) { hsv( }";
  });
  await page.click("#run");
  await page.waitForFunction(
    () => document.getElementById("result").className === "err",
    { timeout: 5000 },
  );
  const errText = await page.$eval("#result", (el) => el.textContent);
  check("page: compile error shown with line:col", /line \d+:\d+/.test(errText), errText);

  await page.screenshot({ path: "web/e2e-serve.png" });
} finally {
  await browser.close();
  server.kill();
}

console.log(failures === 0 ? "\nall checks passed" : `\n${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
