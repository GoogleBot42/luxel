// Per-board pixel cap e2e (Gitea #74): the playground's pixel-count control
// must clamp to the CONNECTED BOARD's cap — `/api/status`'s `max_pixels` —
// not to a hardcoded 2048. Checks both halves: the real mirror (a strip
// device, 2048) and an intercepted status body impersonating a 64x64 HUB75
// panel board (4096). The impersonation is the only way to exercise the
// panel cap until the Seengreat board is on the bench (#75).
//
// Usage (from web/, after `npm run build`):
//   node tools/maxpixels-e2e.mjs [screenshot-dir]
import { execSync, spawn } from "node:child_process";
import puppeteer from "puppeteer-core";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();
const PORT = Number(process.env.E2E_PORT ?? 4195);
const DEV_PORT = 8733;
const DEV = `http://127.0.0.1:${DEV_PORT}`;
const shotDir = process.argv[2] ?? "/tmp";
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
  setTimeout(() => reject(new Error("mirror start timeout")), 30000);
});
const web = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  stdio: ["ignore", "ignore", "inherit"],
});
process.on("exit", () => {
  device.kill();
  web.kill();
});
// vite preview takes a variable moment to bind — poll instead of guessing.
for (let i = 0; i < 60; i++) {
  try {
    await fetch(`http://localhost:${PORT}/`);
    break;
  } catch {
    await sleep(500);
  }
}

// 1. The mirror really emits the new field.
const raw = await (await fetch(`${DEV}/api/status`)).json();
console.log(" status →", JSON.stringify(raw));
if (raw.max_pixels !== 2048) throw new Error(`mirror max_pixels = ${raw.max_pixels}`);
console.log(" ok  mirror /api/status carries max_pixels");

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  args: ["--no-sandbox", "--disable-gpu"],
});

/** Open the playground against the mirror, optionally rewriting the status
 *  body to impersonate a HUB75 panel board, and read back the Pixels input. */
async function run(fakeCap, tag) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  if (fakeCap) {
    // Impersonate a board-seengreat-hub75 device: same mirror, but its
    // status reports the panel cap. This is the only way to drive the
    // 4096 path without the physical panel (Gitea #75).
    await page.setRequestInterception(true);
    page.on("request", async (req) => {
      if (!req.url().includes("/api/status")) return req.continue();
      const res = await fetch(req.url());
      const body = await res.json();
      body.max_pixels = fakeCap;
      req.respond({
        status: 200,
        contentType: "application/json",
        headers: { "access-control-allow-origin": "*" },
        body: JSON.stringify(body),
      });
    });
  }
  await page.goto(`http://localhost:${PORT}/?device=${encodeURIComponent(DEV)}`, {
    waitUntil: "networkidle2",
  });
  await sleep(2500);
  // Real clicks: connecting drops straight into the editor, so back out to
  // the home screen first, then open the Settings tab where Pixels lives.
  const back = await page.$("button.back, [data-role='back']");
  if (back) await back.click();
  else {
    for (const b of await page.$$("button")) {
      const txt = await b.evaluate((e) => e.textContent?.trim());
      if (txt?.includes("Device Patterns")) {
        await b.click();
        break;
      }
    }
  }
  await sleep(800);
  await page.click('[data-role="tab-settings"]');
  await sleep(1200);
  const got = await page.$eval('[data-role="cfg-pixels"]', (e) => ({
    max: e.getAttribute("max"),
    value: e.value,
  }));
  const note = await page.$eval('[data-role="cfg-pixels"]', (e) =>
    e.parentElement?.querySelector("span.dim")?.textContent?.trim(),
  );
  await page.$eval('[data-role="cfg-pixels"]', (e) =>
    e.scrollIntoView({ block: "center" }),
  );
  await sleep(400);
  await page.screenshot({ path: `${shotDir}/maxpixels-${tag}.png` });
  await page.close();
  return { ...got, note };
}

const strip = await run(null, "strip-2048");
console.log(" strip board →", JSON.stringify(strip));
if (strip.max !== "2048") throw new Error(`expected max 2048, got ${strip.max}`);
console.log(" ok  strip device: pixel control clamps to 2048");

const panel = await run(4096, "panel-4096");
console.log(" panel board →", JSON.stringify(panel));
if (panel.max !== "4096") throw new Error(`expected max 4096, got ${panel.max}`);
if (!panel.note?.includes("4096")) throw new Error(`hint text stale: ${panel.note}`);
console.log(" ok  panel device: pixel control clamps to 4096 (per-board cap honored)");

await browser.close();
console.log("all max-pixels checks passed");
process.exit(0);
