// Frame read-back for diagnostics (rendering smoke checks, the desktop self-check). The
// WebGL drawing buffer is only valid during the frame that drew it, so the copy is taken in
// `postRender`, straight after Cesium renders, without enabling `preserveDrawingBuffer`.
import type { Scene } from '@cesium/engine';
import type { FrameCapture } from './index';

/** Brightness (r+g+b) above which a pixel counts as lit; the ground colour sits below it. */
const LIT_THRESHOLD = 60;
/** Gives up when no frame renders (hidden window, lost context). */
const CAPTURE_TIMEOUT_MS = 3_000;

/** Share of pixels brighter than near-black in RGBA data. */
export function litRatio(rgba: Uint8ClampedArray): number {
  const pixels = rgba.length / 4;
  if (pixels === 0) return 0;
  let lit = 0;
  for (let i = 0; i < rgba.length; i += 4) {
    if ((rgba[i] ?? 0) + (rgba[i + 1] ?? 0) + (rgba[i + 2] ?? 0) > LIT_THRESHOLD) lit += 1;
  }
  return lit / pixels;
}

function copy(canvas: HTMLCanvasElement, maxWidth: number): FrameCapture | null {
  const scale = Math.min(1, maxWidth / Math.max(1, canvas.width));
  const out = document.createElement('canvas');
  out.width = Math.max(1, Math.round(canvas.width * scale));
  out.height = Math.max(1, Math.round(canvas.height * scale));
  const ctx = out.getContext('2d');
  if (!ctx) return null;
  ctx.drawImage(canvas, 0, 0, out.width, out.height);
  const ratio = litRatio(ctx.getImageData(0, 0, out.width, out.height).data);
  return {
    dataUrl: out.toDataURL('image/png'),
    width: out.width,
    height: out.height,
    litRatio: Math.round(ratio * 1000) / 1000,
  };
}

export function captureFrame(
  scene: Scene,
  canvas: HTMLCanvasElement,
  maxWidth: number,
): Promise<FrameCapture | null> {
  return new Promise((resolve) => {
    let done = false;
    const finish = (value: FrameCapture | null): void => {
      if (done) return;
      done = true;
      off();
      clearTimeout(timer);
      resolve(value);
    };
    const off = scene.postRender.addEventListener(() => {
      try {
        finish(copy(canvas, maxWidth));
      } catch {
        // A tainted or lost canvas cannot be read back; report nothing rather than fail.
        finish(null);
      }
    });
    const timer = setTimeout(() => finish(null), CAPTURE_TIMEOUT_MS);
    scene.requestRender();
  });
}
