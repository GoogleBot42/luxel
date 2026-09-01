// The browser-blocked device state, in real chromium from a real https origin
// (Gitea #162).
//
// Usage (from web/): npm run build && node tools/lna-e2e.mjs [screenshot-dir]
//
// Why this harness exists rather than a case inside device-e2e.mjs: the whole
// behaviour is keyed on the PAGE being https. `vite preview` serves http, and
// `location.protocol` is [Unforgeable] — you cannot fake it from the page. So
// this serves the built app twice, over TLS (throwaway self-signed cert) and
// over plain http, and drives the same URL against both.
//
// What it can and cannot prove. Headless chromium DENIES Local Network Access
// outright — no prompt, and a CDP `localNetworkAccess` grant did not help in
// Chromium 150 (measured 2026-08-15, re-confirmed against hardware 2026-08-30).
// So the SUCCESS path — https page → LAN device with the permission granted —
// is not reachable here and stays a headful-browser task. What is reachable,
// and what this asserts, is everything around it:
//   1. from https, a device that can't be reached reports the browser-blocked
//      state with its manual routes, not a generic "cannot reach device";
//   2. from plain http, the identical failure reports the GENERIC error — the
//      same condition that gates the `targetAddressSpace` hint gates the UI,
//      so neither fires where it would be wrong;
//   3. the real, un-mocked https→LAN request from headless chromium fails, and
//      the harness prints the browser's own errorText for it (that string is
//      the evidence of what the policy actually did).
// Checks 1 and 2 abort the device request at the browser (page interception)
// so the two origins fail identically and fast; check 3 mocks nothing.

import { execFileSync, execSync } from "node:child_process";
import { createServer as createHttpServer } from "node:http";
import { createServer as createHttpsServer } from "node:https";
import { mkdtempSync, readFileSync, existsSync } from "node:fs";
import { networkInterfaces, tmpdir } from "node:os";
import { join, normalize } from "node:path";
import puppeteer from "puppeteer-core";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();
const shotDir = process.argv[2] ?? "/tmp";
const HTTPS_PORT = Number(process.env.LNA_HTTPS_PORT ?? 4185);
const HTTP_PORT = Number(process.env.LNA_HTTP_PORT ?? 4186);
// A private-space address nothing answers on: local-space enough for the hint
// to apply, dead enough to fail fast. This machine's own RFC1918 address on a
// closed high port refuses instantly — an unassigned LAN address instead hangs
// on ARP for minutes (a slow harness, not a better one), and a LOW port gets
// rejected as net::ERR_UNSAFE_PORT before the network policy is ever consulted.
// Never a real device: this harness touches no hardware.
const DEVICE = process.env.LNA_DEVICE ?? `http://${privateIpv4()}:9997`;

function privateIpv4() {
  for (const ifaces of Object.values(networkInterfaces())) {
    for (const i of ifaces ?? []) {
      if (i.family !== "IPv4" || i.internal) continue;
      const [a, b] = i.address.split(".").map(Number);
      if (a === 10 || (a === 192 && b === 168) || (a === 172 && b >= 16 && b <= 31))
        return i.address;
    }
  }
  console.error("no RFC1918 address on this host — set LNA_DEVICE to a dead LAN address");
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const fails = [];
const check = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) fails.push(name);
};

// ---- throwaway TLS cert (openssl is in the dev shell) ----
const certDir = mkdtempSync(join(tmpdir(), "luxel-lna-"));
execFileSync("openssl", [
  "req", "-x509", "-newkey", "rsa:2048", "-sha256", "-days", "1", "-nodes",
  "-keyout", join(certDir, "key.pem"), "-out", join(certDir, "cert.pem"),
  "-subj", "/CN=localhost",
  "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1",
], { stdio: ["ignore", "ignore", "inherit"] });

// ---- static server for web/dist, one instance per scheme ----
const MIME = {
  ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".json": "application/json", ".wasm": "application/wasm", ".svg": "image/svg+xml",
};
const ROOT = new URL("../dist/", import.meta.url).pathname;
if (!existsSync(join(ROOT, "index.html"))) {
  console.error(`no built app at ${ROOT} — run \`npm run build\` first`);
  process.exit(1);
}
function serve(req, res) {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
  const file = join(ROOT, path.endsWith("/") ? `${path}index.html` : path);
  if (!file.startsWith(ROOT) || !existsSync(file)) {
    res.writeHead(404).end("not found");
    return;
  }
  const ext = file.slice(file.lastIndexOf("."));
  res.writeHead(200, { "content-type": MIME[ext] ?? "application/octet-stream" });
  res.end(readFileSync(file));
}
const httpsServer = createHttpsServer(
  { key: readFileSync(join(certDir, "key.pem")), cert: readFileSync(join(certDir, "cert.pem")) },
  serve,
).listen(HTTPS_PORT);
const httpServer = createHttpServer(serve).listen(HTTP_PORT);

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  headless: true,
  args: [
    "--no-sandbox",
    "--disable-gpu",
    "--ignore-certificate-errors",
    "--window-size=1400,900",
    // Deliberately NOT here, measured 2026-08-31: launching with
    //   --enable-features=LocalNetworkAccessChecks
    //   --ip-address-space-overrides=127.0.0.1:<port>=public
    // (the flags that ought to relabel this loopback origin as public-space —
    // what GitHub Pages is — and turn the checks on) changed nothing: every
    // variant of the A/B below still reached the socket and came back
    // ERR_CONNECTION_REFUSED. Chromium 150 headless does not run the policy
    // here, so don't add them back expecting it to.
  ],
});

