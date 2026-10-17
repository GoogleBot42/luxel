// Which files of dist/ go into the DEVICE asset archive (Gitea #884).
//
// dist/ is also the hosted site (GitHub Pages) and the luxel-web-dist
// tarball, and those keep everything. The device archive leaves out the
// HOSTED-ONLY entry pages and every chunk only they reach: the WLED→Luxel
// installer (flash.html) converts someone else's board and never needs a
// Luxel device to serve it, and it was 12,633 B of a ~983 KB partition.
//
// The chunk graph is read from the built files themselves — `assets/X.js`
// in an HTML entry's inlined loader, `./X.js` in a chunk's imports — so a
// chunk the console shares with an excluded page (assets/app-*.js today)
// stays in. Anything that is neither HTML nor JS (gallery.json, the wasm)
// is kept unconditionally.

import fs from "node:fs";
import path from "node:path";

/** Entry pages that ship on the hosted site but not on a device. */
export const HOSTED_ONLY = ["flash.html"];

function* walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) yield* walk(p);
    else yield p;
  }
}

/** dist-relative `assets/*.js` paths a file references. */
function refs(dist, rel) {
  const text = fs.readFileSync(path.join(dist, rel), "utf8");
  const out = new Set();
  for (const [, f] of text.matchAll(/assets\/([\w.-]+\.js)\b/g)) out.add(`assets/${f}`);
  if (rel.startsWith("assets/")) {
    for (const [, f] of text.matchAll(/["'`]\.\/([\w.-]+\.js)["'`]/g)) out.add(`assets/${f}`);
  }
  return out;
}

function reach(dist, roots) {
  const seen = new Set();
  const todo = [...roots];
  while (todo.length) {
    const f = todo.pop();
    if (seen.has(f)) continue;
    if (!fs.existsSync(path.join(dist, f))) {
      throw new Error(`device-files: ${f} is referenced but not in ${dist}/`);
    }
    seen.add(f);
    for (const r of refs(dist, f)) todo.push(r);
  }
  return seen;
}

/**
 * Split dist/ into what the device archive packs and what it leaves out.
 * Throws if a hosted-only page has gone missing (the list would be stale)
 * or a referenced chunk does not exist. A file reachable from a served
 * page is never dropped, by construction.
 * @returns {{ keep: string[], dropped: string[] }} dist-relative paths
 */
export function deviceFiles(dist) {
  const all = [...walk(dist)].map((f) => path.relative(dist, f).replaceAll("\\", "/"));
  for (const h of HOSTED_ONLY) {
    if (!all.includes(h)) throw new Error(`device-files: hosted-only page ${h} is not in ${dist}/`);
  }
  const served = all.filter((f) => f.endsWith(".html") && !HOSTED_ONLY.includes(f));
  const needed = reach(dist, served);
  const hosted = reach(dist, HOSTED_ONLY);
  const dropped = all.filter((f) => hosted.has(f) && !needed.has(f));
  // Belt and braces: an unreferenced chunk (a lazy import the regexes
  // missed) is KEPT rather than dropped, so the failure mode of a graph
  // this cannot read is a bigger archive, never a broken console.
  const keep = all.filter((f) => !dropped.includes(f));
  return { keep, dropped };
}
