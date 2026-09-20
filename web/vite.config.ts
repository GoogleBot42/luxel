import { svelte } from "@sveltejs/vite-plugin-svelte";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

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
  plugins: [svelte()],
  build: {
    target: "es2022",
    // The device serves this UI from a 3-socket connection pool (RAM
    // budget — see firmware/src/server.rs), and browser-native loads
    // can't go through the app's fetchgate. Splitting CSS per-entry and
    // modulepreloading the shared chunk put 4 native requests in flight
    // at HTML parse; the 4th got TCP-refused on every cold load (#92).
    // One CSS file + no preload caps the native burst at 2 sockets.
    cssCodeSplit: false,
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
