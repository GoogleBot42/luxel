import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  // relative base so the built playground works from any static host or
  // (later) from device flash
  base: "./",
  plugins: [svelte()],
  build: { target: "es2022" },
  resolve: {
    // duplicate @codemirror/state instances silently break editing —
    // classic CM6-under-Vite failure; force a single copy
    dedupe: ["@codemirror/state", "@codemirror/view"],
  },
  optimizeDeps: {
    include: ["@codemirror/state", "@codemirror/view"],
  },
});
