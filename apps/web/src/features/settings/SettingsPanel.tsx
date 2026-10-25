import { useEffect, useState } from 'react';
import { Badge, Button, cx, Panel, SegmentedControl, Toggle } from '../../design';
import type { SecretStatusResponse } from '../../platform/types';
import { usePlatform, usePreferences, useSystem } from '../../runtime';
import type {
  CoordinateFormat,
  MotionPreference,
  UnitSystem,
  VisualPreset,
} from '../../state/preferencesStore';
import styles from './settings.module.css';

interface SecretDefinition {
  readonly id: string;
  readonly label: string;
  readonly envVar: string;
}

const KNOWN_SECRETS: readonly SecretDefinition[] = [
  { id: 'firms', label: 'NASA FIRMS map key', envVar: 'LAVENDER_FIRMS_MAP_KEY' },
  { id: 'aisstream', label: 'AISStream API key', envVar: 'LAVENDER_AISSTREAM_API_KEY' },
  { id: 'opensky_id', label: 'OpenSky client ID', envVar: 'LAVENDER_OPENSKY_CLIENT_ID' },
  {
    id: 'opensky_secret',
    label: 'OpenSky client secret',
    envVar: 'LAVENDER_OPENSKY_CLIENT_SECRET',
  },
  { id: 'll2', label: 'Launch Library 2 API token', envVar: 'LAVENDER_LL2_API_TOKEN' },
  { id: 'mapillary', label: 'Mapillary client token', envVar: 'LAVENDER_MAPILLARY_CLIENT_TOKEN' },
  { id: 'tomtom', label: 'TomTom API key', envVar: 'LAVENDER_TOMTOM_API_KEY' },
];

export interface SettingsPanelProps {
  readonly className?: string | undefined;
}

