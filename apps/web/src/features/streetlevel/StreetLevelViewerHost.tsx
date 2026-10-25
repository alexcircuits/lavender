import { useEffect, useState } from 'react';
import type { GeoPoint } from '../../contracts';
import {
  ArrowSquareOutIcon,
  Button,
  CompassIcon,
  cx,
  EmptyState,
  ErrorState,
  IconButton,
  KeyIcon,
  Skeleton,
  XIcon,
} from '../../design';
import type { MapillaryPhotoItem } from '../../platform/types';
import { usePlatform, useUi } from '../../runtime';
import styles from './streetlevel.module.css';

export interface StreetLevelViewerHostProps {
  readonly imageId?: string | undefined;
  readonly coordinate?: GeoPoint | undefined;
  readonly onClose?: (() => void) | undefined;
  readonly className?: string | undefined;
}

export function StreetLevelViewerHost({
  imageId,
  coordinate,
  onClose,
  className,
}: StreetLevelViewerHostProps) {
  const platform = usePlatform();
  const setSidePanel = useUi((s) => s.setSidePanel);

  const [configured, setConfigured] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);
  const [photos, setPhotos] = useState<readonly MapillaryPhotoItem[]>([]);
  const [selectedPhoto, setSelectedPhoto] = useState<MapillaryPhotoItem | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    platform
      .mapillaryStatus()
      .then((res) => {
        if (!cancelled) {
          setConfigured(res.configured);
          setLoading(false);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [platform]);

  useEffect(() => {
    if (!configured || !coordinate) return;
    let cancelled = false;
    setLoading(true);

    const span = 0.005; // ~500m search box
    const bbox = {
      west: coordinate.lon - span,
      south: coordinate.lat - span,
      east: coordinate.lon + span,
      north: coordinate.lat + span,
    };

    platform
      .mapillarySearch?.(bbox, 12)
      ?.then((res) => {
        if (!cancelled) {
          setPhotos(res.data);
          if (res.data.length > 0) {
            setSelectedPhoto(res.data[0] ?? null);
          }
          setLoading(false);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [configured, coordinate, platform]);

  if (loading) {
    return (
      <div className={cx(styles.container, className)}>
        <Skeleton width="100%" height="240px" shape="block" />
      </div>
    );
  }

  if (configured === false) {
    return (
      <div className={cx(styles.container, styles.unconfiguredNotice, className)}>
        <div className={styles.header}>
          <div className={styles.titleRow}>
            <KeyIcon size={18} />
            <h3 className={styles.title}>Mapillary client token required</h3>
          </div>
          {onClose && <IconButton label="Close" icon={XIcon} onClick={onClose} />}
        </div>
        <p className={styles.attribution}>
          Street-level photo coverage and image viewer require a Mapillary Client Token. Configure{' '}
          <code>LAVENDER_MAPILLARY_CLIENT_TOKEN</code> in your environment or enter it in Settings
          to access global street-level photos.
        </p>
        <Button variant="secondary" onClick={() => setSidePanel('settings')}>
          Open Settings
        </Button>
      </div>
    );
  }

  if (error) {
    return (
      <div className={cx(styles.container, className)}>
        <ErrorState live title="Street-level imagery failed">
          {error}
        </ErrorState>
      </div>
    );
  }

  const currentPhoto = selectedPhoto ?? (imageId ? { id: imageId } : null);
  const photoUrl =
    currentPhoto?.thumb_1024_url ??
    currentPhoto?.thumb_256_url ??
    (currentPhoto?.id
      ? `https://graph.mapillary.com/${currentPhoto.id}?fields=thumb_1024_url`
      : undefined);

  return (
    <div className={cx(styles.container, className)} data-testid="street-level-viewer">
      <div className={styles.header}>
        <div className={styles.titleRow}>
          <h3 className={styles.title}>Street-Level View</h3>
          {currentPhoto && <span className={styles.attribution}>ID: {currentPhoto.id}</span>}
        </div>
        <div style={{ display: 'flex', gap: '4px' }}>
          {currentPhoto && (
            <IconButton
              label="Open in Mapillary"
              icon={ArrowSquareOutIcon}
              onClick={() =>
                window.open(
                  `https://www.mapillary.com/app/?pKey=${currentPhoto.id}`,
                  '_blank',
                  'noopener,noreferrer',
                )
              }
            />
          )}
          {onClose && <IconButton label="Close" icon={XIcon} onClick={onClose} />}
        </div>
      </div>

      <div className={styles.viewerFrame}>
        {photoUrl ? (
          <img src={photoUrl} alt="Street-level capture" className={styles.photo} />
        ) : (
          <EmptyState icon={KeyIcon}>No photo available for this location.</EmptyState>
        )}

        {currentPhoto && (
          <div className={styles.photoOverlay}>
            <div className={styles.telemetry}>
              {currentPhoto.compass_angle !== undefined && (
                <div className={styles.telemetryItem}>
                  <CompassIcon size={14} />
                  <span>{Math.round(currentPhoto.compass_angle)}°</span>
                </div>
              )}
              {currentPhoto.captured_at && (
                <div className={styles.telemetryItem}>
                  <span>{new Date(currentPhoto.captured_at).toLocaleDateString()}</span>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {photos.length > 1 && (
        <div className={styles.thumbsList}>
          {photos.map((p) => {
            const isSel = selectedPhoto?.id === p.id;
            return (
              <button
                key={p.id}
                type="button"
                className={cx(styles.thumbButton, isSel && styles.thumbButtonSelected)}
                onClick={() => setSelectedPhoto(p)}
              >
                {p.thumb_256_url && (
                  <img src={p.thumb_256_url} alt="" className={styles.thumbImage} />
                )}
              </button>
            );
          })}
        </div>
      )}

      <footer className={styles.attribution}>
        Imagery ©{' '}
        <a href="https://www.mapillary.com" target="_blank" rel="noopener noreferrer">
          Mapillary
        </a>{' '}
        contributors,{' '}
        <a
          href="https://creativecommons.org/licenses/by-sa/4.0/"
          target="_blank"
          rel="noopener noreferrer"
        >
          CC BY-SA 4.0
        </a>
      </footer>
    </div>
  );
}
