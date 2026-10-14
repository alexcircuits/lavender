import { useCallback, useEffect, useId, useRef, useState } from 'react';
import type { GeoPoint } from '../../contracts';
import { Button, IconButton, Panel, TextField, XIcon } from '../../design';
import { useGlobe, useUi, useWorkspace } from '../../runtime';
import styles from './annotations.module.css';
import { annotationCentre, geometryError, greatCircleM, measurement } from './geometry';
import {
  type AnnotationDraft,
  MAX_ANNOTATIONS,
  MAX_LABEL_LENGTH,
  MAX_NOTE_LENGTH,
  MAX_VERTICES,
  type MapAnnotation,
  validatePoints,
} from './model';

interface Editing extends AnnotationDraft {
  readonly id: string | null;
  readonly createdAt: string | null;
}
const rounded = (point: GeoPoint): GeoPoint => ({
  lat: Number(point.lat.toFixed(6)),
  lon: Number(point.lon.toFixed(6)),
});
const coordinates = (points: readonly GeoPoint[]) =>
  points.map((p) => `${p.lat}, ${p.lon}`).join('\n');

/** The list provides every geometry and measurement without requiring pointer map input. */
export function AnnotationsPanel({ className }: { readonly className?: string | undefined }) {
  const globe = useGlobe();
  const annotations = useWorkspace((s) => s.annotations);
  const storageError = useWorkspace((s) => s.annotationStorageError);
  const setAnnotations = useWorkspace((s) => s.setAnnotations);
  const claim = useWorkspace((s) => s.claimAnnotations);
  const close = useUi((s) => s.setSidePanel);
  const [draft, setDraft] = useState<Editing | null>(null);
  const [label, setLabel] = useState('');
  const [note, setNote] = useState('');
  const [latitude, setLatitude] = useState('');
  const [longitude, setLongitude] = useState('');
  const [coordinateText, setCoordinateText] = useState('');
  const [error, setError] = useState<string | null>(null);
  const fieldId = useId();

  const addPoint = useCallback((point: GeoPoint | null) => {
    if (!point) {
      setError('Click on the Earth or enter coordinates below.');
      return;
    }
    try {
      validatePoints([point]);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Invalid coordinates.');
      return;
    }
    setDraft((current) => {
      if (!current) return null;
      const p = rounded(point);
      if (current.kind !== 'pin' && current.points.length >= MAX_VERTICES) {
        setError(`Use at most ${MAX_VERTICES} points.`);
        return current;
      }
      const last = current.points.at(-1);
      if (last && current.kind !== 'pin' && greatCircleM(last, p) < 0.5) {
        setError('That point is within 0.5 m of the previous point.');
        return current;
      }
      setError(null);
      const points = current.kind === 'pin' ? [p] : [...current.points, p];
      setCoordinateText(coordinates(points));
      return { ...current, points };
    });
  }, []);

  const seenAnnotations = useRef(annotations);
  useEffect(() => {
    if (seenAnnotations.current !== annotations) {
      seenAnnotations.current = annotations;
      setDraft(null);
      setError(null);
    }
  }, [annotations]);

  useEffect(() => {
    if (!globe) return;
    globe.setAnnotations(annotations, draft);
    globe.setDrawing(draft !== null);
    return () => {
      globe.setDrawing(false);
      globe.setAnnotations(annotations, null);
    };
  }, [globe, annotations, draft]);
  useEffect(() => globe?.onMapPoint(addPoint), [globe, addPoint]);

  const finish = useCallback(() => {
    if (!draft) return;
    const invalid = geometryError(draft.kind, draft.points);
    if (invalid) {
      setError(invalid);
      return;
    }
    try {
      const annotation: MapAnnotation = {
        id: draft.id ?? crypto.randomUUID(),
        kind: draft.kind,
        points: draft.points,
        label: label.trim(),
        note: note.trim(),
        createdAt: draft.createdAt ?? new Date().toISOString(),
        approximate: true,
      };
      setAnnotations(
        draft.id
          ? annotations.map((a) => (a.id === draft.id ? annotation : a))
          : [...annotations, annotation],
      );
      setDraft(null);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'This annotation could not be saved.');
    }
  }, [draft, label, note, annotations, setAnnotations]);
  const undo = useCallback(() => {
    setDraft((current) => {
      if (!current) return null;
      const points = current.points.slice(0, -1);
      setCoordinateText(coordinates(points));
      return { ...current, points };
    });
    setError(null);
  }, []);
  useEffect(() => {
    if (!draft) return;
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        setDraft(null);
        setError(null);
        return;
      }
      if (
        event.target instanceof HTMLElement &&
        (event.target.matches('input, textarea, button, select') || event.target.isContentEditable)
      )
        return;
      if (event.key === 'Enter') {
        event.preventDefault();
        finish();
      }
      if (event.key === 'Backspace') {
        event.preventDefault();
        undo();
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [draft, finish, undo]);

  const begin = (kind: MapAnnotation['kind'], existing?: MapAnnotation) => {
    claim();
    const points = existing?.points ?? [];
    setDraft({ kind, points, id: existing?.id ?? null, createdAt: existing?.createdAt ?? null });
    setLabel(existing?.label ?? '');
    setNote(existing?.note ?? '');
    setCoordinateText(coordinates(points));
    setError(null);
  };

  const applyCoordinates = () => {
    if (!draft) return;
    try {
      const points = validatePoints(
        coordinateText
          .trim()
          .split('\n')
          .filter(Boolean)
          .map((line) => {
            const pair = line.split(',');
            if (pair.length !== 2 || pair.some((value) => value.trim() === ''))
              throw new Error('Enter one latitude, longitude pair per line.');
            return { lat: Number(pair[0]), lon: Number(pair[1]) };
          }),
      ).map(rounded);
      if (draft.kind === 'pin' && points.length > 1)
        throw new Error('A pin needs one coordinate pair.');
      setDraft({ ...draft, points });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Invalid coordinates.');
    }
  };

  const finishProblem = draft ? geometryError(draft.kind, draft.points) : null;

  return (
    <Panel
      title="Annotations"
      dense
      className={className}
      actions={<IconButton label="Close annotations" icon={XIcon} onClick={() => close(null)} />}
    >
      <div className={styles.stack}>
        <p className={styles.help}>
          Your marks stay on this device and travel in copied view links. Measurements are
          approximate surface estimates on a sphere, without terrain.
        </p>
        {storageError ? (
          <p role="alert" className={styles.error}>
            {storageError}
          </p>
        ) : null}
        {!draft ? (
          <div className={styles.actions}>
            {(['pin', 'line', 'area'] as const).map((kind) => (
              <Button
                key={kind}
                variant="secondary"
                disabled={annotations.length >= MAX_ANNOTATIONS}
                onClick={() => begin(kind)}
              >
                New {kind}
              </Button>
            ))}
          </div>
        ) : (
          <div className={styles.editor}>
            <h3>
              {draft.id ? 'Edit' : 'Draw'} {draft.kind}
            </h3>
            <p className={styles.help}>
              Click the map or enter points. Enter finishes, Backspace undoes, Escape cancels when
              drawing on the map.
            </p>
            <p aria-live="polite">
              {draft.points.length} / {MAX_VERTICES} points ·{' '}
              {measurement(draft.kind, draft.points)}
            </p>
            <TextField
              label="Label"
              maxLength={MAX_LABEL_LENGTH}
              value={label}
              onChange={(e) => setLabel(e.target.value)}
            />
            <TextField
              label="Note"
              maxLength={MAX_NOTE_LENGTH}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
            <div className={styles.coordinates}>
              <TextField
                label="Latitude"
                inputMode="decimal"
                mono
                value={latitude}
                onChange={(e) => setLatitude(e.target.value)}
              />
              <TextField
                label="Longitude"
                inputMode="decimal"
                mono
                value={longitude}
                onChange={(e) => setLongitude(e.target.value)}
              />
            </div>
            <Button
              variant="secondary"
              onClick={() => {
                if (!latitude.trim() || !longitude.trim()) {
                  setError('Enter both latitude and longitude.');
                  return;
                }
                addPoint({ lat: Number(latitude), lon: Number(longitude) });
              }}
            >
              Add point
            </Button>
            <label htmlFor={fieldId}>Vertices (latitude, longitude per line)</label>
            <textarea
              id={fieldId}
              className={styles.textarea}
              rows={4}
              maxLength={2_000}
              value={coordinateText}
              onChange={(e) => setCoordinateText(e.target.value)}
            />
            <Button variant="secondary" onClick={applyCoordinates}>
              Apply vertices
            </Button>
            {!error && finishProblem ? <p className={styles.help}>{finishProblem}</p> : null}
            {error ? (
              <p role="alert" className={styles.error}>
                {error}
              </p>
            ) : null}
            <div className={styles.actions}>
              <Button onClick={finish} disabled={finishProblem !== null}>
                Save annotation
              </Button>
              <Button variant="secondary" onClick={undo} disabled={!draft.points.length}>
                Undo point
              </Button>
              <Button
                variant="ghost"
                onClick={() => {
                  setDraft(null);
                  setError(null);
                }}
              >
                Cancel drawing
              </Button>
            </div>
          </div>
        )}
        <p className={styles.help}>
          {annotations.length} / {MAX_ANNOTATIONS} saved annotations
        </p>
        {!annotations.length ? (
          <p>No annotations yet. Add a pin, distance line or area.</p>
        ) : (
          <ol className={styles.list} aria-label="Saved annotations">
            {annotations.map((annotation) => (
              <li key={annotation.id} className={styles.mark}>
                <h3>{annotation.label || `Untitled ${annotation.kind}`}</h3>
                <p>{measurement(annotation.kind, annotation.points)}</p>
                {annotation.note ? <p className={styles.note}>{annotation.note}</p> : null}
                <p className={styles.help}>
                  User geometry · {annotation.kind} ·{' '}
                  <time dateTime={annotation.createdAt}>{annotation.createdAt}</time>
                </p>
                <details>
                  <summary>
                    {annotation.points.length} coordinate{annotation.points.length === 1 ? '' : 's'}
                  </summary>
                  <ol className={styles.vertices}>
                    {annotation.points.map((p, index) => (
                      // biome-ignore lint/suspicious/noArrayIndexKey: static ordered vertex coordinates have no component state and may repeat in a path.
                      <li key={`${index}-${p.lat}-${p.lon}`} className="mono">
                        {p.lat.toFixed(6)}, {p.lon.toFixed(6)}
                      </li>
                    ))}
                  </ol>
                </details>
                <div className={styles.actions}>
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={draft !== null}
                    onClick={() => begin(annotation.kind, annotation)}
                  >
                    Edit {annotation.label || annotation.kind}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={!globe}
                    onClick={() => {
                      const centre = annotationCentre(annotation.points);
                      if (centre) globe?.flyTo({ kind: 'point', position: centre });
                    }}
                  >
                    Locate {annotation.label || annotation.kind}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={draft !== null}
                    onClick={() =>
                      setAnnotations(annotations.filter((a) => a.id !== annotation.id))
                    }
                  >
                    Delete {annotation.label || annotation.kind}
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
