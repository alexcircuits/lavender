import { useEffect, useMemo, useReducer, useRef, useState } from 'react';
import type { EvidencePack } from '../../contracts';
import { Button, IconButton, Panel, PauseIcon, PlayIcon, XIcon } from '../../design';
import { useGlobe, useUi, useWorkspace } from '../../runtime';
import { useScenePacks } from './api';
import { type Clock, missingLayers, ScenePlayer, shotFlyTarget, systemClock } from './model';
import styles from './scenes.module.css';

export interface ScenesPanelProps {
  readonly className?: string | undefined;
  /** Packs to show; when omitted they are fetched from GET /api/v1/scenes/packs. */
  readonly packs?: readonly EvidencePack[] | undefined;
  readonly clock?: Clock | undefined;
}

/** Director-led evidence tours (LB-118): pick a pack, play its shots, fly the camera. */
export function ScenesPanel({ className, packs, clock = systemClock }: ScenesPanelProps) {
  const setSidePanel = useUi((s) => s.setSidePanel);
  const loaded = useScenePacks(packs === undefined);
  const list = packs ?? loaded.data ?? [];
  const [packId, setPackId] = useState<string | null>(null);
  const pack = list.find((p) => p.id === packId) ?? null;

  return (
    <Panel
      title="Scenes"
      dense
      className={className}
      actions={<IconButton label="Close scenes" icon={XIcon} onClick={() => setSidePanel(null)} />}
    >
      <div className={styles.container}>
        {pack ? (
          <PackTour key={pack.id} pack={pack} clock={clock} onBack={() => setPackId(null)} />
        ) : packs === undefined && loaded.pending ? (
          <div className={styles.card}>Loading evidence packs…</div>
        ) : packs === undefined && loaded.error ? (
          <div className={styles.card} role="alert">
            Could not load evidence packs: {loaded.error}
          </div>
        ) : list.length === 0 ? (
          <div className={styles.card}>No evidence packs available.</div>
        ) : (
          list.map((p) => (
            <button
              key={p.id}
              type="button"
              className={styles.card}
              onClick={() => setPackId(p.id)}
            >
              <span className={styles.kicker}>{p.region}</span>
              <span className={styles.title}>{p.title}</span>
              <span>{p.summary}</span>
              <span className={styles.meta}>{p.shots.length} shots</span>
            </button>
          ))
        )}
      </div>
    </Panel>
  );
}

function PackTour({
  pack,
  clock,
  onBack,
}: {
  readonly pack: EvidencePack;
  readonly clock: Clock;
  readonly onBack: () => void;
}) {
  const globe = useGlobe();
  const layers = useWorkspace((s) => s.layers);
  const globeRef = useRef(globe);
  globeRef.current = globe;
  const [, rerender] = useReducer((n: number) => n + 1, 0);
  const player = useMemo(
    () => new ScenePlayer(pack, (shot) => globeRef.current?.flyTo(shotFlyTarget(shot)), clock),
    [pack, clock],
  );
  useEffect(() => {
    const off = player.subscribe(rerender);
    return () => {
      off();
      player.dispose();
    };
  }, [player]);

  const { phase, index } = player.state;
  const playing = phase === 'flying' || phase === 'holding';
  const enabled = useMemo(
    () =>
      new Set(
        Object.entries(layers)
          .filter(([, v]) => v?.enabled)
          .map(([k]) => k),
      ),
    [layers],
  );
  const missing = missingLayers(pack, enabled);
  const shot = pack.shots[index];

  return (
    <>
      <div className={styles.controls}>
        <Button onClick={onBack}>All packs</Button>
        <IconButton label="Previous shot" icon={PlayIcon} onClick={() => player.prev()} />
        {playing ? (
          <IconButton label="Pause" icon={PauseIcon} onClick={() => player.pause()} />
        ) : (
          <IconButton label="Play" icon={PlayIcon} onClick={() => player.play()} />
        )}
        <IconButton label="Next shot" icon={PlayIcon} onClick={() => player.next()} />
      </div>
      <div className={styles.card}>
        <span className={styles.kicker}>{pack.region}</span>
        <span className={styles.title}>{pack.title}</span>
        <span>{pack.summary}</span>
        {missing.length > 0 ? (
          <span className={styles.warn}>Layers not enabled: {missing.join(', ')}</span>
        ) : null}
      </div>
      <ol className={styles.shots} aria-label="Shot list">
        {pack.shots.map((s, i) => (
          <li key={s.id}>
            <button
              type="button"
              className={`${styles.card} ${i === index && phase !== 'idle' ? styles.shotActive : ''}`}
              aria-current={i === index && phase !== 'idle' ? 'step' : undefined}
              onClick={() => player.goTo(i)}
            >
              <span className={styles.title}>{s.title}</span>
              <span className={styles.meta}>
                fly {s.durationS}s · hold {s.holdS}s
              </span>
            </button>
          </li>
        ))}
      </ol>
      {shot?.narration && phase !== 'idle' ? (
        <div className={styles.card}>{shot.narration}</div>
      ) : null}
      <div className={styles.meta} aria-live="polite">
        {phase === 'idle' ? 'Ready' : phase}
      </div>
    </>
  );
}
