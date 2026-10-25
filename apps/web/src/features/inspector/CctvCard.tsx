import { useEffect, useState } from 'react';
import type { CameraSource, CctvDetectionsResponse, GeoPoint } from '../../contracts';
import { Button, CopyIcon, NavigationArrowIcon, Panel, XIcon } from '../../design';
import { useGlobe, usePlatform, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export interface CctvCardProps {
  readonly camera: CameraSource;
  readonly className?: string | undefined;
}

export function cctvFacts(camera: CameraSource): Fact[] {
  const facts: Fact[] = [
    { label: 'Camera ID', value: camera.id, mono: true },
    { label: 'Provider', value: camera.provider },
    { label: 'City / Region', value: camera.city || camera.cityId || 'Unknown' },
  ];

  if (camera.headingDeg !== undefined) {
    facts.push({
      label: 'Azimuth / Heading',
      value: `${camera.headingDeg.toFixed(1)}°`,
      mono: true,
    });
  }

  if (camera.fovDeg !== undefined) {
    facts.push({
      label: 'Field of view (FOV)',
      value: `${camera.fovDeg.toFixed(1)}°`,
      mono: true,
    });
  }

  if (camera.mountHeightM !== undefined) {
    facts.push({
      label: 'Mount height',
      value: `${camera.mountHeightM.toFixed(1)} m`,
      mono: true,
    });
  }

  if (camera.rangeM !== undefined) {
    facts.push({
      label: 'Optical range',
      value: `${camera.rangeM.toFixed(0)} m`,
      mono: true,
    });
  }

  facts.push({
    label: 'Position',
    value: `${camera.lat.toFixed(5)}°, ${camera.lon.toFixed(5)}°`,
    mono: true,
  });

  return facts;
}

export function CctvCard({ camera, className }: CctvCardProps) {
  const globe = useGlobe();
  const platform = usePlatform();
  const select = useWorkspace((s) => s.select);
  const [detections, setDetections] = useState<CctvDetectionsResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [imgError, setImgError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    platform
      .cctvDetections(camera.id)
      .then((res: CctvDetectionsResponse) => {
        if (!cancelled) {
          setDetections(res);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [camera.id, platform]);

  const facts = cctvFacts(camera);
  const position: GeoPoint = { lat: camera.lat, lon: camera.lon };

  const handleFlyTo = () => {
    globe?.flyTo({ kind: 'point', position });
  };

  const handleClose = () => {
    select(null);
  };

  return (
    <Panel title="Traffic Camera" className={className}>
      <div className={styles.summary}>
        <span className={styles.summaryText}>{camera.name}</span>
        <button
          type="button"
          className={styles.linkButton}
          onClick={handleClose}
          aria-label="Close inspector"
        >
          <XIcon size={14} />
        </button>
      </div>

      <div className={styles.noticeBox}>
        <strong>Sensitive feed (ADR 0008):</strong> Public traffic camera. Imagery is ephemeral and
        not recorded without an active audit trail.
        {camera.credit && <div className={styles.noticeAttribution}>{camera.credit}</div>}
      </div>

      {camera.snapshotUrl && !imgError && (
        <div style={{ marginTop: 'var(--s-3)', borderRadius: 'var(--r-sm)', overflow: 'hidden' }}>
          <img
            src={camera.snapshotUrl}
            alt={camera.name}
            onError={() => setImgError(true)}
            style={{
              width: '100%',
              height: 'auto',
              display: 'block',
              maxHeight: 200,
              objectFit: 'cover',
            }}
          />
        </div>
      )}

      <div className={styles.subhead}>Camera Specification</div>
      <FactSheet label="Camera Specification" facts={facts} />

      <div className={styles.subhead}>Mojo MLIR Perception (ADR 0009)</div>
      {loading ? (
        <div className={styles.historyNote}>Analyzing frame with MLIR SIMD kernel...</div>
      ) : detections && detections.detections.length > 0 ? (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 'var(--s-2)',
            marginTop: 'var(--s-2)',
          }}
        >
          {detections.detections.map((det) => (
            <div
              key={`${det.className}-${det.bbox[0]}-${det.bbox[1]}-${det.bbox[2]}-${det.bbox[3]}`}
              style={{
                background: 'var(--well)',
                padding: 'var(--s-2)',
                borderRadius: 'var(--r-xs)',
                fontSize: 'var(--type-caption)',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', fontWeight: 600 }}>
                <span>{det.className.toUpperCase()}</span>
                <span>{(det.confidence * 100).toFixed(0)}%</span>
              </div>
              <div
                style={{
                  height: 4,
                  background: 'var(--line)',
                  borderRadius: 2,
                  marginTop: 4,
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    height: '100%',
                    width: `${Math.round(det.confidence * 100)}%`,
                    background: det.confidence > 0.8 ? 'var(--good)' : 'var(--warning)',
                  }}
                />
              </div>
              {det.plateText && (
                <div style={{ marginTop: 4, fontFamily: 'var(--font-mono)', color: 'var(--ink)' }}>
                  Plate: {det.plateText}
                </div>
              )}
            </div>
          ))}
        </div>
      ) : (
        <div className={styles.historyNote}>No targets currently detected in camera frustum.</div>
      )}

      <div className={styles.actions}>
        <Button variant="ghost" onClick={handleFlyTo}>
          <NavigationArrowIcon size={14} />
          Fly to
        </Button>
        <Button variant="ghost" onClick={() => copyCoordinates(position)}>
          <CopyIcon size={14} />
          Copy coordinates
        </Button>
      </div>
    </Panel>
  );
}
