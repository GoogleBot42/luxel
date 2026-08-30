// Drive the WLED→Luxel installer page (flash.html) in real chromium against
// the fake-wled fixture (tools/fake-wled.mjs) — no hardware involved.
//
// Scenarios:
//   1. bundled mode + CORS-less WLED 0.13 (the common worst case):
//      opaque probe → manual board pick → auto flash → reboot →
//      Luxel detected → assets push → done
//   2. WLED 0.14+ (CORS on): arch is read and the board list filters
//   3. esp8266: hard stop; 3b: an ESP32 chip with release images but no
//      takeover support (S3) stops too, pointing at the downloads
//   4. github mode (api.github.com mocked via request interception):
//      binaries via file picker instead of same-origin fetch
//
// Usage (from web/): npm run build && node tools/flash-e2e.mjs [shot-dir]
// A firmware fixture (fake bins + manifest) is composed into dist/firmware
// and removed afterwards.

import { execSync, spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import puppeteer from "puppeteer-core";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();
const shotDir = process.argv[2] ?? "/tmp";
const PORT = Number(process.env.E2E_PORT ?? 4183);
const WLED_PORT = PORT + 100;
const VERSION = "9.9.9";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const fails = [];
const check = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) fails.push(name);
};

// ── firmware fixture inside dist/ (vite preview serves dist verbatim) ──
const fwDir = "dist/firmware";
fs.rmSync(fwDir, { recursive: true, force: true });
fs.mkdirSync(fwDir, { recursive: true });
const BIN = `luxel-esp32-generic-${VERSION}-ota.bin`;
const LUXA = `luxel-web-assets-${VERSION}.luxa`;
const binBytes = 200_000, luxaBytes = 50_000;
fs.writeFileSync(path.join(fwDir, BIN), Buffer.alloc(binBytes, 0xe9));
fs.writeFileSync(path.join(fwDir, `luxel-athom-music-${VERSION}-ota.bin`), Buffer.alloc(1000, 0xe9));
fs.writeFileSync(path.join(fwDir, `luxel-c3-devkit-${VERSION}-ota.bin`), Buffer.alloc(1000, 0xe9));
fs.writeFileSync(path.join(fwDir, LUXA), Buffer.alloc(luxaBytes, 0x4c));
execSync(`node tools/gen-flash-manifest.mjs ${VERSION} ${fwDir}`, { stdio: "inherit" });

const server = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  stdio: "ignore",
});

let wled = null;
let wledLog = "";
function startWled(env) {
  wled?.kill();
  wledLog = "";
  wled = spawn("node", ["tools/fake-wled.mjs"], {
    env: { ...process.env, FAKE_WLED_PORT: String(WLED_PORT), FAKE_WLED_REBOOT_MS: "3000", ...env },
    stdio: ["ignore", "pipe", "inherit"],
  });
  wled.stdout.on("data", (d) => (wledLog += d.toString()));
  return sleep(400);
}

await sleep(1500); // vite preview startup

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  headless: true,
  args: ["--no-sandbox", "--disable-gpu", "--window-size=1200,1400"],
});

async function openPage({ blockManifest = false, mockGithub = false } = {}) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1200, height: 1400 });
  page.on("pageerror", (e) => check("no page error", false, String(e)));
  if (blockManifest || mockGithub) {
    await page.setRequestInterception(true);
    page.on("request", (req) => {
      const u = req.url();
      if (blockManifest && u.includes("/firmware/manifest.json")) return void req.respond({ status: 404, body: "" });
      if (mockGithub && u.startsWith("https://api.github.com/")) {
        return void req.respond({
          status: 200,
          contentType: "application/json",
          headers: { "access-control-allow-origin": "*" },
          body: JSON.stringify({
            tag_name: `v${VERSION}`,
            html_url: "https://github.com/GoogleBot42/luxel/releases",
            assets: [
              { name: BIN, size: binBytes, browser_download_url: `http://localhost:${PORT}/firmware/${BIN}` },
              { name: LUXA, size: luxaBytes, browser_download_url: `http://localhost:${PORT}/firmware/${LUXA}` },
            ],
          }),
        });
      }
      return void req.continue();
    });
  }
  await page.goto(`http://localhost:${PORT}/flash.html`, { waitUntil: "networkidle0" });
  return page;
}

async function probeAt(page, addr) {
  await page.click('[data-role="ip-input"]', { clickCount: 3 });
  await page.type('[data-role="ip-input"]', addr);
  await page.click('[data-role="probe-btn"]');
  await page.waitForSelector('[data-role="probe-result"]', { timeout: 15000 });
}

