// Drive the built playground in real chromium (nix dev shell) via
// puppeteer-core: verify typing, live recompile, control readouts, //#
// hints, layout editing, and take screenshots for human review.
//
// Usage (from web/): npm run build && node tools/e2e.mjs [screenshot-dir]

import { execSync, spawn } from "node:child_process";
import puppeteer from "puppeteer-core";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

const shotDir = process.argv[2] ?? "/tmp";
const PORT = 4179;

const server = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  stdio: "ignore",
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
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

try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  const pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") pageErrors.push(m.text());
  });

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle0" });
  await page.waitForSelector(".cm-content");
  await sleep(800); // wasm load + first frames

  // 1. rendering: fps counter alive, waterfall canvas non-black
  const fpsText = await page.$eval("header .mono", (el) => el.textContent ?? "");
  check("engine renders (fps > 0)", parseInt(fpsText) > 10, fpsText.trim());
  const lit = await page.$eval(".waterfall", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, 3).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("waterfall shows pixels", lit);
  await page.screenshot({ path: `${shotDir}/e2e-1-rainbow.png` });

  // 2. typing works (THE bug): click editor, type garbage, expect the text
  //    to land and a compile-error banner to appear while pixels keep going.
  //    `~` is an unterminated token (no bracket auto-close, no completion
  //    popup to interfere with the later fix).
  await page.click(".cm-content");
  await page.keyboard.press("End");
  await page.keyboard.type(" @@@");
  await sleep(300);
  const doc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("editor accepts typing", doc.includes("@@@"));
  await page.waitForSelector(".banner.error", { timeout: 3000 }).catch(() => null);
  const banner = await page.$(".banner.error");
  check("compile error banner appears", banner !== null);
  await page.waitForSelector(".cm-lintRange-error", { timeout: 2000 }).catch(() => null);
  check("error squiggle rendered", (await page.$(".cm-lintRange-error")) !== null);
  // fix it by undoing the typed garbage — restores the exact original
  // 3-line rainbow (the debugger test below needs line 3 = hsv). Robust
  // against auto-closed brackets / completion state.
  await page.keyboard.press("Escape"); // dismiss any completion popup
  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(500);
  const fixedDoc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("banner clears after fix", (await page.$(".banner.error")) === null && !fixedDoc.includes("@@@"));
  await page.screenshot({ path: `${shotDir}/e2e-2-typing.png` });

  // 2.5 debugger: gutter breakpoint on the hsv line pauses per pixel
  const lineRect = await page.$$eval(".cm-line", (els) => {
    const r = els[2].getBoundingClientRect(); // line 3 = hsv(...)
    return { x: r.x, y: r.y, h: r.height };
  });
  const gutterRect = await page.$eval(".cm-bp-gutter", (el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, w: r.width };
  });
  await page.mouse.click(gutterRect.x + gutterRect.w / 2, lineRect.y + lineRect.h / 2);
  await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).catch(() => null);
  const paused = await page.$('.debugger[data-paused="true"]');
  check("breakpoint pauses execution", paused !== null);
  const status = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("paused at hsv line, pixel 0", status.includes("line 3") && status.includes("pixel 0"), status.trim());
  const stackTxt = await page.$eval(".stack", (el) => el.textContent ?? "").catch(() => "");
  check("stack shows render + index local", stackTxt.includes("render") && stackTxt.includes("index"), "");
  check("current line highlighted", (await page.$(".cm-debug-line")) !== null);
  await page.screenshot({ path: `${shotDir}/e2e-debugger.png` });

  // hover an identifier → value tooltip
  const wordRect = await page.evaluate(() => {
    for (const lineEl of document.querySelectorAll(".cm-line")) {
      const walker = document.createTreeWalker(lineEl, NodeFilter.SHOW_TEXT);
      let node;
      while ((node = walker.nextNode())) {
        const i = node.textContent.indexOf("index");
        if (i >= 0) {
          const r = document.createRange();
          r.setStart(node, i);
          r.setEnd(node, i + 5);
          const b = r.getBoundingClientRect();
          return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
        }
      }
    }
    return null;
  });
  check("found hover target", wordRect !== null);
  if (wordRect) {
    await page.mouse.move(wordRect.x, wordRect.y);
    await sleep(500);
    const tip = await page.$eval(".cm-hover-value", (el) => el.textContent ?? "").catch(() => "");
    check("hover shows variable value", tip.includes("index = 0"), tip);
    await page.mouse.move(5, 5); // dismiss
    await sleep(200);
  }

  // step lands on the next pixel's first line
  await page.click(".db-over");
  await sleep(150);
  const status2 = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("step flows to next pixel", status2.includes("pixel 1"), status2.trim());
  // continue hits the breakpoint again on pixel 2
  await page.click(".db-continue");
  await sleep(150);
  const status3 = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("continue re-arms breakpoint", status3.includes("pixel 2"), status3.trim());

  // KITT: implicit globals (v, leds, pos) appear in the globals pane —
  // the exact scenario from review feedback
  await page.select("header select", "KITT");
  await sleep(700);
  const kittLine = await page.$$eval(".cm-line", (els) => {
    const el = els[11]; // line 12: rgb(...) — right after v = leds[index]
    const r = el.getBoundingClientRect();
    return { y: r.y, h: r.height };
  });
  await page.mouse.click(gutterRect.x + gutterRect.w / 2, kittLine.y + kittLine.h / 2);
  await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).catch(() => null);
  const kittGlobals = await page
    .$eval('[data-role="globals"]', (el) => el.textContent ?? "")
    .catch(() => "");
  check(
    "globals pane shows implicit globals (v, leds, pos)",
    kittGlobals.includes("v") && kittGlobals.includes("leds") && kittGlobals.includes("pos"),
    kittGlobals.slice(0, 60),
  );
  await page.screenshot({ path: `${shotDir}/e2e-debugger-globals.png` });

  // clean up: disable debug (example switch already cleared old breakpoints)
  await page.click(".debug-toggle");
  await sleep(400);
  check("debug off resumes rendering", (await page.$(".debugger")) === null);

  // 3. Blink Fade: slider + number entry + readout reactivity
  await page.select("header select", "Blink Fade");
  await sleep(600);
  await page.waitForSelector('input[type="range"]');
  const before = await page.$eval(".control .num", (el) => el.value);
  await page.$eval('input[type="range"]', (el) => {
    el.value = "0.9";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  const after = await page.$eval(".control .num", (el) => el.value);
  check("slider moves the numeric readout", before !== after && Number(after) === 0.9, `${before} → ${after}`);
  // manual number entry drives the engine too
  await page.$eval(".control .num", (el) => {
    el.value = "0.25";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(200);
  const rangeNow = await page.$eval('input[type="range"]', (el) => el.value);
  check("number entry moves the slider", Number(rangeNow) === 0.25, rangeNow);

  // 4. hints: Spinning Plasma slider bounded by //# min/max
  await page.select("header select", "Spinning Plasma 2D");
  await sleep(700);
  const [mn, mx, val] = await page.$eval('input[type="range"]', (el) => [el.min, el.max, el.value]);
  check("//# hint bounds the slider", mn === "0.1" && mx === "1.5", `min=${mn} max=${mx}`);
  check("//# default applied", Number(val) === 0.45, val);
  const gridLit = await page.$eval(".grid", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("2D grid preview renders", gridLit);
  await page.screenshot({ path: `${shotDir}/e2e-3-plasma.png` });

  // 5. layout editing: bump grid to 24×24 and keep rendering
  await page.$eval("header .group .num", (el) => {
    el.value = "24";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(500);
  const gridW = await page.$eval(".grid", (c) => c.width);
  check("layout edit resizes the render", gridW === 24, String(gridW));

  // 6. pause stops the fps counter from advancing frames
  await page.click("header button[title]");
  await sleep(300);
  const litBefore = await page.$eval(".grid", (c) => c.getContext("2d").getImageData(0, 0, 8, 8).data.join());
  await sleep(400);
  const litAfter = await page.$eval(".grid", (c) => c.getContext("2d").getImageData(0, 0, 8, 8).data.join());
  check("pause freezes the preview", litBefore === litAfter);
  await page.click("header button[title]");

  // 7. vars watcher shows exported values
  const varText = await page.$eval("table", (el) => el.textContent ?? "").catch(() => "");
  check("var watcher lists zoom", varText.includes("zoom"));

  await page.screenshot({ path: `${shotDir}/e2e-4-final.png` });

  check("no page errors", pageErrors.length === 0, pageErrors.slice(0, 3).join(" | "));
} finally {
  await browser.close();
  server.kill();
}

if (fails.length > 0) {
  console.error(`\n${fails.length} FAILURES: ${fails.join(", ")}`);
  process.exit(1);
}
console.log("\ne2e: all checks pass");
