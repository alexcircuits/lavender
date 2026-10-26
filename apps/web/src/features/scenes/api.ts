import { useEffect, useState } from 'react';
import type { EvidencePack, MapCatalogSnapshot } from '../../contracts';
import { usePlatform } from '../../runtime';

export interface Loaded<T> {
  readonly data: T | null;
  readonly error: string | null;
  readonly pending: boolean;
}

/** Queries registered scene packs through LavenderPlatform (LB-118, ADR 0005). */
export function useScenePacks(enabled = true): Loaded<readonly EvidencePack[]> {
  const platform = usePlatform();
  const [state, setState] = useState<Loaded<readonly EvidencePack[]>>({
    data: null,
    error: null,
    pending: enabled,
  });

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    if (!platform.scenePacks) {
      setState({ data: [], error: null, pending: false });
      return;
    }
    platform.scenePacks({ signal: controller.signal }).then(
      (data) => setState({ data, error: null, pending: false }),
      (e: unknown) => {
        if (controller.signal.aborted) return;
        setState({
          data: null,
          error: e instanceof Error ? e.message : 'Request failed',
          pending: false,
        });
      },
    );
    return () => controller.abort();
  }, [platform, enabled]);

  return state;
}

/** Queries the map stacks catalog through LavenderPlatform (LB-119, ADR 0005). */
export function useMapCatalog(): Loaded<MapCatalogSnapshot> {
  const platform = usePlatform();
  const [state, setState] = useState<Loaded<MapCatalogSnapshot>>({
    data: null,
    error: null,
    pending: true,
  });

  useEffect(() => {
    const controller = new AbortController();
    if (!platform.mapCatalog) {
      setState({ data: null, error: 'Map catalog not supported', pending: false });
      return;
    }
    platform.mapCatalog({ signal: controller.signal }).then(
      (data) => setState({ data, error: null, pending: false }),
      (e: unknown) => {
        if (controller.signal.aborted) return;
        setState({
          data: null,
          error: e instanceof Error ? e.message : 'Request failed',
          pending: false,
        });
      },
    );
    return () => controller.abort();
  }, [platform]);

  return state;
}
