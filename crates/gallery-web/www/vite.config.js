import { resolve } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  base: './',
  build: {
    target: 'esnext',
    sourcemap: false,
    rollupOptions: {
      input: {
        index: resolve(import.meta.dirname, 'index.html'),
        embed: resolve(import.meta.dirname, 'embed.html'),
      },
    },
  },
  server: {
    port: 4173,
    headers: {
      'Cross-Origin-Embedder-Policy': 'require-corp',
      'Cross-Origin-Opener-Policy': 'same-origin',
    },
  },
});
