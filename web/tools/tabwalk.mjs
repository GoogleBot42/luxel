// Walk every console tab and every full-screen editor in real chromium and
// screenshot each, against a `luxel serve` mirror of a given board shape.
//
// Written for Gitea #691 (per-entry CSS): the bundle-shape harnesses check
// that a stylesheet is INLINE, and `mockdiff` checks computed values on the
// frames it maps, but neither walks every surface to see that its rules
// actually arrived. A CSS-splitting bug drops rules for ONE screen, which is
// exactly the screen nobody screenshots.
//
// It fails on an unstyled screen rather than just saving a picture: for each
// surface it asserts the shell's own tokens resolved (`--bg`), that the body
// is actually painted (not the UA white), and that a named element of that
// screen has a non-UA box — plus the usual "no page-level console.error".
//
// Usage (from web/, inside `nix develop`, after `npm run build`):
//   E2E_PORT=4795 node tools/tabwalk.mjs [shot-dir] [--board panel|strip]
import { execSync, spawn } from "node:child_process";
import fs from "node:fs";
import puppeteer from "puppeteer-core";

const SHOTS = process.argv[2]?.startsWith("--") ? "/tmp" : (process.argv[2] ?? "/tmp");
const BOARD = process.argv.includes("--board")
  ? process.argv[process.argv.indexOf("--board") + 1]
  : "panel";
const PORT = Number(process.env.E2E_PORT ?? 4795);
const DEV_PORT = Number(process.env.TABWALK_DEV_PORT ?? 8741);
const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

fs.mkdirSync(SHOTS, { recursive: true });
let failed = 0;
const ok = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) failed++;
};

execSync("cargo build -q -p luxel-cli", { cwd: "..", stdio: "inherit" });

const device = spawn(
  "../target/debug/luxel",
  ["serve", "--port", String(DEV_PORT), "--board", BOARD, "--name", `tabwalk-${BOARD}`],
  { stdio: ["ignore", "pipe", "inherit"] },
);
await new Promise((r) => device.stdout.on("data", (d) => String(d).includes("luxel serve:") && r()));

const web = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  stdio: ["ignore", "pipe", "inherit"],
});
await new Promise((r) => web.stdout.on("data", (d) => String(d).includes("Local:") && r()));

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  args: ["--no-sandbox", "--disable-gpu"],
});
const page = await browser.newPage();
await page.setViewport({ width: 1280, height: 900 });
const errors = [];
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
page.on("pageerror", (e) => errors.push(String(e)));

await page.goto(`http://localhost:${PORT}/?device=http://127.0.0.1:${DEV_PORT}`, {
  waitUntil: "networkidle0",
});
await page.waitForFunction(() => !document.querySelector("[data-role='boot']"), {
  timeout: 30000,
});

/** The shell tokens + a painted body: the #592 "silently unstyled" check. */
async function styled(label) {
  const v = await page.evaluate(() => {
    const cs = getComputedStyle(document.documentElement);
    return {
      bg: cs.getPropertyValue("--bg").trim(),
      body: getComputedStyle(document.body).backgroundColor,
    };
  });
  ok(`${label}: stylesheet applied`, v.bg !== "" && v.body !== "rgba(0, 0, 0, 0)", JSON.stringify(v));
}

/** A named element of THIS screen has real geometry and a real (non-UA) font. */
async function surface(label, selector) {
  await page.waitForSelector(selector, { visible: true, timeout: 20000 });
  const box = await page.$eval(selector, (el) => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return { w: r.width, h: r.height, font: cs.fontFamily, size: cs.fontSize };
  });
  ok(`${label}: laid out`, box.w > 0 && box.h > 0, `${Math.round(box.w)}x${Math.round(box.h)}`);
  ok(`${label}: styled type`, !/^Times/i.test(box.font), box.font.slice(0, 40));
  await styled(label);
  await page.screenshot({ path: `${SHOTS}/tabwalk-${BOARD}-${label}.png` });
}

const tab = async (name) => {
  await page.click(`[data-role="tab-${name}"]`);
  await new Promise((r) => setTimeout(r, 400));
};

console.log(`\n== tabwalk: board=${BOARD}, mirror :${DEV_PORT}, app :${PORT}\n`);

await surface("patterns", "[data-role='patterns-panel']");
for (const t of ["scenes", "sprites", "playlist", "settings"]) {
  const present = await page.$(`[data-role="tab-${t}"]`);
  if (!present) {
    console.log(` --   ${t}: tab not offered on this board (expected on a strip)`);
    continue;
  }
  await tab(t);
  await surface(t, `[data-role="${t}-panel"]`);
}

// --- the three full-screen editors, each reached the way a user reaches it ---
await tab("patterns");
await page.click("[data-role='new-pattern']").catch(() => {});
await new Promise((r) => setTimeout(r, 800));
if (await page.$("[data-role='editor-back']")) {
  await surface("pattern-editor", ".editor-frame");
  await page.click("[data-role='editor-back']");
  await new Promise((r) => setTimeout(r, 400));
} else {
  ok("pattern-editor: opened", false, "no editor-back after patterns-new");
}

// The two document editors open by CREATING and routing (there is no `new`
// route — see lib/router.ts), which is exactly what the tab's primary button
// does. On a strip the tabs are not offered at all, so neither is the editor.
for (const [label, kind, view] of [
  ["scene-editor", "scenes", "scene-editor-view"],
  ["sprite-editor", "sprites", "sprite-editor-view"],
]) {
  if (!(await page.$(`[data-role="tab-${kind}"]`))) {
    console.log(` --   ${label}: ${kind} tab not offered on this board`);
    continue;
  }
  await tab(kind);
  const create =
    (await page.$(`[data-role="new-${kind.slice(0, -1)}"]`)) ??
    (await page.$(`[data-role="new-${kind.slice(0, -1)}-empty"]`));
  if (!create) {
    ok(`${label}: opened`, false, "no create button on the tab");
    continue;
  }
  await create.click();
  await new Promise((r) => setTimeout(r, 1200));
  await surface(label, `[data-role='${view}']`);
  await page.click(`[data-role='${kind.slice(0, -1)}-editor-back']`).catch(() => {});
  await new Promise((r) => setTimeout(r, 500));
}

ok("no page-level console errors", errors.length === 0, errors.slice(0, 3).join(" | "));

await browser.close();
web.kill();
device.kill();
console.log(failed === 0 ? `\ntabwalk(${BOARD}): all checks passed` : `\ntabwalk(${BOARD}): ${failed} FAILED`);
process.exit(failed === 0 ? 0 : 1);