export function SettingsPanel({ className }: SettingsPanelProps) {
  const preferences = usePreferences((s) => s);
  const platform = usePlatform();
  const catalog = useSystem((s) => s.catalog);

  const [secretInfo, setSecretInfo] = useState<SecretStatusResponse | null>(null);
  const [draftKeys, setDraftKeys] = useState<Record<string, string>>({});
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'all' | 'display' | 'units' | 'keys' | 'about'>('all');

  useEffect(() => {
    let unmounted = false;
    if (platform.secretStatus) {
      platform
        .secretStatus()
        .then((res) => {
          if (!unmounted) setSecretInfo(res);
        })
        .catch(() => {
          // Platform or backend does not expose secret status
        });
    }
    return () => {
      unmounted = true;
    };
  }, [platform]);

  const handleSaveKey = async (id: string) => {
    const val = draftKeys[id]?.trim();
    if (!val || !platform.setSecret) return;
    setBusyKey(id);
    try {
      await platform.setSecret(id, val);
      setDraftKeys((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      if (platform.secretStatus) {
        const updated = await platform.secretStatus();
        setSecretInfo(updated);
      }
    } finally {
      setBusyKey(null);
    }
  };

  const handleClearKey = async (id: string) => {
    if (!platform.clearSecret) return;
    setBusyKey(id);
    try {
      await platform.clearSecret(id);
      if (platform.secretStatus) {
        const updated = await platform.secretStatus();
        setSecretInfo(updated);
      }
    } finally {
      setBusyKey(null);
    }
  };

  const imageryList = catalog?.imagery ?? [];

  return (
    <Panel title="Settings" dense className={cx(styles.panel, className)}>
      <div className={styles.viewTabs}>
        <SegmentedControl
          aria-label="Settings section"
          options={[
            { value: 'all', label: 'All' },
            { value: 'display', label: 'Display' },
            { value: 'units', label: 'Units' },
            { value: 'keys', label: 'Keys' },
            { value: 'about', label: 'About' },
          ]}
          value={activeTab}
          onValueChange={(v) => setActiveTab(v as typeof activeTab)}
        />
      </div>

      {/* 1. Display & Imagery */}
      <section
        className={styles.section}
        aria-labelledby="display-heading"
        hidden={activeTab !== 'all' && activeTab !== 'display'}
      >
        <div className={styles.sectionHeader}>
          <h3 id="display-heading" className={styles.sectionTitle}>
            Display & Imagery
          </h3>
          <p className={styles.sectionDescription}>
            Configure base cartography, tactical optics, animation intensity, and spatial rendering
            acceleration.
          </p>
        </div>

        <div className={styles.settingRow}>
          <div className={styles.settingHeader}>
            <span className={styles.settingLabel}>Tactical Optics Preset</span>
            <span className={styles.settingSubtext}>GPU compositor post-processing</span>
          </div>
          <SegmentedControl
            aria-label="Tactical optics preset"
            options={[
              { value: 'normal', label: 'True Color' },
              { value: 'noir', label: 'Noir' },
              { value: 'thermal', label: 'FLIR' },
              { value: 'nvg', label: 'NVG' },
              { value: 'crt', label: 'CRT' },
            ]}
            value={preferences.visualPreset}
            onValueChange={(val) => preferences.setVisualPreset(val as VisualPreset)}
          />
        </div>

        {imageryList.length > 0 && (
          <div className={styles.settingRow}>
            <div className={styles.settingHeader}>
              <label htmlFor="default-imagery" className={styles.settingLabel}>
                Default Base Imagery
              </label>
            </div>
            <select
              id="default-imagery"
              className={styles.select}
              value={preferences.defaultImagery ?? imageryList[0]?.id ?? ''}
              onChange={(e) => preferences.setDefaultImagery(e.target.value)}
            >
              {imageryList.map((img) => (
                <option key={img.id} value={img.id}>
                  {img.name}
                </option>
              ))}
            </select>
          </div>
        )}

        <div className={styles.settingRow}>
          <div className={styles.settingHeader}>
            <span className={styles.settingLabel}>Motion</span>
            <span className={styles.settingSubtext}>Camera panning and transitions</span>
          </div>
          <SegmentedControl
            aria-label="Motion preference"
            options={[
              { value: 'system', label: 'System' },
              { value: 'reduced', label: 'Reduced' },
              { value: 'full', label: 'Full' },
            ]}
            value={preferences.motion}
            onValueChange={(val) => preferences.setMotion(val as MotionPreference)}
          />
        </div>

        <div className={styles.settingRow}>
          <div className={styles.settingHeader}>
            <div>
              <div className={styles.settingLabel}>Screen-Space Label Collision Arbiter</div>
              <div className={styles.settingSubtext}>
                Prunes colliding entity text in real-time via 32px spatial hash grid
              </div>
            </div>
            <Badge tone="accent">Zig WASM SIMD</Badge>
          </div>
          <Toggle
            aria-label="Enable label collision arbiter"
            checked={preferences.labelArbitration}
            onCheckedChange={(checked) => preferences.setLabelArbitration(checked)}
          />
        </div>
      </section>

      {/* 2. Units & Coordinates */}
      <section
        className={styles.section}
        aria-labelledby="units-heading"
        hidden={activeTab !== 'all' && activeTab !== 'units'}
      >
        <div className={styles.sectionHeader}>
          <h3 id="units-heading" className={styles.sectionTitle}>
            Units & Coordinates
          </h3>
          <p className={styles.sectionDescription}>
            Display formats for latitude, longitude, altitude, and velocity.
          </p>
        </div>

        <div className={styles.settingRow}>
          <div className={styles.settingHeader}>
            <span className={styles.settingLabel}>Coordinate Format</span>
          </div>
          <SegmentedControl
            aria-label="Coordinate format"
            options={[
              { value: 'dd', label: 'DD (48.8566° N)' },
              { value: 'dms', label: 'DMS (48°51\'24" N)' },
              { value: 'mgrs', label: 'MGRS (31U DQ)' },
            ]}
            value={preferences.coordinateFormat}
            onValueChange={(val) => preferences.setCoordinateFormat(val as CoordinateFormat)}
          />
        </div>

        <div className={styles.settingRow}>
          <div className={styles.settingHeader}>
            <span className={styles.settingLabel}>Measurement System</span>
          </div>
          <SegmentedControl
            aria-label="Measurement system"
            options={[
              { value: 'nautical', label: 'Nautical (kt · ft · NM)' },
              { value: 'metric', label: 'Metric (km/h · m · km)' },
              { value: 'imperial', label: 'Imperial (mph · ft · mi)' },
            ]}
            value={preferences.unitSystem}
            onValueChange={(val) => preferences.setUnitSystem(val as UnitSystem)}
          />
        </div>
      </section>

      {/* 3. Data Sources & API Keys */}
      <section
        className={styles.section}
        aria-labelledby="datasources-heading"
        hidden={activeTab !== 'all' && activeTab !== 'keys'}
      >
        <div className={styles.sectionHeader}>
          <h3 id="datasources-heading" className={styles.sectionTitle}>
            Data Sources & Credentials
          </h3>
          <p className={styles.sectionDescription}>
            API keys and tokens required for real-time telemetry providers.
          </p>
        </div>

        {secretInfo?.managedBy === 'operator' ? (
          <div className={styles.notice} role="status">
            <strong>Server Managed</strong>
            <span>
              Upstream data source credentials are managed securely by the server operator.
            </span>
          </div>
        ) : (
          <>
            <div className={styles.notice}>
              <strong>OS Keychain Protected</strong>
              <span>
                Desktop credentials are stored in your operating system keychain. Key values are
                write-only and never exposed to the web view.
              </span>
            </div>

            <ul className={styles.secretList}>
              {KNOWN_SECRETS.map((secret) => {
                const isConfigured = secretInfo?.status[secret.id] ?? false;
                const draft = draftKeys[secret.id] ?? '';
                const isBusy = busyKey === secret.id;

                return (
                  <li key={secret.id} className={styles.secretCard}>
                    <div className={styles.secretHeader}>
                      <div className={styles.secretLabelGroup}>
                        <span className={styles.secretName}>{secret.label}</span>
                        <span className={styles.secretKeyName}>{secret.envVar}</span>
                      </div>
                      <Badge tone={isConfigured ? 'accent' : 'neutral'}>
                        {isConfigured ? 'Configured' : 'Not configured'}
                      </Badge>
                    </div>

                    <div className={styles.secretForm}>
                      <input
                        type="password"
                        placeholder={isConfigured ? '••••••••••••••••' : 'Paste API key or token'}
                        value={draft}
                        aria-label={`Enter ${secret.label}`}
                        disabled={isBusy}
                        autoComplete="off"
                        className={styles.secretInput}
                        onChange={(e) =>
                          setDraftKeys((prev) => ({ ...prev, [secret.id]: e.target.value }))
                        }
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            void handleSaveKey(secret.id);
                          }
                        }}
                      />
                      <Button
                        variant="secondary"
                        size="sm"
                        disabled={!draft.trim() || isBusy}
                        onClick={() => void handleSaveKey(secret.id)}
                      >
                        Save
                      </Button>
                      {isConfigured && (
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={isBusy}
                          onClick={() => void handleClearKey(secret.id)}
                        >
                          Clear
                        </Button>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </section>

      {/* 4. About & Polyglot Engine */}
      <section
        className={styles.section}
        aria-labelledby="about-heading"
        hidden={activeTab !== 'all' && activeTab !== 'about'}
      >
        <div className={styles.sectionHeader}>
          <h3 id="about-heading" className={styles.sectionTitle}>
            About Lavender
          </h3>
          <p className={styles.sectionDescription}>
            Lavender v0.1.0 · High-performance polyglot OSINT and geospatial intelligence platform.
          </p>
        </div>

        <div className={styles.polyglotGrid}>
          <div className={styles.archCard}>
            <span className={styles.archTitle}>Rust Core</span>
            <span className={styles.archDetail}>
              Egress governance, spatial feed hub, thread-safe memory & OS keychain
            </span>
          </div>
          <div className={styles.archCard}>
            <span className={styles.archTitle}>Zig WASM</span>
            <span className={styles.archDetail}>
              Label arbitration, GRIB2 wind streamlines, SGP4 propagator & geometry
            </span>
          </div>
          <div className={styles.archCard}>
            <span className={styles.archTitle}>DuckDB Analytics</span>
            <span className={styles.archDetail}>
              Columnar Parquet trajectory archive & fast spatiotemporal SQL
            </span>
          </div>
          <div className={styles.archCard}>
            <span className={styles.archTitle}>Mojo Perception</span>
            <span className={styles.archDetail}>
              Zero-GIL MLIR computer vision sidecar for CCTV & ALPR inference
            </span>
          </div>
        </div>

        <div>
          <a href="/docs/" target="_blank" rel="noreferrer" className={styles.docLink}>
            Documentation & Third-Party Notices →
          </a>
        </div>
      </section>
    </Panel>
  );
}
