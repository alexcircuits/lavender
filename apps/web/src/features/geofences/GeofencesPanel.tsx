import { useCallback, useEffect, useState } from 'react';
import type { GeoPoint } from '../../contracts';
import { Button, IconButton, Panel, TextField, XIcon } from '../../design';
import { useGlobe, useRuntime, useUi, useWorkspace } from '../../runtime';
import styles from '../annotations/annotations.module.css';
import { annotationCentre, measurement } from '../annotations/geometry';
import { visiblePopupOwnsKeys } from '../annotations/keyboardOwnership';
import { MAX_ANNOTATIONS, MAX_LABEL_LENGTH, type MapAnnotation } from '../annotations/model';
import { GLOBE_REGION_ID } from '../commands';
import {
  canCloseGeofence,
  type GeofenceDrawMode,
  type GeofenceDrawState,
  geofenceDrawReducer,
  geofencePoints,
  geofencePreviewPoints,
} from './draw';

const rounded = (point: GeoPoint): GeoPoint => ({
  lat: Number(point.lat.toFixed(6)),
  lon: Number(point.lon.toFixed(6)),
});

/**
 * Draws named geofence regions on the globe and keeps them with the workspace annotations.
 * Polygon mode collects clicked vertices (double-click or Enter closes); box mode is two
 * clicks for the opposite corners. Saved fences are session-toggleable and persist with the
 * workspace, so they survive reloads and travel in share links.
 */
