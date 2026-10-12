// Feasibility spike: does @cesium/engine render inside a Tauri 2 WebView under a
// strict CSP, do its workers and static assets resolve over the app's custom
// scheme, and do IPC commands and channels work? Results are measured, not
// eyeballed: the page reports to Rust, which writes them to disk.
import '@cesium/engine/Source/Widget/CesiumWidget.css';
import { Channel, invoke, isTauri } from '@tauri-apps/api/core';

declare global {
  interface Window {
    CESIUM_BASE_URL: string;
  }
}

window.CESIUM_BASE_URL = '/cesium/';

const out = document.getElementById('report') as HTMLPreElement;
const violations: string[] = [];
document.addEventListener('securitypolicyviolation', (e) => {
  violations.push(`${e.violatedDirective} ← ${e.blockedURI || 'inline'}`);
});
const errors: string[] = [];
window.addEventListener('error', (e) => errors.push(String(e.message)));
window.addEventListener('unhandledrejection', (e) => errors.push(String(e.reason)));

function log(line: string) {
  out.textContent = `${out.textContent}\n${line}`;
  if (isTauri()) void invoke('log_stage', { line }).catch(() => undefined);
}
log(`boot visibility=${document.visibilityState}`);

async function main() {
  const t0 = performance.now();
  log('importing engine');
  const Cesium = await import('@cesium/engine');
  log('engine imported');
  Cesium.Ion.defaultAccessToken = '';

  const widget = new Cesium.CesiumWidget('globe', {
    baseLayer: Cesium.ImageryLayer.fromProviderAsync(
      Cesium.TileMapServiceImageryProvider.fromUrl(
        Cesium.buildModuleUrl('Assets/Textures/NaturalEarthII'),
      ),
    ),
    terrainProvider: new Cesium.EllipsoidTerrainProvider(),
    creditContainer: 'credits',
    contextOptions: { webgl: { preserveDrawingBuffer: true } },
    useBrowserRecommendedResolution: true,
  });
  const scene = widget.scene;
  const tCreated = performance.now();
  log(`widget created in ${(tCreated - t0).toFixed(0)} ms`);

  // 20k GPU points: the representative "many live contacts" workload.
  const points = scene.primitives.add(new Cesium.PointPrimitiveCollection());
  for (let i = 0; i < 20_000; i += 1) {
    points.add({
      position: Cesium.Cartesian3.fromDegrees(Math.random() * 360 - 180, Math.random() * 160 - 80, 10_000),
      pixelSize: 3,
      color: Cesium.Color.fromCssColorString('#b9a6ff'),
    });
  }

  // Asynchronous geometry is built in a web worker: proves Workers resolve.
  const polygon = scene.primitives.add(
    new Cesium.Primitive({
      geometryInstances: new Cesium.GeometryInstance({
        geometry: new Cesium.PolygonGeometry({
          polygonHierarchy: new Cesium.PolygonHierarchy(
            Cesium.Cartesian3.fromDegreesArray([-10, 35, 30, 35, 30, 60, -10, 60]),
          ),
        }),
        attributes: {
          color: Cesium.ColorGeometryInstanceAttribute.fromColor(Cesium.Color.fromCssColorString('#8f7cf0').withAlpha(0.35)),
        },
      }),
      appearance: new Cesium.PerInstanceColorAppearance({ flat: true }),
    }),
  );

  const gl = scene.context._gl as WebGLRenderingContext | WebGL2RenderingContext;
  const dbg = gl.getExtension('WEBGL_debug_renderer_info');
  const renderer = dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);

  // Wait for the globe's first full tile load (bounded), then measure motion FPS.
  const tilesLoaded = await new Promise<boolean>((resolve) => {
    const deadline = performance.now() + 20_000;
    const remove = scene.postRender.addEventListener(() => {
      if (scene.globe.tilesLoaded) { remove(); resolve(true); }
      else if (performance.now() > deadline) { remove(); resolve(false); }
    });
  });
  const tTiles = performance.now();
  log(`tiles loaded=${tilesLoaded} after ${(tTiles - t0).toFixed(0)} ms`);

  // Wall-clock bounded: an occluded WKWebView window pauses requestAnimationFrame.
  let frames = 0;
  const fps = await new Promise<number>((resolve) => {
    const start = performance.now();
    const finish = () => { remove(); resolve((frames * 1000) / (performance.now() - start)); };
    const remove = scene.postRender.addEventListener(() => {
      frames += 1;
      scene.camera.rotate(Cesium.Cartesian3.UNIT_Z, 0.004);
      if (performance.now() - start > 3000) finish();
    });
    setTimeout(finish, 6000);
  });
  log(`motion fps ${fps.toFixed(1)} (${frames} frames)`);
  scene.render();

  const workerGeometryReady = polygon.ready === true;
  const png = scene.canvas.toDataURL('image/png');
  const lit = await litPixelRatio(png);

  const report = {
    userAgent: navigator.userAgent,
    tauri: isTauri(),
    webgl2: scene.context.webgl2,
    renderer,
    maxTextureSize: gl.getParameter(gl.MAX_TEXTURE_SIZE),
    widgetCreateMs: Math.round(tCreated - t0),
    tilesLoaded,
    tilesLoadedMs: Math.round(tTiles - t0),
    motionFps: Math.round(fps * 10) / 10,
    points: points.length,
    workerGeometryReady,
    litPixelRatio: Math.round(lit * 1000) / 1000,
    cspViolations: violations,
    errors,
    ipc: null as unknown,
  };

  if (isTauri()) {
    const info = await invoke<{ core: string; distanceKm: number }>('spike_core_info');
    const ticks: number[] = [];
    const channel = new Channel<{ seq: number }>();
    channel.onmessage = (m) => ticks.push(m.seq);
    const tIpc = performance.now();
    await invoke('stream_ticks', { onTick: channel, count: 200 });
    // Channel delivery is async relative to the command's resolution.
    await new Promise((r) => setTimeout(r, 250));
    report.ipc = {
      command: info,
      channelReceived: ticks.length,
      channelInOrder: ticks.every((v, i) => v === i),
      channelMs: Math.round(performance.now() - tIpc - 250),
    };
    log(JSON.stringify(report, null, 2));
    await invoke('report_smoke', { report, pngDataUrl: png });
  } else {
    log(JSON.stringify(report, null, 2));
  }
}

async function litPixelRatio(dataUrl: string): Promise<number> {
  const img = new Image();
  img.src = dataUrl;
  await img.decode();
  const c = document.createElement('canvas');
  c.width = 256; c.height = 160;
  const ctx = c.getContext('2d');
  if (!ctx) return 0;
  ctx.drawImage(img, 0, 0, c.width, c.height);
  const { data } = ctx.getImageData(0, 0, c.width, c.height);
  let lit = 0;
  for (let i = 0; i < data.length; i += 4) if (data[i] + data[i + 1] + data[i + 2] > 60) lit += 1;
  return lit / (c.width * c.height);
}

main().catch((e) => {
  errors.push(String(e?.stack ?? e));
  log(`FAILED: ${e?.stack ?? e}`);
  if (isTauri()) void invoke('report_smoke', { report: { fatal: String(e), errors, cspViolations: violations }, pngDataUrl: '' });
});