try {
  // ── scenario 1: bundled + CORS-less WLED → full automatic run ──
  await startWled({ FAKE_WLED_CORS: "0", FAKE_WLED_ARCH: "esp32" });
  let page = await openPage();
  check(
    "S1 bundled source detected",
    await page.$eval('[data-role="fw-source"]', (e) => e.textContent.includes("bundled with this page")),
  );
  await probeAt(page, `localhost:${WLED_PORT}`);
  check(
    "S1 opaque probe → reachable-but-unreadable",
    await page.$eval('[data-role="probe-result"]', (e) => e.textContent.includes("Something answered")),
  );
  await page.select('[data-role="board-select"]', "esp32-generic");
  await page.screenshot({ path: `${shotDir}/flash-e2e-1-ready.png` });
  await page.click('[data-role="flash-btn"]');
  await page.waitForSelector('[data-role="wait-status"]', { timeout: 15000 });
  await page.screenshot({ path: `${shotDir}/flash-e2e-2-waiting.png` });
  await page.waitForSelector('[data-role="takeover-ok"]', { timeout: 60000 });
  check("S1 fake device got the image", wledLog.includes("/update received"), wledLog.trim());
  const gotBytes = Number(wledLog.match(/\/update received (\d+)/)?.[1] ?? 0);
  check("S1 uploaded ≥ image size (multipart framing adds a bit)", gotBytes >= binBytes, String(gotBytes));
  check(
    "S1 takeover detected",
    await page.$eval('[data-role="takeover-ok"]', (e) => e.textContent.includes("Luxel v9.9.9")),
  );
  await page.click('[data-role="assets-btn"]');
  await page.waitForSelector('[data-role="done-link"]', { timeout: 20000 });
  check("S1 assets landed", wledLog.includes(`/api/assets received`), wledLog.trim());
  const luxaGot = Number(wledLog.match(/\/api\/assets received (\d+)/)?.[1] ?? 0);
  check("S1 luxa byte-exact", luxaGot === luxaBytes, String(luxaGot));
  await page.screenshot({ path: `${shotDir}/flash-e2e-3-done.png` });
  await page.close();

  // ── scenario 2: WLED 0.14 CORS → arch read, board list filtered ──
  await startWled({ FAKE_WLED_CORS: "1", FAKE_WLED_ARCH: "esp32-c3" });
  page = await openPage();
  await probeAt(page, `localhost:${WLED_PORT}`);
  check(
    "S2 arch read from /json/info",
    await page.$eval('[data-role="probe-result"]', (e) => e.textContent.includes("esp32-c3")),
  );
  const opts = await page.$$eval('[data-role="board-select"] option:not([disabled])', (o) =>
    o.map((x) => x.value),
  );
  check("S2 board list filtered to c3", opts.length === 1 && opts[0] === "c3-devkit", opts.join(","));
  await page.close();

  // ── scenario 3: esp8266 → hard stop ──
  await startWled({ FAKE_WLED_CORS: "1", FAKE_WLED_ARCH: "esp8266" });
  page = await openPage();
  await probeAt(page, `localhost:${WLED_PORT}`);
  await page.waitForSelector('[data-role="arch-stop"]', { timeout: 5000 });
  check(
    "S3 esp8266 stops with an explanation",
    await page.$eval('[data-role="arch-stop"]', (e) => e.textContent.includes("ESP8266")),
  );
  check("S3 no flash section offered", (await page.$('[data-role="flash-btn"]')) === null);
  await page.screenshot({ path: `${shotDir}/flash-e2e-4-esp8266.png` });
  await page.close();

  // ── scenario 3b: an ESP32 chip releases build for but the takeover can't ──
  await startWled({ FAKE_WLED_CORS: "1", FAKE_WLED_ARCH: "esp32-s3" });
  page = await openPage();
  await probeAt(page, `localhost:${WLED_PORT}`);
  await page.waitForSelector('[data-role="arch-stop"]', { timeout: 5000 });
  const s3Stop = await page.$eval('[data-role="arch-stop"]', (e) => e.textContent);
  check(
    "S3b unflashable ESP32 chip points at the release downloads",
    s3Stop.includes("esp32s3") && /untested on real hardware/.test(s3Stop),
    s3Stop.replace(/\s+/g, " ").trim(),
  );
  check("S3b no flash section offered", (await page.$('[data-role="flash-btn"]')) === null);
  await page.screenshot({ path: `${shotDir}/flash-e2e-4b-esp32s3.png` });
  await page.close();

  // ── scenario 4: github mode → file-picker path ──
  await startWled({ FAKE_WLED_CORS: "0", FAKE_WLED_ARCH: "esp32" });
  page = await openPage({ blockManifest: true, mockGithub: true });
  check(
    "S4 github source detected",
    await page.$eval('[data-role="fw-source"]', (e) => e.textContent.includes("from GitHub releases")),
  );
  await probeAt(page, `localhost:${WLED_PORT}`);
  await page.select('[data-role="board-select"]', "esp32-generic");
  await page.waitForSelector('[data-role="bin-file"]', { timeout: 5000 });
  const fileInput = await page.$('[data-role="bin-file"]');
  await fileInput.uploadFile(path.join(fwDir, BIN));
  await page.screenshot({ path: `${shotDir}/flash-e2e-5-github-mode.png` });
  await page.click('[data-role="flash-btn"]');
  await page.waitForSelector('[data-role="takeover-ok"]', { timeout: 60000 });
  check("S4 file-picker flash reached the device", wledLog.includes("/update received"));
  const luxaInput = await page.$('[data-role="luxa-file"]');
  check("S4 luxa file picker present in github mode", luxaInput !== null);
  await luxaInput.uploadFile(path.join(fwDir, LUXA));
  await page.click('[data-role="assets-btn"]');
  await page.waitForSelector('[data-role="done-link"]', { timeout: 20000 });
  check("S4 assets landed via file picker", wledLog.includes("/api/assets received"));
  await page.close();
} finally {
  await browser.close();
  server.kill();
  wled?.kill();
  fs.rmSync(fwDir, { recursive: true, force: true });
}

if (fails.length) {
  console.error(`\n${fails.length} FAILED: ${fails.join(", ")}`);
  process.exit(1);
}
console.log("\nflash-e2e: all checks passed");
