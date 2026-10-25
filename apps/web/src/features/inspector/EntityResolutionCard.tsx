import { useEffect, useState } from 'react';
import type { CanonicalEntity, EntityIdentifier } from '../../contracts';
import { Badge, CopyIcon, IconButton, showToast } from '../../design';
import { formatUtcTime } from '../../lib/time';
import { usePlatform } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import styles from './inspector.module.css';

export interface EntityResolutionCardProps {
  readonly feed: string;
  readonly entityId: string;
}

function resolveScheme(feed: string): string | null {
  switch (feed) {
    case 'aircraft':
      return 'icao24';
    case 'vessels':
      return 'mmsi';
    case 'satellites':
      return 'norad_cat_id';
    default:
      return null;
  }
}

export function EntityResolutionCard({ feed, entityId }: EntityResolutionCardProps) {
  const platform = usePlatform();
  const [loading, setLoading] = useState(false);
  const [entity, setEntity] = useState<CanonicalEntity | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const scheme = resolveScheme(feed);
    if (!scheme || !platform.resolveEntity) {
      setEntity(null);
      setError(null);
      return;
    }

    let active = true;
    setLoading(true);
    setError(null);

    platform
      .resolveEntity(scheme, entityId)
      .then((canonical) => {
        if (!active) return;
        setEntity(canonical);
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (!active) return;
        setLoading(false);
        const msg = err instanceof Error ? err.message : 'Not resolved';
        setError(msg);
      });

    return () => {
      active = false;
    };
  }, [feed, entityId, platform]);

  const scheme = resolveScheme(feed);
  if (!scheme || (!loading && !entity && error)) {
    return null;
  }

  if (loading) {
    return (
      <section className={styles.section} aria-label="Entity Resolution">
        <header className={styles.sectionHeader}>
          <h3>Canonical Resolution</h3>
          <span className={styles.subtext}>Resolving cross-feed graph…</span>
        </header>
      </section>
    );
  }

  if (!entity) return null;

  const copyId = async () => {
    try {
      if (navigator.clipboard) {
        await navigator.clipboard.writeText(entity.id);
        showToast({
          kind: 'success',
          title: 'Canonical ID Copied',
          description: entity.id,
        });
      }
    } catch {
      showToast({
        kind: 'error',
        title: 'Copy Failed',
        description: entity.id,
      });
    }
  };

  const confidencePct = Math.round(entity.confidence * 100);

  const facts: Fact[] = [
    { label: 'Canonical ID', value: entity.id, mono: true },
    { label: 'Primary Entity', value: entity.primaryLabel },
    { label: 'Entity Kind', value: entity.kind },
    { label: 'Resolution Confidence', value: `${confidencePct}% deterministic` },
    ...(entity.firstSeen > 0n
      ? [{ label: 'First Seen', value: formatUtcTime(Number(entity.firstSeen) * 1000), mono: true }]
      : []),
    ...(entity.lastSeen > 0n
      ? [{ label: 'Last Seen', value: formatUtcTime(Number(entity.lastSeen) * 1000), mono: true }]
      : []),
  ];

  return (
    <section className={styles.section} aria-label="Entity Resolution">
      <header className={styles.sectionHeader}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            width: '100%',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <h3>Canonical Identity</h3>
            <Badge>{entity.kind}</Badge>
          </div>
          <IconButton
            size="sm"
            label="Copy canonical ID"
            icon={CopyIcon}
            onClick={() => void copyId()}
          />
        </div>
      </header>

      <FactSheet label="Canonical Entity Details" facts={facts} />

      {entity.identifiers.length > 0 && (
        <div style={{ marginTop: '12px' }}>
          <div
            style={{
              fontSize: '11px',
              color: 'var(--ink-2)',
              marginBottom: '6px',
              fontWeight: 600,
            }}
          >
            KNOWN IDENTIFIERS & ALIASES ({entity.identifiers.length})
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
            {entity.identifiers.map((ident: EntityIdentifier) => (
              <span
                key={`${ident.scheme}:${ident.value}`}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '4px',
                  background: 'var(--control)',
                  border: 'none',
                  borderRadius: 'var(--r-xs)',
                  padding: '2px 8px',
                  fontSize: '11px',
                  fontFamily: 'var(--font-mono)',
                }}
              >
                <span
                  style={{ color: 'var(--ink-2)', textTransform: 'uppercase', fontSize: '9px' }}
                >
                  {ident.scheme}:
                </span>
                <span style={{ color: 'var(--ink)', fontWeight: 500 }}>{ident.value}</span>
              </span>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}
