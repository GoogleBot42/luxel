// The #592 boot loader, checked against `vite preview` (no device needed):
// refuse the module bundle's request the way a full device socket pool does
// and see the page load it anyway.
//
// The built page has NO `<script src>` tag — `inlineBoot()` (vite.config.ts)
// emits a loader that appends the script after `DOMContentLoaded` (so it rides
// the document's keep-alive socket instead of opening a second one) and
// re-appends it up to 3 times with backoff when it is refused. A native tag
// could do neither.
//
// Run from web/ after a build:
//   E2E_PORT=9300 node tools/bootretry-check.mjs           # refuse once
//   E2E_PORT=9300 REFUSE=all node tools/bootretry-check.mjs # loop guard
import puppeteer from "puppeteer-core";
import { execSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PORT } from "./e2e-common.mjs";

const PREVIEW = PORT.web.bootRetry;
const ALL = process.env.REFUSE === "all";
const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

const preview = spawn("npx", ["vite", "preview", "--port", String(PREVIEW), "--strictPort"], {
  stdio: "ignore",
});
await new Promise((r) => setTimeout(r, 2500));

const profile = mkdtempSync(join(tmpdir(), "bootretry-"));
const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  args: ["--no-sandbox", "--disable-gpu"],
  userDataDir: profile,
});
const page = await browser.newPage();
await page.setRequestInterception(true);
const t0 = Date.now();
const attempts = [];
page.on("request", (r) => {
  const u = r.url();
  if (/\/assets\/index-[\w-]+\.js/.test(u)) {
    attempts.push(Date.now() - t0);
    if (attempts.length === 1 || ALL) {
      console.log(`  refusing attempt ${attempts.length} at ${Date.now() - t0} ms`);
      return r.abort("connectionrefused");
    }
  }
  r.continue();
});

await page.goto(`http://localhost:${PREVIEW}/`, { waitUntil: "domcontentloaded" });
let ok = false;
try {
  await page.waitForSelector('[data-role="patterns-grid"]', { timeout: 20000 });
  ok = true;
} catch (e) {
  if (!ALL) console.log(`  ${String(e).split("\n")[0]}`);
}
if (ALL) {
  await new Promise((r) => setTimeout(r, 10000)); // let a would-be loop show itself
  const bounded = attempts.length === 4; // the first try plus three retries
  console.log(
    `loop guard: ${attempts.length} attempt(s) at ${attempts.join(", ")} ms — ` +
      (bounded ? "bounded" : "WRONG (expected 4: one try + three retries)"),
  );
  await finish(bounded && !ok);
}

const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
console.log(
  `boot-retry: app ${ok ? "came back" : "DID NOT come back"} after ${Date.now() - t0} ms; ` +
    `${attempts.length} bundle attempt(s) at ${attempts.join(", ")} ms; body bg ${bg}; url ${page
      .url()
      .replace(/^https?:\/\/[^/]+/, "")}`,
);
await finish(ok && attempts.length === 2 && bg === "rgb(20, 22, 26)");

async function finish(pass) {
  await browser.close();
  rmSync(profile, { recursive: true, force: true });
  preview.kill();
  process.exit(pass ? 0 : 1);
}
