import { svelte } from "@sveltejs/vite-plugin-svelte";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

// The loader that replaces the emitted `<script type=module src=…>` tag.
//
// A tag is fetched by the preload scanner WHILE the document is still being
// received, so the browser needs a SECOND socket for it — which is the socket
// a full device pool refuses (measured on the Seengreat panel with the pool at
// `"web":[1,1,1]`: the document landed and the bundle was refused, on every
// cold load). Appending the script after `DOMContentLoaded` instead means the
// document's own connection is idle and keep-alive, so the bundle rides that
// one socket and a whole cold load — document, bundle, wasm, every `/api/*` —
// fits in ONE. And because it is a script we append, we can retry it, which a
// native tag can never be (Gitea #592).
const bootLoader = (src: string) => `
      // Gitea #592 — see web/index.html. Appended after parse so it reuses the
      // document's socket rather than opening a second one the device may
      // refuse; re-appended up to 3 times (2s, 4s, 6s) if it is.
      (function () {
        var tries = 0;
        function boot() {
          var s = document.createElement("script");
          s.type = "module";
          s.crossOrigin = "anonymous";
          // cache-bust the retries only: a refused request caches nothing, but
          // a 404 from a half-written asset partition would
          s.src = tries ? ${JSON.stringify(src)} + "?r=" + Date.now() : ${JSON.stringify(src)};
          s.onerror = function () {
            if (++tries <= 3) setTimeout(boot, tries * 2000);
          };
          document.head.appendChild(s);
        }
        if (document.readyState === "loading") addEventListener("DOMContentLoaded", boot);
        else boot();
      })();`;

// Fold each entry HTML's OWN stylesheets INTO it and drop them from the
// output, and turn the module tag into the loader above, so a device cold load
// makes NO browser-native subresource request at all.
//
// Why (Gitea #592): a `<link rel=stylesheet>` is a BROWSER load, not one of
// the app's `fetchgate` calls, so nothing retries it. When the device's
// 3-socket web pool has no slot free the stylesheet comes back
// `ERR_CONNECTION_REFUSED` and the console renders as completely unstyled
// HTML — a silent failure, because everything the app fetches itself
// (`luxel.wasm`, `/api/*`) IS retried and lands, so the boot completes.
// #92 capped the native burst at 2 sockets, which fits a 3-socket pool only
// when the pool is empty; one long-lived console tab is enough to break it.
// The cost is the CSS's separate cache entry (it now rides in the HTML,
// which is `no-cache`-revalidated rather than immutable) and ~10 KB gzipped
// on every document load.
//
// PER ENTRY, not one global sheet (Gitea #691): `cssCodeSplit` is on, so
// rollup emits one sheet per entry plus one for the chunk they share, and
// each HTML carries only the links it actually needs. Before this, ONE
// stylesheet was inlined into BOTH pages, so the 82 KB console sheet rode in
// `flash.html` (and the installer's in `index.html`) — 12,794 B gzipped of
// the 983,040 B assets partition spent on CSS neither page can use. The #592
// invariant is untouched: the sheets are still INLINE, so the emitted
// `<script>`/`<link>` set of every `dist/*.html` is unchanged at zero
// browser-native requests (`tests/bundleShape.test.mjs` asserts both halves,
// and now also that `flash.html` does not carry the console's CSS).
function inlineBoot(): Plugin {
  return {
    name: "luxel-inline-boot",
    // after vite:build-html has injected the <script>/<link> tags, so the
    // link we rewrite is the emitted one
    enforce: "post",
    apply: "build",
    generateBundle(_options, bundle) {
      const sheets = Object.values(bundle).filter(
        (c): c is Extract<typeof c, { type: "asset" }> =>
          c.type === "asset" && c.fileName.endsWith(".css"),
      );
      if (sheets.length === 0) return;
      for (const sheet of sheets) {
        // A literal `</style` inside the CSS would end the element early. It
        // cannot happen with the rules we author, so fail the build loudly
        // instead of shipping a half-parsed page.
        if (/<\/style/i.test(String(sheet.source))) {
          throw new Error(
            `luxel-inline-boot: ${sheet.fileName} contains \`</style\`, refusing to inline`,
          );
        }
      }
      const tag = /\s*<script type="module"[^>]*src="([^"]+)"[^>]*><\/script>/g;
      /** Sheets that some entry HTML actually linked. Anything left over would
       *  be deleted below without its rules ever reaching a page, which is the
       *  silent-unstyled-console failure #592 is about — so it is an error. */
      const used = new Set<string>();
      let inlined = 0;
      for (const chunk of Object.values(bundle)) {
        if (chunk.type !== "asset" || !chunk.fileName.endsWith(".html")) continue;
        let html = String(chunk.source);
        // Replace each link IN PLACE so the cascade keeps rollup's order
        // (the shared chunk's sheet before the entry's own).
        let hit = 0;
        for (const sheet of sheets) {
          const link = new RegExp(
            `\\s*<link rel="stylesheet"[^>]*href="[^"]*${sheet.fileName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"[^>]*>`,
            "g",
          );
          if (!link.test(html)) continue;
          link.lastIndex = 0;
          html = html.replace(link, `\n    <style>${String(sheet.source)}</style>`);
          used.add(sheet.fileName);
          hit++;
        }
        if (hit === 0) continue;
        const scripts = html.match(tag) ?? [];
        tag.lastIndex = 0;
        if (scripts.length !== 1) {
          throw new Error(
            `luxel-inline-boot: ${chunk.fileName} has ${scripts.length} module scripts, expected 1`,
          );
        }
        chunk.source = html.replace(
          tag,
          (_m, src: string) => `\n    <script>${bootLoader(src)}\n    </script>`,
        );
        inlined++;
      }
      if (inlined === 0) {
        throw new Error("luxel-inline-boot: no entry HTML referenced a stylesheet");
      }
      const orphans = sheets.filter((s) => !used.has(s.fileName)).map((s) => s.fileName);
      if (orphans.length > 0) {
        throw new Error(
          `luxel-inline-boot: no entry HTML linked ${orphans.join(", ")} — its rules would be dropped`,
        );
      }
      // drop the now-unreferenced assets so they are neither written nor packed
      for (const sheet of sheets) delete bundle[sheet.fileName];
    },
  };
}

