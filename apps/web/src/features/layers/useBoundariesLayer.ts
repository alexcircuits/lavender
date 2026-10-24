import { useEffect } from 'react';
import { useGlobe, usePlatform, useWorkspace } from '../../runtime';
import {
  boundaryDetailBand,
  boundaryZoomDefaults,
  boundaryZoomForHeightM,
} from './boundaryHierarchy';

/**
 * Synchronizes boundary vector geometry from the Rust platform backend to the Cesium
 * globe engine, and coordinates per-class visibility toggles.
 *
 * The endpoint simplifies per zoom level, so the settled zoom band drives refetches and,
 * while zoom automation is on, zoom-appropriate class defaults (maritime indicative lines
 * appear only once the analyst is close enough for them to read as more than noise).
 */
export function useBoundariesLayer(): void {
  const globe = useGlobe();
  const platform = usePlatform();
  const enabled = useWorkspace((s) => s.boundariesEnabled);
  const classes = useWorkspace((s) => s.boundaryClasses);
  const automation = useWorkspace((s) => s.boundaryZoomAutomation);
  const applyDefaults = useWorkspace((s) => s.applyBoundaryZoomDefaults);

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

  // Load boundary features from backend platform when globe is available and enabled,
  // refetching when the camera settles at a new zoom level (per-zoom simplification).
  useEffect(() => {
    if (!globe || !enabled) return;
    let lastZoom: number | null = null;
    const controller = new AbortController();

    const load = (zoom: number) => {
      platform
        .boundaries(
          { west: -180, south: -90, east: 180, north: 90 },
          { zoom, signal: controller.signal },
        )
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
    };

    load(boundaryZoomForHeightM(globe.cameraPose()?.heightM ?? Number.NaN));

    // Only settled views are reported (moveEnd); each carries the camera height.
    const stopView = globe.onViewChange((view) => {
      const zoom = boundaryZoomForHeightM(view.heightM);
      if (automation) applyDefaults(boundaryZoomDefaults(boundaryDetailBand(zoom)));
      if (zoom !== lastZoom) {
        lastZoom = zoom;
        load(zoom);
      }
    });

    return () => {
      controller.abort();
      stopView();
    };
  }, [globe, platform, enabled, automation, applyDefaults]);
}
