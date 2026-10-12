import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { satelliteJsWithoutWasm } from './satellite-js-without-wasm.ts';

/** lavender-server's default listen address (docs/architecture/frontend.md §7). */
const API_TARGET = 'http://127.0.0.1:7410';

/**
 * The web variant of the desktop policy (docs/architecture/tauri.md §5.2). Only `vite preview`
 * sends it, so the Playwright smoke test exercises the built app under the real policy; in
 * production lavender-server sets it and Tauri injects its own.
 */
const PREVIEW_CSP = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "font-src 'self'",
  "connect-src 'self'",
  "worker-src 'self' blob:",
  "object-src 'none'",
  "base-uri 'self'",
  "frame-ancestors 'none'",
].join('; ');

// Cesium's static half (workers, WASM, assets) is copied to public/cesium by
// scripts/copy-cesium-assets.mjs before dev and build; the engine code itself is only ever
// reached through the dynamic import in src/globe/index.ts, so Rolldown emits it as its own
// lazily loaded chunk and the shell paints without it.
export default defineConfig({
  plugins: [react(), satelliteJsWithoutWasm()],
  server: {
    port: 5410,
    strictPort: true,
    proxy: {
      '/api': { target: API_TARGET, changeOrigin: false, ws: true },
    },
  },
  preview: {
    port: 5411,
    strictPort: true,
    headers: { 'Content-Security-Policy': PREVIEW_CSP },
  },
  css: {
    modules: {
      localsConvention: 'camelCaseOnly',
      generateScopedName: '[local]_[hash:base64:5]',
    },
  },
  build: {
    target: 'es2022',
    // The engine chunk is ~4 MB minified by nature; the shell has its own, much tighter budget
    // (≤ 250 KiB gzip initial JS), enforced per chunk by scripts/check-dist.mjs. Vite's limit
    // is global, so it is raised only to stop the engine chunk from drowning the build log.
    chunkSizeWarningLimit: 6000,
  },
});