/** Load the app pointed at DEVICE from `origin`. `blockDevice` aborts every
 *  request to the device at the browser, so both origins see the identical
 *  failure and neither waits on a real network timeout. */
async function open(origin, { blockDevice }) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  const failures = [];
  const verdicts = new Map(); // url → the browser's own errorText
  page.on("requestfailed", (r) => {
    if (!r.url().startsWith(DEVICE)) return;
    failures.push(r.failure()?.errorText ?? "?");
    verdicts.set(r.url(), r.failure()?.errorText ?? "?");
  });
  if (blockDevice) {
    await page.setRequestInterception(true);
    page.on("request", (r) => {
      if (r.url().startsWith(DEVICE)) void r.abort("accessdenied");
      else void r.continue();
    });
  }
  await page.goto(`${origin}/?device=${encodeURIComponent(DEVICE)}`, {
    waitUntil: "networkidle2",
    timeout: 90_000,
  });
  // the connect handshake retries with backoff before it gives up (fetchgate)
  await page.waitForFunction(() => !document.querySelector('[data-role="boot"]'), { timeout: 90_000 })
    .catch(() => {});
  await sleep(2000);
  return { page, failures, verdicts };
}

const text = (page, sel) =>
  page.$eval(sel, (el) => el.innerText.replace(/\s+/g, " ").trim()).catch(() => null);

try {
  // 1. https origin: the browser-blocked state, with the manual routes.
  {
    const { page } = await open(`https://localhost:${HTTPS_PORT}`, { blockDevice: true });
    const banner = await text(page, '[data-role="device-blocked"]');
    check("https origin shows the browser-blocked banner", banner !== null);
    check(
      "banner names the https/http mismatch and Local Network Access",
      banner !== null && /https/.test(banner) && /Local Network Access/i.test(banner),
      banner ?? "",
    );
    check(
      "banner offers the device-served console as the manual route",
      banner !== null && /open the console from the device/i.test(banner),
    );
    const href = await page
      .$eval('[data-role="device-blocked-link"]', (a) => a.getAttribute("href"))
      .catch(() => null);
    check("manual route links at the device itself", href === DEVICE, String(href));
    const generic = await text(page, ".banner.error");
    check(
      "no generic 'cannot reach device' error alongside it",
      generic === null || !/cannot reach device/i.test(generic),
      generic ?? "",
    );
    await page.screenshot({ path: `${shotDir}/lna-blocked-https.png` });
    await page.close();
  }

  // 2. plain-http origin: identical failure, GENERIC message — the state is
  //    conditional on the origin, exactly like the hint is.
  {
    const { page } = await open(`http://localhost:${HTTP_PORT}`, { blockDevice: true });
    const banner = await text(page, '[data-role="device-blocked"]');
    check("http origin does NOT claim the browser blocked it", banner === null, banner ?? "");
    const offline = await text(page, '[data-role="device-offline"]');
    const generic = await text(page, ".banner.error");
    check(
      "http origin reports the plain unreachable error",
      /cannot reach device/i.test(`${offline ?? ""} ${generic ?? ""}`),
      `${offline ?? ""} | ${generic ?? ""}`,
    );
    await page.screenshot({ path: `${shotDir}/lna-blocked-http.png` });
    await page.close();
  }

  // 3. the real thing, nothing mocked: https → a private-space http address.
  //    Then, from that same page, an A/B of the hint itself against the
  //    browser's own errorText — the closest a headless run can get to the
  //    granted path. Each variant carries a distinct query so the requestfailed
  //    events can be told apart; from JS all three are the same TypeError.
  {
    const { page, failures, verdicts } = await open(`https://localhost:${HTTPS_PORT}`, {
      blockDevice: false,
    });
    const banner = await text(page, '[data-role="device-blocked"]');
    check("un-mocked https→LAN request also lands in the blocked state", banner !== null);
    console.log(
      `  the app's own request to ${DEVICE}: ${failures.length ? [...new Set(failures)].join(", ") : "(no requestfailed event)"}`,
    );
    await page.screenshot({ path: `${shotDir}/lna-blocked-real.png` });

    await page.evaluate(async (base) => {
      const one = async (probe, init) => {
        try {
          await fetch(`${base}/api/status?probe=${probe}`, init);
        } catch {
          /* every variant fails; WHY is only visible from CDP */
        }
      };
      await one("none", {});
      await one("local", { targetAddressSpace: "local" });
      await one("public", { targetAddressSpace: "public" });
    }, DEVICE);
    await sleep(1500);
    const verdict = (p) => verdicts.get(`${DEVICE}/api/status?probe=${p}`) ?? "(none)";
    console.log(
      `  hint A/B from https → ${DEVICE}: none=${verdict("none")} local=${verdict("local")} public=${verdict("public")}`,
    );
    // The hinted request must not fare WORSE than the unhinted one: a hint
    // that mismatches the target's real address space hard-fails (measured on
    // the live site), so this is the guard against ever shipping such a hint.
    // Read it for what it is — when all three verdicts are identical, this
    // browser did not run the policy at all, and the check only proves the
    // hint is inert-safe. The granted path stays a headful task (#162).
    check(
      'targetAddressSpace: "local" does not make a local-space request fail sooner',
      verdict("local") === verdict("none"),
      `none=${verdict("none")} local=${verdict("local")}`,
    );
    if (verdict("none") === verdict("local") && verdict("local") === verdict("public"))
      console.log("  (identical verdicts — this browser is not enforcing the policy)");
    await page.close();
  }
} finally {
  await browser.close();
  httpsServer.close();
  httpServer.close();
}

console.log(fails.length ? `\n${fails.length} FAILED: ${fails.join(", ")}` : "\nall checks passed");
process.exit(fails.length ? 1 : 0);
