import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The site is served from a project page, so every asset URL carries the
// repository name. Deep links only work because S-02 pre-renders each route to
// real HTML — GitHub Pages has no server to fall back to.
export default defineConfig({
  base: "/gpui-ai/",
  plugins: [react()],
  build: {
    target: "esnext",
    sourcemap: false,
    rollupOptions: {
      input: { index: resolve(import.meta.dirname, "index.html") },
    },
  },
  server: { port: 5175 },
  preview: { port: 5176 },
});
