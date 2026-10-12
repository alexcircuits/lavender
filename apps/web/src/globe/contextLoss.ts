/**
 * WebGL context loss. Cesium has no context-restore path (it never rebuilds its GPU
 * resources), so a lost context would otherwise leave a silently frozen globe. The loss is
 * reported through `onRenderError` (the shell shows its failure state); `preventDefault` lets
 * the browser restore the context, which is logged so a remount can be diagnosed.
 */

export const CONTEXT_LOST_MESSAGE = 'The WebGL context was lost; the globe stopped drawing.';

export interface ContextLossHandlers {
  readonly onLost: (error: Error) => void;
  readonly onRestored?: () => void;
}

/** Watches a canvas for context loss/restore; returns the function that stops watching. */
export function watchContextLoss(
  canvas: HTMLCanvasElement,
  handlers: ContextLossHandlers,
): () => void {
  const lost = (event: Event) => {
    event.preventDefault();
    handlers.onLost(new Error(CONTEXT_LOST_MESSAGE));
  };
  const restored = () => {
    console.warn('Globe: the WebGL context was restored; remount the globe to draw again.');
    handlers.onRestored?.();
  };
  canvas.addEventListener('webglcontextlost', lost);
  canvas.addEventListener('webglcontextrestored', restored);
  return () => {
    canvas.removeEventListener('webglcontextlost', lost);
    canvas.removeEventListener('webglcontextrestored', restored);
  };
}
