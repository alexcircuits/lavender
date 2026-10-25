import { useCallback } from 'react';
import type { BBox } from '../../contracts';
import {
  Button,
  CrosshairIcon,
  cx,
  EmptyState,
  ErrorState,
  Panel,
  PlanetIcon,
  Skeleton,
  SquareIcon,
  TrashIcon,
} from '../../design';
import type { ImageryCandidate } from '../../platform/recentImageryTypes';
import { usePlatform, useUi } from '../../runtime';
import { type ImageryMode, useImageryStore } from './imageryStore';
import styles from './recentImagery.module.css';

export interface RecentImageryPanelProps {
  readonly className?: string | undefined;
}

const DAYS_OPTIONS = [3, 7, 14, 30] as const;

export function RecentImageryPanel({ className }: RecentImageryPanelProps) {
  const platform = usePlatform();
  const globe = useUi((s) => s.globe);

  const box = useImageryStore((s) => s.box);
  const days = useImageryStore((s) => s.days);
  const mode = useImageryStore((s) => s.mode);
  const opacity = useImageryStore((s) => s.opacity);
  const slotA = useImageryStore((s) => s.slotA);
  const slotB = useImageryStore((s) => s.slotB);
  const candidates = useImageryStore((s) => s.candidates);
  const tileTemplates = useImageryStore((s) => s.tileTemplates);
  const rankedLatest = useImageryStore((s) => s.rankedLatest);
  const attribution = useImageryStore((s) => s.attribution);
  const isSearching = useImageryStore((s) => s.isSearching);
  const isDrawingBox = useImageryStore((s) => s.isDrawingBox);
  const error = useImageryStore((s) => s.error);

  const setBox = useImageryStore((s) => s.setBox);
  const setDays = useImageryStore((s) => s.setDays);
  const setMode = useImageryStore((s) => s.setMode);
  const setOpacity = useImageryStore((s) => s.setOpacity);
  const setCandidates = useImageryStore((s) => s.setCandidates);
  const setIsSearching = useImageryStore((s) => s.setIsSearching);
  const setIsDrawingBox = useImageryStore((s) => s.setIsDrawingBox);
  const setError = useImageryStore((s) => s.setError);
  const clear = useImageryStore((s) => s.clear);

  const executeSearch = useCallback(
    async (targetBox: BBox, searchDays: number) => {
      setIsSearching(true);
      setError(null);
      try {
        const res = await platform.recentImagery(targetBox, searchDays);
        setCandidates(
          res.candidates,
          res.tileTemplates ?? {},
          res.rankedLatest,
          res.attribution?.statement ?? null,
        );

        // Pick recommended candidate or first for Slot A
        const pickA = res.rankedLatest?.candidate ?? res.candidates[0] ?? null;
        if (pickA) {
          useImageryStore.getState().setSlotA(pickA);
          globe?.setRecentImagerySlot('a', {
            candidate: pickA,
            box: targetBox,
            tileTemplate: res.tileTemplates?.[pickA.product],
          });
        }

        // Pick alternative candidate for Slot B if in A/B mode
        if (res.candidates.length > 1) {
          const pickB =
            res.candidates.find((c) => c.key !== pickA?.key) ?? res.candidates[1] ?? null;
          if (pickB) {
            useImageryStore.getState().setSlotB(pickB);
            globe?.setRecentImagerySlot('b', {
              candidate: pickB,
              box: targetBox,
              tileTemplate: res.tileTemplates?.[pickB.product],
            });
          }
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setIsSearching(false);
      }
    },
    [platform, globe, setCandidates, setIsSearching, setError],
  );

  const onToggleDrawBox = () => {
    if (isDrawingBox) {
      globe?.cancelImageryBoxDrawing();
      setIsDrawingBox(false);
    } else {
      setIsDrawingBox(true);
      globe?.startImageryBoxDrawing(
        (drawnBox) => {
          setIsDrawingBox(false);
          setBox(drawnBox);
          globe.setRecentImageryBox(drawnBox);
          void executeSearch(drawnBox, days);
        },
        () => {
          setIsDrawingBox(false);
        },
      );
    }
  };

  const onUseViewport = () => {
    const center = globe?.centrePosition() ?? { lon: -97.7431, lat: 30.2672 };
    const span = 0.1; // ~20 km box
    const viewportBox: BBox = {
      west: center.lon - span,
      south: center.lat - span,
      east: center.lon + span,
      north: center.lat + span,
    };
    setBox(viewportBox);
    globe?.setRecentImageryBox(viewportBox);
    void executeSearch(viewportBox, days);
  };

  const onClearAll = () => {
    globe?.cancelImageryBoxDrawing();
    globe?.setRecentImageryBox(null);
    globe?.setRecentImagerySlot('a', null);
    globe?.setRecentImagerySlot('b', null);
    clear();
  };

  const onSelectSlot = (slot: 'a' | 'b', candidate: ImageryCandidate) => {
    if (!box) return;
    if (slot === 'a') {
      useImageryStore.getState().setSlotA(candidate);
      globe?.setRecentImagerySlot('a', {
        candidate,
        box,
        tileTemplate: tileTemplates[candidate.product],
      });
    } else {
      useImageryStore.getState().setSlotB(candidate);
      globe?.setRecentImagerySlot('b', {
        candidate,
        box,
        tileTemplate: tileTemplates[candidate.product],
      });
    }
  };

  const onModeChange = (nextMode: ImageryMode) => {
    setMode(nextMode);
    globe?.setRecentImageryMode(nextMode);
  };

  const onOpacityChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = Number.parseFloat(e.target.value);
    setOpacity(val);
    globe?.setRecentImageryOpacity(val);
  };

  return (
    <Panel title="Recent satellite imagery" dense className={cx(styles.panel, className)}>
      {/* Box Selection */}
      <section className={styles.section}>
        <h4 className={styles.sectionTitle}>Area of Interest</h4>
        <div className={styles.boxControls}>
          <Button
            variant={isDrawingBox ? 'primary' : 'secondary'}
            icon={SquareIcon}
            onClick={onToggleDrawBox}
          >
            {isDrawingBox ? 'Cancel Draw' : 'Draw Box'}
          </Button>
          <Button variant="secondary" icon={CrosshairIcon} onClick={onUseViewport}>
            Current View
          </Button>
          {box && (
            <Button variant="ghost" icon={TrashIcon} onClick={onClearAll} title="Clear Area">
              Clear
            </Button>
          )}
        </div>

        {isDrawingBox && (
          <p className={styles.hint}>Click and drag on the globe to draw your bounding box.</p>
        )}

        {box && (
          <div className={styles.boxInfo}>
            <div className={styles.boxCoords}>
              <span>
                W: {box.west.toFixed(3)}°, S: {box.south.toFixed(3)}°
              </span>
              <span>
                E: {box.east.toFixed(3)}°, N: {box.north.toFixed(3)}°
              </span>
            </div>
          </div>
        )}
      </section>

      {/* Days & Search */}
      <section className={styles.section}>
        <h4 className={styles.sectionTitle}>Time Range</h4>
        <div className={styles.daysSelector}>
          {DAYS_OPTIONS.map((d) => (
            <button
              key={d}
              type="button"
              className={cx(styles.dayPill, days === d && styles.dayPillActive)}
              onClick={() => {
                setDays(d);
                if (box) void executeSearch(box, d);
              }}
            >
              {d} days
            </button>
          ))}
        </div>
      </section>

      {/* Display Mode */}
      {candidates.length > 0 && (
        <section className={styles.section}>
          <h4 className={styles.sectionTitle}>Comparison Mode</h4>
          <div className={styles.modeSelector}>
            <button
              type="button"
              className={cx(styles.modeButton, mode === 'image' && styles.modeButtonActive)}
              onClick={() => onModeChange('image')}
            >
              Image
            </button>
            <button
              type="button"
              className={cx(styles.modeButton, mode === 'basemap' && styles.modeButtonActive)}
              onClick={() => onModeChange('basemap')}
            >
              vs Basemap
            </button>
            <button
              type="button"
              className={cx(styles.modeButton, mode === 'ab' && styles.modeButtonActive)}
              onClick={() => onModeChange('ab')}
            >
              A / B Swipe
            </button>
          </div>

          <div className={styles.sliderRow}>
            <div className={styles.sliderHeader}>
              <span>Layer Opacity</span>
              <span>{Math.round(opacity * 100)}%</span>
            </div>
            <input
              type="range"
              min="0.1"
              max="1.0"
              step="0.05"
              value={opacity}
              onChange={onOpacityChange}
              className={styles.slider}
              aria-label="Layer opacity"
            />
          </div>
        </section>
      )}

      {/* Start Here Recommendation */}
      {rankedLatest?.candidate && (
        <div className={styles.startHereBanner}>
          <div className={styles.startHereHeader}>
            <span>Recommended Pass</span>
            <span className={styles.productTag}>{rankedLatest.candidate.product}</span>
          </div>
          <div className={styles.candidateRow}>
            <span className={styles.candidateDay}>{rankedLatest.candidate.day}</span>
            <span className={cx(styles.candidateCloud, styles.cloudLow)}>
              {rankedLatest.candidate.cloud
                ? `${rankedLatest.candidate.cloud.min.toFixed(0)}% cloud`
                : 'Clear sky'}
            </span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                const c = rankedLatest.candidate;
                if (c) onSelectSlot('a', c);
              }}
            >
              Apply
            </Button>
          </div>
        </div>
      )}

      {/* Candidate List */}
      <section className={styles.section}>
        <h4 className={styles.sectionTitle}>
          Available Passes {candidates.length > 0 && `(${candidates.length})`}
        </h4>

        {isSearching && (
          <div className={styles.candidatesList}>
            <Skeleton width="100%" height="48px" shape="block" />
            <Skeleton width="100%" height="48px" shape="block" />
            <Skeleton width="100%" height="48px" shape="block" />
          </div>
        )}

        {error && (
          <ErrorState
            live
            title="Search failed"
            action={
              box ? (
                <Button variant="secondary" onClick={() => executeSearch(box, days)}>
                  Retry
                </Button>
              ) : undefined
            }
          >
            {error}
          </ErrorState>
        )}

        {!isSearching && !error && candidates.length === 0 && (
          <EmptyState icon={PlanetIcon}>
            {box
              ? 'No satellite passes found for this area and time window.'
              : 'Draw an area or use the current view to search recent satellite passes.'}
          </EmptyState>
        )}

        {!isSearching && candidates.length > 0 && (
          <div className={styles.candidatesList}>
            {candidates.map((c) => {
              const isSelectedA = slotA?.key === c.key;
              const isSelectedB = slotB?.key === c.key;
              const cloudVal = c.cloud?.min ?? 0;
              const cloudClass =
                cloudVal < 20
                  ? styles.cloudLow
                  : cloudVal < 50
                    ? styles.cloudMid
                    : styles.cloudHigh;

              const prodClass =
                c.product === 'S30'
                  ? styles.productS30
                  : c.product === 'L30'
                    ? styles.productL30
                    : styles.productVIIRS;

              return (
                <div
                  key={c.key}
                  className={cx(
                    styles.candidateCard,
                    isSelectedA && styles.candidateCardSelectedA,
                    isSelectedB && styles.candidateCardSelectedB,
                  )}
                >
                  <div className={styles.candidateRow}>
                    <div className={styles.candidateInfo}>
                      <span className={cx(styles.productTag, prodClass)}>{c.product}</span>
                      <span className={styles.candidateDay}>{c.day}</span>
                    </div>
                    <div className={styles.slotButtons}>
                      <button
                        type="button"
                        className={cx(styles.slotButton, isSelectedA && styles.slotButtonActiveA)}
                        onClick={() => onSelectSlot('a', c)}
                      >
                        {isSelectedA ? 'Slot A ✓' : 'Set A'}
                      </button>
                      {mode === 'ab' && (
                        <button
                          type="button"
                          className={cx(styles.slotButton, isSelectedB && styles.slotButtonActiveB)}
                          onClick={() => onSelectSlot('b', c)}
                        >
                          {isSelectedB ? 'Slot B ✓' : 'Set B'}
                        </button>
                      )}
                    </div>
                  </div>
                  <div className={styles.candidateRow}>
                    <span className={cx(styles.candidateCloud, cloudClass)}>
                      {c.cloud
                        ? `${c.cloud.min.toFixed(0)}%–${c.cloud.max.toFixed(0)}% cloud`
                        : 'Daily mosaic'}
                    </span>
                    <span className={styles.hint}>{c.granuleCount} granules</span>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* NASA Attribution Notice */}
      {attribution && <footer className={styles.attributionCard}>{attribution}</footer>}
    </Panel>
  );
}
