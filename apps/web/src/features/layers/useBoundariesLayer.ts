import { useEffect } from 'react';
import { useGlobe, usePlatform, useWorkspace } from '../../runtime';

/**
 * Synchronizes boundary vector geometry from the Rust platform backend to the Cesium
 * globe engine, and coordinates per-class visibility toggles.
 */
export function useBoundariesLayer(): void {
  const globe = useGlobe();
  const platform = usePlatform();
  const enabled = useWorkspace((s) => s.boundariesEnabled);
  const classes = useWorkspace((s) => s.boundaryClasses);

  // Sync visibility with globe when classes change or layer is toggled
  useEffect(() => {
    if (!globe) return;
    if (!enabled) {
      globe.setBoundaryClassVisibility({
        internationalLand: false,
        disputedLine: false,
        breakawayArea: false,
        maritimeIndicative: false,
      });
    } else {
      globe.setBoundaryClassVisibility(classes);
    }
  }, [globe, enabled, classes]);

  // Load boundary features from backend platform when globe is available and enabled
  useEffect(() => {
    if (!globe || !enabled) return;
    const controller = new AbortController();

    platform
      .boundaries({ west: -180, south: -90, east: 180, north: 90 }, { signal: controller.signal })
      .then((res) => {
        if (!controller.signal.aborted) {
          globe.setBoundaries(res.features);
        }
      })
      .catch((err) => {
        if (!controller.signal.aborted) {
          console.warn('Could not load boundaries:', err);
        }
      });

    return () => {
      controller.abort();
    };
  }, [globe, platform, enabled]);
}
