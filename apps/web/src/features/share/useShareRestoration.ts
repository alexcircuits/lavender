import { useEffect, useRef } from 'react';
import { showToast } from '../../design';
import type { GlobeController } from '../../globe';
import { usePreferences, useWorkspace } from '../../runtime';
import { decodeShareHash } from './shareCodec';

/**
 * Listens for startup URL hash or hash change and restores camera, layers,
 * visual preset, and selection.
 */
export function useShareRestoration(globe: GlobeController | null) {
  const restoredRef = useRef(false);
  const setVisualPreset = usePreferences((s) => s.setVisualPreset);
  const setHudEnabled = usePreferences((s) => s.setHudEnabled);
  const setScopeMaskEnabled = usePreferences((s) => s.setScopeMaskEnabled);
  const setSonarEnabled = usePreferences((s) => s.setSonarEnabled);
  const setLayerEnabled = useWorkspace((s) => s.setLayerEnabled);
  const select = useWorkspace((s) => s.select);
  const setImagery = useWorkspace((s) => s.setImagery);

  useEffect(() => {
    if (!globe || typeof window === 'undefined') return;
    if (restoredRef.current) return;

    const hash = window.location.hash;
    if (!hash || hash.length <= 1) return;

    const decoded = decodeShareHash(hash);
    if (!decoded) return;

    restoredRef.current = true;

    // 1. Restore camera
    if (decoded.camera) {
      const headingRad = (decoded.camera.headingDeg * Math.PI) / 180;
      const pitchRad = (decoded.camera.pitchDeg * Math.PI) / 180;
      const rollRad = (decoded.camera.rollDeg * Math.PI) / 180;

      globe.flyTo({
        kind: 'point',
        position: { lat: decoded.camera.lat, lon: decoded.camera.lon },
        heightM: decoded.camera.altM,
      });

      // After a tick, ensure exact heading and pitch are set
      setTimeout(() => {
        globe.restoreCamera({
          lat: decoded.camera?.lat ?? 0,
          lon: decoded.camera?.lon ?? 0,
          heightM: decoded.camera?.altM ?? 10_000_000,
          headingRad,
          pitchRad,
          rollRad,
        });
      }, 500);
    }

    // 2. Restore enabled layers
    if (decoded.enabledLayers && decoded.enabledLayers.length > 0) {
      for (const feed of decoded.enabledLayers) {
        setLayerEnabled(feed, true);
      }
    }

    // 3. Restore visual preset
    if (decoded.visualPreset) {
      setVisualPreset(decoded.visualPreset);
    }

    // 4. Restore overlays
    setHudEnabled(decoded.hudEnabled);
    setScopeMaskEnabled(decoded.scopeMaskEnabled);
    setSonarEnabled(decoded.sonarEnabled);

    // 5. Restore imagery basemap
    if (decoded.imageryId) {
      setImagery(decoded.imageryId);
    }

    // 6. Restore selection & tracking mode
    if (decoded.selection) {
      select(decoded.selection);
      if (decoded.trackingMode === 'cockpit') {
        globe.enterCockpit(decoded.selection);
      } else if (decoded.trackingMode === 'follow') {
        globe.startFollowing(decoded.selection);
      }
    }

    showToast({ title: 'Workspace view restored from link' });
  }, [
    globe,
    setVisualPreset,
    setHudEnabled,
    setScopeMaskEnabled,
    setSonarEnabled,
    setLayerEnabled,
    select,
    setImagery,
  ]);
}
