// Cold-load soak against a real 2-slot device: N fresh-profile chromium
// launches, cache disabled, counting refused/failed network requests and
// requiring the full device-mode boot (the Patterns page, on the On device
// source, with the running pattern's tile lit and a live device session)
// every time. Usage: node coldload.mjs <device-url> [N]
import puppeteer from "puppeteer-core";
import { execSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// same resolution as e2e.mjs — puppeteer needs an absolute path, a bare
// "chromium" fails even with chromium on PATH
const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

// trailing slashes stripped: DEV + "/" with a slashed arg requested "//",
// a 404, and every load read as boot-FAILED (cost a confused run 2026-08-15)
const DEV = (process.argv[2] ?? "http://192.168.0.183").replace(/\/+$/, "");
const N = Number(process.argv[3] ?? 10);
const SHOT_DIR = process.argv[4] ?? "/tmp";

let cleanLoads = 0;
const allFailures = [];

for (let i = 1; i <= N; i++) {
  const profile = mkdtempSync(join(tmpdir(), "coldload-"));
  const browser = await puppeteer.launch({
    executablePath: CHROMIUM,
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
    // A console boots on the PATTERNS page since #538 — it no longer opens the
    // editor full-screen on the running pattern, so waiting for `editor-back`
    // /`.cm-content` could only ever time out (it did, on every load, against
    // a perfectly healthy board on 2026-09-20). The boot this tool means is
    // now: the Patterns page up, on the On device source, with the running
    // pattern's tile lit — i.e. `/api/patterns` AND `/api/playlist` have both
    // landed and been rendered.
    //
    // The signal that the whole handshake landed is still the shell's fps
    // readout, but it is now in that readout's TITLE, not its text: a console
    // prints the device's own rate as a bare `123 fps` (App.svelte's
    // `fpsReadout`), and only the title says where the number came from —
    // "local preview loop in this browser tab" with no session, "rendered by
    // the device" / "displayed by the panel" with one. The device chip is NOT
    // that signal — it appears as soon as the probe finds a base, i.e. before
    // `/api/layout` answers, while the layout still names the 60 px default
    // strip (a panel read there reports a strip).
    await page.waitForSelector('[data-role="patterns-grid"][data-source="device"]:not([hidden])', {
      timeout: 30000,
    });
    await page.waitForSelector('[data-role="tile-playing"]', { timeout: 30000 });
    await page.waitForFunction(
      () =>
        /rendered by the device|displayed by the panel/.test(
          document.querySelector('[data-role="fps"]')?.getAttribute("title") ?? "",
        ),
      { timeout: 30000 },
    );
    const chip = await page.$eval('[data-role="layout-chip"]', (el) =>
      (el.textContent ?? "").replace(/\s+/g, " ").trim(),
    );
    const tiles = await page.$$eval(
      '[data-role="patterns-grid"][data-source="device"] [data-role="tile"]',
      (els) => els.length,
    );
    ok = tiles > 0;
    detail = `chip="${chip}", ${tiles} on-device tile(s), playing tile lit`;
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
