import type { Plugin } from 'vite';

/**
 * satellite.js 7 re-exports an optional WASM bulk propagator from its root entry
 * (`export * from './wasm/index.js'`). That build is Emscripten with pthreads: it spawns its
 * own module Web Worker whose code starts with a top-level `await import("node:worker_threads")`.
 * Vite bundles every `new Worker(new URL(...))` it finds as a separate worker entry, and the
 * default `iife` worker format cannot hold top-level await, so the production build fails:
 *
 *   [UNSUPPORTED_FEATURE] Top-level await is currently not supported with the 'iife' output
 *   format (satellite.js/wasm-build/pthreads-release/index.js)
 *
 * Lavender uses only the classic JavaScript SGP4 functions (`json2satrec`, `propagate`,
 * `gstime`, `eciToGeodetic`, ...). Switching the worker format to `es` would make the build
 * pass but ship an unused threaded WASM worker that also needs cross-origin isolation. So the
 * WASM sub-module is replaced by an empty module when, and only when, satellite.js's own entry
 * asks for it. If a future satellite.js moves the file, the stub stops matching and the build
 * fails loudly again, which is the point.
 */
const STUB_ID = '\0lavender:satellite-js-wasm-disabled';
const SATELLITE_ENTRY = /[\\/]satellite\.js[\\/]dist[\\/]index\.js$/;

export function satelliteJsWithoutWasm(): Plugin {
  return {
    name: 'lavender:satellite-js-without-wasm',
    enforce: 'pre',
    resolveId(source, importer) {
      if (source === './wasm/index.js' && importer && SATELLITE_ENTRY.test(importer))
        return STUB_ID;
      return null;
    },
    load(id) {
      return id === STUB_ID ? 'export {};' : null;
    },
  };
}
