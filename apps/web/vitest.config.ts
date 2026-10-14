import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';
import { satelliteJsWithoutWasm } from './satellite-js-without-wasm.ts';

export default defineConfig({
  // Docs read checked-in Markdown and evidence from the Lavender repository, outside web/.
  server: { fs: { allow: [fileURLToPath(new URL('../..', import.meta.url))] } },
  // Tests resolve satellite.js exactly as the production build does.
  plugins: [react(), satelliteJsWithoutWasm()],
  css: {
    modules: {
      localsConvention: 'camelCaseOnly',
      generateScopedName: '[local]',
    },
  },
  test: {
    environment: 'happy-dom',
    include: ['src/**/*.test.{ts,tsx}'],
    setupFiles: ['src/test/setup.ts'],
    css: { include: [/\.module\.css$/] },
    restoreMocks: true,
  },
});
