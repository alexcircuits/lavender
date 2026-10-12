import { type RefObject, useEffect, useRef, useState } from 'react';
import type { GlobeController, MountGlobe } from '../globe';
import { useRuntime } from '../runtime';

export type GlobeStatus = 'loading' | 'ready' | 'failed';

export interface MountedGlobe {
  readonly hostRef: RefObject<HTMLDivElement | null>;
  readonly creditsRef: RefObject<HTMLDivElement | null>;
  readonly status: GlobeStatus;
}

/**
 * Mounts the globe once into `hostRef`, attaches it to the runtime and tears it down on
 * unmount. Failures are reported in words, never left as a blank or frozen canvas.
 */
export function useMountedGlobe(mountGlobe: MountGlobe): MountedGlobe {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const creditsRef = useRef<HTMLDivElement | null>(null);
  const [status, setStatus] = useState<GlobeStatus>('loading');
  const runtime = useRuntime();

  useEffect(() => {
    const host = hostRef.current;
    const credits = creditsRef.current;
    if (!host || !credits) return;
    let cancelled = false;
    let globe: GlobeController | null = null;
    mountGlobe({
      host,
      credits,
      onRenderError: (error) => {
        console.error('Globe render error', error);
        if (!cancelled) setStatus('failed');
      },
    })
      .then((mounted) => {
        if (cancelled) {
          mounted.destroy();
          return;
        }
        globe = mounted;
        runtime.attachGlobe(mounted);
        setStatus('ready');
      })
      .catch((error: unknown) => {
        console.error('Globe failed to start', error);
        if (!cancelled) setStatus('failed');
      });
    return () => {
      cancelled = true;
      if (globe) runtime.detachGlobe();
      globe?.destroy();
    };
  }, [mountGlobe, runtime]);

  return { hostRef, creditsRef, status };
}
