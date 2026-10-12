import { useId, useMemo } from 'react';
import type { Catalog, ProviderHealth, SourceInfo } from '../../contracts';
import {
  ArrowSquareOutIcon,
  CircleIcon,
  formatAge,
  type Icon,
  IconButton,
  InfoIcon,
  KeyIcon,
  LockSimpleIcon,
  Panel,
  SquareIcon,
  TriangleIcon,
  WarningIcon,
  XIcon,
} from '../../design';
import { safeHttpsUrl } from '../../lib/safeUrl';
import { parseTime, secondsBetween } from '../../lib/time';
import { useNow } from '../../lib/useNow';
import { useSystem, useUi } from '../../runtime';
import {
  commercialWords,
  type HealthTone,
  healthSpec,
  licenceModeNotice,
  licencePolicyReason,
  sourceUse,
  UNKNOWN_HEALTH,
} from './describe';
import styles from './sources.module.css';

const TONE_GLYPH: Readonly<Record<HealthTone, { icon: Icon; weight: 'fill' | 'bold' }>> = {
  good: { icon: CircleIcon, weight: 'fill' },
  warning: { icon: TriangleIcon, weight: 'fill' },
  alert: { icon: SquareIcon, weight: 'fill' },
  neutral: { icon: CircleIcon, weight: 'bold' },
};

function HealthBadge({ health }: { health: ProviderHealth | undefined }) {
  const spec = health ? healthSpec(health.state) : UNKNOWN_HEALTH;
  const glyph = TONE_GLYPH[spec.tone];
  const Glyph = glyph.icon;
  return (
    <span className={styles.health} data-tone={spec.tone}>
      <Glyph size={10} weight={glyph.weight} aria-hidden />
      {spec.word}
    </span>
  );
}

function LastSuccess({ at }: { at: string | undefined }) {
  const now = useNow();
  const ms = parseTime(at);
  return <>{ms === null ? 'Never' : `${formatAge(secondsBetween(ms, now))} ago`}</>;
}

/** An https link, or the raw text when the provider gave something else. */
function SafeLink({ href, children }: { href: string; children: string }) {
  const safe = safeHttpsUrl(href);
  if (!safe) return <span className={styles.unlinked}>{children}</span>;
  return (
    <a className={styles.link} href={safe} target="_blank" rel="noopener noreferrer">
      {children}
      <ArrowSquareOutIcon size={12} weight="bold" aria-hidden />
      <span className="visually-hidden"> (opens in a new tab)</span>
    </a>
  );
}

function hostOf(url: string): string {
  const safe = safeHttpsUrl(url);
  return safe ? new URL(safe).hostname : url;
}

interface SourceCardProps {
  readonly source: SourceInfo;
  readonly catalog: Catalog;
  readonly health: ProviderHealth | undefined;
}

function SourceCard({ source, catalog, health }: SourceCardProps) {
  const headingId = useId();
  const use = sourceUse(source, catalog.layers);
  return (
    <li className={styles.item}>
      <article className={styles.source} aria-labelledby={headingId} data-source={source.id}>
        <header className={styles.sourceHeader}>
          <h3 id={headingId} className={styles.name}>
            {source.name}
          </h3>
          <HealthBadge health={health} />
        </header>

        <div className={styles.attributionBlock}>
          <span className="visually-hidden">Attribution: </span>
          <p className={styles.attribution} data-attribution="">
            {source.attribution}
          </p>
        </div>

        {use.licenceDisabled ? (
          <p className={styles.note} data-kind="licence">
            <LockSimpleIcon size={14} weight="fill" aria-hidden className={styles.noteIcon} />
            {licencePolicyReason(source)}
          </p>
        ) : null}
        {use.missing.length > 0 ? (
          <p className={styles.note} data-kind="config">
            <KeyIcon size={14} weight="fill" aria-hidden className={styles.noteIcon} />
            Not configured: the server needs {use.missing.join(', ')}. Add it to the server’s
            configuration, then restart it.
          </p>
        ) : null}

        <dl className={styles.facts}>
          <div>
            <dt>Homepage</dt>
            <dd>
              <SafeLink href={source.homepage}>{hostOf(source.homepage)}</SafeLink>
            </dd>
          </div>
          <div>
            <dt>Licence</dt>
            <dd>
              <SafeLink href={source.licence.url}>{source.licence.name}</SafeLink>
            </dd>
          </div>
          <div>
            <dt>Commercial use</dt>
            <dd>{commercialWords(source.commercial)}</dd>
          </div>
          <div>
            <dt>Key</dt>
            <dd>{source.requiresKey ? 'Needs a key' : 'No key needed'}</dd>
          </div>
          <div>
            <dt>Privacy</dt>
            <dd>
              {source.privacy === 'sensitive' ? (
                <span className={styles.sensitive}>
                  <WarningIcon
                    size={12}
                    weight="fill"
                    aria-hidden
                    className={styles.sensitiveIcon}
                  />
                  Sensitive: off by default
                </span>
              ) : (
                'Normal'
              )}
            </dd>
          </div>
          {use.layers.length > 0 ? (
            <div>
              <dt>Used by</dt>
              <dd>{use.layers.map((l) => l.name).join(', ')}</dd>
            </div>
          ) : null}
          <div>
            <dt>Last success</dt>
            <dd>{health ? <LastSuccess at={health.lastSuccess} /> : 'Unknown'}</dd>
          </div>
          {health?.lastError ? (
            <div>
              <dt>Last error</dt>
              <dd className={styles.error}>
                {health.lastError}
                {health.consecutiveFailures > 1
                  ? ` (${health.consecutiveFailures} failures in a row)`
                  : ''}
              </dd>
            </div>
          ) : null}
          {source.notes ? (
            <div>
              <dt>Notes</dt>
              <dd>{source.notes}</dd>
            </div>
          ) : null}
        </dl>
      </article>
    </li>
  );
}

export interface SourcesPanelProps {
  readonly catalog: Catalog;
  readonly className?: string | undefined;
}

/**
 * Sources and attribution (product spec §5.6): every registered provider with its mandated
 * attribution verbatim, licence, commercial terms, key and privacy, and its live health.
 */
export function SourcesPanel({ catalog, className }: SourcesPanelProps) {
  const status = useSystem((s) => s.status);
  const setSidePanel = useUi((s) => s.setSidePanel);
  const mode = status?.licenceMode ?? catalog.licenceMode;
  const notice = licenceModeNotice(mode);
  const health = useMemo(
    () => new Map((status?.sources ?? []).map((h) => [h.sourceId, h])),
    [status],
  );

  return (
    <Panel
      title="Sources"
      dense
      className={className}
      actions={<IconButton label="Close sources" icon={XIcon} onClick={() => setSidePanel(null)} />}
    >
      <section className={styles.mode} aria-label="Licence mode" data-mode={mode}>
        <InfoIcon size={16} weight="fill" aria-hidden className={styles.modeIcon} />
        <div>
          <p className={styles.modeTitle}>{notice.title}</p>
          <p className={styles.modeBody}>{notice.body}</p>
        </div>
      </section>
      {catalog.sources.length === 0 ? (
        <p className={styles.empty}>This server has no registered sources.</p>
      ) : (
        <ul className={styles.list} aria-label="Data sources">
          {catalog.sources.map((source) => (
            <SourceCard
              key={source.id}
              source={source}
              catalog={catalog}
              health={health.get(source.id)}
            />
          ))}
        </ul>
      )}
    </Panel>
  );
}
