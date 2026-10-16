/**
 * In-page instrumentation for the performance harness, installed with `page.addInitScript`
 * before any application code runs. It observes the production build from the outside, so
 * no measurement hook ships in the app:
 *
 * - Rendered frames: WebGL draw calls are counted, and an animation-frame loop records each
 *   frame in which the count grew. Cesium renders inside animation frames, so this equals
 *   `scene.postRender` counting (request-render mode: an idle globe adds nothing).
 * - Startup marks: when the shell (`main[aria-label="Globe"]`) and the globe canvas appear.
 * - Stream frames: feed, `fetchedAt` and the synchronous cost of each WebSocket `onmessage` handler (JSON parse,
 *   globe apply, store updates), the cost up to the end of its microtasks (React's
 *   synchronous re-render), and for large frames a separate `JSON.parse` of the same text.
 * - Clicks and Event Timing entries (interaction latency, the INP measure).
 *
 * This file is serialised into the page: everything it uses must be defined inside.
 */

export interface StreamFrame {
  readonly at: number;
  readonly feed: string | null;
  /** The snapshot's `fetchedAt`: a new value means new upstream data, a repeat a re-send. */
  readonly fetchedAt: string | null;
  readonly bytes: number;
  /** The `onmessage` handler itself. */
  readonly handlerMs: number;
  /** Handler plus the microtasks it queued (React's synchronous render). */
  taskMs: number | null;
  /** A second `JSON.parse` of the same text, for frames over 100 kB (else null). */
  readonly parseMs: number | null;
}

export interface PerfState {
  draws: number;
  lastDraws: number;
  rafTicks: number;
  readonly frameTimes: number[];
  readonly marks: Record<string, number>;
  readonly frames: StreamFrame[];
  readonly clicks: Array<{ readonly at: number; readonly label: string }>;
  readonly events: Array<{ readonly name: string; readonly start: number; readonly ms: number }>;
}

export type PerfWindow = Window & { __lavenderPerf?: PerfState };

export function instrument(): void {
  const state: PerfState = {
    draws: 0,
    lastDraws: 0,
    rafTicks: 0,
    frameTimes: [],
    marks: {},
    frames: [],
    clicks: [],
    events: [],
  };
  (window as PerfWindow).__lavenderPerf = state;
  const mark = (name: string) => {
    if (state.marks[name] === undefined) state.marks[name] = performance.now();
  };
  // Cesium alone requests hundreds of resources; the default buffer holds 250.
  performance.setResourceTimingBufferSize(10_000);

  for (const proto of [WebGLRenderingContext.prototype, WebGL2RenderingContext.prototype]) {
    for (const name of [
      'drawArrays',
      'drawElements',
      'drawArraysInstanced',
      'drawElementsInstanced',
    ]) {
      const methods = proto as unknown as Record<string, (...args: unknown[]) => unknown>;
      const original = methods[name];
      if (typeof original !== 'function') continue;
      methods[name] = function (this: unknown, ...args: unknown[]) {
        state.draws += 1;
        return original.apply(this, args);
      };
    }
  }

  const tick = (now: number) => {
    state.rafTicks += 1;
    if (state.draws !== state.lastDraws) {
      state.lastDraws = state.draws;
      state.frameTimes.push(now);
      // Bounded: about five minutes at 60 fps.
      if (state.frameTimes.length > 20_000) state.frameTimes.splice(0, 10_000);
      mark('firstFrame');
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);

  new MutationObserver(() => {
    if (document.querySelector('main[aria-label="Globe"]')) mark('shell');
    if (document.querySelector('.lv-globe-host canvas')) mark('canvas');
  }).observe(document, { subtree: true, childList: true });

  const socket = window.WebSocket.prototype;
  const handler = Object.getOwnPropertyDescriptor(socket, 'onmessage');
  if (handler?.set && handler.get) {
    const { get, set } = handler;
    Object.defineProperty(socket, 'onmessage', {
      configurable: true,
      enumerable: true,
      get() {
        return get.call(this);
      },
      set(listener: unknown) {
        if (typeof listener !== 'function') {
          set.call(this, listener);
          return;
        }
        set.call(this, function (this: WebSocket, event: MessageEvent) {
          const start = performance.now();
          try {
            return listener.call(this, event);
          } finally {
            const handlerMs = performance.now() - start;
            const text = typeof event.data === 'string' ? event.data : '';
            let parseMs: number | null = null;
            if (text.length > 100_000) {
              const parseStart = performance.now();
              JSON.parse(text);
              parseMs = performance.now() - parseStart;
            }
            const head = text.slice(0, 600);
            const frame: StreamFrame = {
              at: start,
              feed: /"feed":"(\w+)"/.exec(head)?.[1] ?? null,
              fetchedAt: /"fetchedAt":"([^"]+)"/.exec(head)?.[1] ?? null,
              bytes: text.length,
              handlerMs,
              taskMs: null,
              parseMs,
            };
            state.frames.push(frame);
            if (state.frames.length > 500) state.frames.splice(0, 250);
            // Queued after the handler's own microtasks, so this runs once they are done.
            queueMicrotask(() => {
              frame.taskMs = performance.now() - start - (parseMs ?? 0);
            });
          }
        });
      },
    });
  }

  document.addEventListener(
    'click',
    (event) => {
      const target = event.target instanceof Element ? event.target : null;
      const label = target?.closest('[aria-label]')?.getAttribute('aria-label') ?? '';
      state.clicks.push({ at: event.timeStamp, label });
    },
    { capture: true },
  );
  new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      state.events.push({ name: entry.name, start: entry.startTime, ms: entry.duration });
    }
  }).observe({ type: 'event', durationThreshold: 16, buffered: true } as PerformanceObserverInit);
}