// The scraped-corpus gallery is a LOCAL dev artefact: `tools/gen-gallery.mjs`
// (which `npm run build`/`npm run dev` runs first) writes
// public/pixelblaze-library.json only when `corpus/` is populated, and deletes
// it otherwise — so a release build, and therefore every device asset bundle,
// is without it. Its presence is a BUILD-time fact and the app must not spend
// a request asking (a HEAD is still a request): on hardware the answer is
// always 404, twice per cold load, against a 3-socket pool (Gitea #564).
// `App.svelte` ANDs this with "no device", so even a console served from a
// bundle packed out of a corpus-carrying tree never asks.
const hasPixelblazeLibrary = fs.existsSync(
  fileURLToPath(new URL("public/pixelblaze-library.json", import.meta.url)),
);

export default defineConfig({
  define: {
    __HAS_PIXELBLAZE_LIBRARY__: JSON.stringify(hasPixelblazeLibrary),
  },
  // relative base so the built playground works from any static host or
  // (later) from device flash
  base: "./",
  plugins: [svelte(), inlineBoot()],
  build: {
    target: "es2022",
    // esbuild (the default) is fast; terser is ~4 % smaller gzipped on this
    // bundle, and the bundle is gated at the 983,040 B assets partition
    // (Gitea #683), so the seconds are worth it. NOT `drop_console`: the e2e
    // harness fails a run on any page-level `console.error` (web/tools/e2e.mjs),
    // so dropping them would disable that assertion rather than shrink much.
    minify: "terser",
    terserOptions: { compress: { passes: 2 }, format: { comments: false } },
    // The device serves this UI from a 3-socket connection pool (RAM
    // budget — see firmware/src/server.rs), and browser-native loads
    // can't go through the app's fetchgate. Splitting CSS per-entry and
    // modulepreloading the shared chunk put 4 native requests in flight
    // at HTML parse; the 4th got TCP-refused on every cold load (#92).
    // What fixed that was `inlineBoot()` below taking the native burst to
    // ZERO — the sheets go INLINE and the module tag becomes a post-parse
    // loader (#592) — and an inline sheet costs no request whether there
    // is one of them or three. So per-entry CSS is back on (Gitea #691):
    // it emits one sheet per entry plus one shared, each page inlines only
    // the ones it links, and `flash.html` stops carrying the console's
    // 82 KB stylesheet — 12,794 B gzipped off the assets partition.
    // `modulePreload` stays OFF: a modulepreload IS a native request.
    cssCodeSplit: true,
    modulePreload: false,
    rollupOptions: {
      // two pages: the playground/console app and the WLED→Luxel installer
      input: { index: "index.html", flash: "flash.html" },
    },
  },
  resolve: {
    // duplicate @codemirror/state instances silently break editing —
    // classic CM6-under-Vite failure; force a single copy
    dedupe: ["@codemirror/state", "@codemirror/view"],
  },
  optimizeDeps: {
    include: ["@codemirror/state", "@codemirror/view"],
  },
});
