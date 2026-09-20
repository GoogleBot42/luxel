// The emitted shape of `dist/*.html` is load-bearing, not cosmetic (Gitea #92,
// #592): the device serves this UI from a 3-socket pool, and a browser-NATIVE
// request (a `<script src>` tag, a `<link rel=stylesheet>`, a modulepreload)
// does NOT go through the app's `fetchgate`, so nothing retries it when the
// pool refuses it — a refused stylesheet rendered the console as raw unstyled
// HTML with nothing anywhere reporting a failure, and a refused bundle gives a
// blank page. So the build emits NEITHER: `inlineBoot()` in vite.config.ts
// inlines the one stylesheet and replaces the module tag with a loader that
// appends the script after parse (reusing the document's keep-alive socket)
// and retries it. A cold load's whole boot fits in one socket.
//
// This test is the free half of that invariant (the paid half is
// `tools/coldload.mjs` against real hardware). Run: `npm test` from web/,
// after a build — CI builds first. Skipped on a tree with no `dist/`.
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const dist = (f) => fileURLToPath(new URL(`../dist/${f}`, import.meta.url));
const built = existsSync(dist("index.html"));

/** Tags whose `src`/`href` makes the BROWSER open a connection of its own. */
function nativeRequests(html) {
  const out = [];
  for (const [, tag, attrs] of html.matchAll(/<(script|link)\b([^>]*)>/gi)) {
    const url = attrs.match(/\b(?:src|href)="([^"]*)"/i)?.[1];
    if (!url || url.startsWith("data:")) continue; // inline script / data: favicon
    out.push(`${tag.toLowerCase()} ${url}`);
  }
  return out;
}

for (const [page, chunk] of [
  ["index.html", "index"],
  ["flash.html", "flash"],
]) {
  test(`dist/${page} asks the browser for nothing`, { skip: !built }, () => {
    const html = readFileSync(dist(page), "utf8");
    assert.deepEqual(nativeRequests(html), [], "every subresource must be inline or loaded by JS");
    assert.doesNotMatch(html, /rel="?stylesheet/i, "stylesheet link must be inlined (#592)");
    assert.doesNotMatch(html, /modulepreload/i, "modulePreload must stay off (#92)");
  });

  test(`dist/${page} carries its own CSS`, { skip: !built }, () => {
    const html = readFileSync(dist(page), "utf8");
    const style = html.match(/<style>([\s\S]*?)<\/style>/);
    assert.ok(style, "no inlined <style>");
    assert.match(style[1], /--bg:\s*#14161a/, "inlined CSS is not app.css");
    assert.ok(style[1].length > 10000, `inlined CSS looks truncated (${style[1].length} bytes)`);
  });

  test(`dist/${page} loads its bundle after parse, with retries`, { skip: !built }, () => {
    const html = readFileSync(dist(page), "utf8");
    assert.match(html, /DOMContentLoaded/, "the loader must wait for the document (#592)");
    assert.match(
      html,
      new RegExp(`s\\.src = tries \\? "\\./assets/${chunk}-[\\w-]+\\.js"`),
      "the loader does not point at this page's bundle",
    );
    assert.match(html, /onerror[\s\S]{0,80}tries <= 3/, "no bounded retry — refusals are fatal");
  });
}

test("no CSS asset is emitted at all", { skip: !built }, () => {
  const assets = fileURLToPath(new URL("../dist/assets", import.meta.url));
  const css = existsSync(assets) ? readdirSync(assets).filter((f) => f.endsWith(".css")) : [];
  assert.deepEqual(css, [], "the inlined stylesheet must not also ship as a file");
});
