import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  // relative base so the built playground works from any static host or
  // (later) from device flash
  base: "./",
  plugins: [svelte()],
  build: {
    target: "es2022",
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
