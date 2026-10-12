import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakePlatform, fakeGlobe, quakeSnapshot } from '../test/fakes';
import { createRuntime } from './createRuntime';
import { startSelfCheck } from './selfCheck';

describe('startSelfCheck', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const setup = () => {
    const platform = new FakePlatform();
    const runtime = createRuntime(platform);
    const reports: Array<Record<string, unknown>> = [];
    const sink = { report: async (r: Readonly<Record<string, unknown>>) => void reports.push(r) };
    const target = new EventTarget() as unknown as Window;
    return { platform, runtime, reports, sink, target };
  };

  it('reports success once live data arrived and the globe is attached', async () => {
    const { platform, runtime, reports, sink, target } = setup();
    runtime.start();
    startSelfCheck(runtime, sink, { settleMs: 100, target });
    await vi.advanceTimersByTimeAsync(0);
    runtime.stores.workspace.getState().setLayerEnabled('earthquakes', true);
    platform
      .live('earthquakes')
      ?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(['a', 'b']) });
    expect(reports).toHaveLength(0);
    const { globe } = fakeGlobe();
    vi.mocked(globe.captureFrame).mockResolvedValue({
      dataUrl: 'data:image/png;base64,x',
      width: 2,
      height: 1,
      litRatio: 0.4,
    });
    runtime.attachGlobe(globe);
    await vi.advanceTimersByTimeAsync(1_200); // settle + one second of frame counting
    expect(reports).toEqual([
      expect.objectContaining({
        ok: true,
        feed: 'earthquakes',
        count: 2,
        globeAttached: true,
        litRatio: 0.4,
      }),
    ]);
  });

  it('fails when the globe frame is blank even though data arrived', async () => {
    const { platform, runtime, reports, sink, target } = setup();
    runtime.start();
    startSelfCheck(runtime, sink, { settleMs: 100, target });
    await vi.advanceTimersByTimeAsync(0);
    runtime.stores.workspace.getState().setLayerEnabled('earthquakes', true);
    platform.live('earthquakes')?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(['a']) });
    const { globe } = fakeGlobe();
    vi.mocked(globe.captureFrame).mockResolvedValue({
      dataUrl: 'data:,',
      width: 1,
      height: 1,
      litRatio: 0,
    });
    runtime.attachGlobe(globe);
    await vi.advanceTimersByTimeAsync(1_200); // settle + one second of frame counting
    expect(reports).toEqual([expect.objectContaining({ ok: false, litRatio: 0 })]);
  });

  it('reports failure with what it saw when the deadline passes', async () => {
    const { runtime, reports, sink, target } = setup();
    runtime.start();
    startSelfCheck(runtime, sink, { timeoutMs: 1_000, target });
    target.dispatchEvent(new ErrorEvent('error', { message: 'boom' }));
    await vi.advanceTimersByTimeAsync(2_100);
    expect(reports).toEqual([expect.objectContaining({ ok: false, count: 0, errors: ['boom'] })]);
  });
});
