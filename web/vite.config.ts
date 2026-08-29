import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
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
