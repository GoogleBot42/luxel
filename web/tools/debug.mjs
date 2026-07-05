// One-off diagnostics for the typing + controls bugs.
import { execSync, spawn } from "node:child_process";
import puppeteer from "puppeteer-core";

const CHROMIUM = execSync("command -v chromium", { encoding: "utf8" }).trim();
const server = spawn("npx", ["vite", "preview", "--port", "4181", "--strictPort"], {
  stdio: "ignore",
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await sleep(1500);

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  headless: true,
  args: ["--no-sandbox", "--disable-gpu"],
});
const page = await browser.newPage();
page.on("console", (m) => console.log("console:", m.type(), m.text()));
page.on("pageerror", (e) => console.log("PAGEERROR:", String(e)));
await page.goto("http://localhost:4181/", { waitUntil: "networkidle0" });
await page.waitForSelector(".cm-content");
await sleep(800);

// instrument key events at capture phase on window and on the editor
await page.evaluate(() => {
  window.__log = [];
  window.addEventListener("keydown", (e) => window.__log.push(`win-key:${e.key}:target=${e.target.className}`), true);
  const content = document.querySelector(".cm-content");
  content.addEventListener("keydown", (e) => window.__log.push(`cm-key:${e.key}:defaultPrevented=${e.defaultPrevented}`), true);
  content.addEventListener("beforeinput", (e) => window.__log.push(`beforeinput:${e.inputType}:${e.data}`), true);
  content.addEventListener("input", (e) => window.__log.push(`input`), true);
});

await page.click(".cm-content");
const info1 = await page.evaluate(() => ({
  active: document.activeElement?.className,
  editable: document.querySelector(".cm-content")?.getAttribute("contenteditable"),
}));
console.log("after click:", JSON.stringify(info1));

await page.keyboard.type("Q");
await sleep(300);
const info2 = await page.evaluate(() => ({
  log: window.__log,
  head: (document.querySelector(".cm-content")?.textContent ?? "").slice(0, 60),
}));
console.log("after typing:", JSON.stringify(info2, null, 1));

// controls: instrument the slider handler path
await page.select("header select", "Blink Fade");
await sleep(700);
await page.evaluate(() => {
  window.__log = [];
  const r = document.querySelector('input[type="range"]');
  r.addEventListener("input", () => window.__log.push(`range-input value=${r.value}`), true);
});
await page.$eval('input[type="range"]', (el) => {
  el.value = "0.9";
  el.dispatchEvent(new Event("input", { bubbles: true }));
});
await sleep(300);
const info3 = await page.evaluate(() => ({
  log: window.__log,
  num: document.querySelector(".control .num")?.value,
  range: document.querySelector('input[type="range"]')?.value,
}));
console.log("after slider poke:", JSON.stringify(info3, null, 1));

await browser.close();
server.kill();