export function GeofencesPanel({ className }: { readonly className?: string | undefined }) {
  const globe = useGlobe();
  const runtime = useRuntime();
  const annotations = useWorkspace((s) => s.annotations);
  const setAnnotations = useWorkspace((s) => s.setAnnotations);
  const hidden = useWorkspace((s) => s.hiddenGeofences);
  const toggleHidden = useWorkspace((s) => s.toggleGeofenceVisibility);
  const claim = useWorkspace((s) => s.claimAnnotations);
  const close = useUi((s) => s.setSidePanel);
  const [draw, setDraw] = useState<GeofenceDrawState | null>(null);
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);

  const fences = annotations.filter((a) => a.kind === 'geofence');

  const dispatch = useCallback((event: Parameters<typeof geofenceDrawReducer>[1]) => {
    setDraw((current) => geofenceDrawReducer(current, event));
  }, []);

  // Map clicks become vertices while a draw is active.
  useEffect(
    () =>
      globe?.onMapPoint((point) => {
        if (!point) return;
        setDraw((current) =>
          geofenceDrawReducer(current, { type: 'point', point: rounded(point), at: Date.now() }),
        );
      }),
    [globe],
  );

  const cancel = useCallback(() => {
    setDraw(null);
    setName('');
    setError(null);
  }, []);

  const undo = useCallback(() => dispatch({ type: 'undo' }), [dispatch]);

  const closeDraw = useCallback(() => {
    setDraw((current) => {
      if (!canCloseGeofence(current)) return current;
      return geofenceDrawReducer(current, { type: 'close' });
    });
  }, []);

  // Keyboard follows the annotations panel's capture-phase pattern: Escape cancels while
  // drawing, Enter closes the geometry (then the naming form owns the keys), Backspace undoes.
  useEffect(() => {
    if (!draw || draw.closed) return;
    const key = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      const ui = runtime.stores.ui.getState();
      if (ui.paletteOpen || ui.shortcutsOpen) return;
      if (visiblePopupOwnsKeys()) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        cancel();
        return;
      }
      if (
        !(event.target instanceof HTMLElement) ||
        !event.target.closest(`#${GLOBE_REGION_ID}`) ||
        event.target.isContentEditable ||
        event.target.closest('input, textarea, button, select, a, summary, [role="button"]')
      )
        return;
      if (event.key === 'Enter') {
        event.preventDefault();
        event.stopPropagation();
        closeDraw();
      }
      if (event.key === 'Backspace') {
        event.preventDefault();
        event.stopPropagation();
        undo();
      }
    };
    window.addEventListener('keydown', key, { capture: true });
    return () => window.removeEventListener('keydown', key, { capture: true });
  }, [draw, cancel, closeDraw, undo, runtime]);

  // Saved fences render with the other annotations; hidden ones are a session choice only.
  const visibleAnnotations = annotations.filter((a) => !hidden.has(a.id));
  useEffect(() => {
    if (!globe) return;
    try {
      globe.setAnnotations(
        visibleAnnotations,
        draw
          ? { kind: 'geofence', points: geofencePreviewPoints(draw), cursor: draw.cursor }
          : null,
      );
      globe.setDrawing(draw !== null && !draw.closed);
      return () => {
        globe.setDrawing(false);
      };
    } catch (e) {
      console.error('Globe could not preview geofences', e);
      setError('The globe could not draw this geofence.');
      return;
    }
  }, [globe, visibleAnnotations, draw]);

  useEffect(
    () => () => {
      try {
        globe?.setAnnotations(annotations, null);
        globe?.setDrawing(false);
      } catch (e) {
        console.error('Globe could not clear geofence preview', e);
      }
    },
    [globe, annotations],
  );

  const begin = (mode: GeofenceDrawMode) => {
    claim();
    setName('');
    setError(null);
    dispatch({ type: 'start', mode });
  };

  const save = () => {
    if (!draw) return;
    const points = geofencePoints(draw);
    if (!points) {
      setError('Close the geometry before saving.');
      return;
    }
    const annotation: MapAnnotation = {
      id: crypto.randomUUID(),
      kind: 'geofence',
      points,
      label: name.trim(),
      note: '',
      createdAt: new Date().toISOString(),
      approximate: true,
    };
    try {
      setAnnotations([...annotations, annotation]);
      cancel();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'This geofence could not be saved.');
    }
  };

  return (
    <Panel
      title="Geofences"
      dense
      className={className}
      actions={<IconButton label="Close geofences" icon={XIcon} onClick={() => close(null)} />}
    >
      <div className={styles.stack}>
        <p className={styles.help}>
          Named regions you draw on the globe. Polygon: click vertices, double-click or Enter to
          close. Box: click two opposite corners. Fences persist with this workspace and travel in
          share links.
        </p>
        {error ? (
          <p role="alert" className={styles.error}>
            {error}
          </p>
        ) : null}
        {!draw ? (
          <div className={styles.actions}>
            <Button
              variant="secondary"
              disabled={annotations.length >= MAX_ANNOTATIONS}
              onClick={() => begin('polygon')}
            >
              Draw polygon
            </Button>
            <Button
              variant="secondary"
              disabled={annotations.length >= MAX_ANNOTATIONS}
              onClick={() => begin('box')}
            >
              Draw box
            </Button>
          </div>
        ) : draw.closed ? (
          <div className={styles.editor}>
            <h3>Name this geofence</h3>
            <p aria-live="polite">
              {draw.points.length} vertices · {measurement('geofence', draw.points)}
            </p>
            <TextField
              label="Name"
              maxLength={MAX_LABEL_LENGTH}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <div className={styles.actions}>
              <Button onClick={save}>Save geofence</Button>
              <Button variant="ghost" onClick={cancel}>
                Discard
              </Button>
            </div>
          </div>
        ) : (
          <div className={styles.editor}>
            <h3>
              Drawing {draw.mode === 'box' ? 'box' : 'polygon'} · {draw.points.length} vertices
            </h3>
            <p className={styles.help}>
              Click the globe to add vertices. Enter closes, Backspace undoes the last vertex,
              Escape cancels.
            </p>
            <div className={styles.actions}>
              <Button onClick={closeDraw} disabled={!canCloseGeofence(draw)}>
                Close shape
              </Button>
              <Button variant="secondary" onClick={undo} disabled={!draw.points.length}>
                Undo vertex
              </Button>
              <Button variant="ghost" onClick={cancel}>
                Cancel
              </Button>
            </div>
          </div>
        )}
        <p className={styles.help}>
          {fences.length} / {MAX_ANNOTATIONS} saved annotations (all kinds share the cap)
        </p>
        {!fences.length ? (
          <p>No geofences yet. Draw one to monitor a region.</p>
        ) : (
          <ol className={styles.list} aria-label="Saved geofences">
            {fences.map((fence) => (
              <li key={fence.id} className={styles.mark}>
                <h3>{fence.label || 'Untitled geofence'}</h3>
                <p>{measurement('geofence', fence.points)}</p>
                <p className={styles.help}>
                  {fence.points.length} vertices ·{' '}
                  <time dateTime={fence.createdAt}>{fence.createdAt}</time>
                </p>
                <div className={styles.actions}>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => toggleHidden(fence.id)}
                    disabled={draw !== null}
                  >
                    {hidden.has(fence.id) ? 'Show' : 'Hide'}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={!globe}
                    onClick={() => {
                      const centre = annotationCentre(fence.points);
                      if (centre) globe?.flyTo({ kind: 'point', position: centre });
                    }}
                  >
                    Locate
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={draw !== null}
                    onClick={() => setAnnotations(annotations.filter((a) => a.id !== fence.id))}
                  >
                    Delete
                  </Button>
                </div>
              </li>
            ))}
          </ol>
        )}
      </div>
    </Panel>
  );
}
