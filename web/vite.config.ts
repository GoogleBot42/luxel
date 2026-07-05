import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  // relative base so the built playground works from any static host or
  // (later) from device flash
  base: "./",
  plugins: [svelte()],
  build: { target: "es2022" },
});
