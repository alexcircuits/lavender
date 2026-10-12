import { defineConfig } from 'vite';

// Cesium's static half is copied to public/cesium by scripts/copy-cesium-assets.mjs.
export default defineConfig({
  server: { port: 5410, strictPort: true },
  build: { target: 'es2022', chunkSizeWarningLimit: 6000 },
});
