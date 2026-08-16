// Cold-load soak against a real 2-slot device: N fresh-profile chromium
// launches, cache disabled, counting refused/failed network requests and
// requiring the full device-mode boot (device tab + editor with the
// running pattern) every time. Usage: node coldload.mjs <device-url> [N]
import puppeteer from "puppeteer-core";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const DEV = process.argv[2] ?? "http://192.168.0.183";
const N = Number(process.argv[3] ?? 10);
const SHOT_DIR = process.argv[4] ?? "/tmp";

let cleanLoads = 0;
const allFailures = [];

for (let i = 1; i <= N; i++) {
  const profile = mkdtempSync(join(tmpdir(), "coldload-"));
  const browser = await puppeteer.launch({
    executablePath: process.env.CHROMIUM ?? "chromium",
    args: ["--no-sandbox", "--disable-gpu", ...(process.env.NO_PRECONNECT ? ["--disable-features=NetworkPrediction,PreconnectToOrigin,LoadingPredictorPrefetch"] : [])],
    userDataDir: profile,
  });
  const page = await browser.newPage();
  const cdp = await page.createCDPSession();
  await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });

  const failures = [];
  const t0 = Date.now();
  const trace = [];
  const short = (u) => u.replace(/^https?:\/\/[^/]+/, "");
  page.on("request", (r) => trace.push(`${Date.now() - t0}ms  >  ${short(r.url())}`));
  page.on("requestfinished", (r) =>
    trace.push(`${Date.now() - t0}ms  ok ${short(r.url())} (${r.response()?.status()})`),
  );
  page.on("requestfailed", (r) => {
    const err = r.failure()?.errorText ?? "?";
    trace.push(`${Date.now() - t0}ms  XX ${short(r.url())} ${err}`);
    if (err === "net::ERR_ABORTED") return; // deliberate aborts (status probe timeout)
    failures.push(`${err} ${r.url()}`);
  });
  const pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));

  let ok = false;
  let detail = "";
  try {
    await page.goto(DEV + "/", { waitUntil: "domcontentloaded", timeout: 30000 });
    // Device-mode boot opens the editor FULL-SCREEN on the running pattern —
    // the tab bar is not in the DOM then. The editor's back button labeled
    // "Device Patterns" is the device-mode signal.
    await page.waitForSelector('[data-role="editor-back"]', { timeout: 30000 });
    await page.waitForSelector(".cm-content", { timeout: 30000 });
    const back = await page.$eval('[data-role="editor-back"]', (el) => el.textContent ?? "");
    const src = await page.$eval(".cm-content", (el) => el.textContent ?? "");
    ok = src.trim().length > 0 && back.includes("Device");
    detail = `back="${back.trim()}", editor has ${src.trim().length} chars`;
  } catch (e) {
    detail = String(e).split("\n")[0];
  }
  const ms = Date.now() - t0;

  const clean = ok && failures.length === 0 && pageErrors.length === 0;
  if (clean) cleanLoads++;
  const wall = new Date().toTimeString().slice(0, 8);
  console.log(
    `[${wall}] load ${i}/${N}: ${clean ? "CLEAN" : "DIRTY"} (${ms} ms, boot ${ok ? "ok" : "FAILED"}, ` +
      `${failures.length} failed reqs, ${pageErrors.length} page errors) ${detail}`,
  );
  for (const e of pageErrors) console.log(`   pageerror: ${e}`);
  if (!clean || process.env.TRACE) for (const l of trace) console.log(`   ${l}`);
  allFailures.push(...failures);

  if (i === 1 || i === N) {
    try {
      await page.screenshot({ path: join(SHOT_DIR, `coldload-${i}.png`) });
    } catch {
      /* a failed navigation can leave no screenshotable target */
    }
  }
  await browser.close();
  rmSync(profile, { recursive: true, force: true });
  // Let the device finish tearing down this browser's connections (slot
  // reclaim runs ~7 s after our sockets vanish) — back-to-back launches
  // otherwise measure the previous iteration's teardown, not a cold load.
  if (i < N) await new Promise((r) => setTimeout(r, 12000));
}

console.log(`\n${cleanLoads}/${N} clean cold loads; ${allFailures.length} failed requests total`);
process.exit(cleanLoads === N ? 0 : 1);
