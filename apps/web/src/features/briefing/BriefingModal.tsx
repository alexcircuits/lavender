import { useId, useMemo, useState } from 'react';
import { Button, Dialog, DialogClose, showToast, TextField } from '../../design';
import { useFeeds, useGlobe, useSystem, useUi, useWorkspace } from '../../runtime';
import styles from './briefing.module.css';
import {
  compileBriefing,
  copyToClipboard,
  defaultBriefingTitle,
  downloadFile,
  exportToHtml,
  exportToJson,
  exportToMarkdown,
  slugify,
} from './briefingModel';

export interface BriefingModalProps {
  readonly open?: boolean;
  readonly onOpenChange?: (open: boolean) => void;
  readonly defaultOpen?: boolean;
  readonly className?: string | undefined;
}

export function BriefingModal({ open, onOpenChange, defaultOpen, className }: BriefingModalProps) {
  const storeOpen = useUi((s) => s.briefingOpen);
  const setStoreOpen = useUi((s) => s.setBriefingOpen);

  const isOpen = open !== undefined ? open : storeOpen;
  const handleOpenChange = onOpenChange ?? setStoreOpen;

  const catalog = useSystem((s) => s.catalog);
  const layers = useWorkspace((s) => s.layers);
  const summaries = useFeeds((s) => s.summaries);
  const selection = useWorkspace((s) => s.selection);
  const selectedEntity = useFeeds((s) => s.selectedEntity);
  const annotations = useWorkspace((s) => s.annotations);
  const globe = useGlobe();

  const [title, setTitle] = useState(() => defaultBriefingTitle());
  const [analystNotes, setAnalystNotes] = useState('');

  const notesId = useId();

  const dossier = useMemo(() => {
    return compileBriefing({
      title,
      analystNotes,
      globe,
      catalog,
      layers,
      summaries,
      selection,
      selectedEntity,
      annotations,
    });
  }, [
    title,
    analystNotes,
    globe,
    catalog,
    layers,
    summaries,
    selection,
    selectedEntity,
    annotations,
  ]);

  const handleCopyMarkdown = async () => {
    const md = exportToMarkdown(dossier);
    const copied = await copyToClipboard(md);
    if (copied) {
      showToast({
        kind: 'success',
        title: 'Markdown copied',
        description: 'Briefing dossier markdown copied to clipboard.',
      });
    } else {
      showToast({
        kind: 'error',
        title: 'Copy failed',
        description: 'Could not access clipboard. Please check browser permissions.',
      });
    }
  };

  const handleDownloadHtml = () => {
    const html = exportToHtml(dossier);
    const filename = `${slugify(dossier.title)}.html`;
    downloadFile(html, filename, 'text/html');
    showToast({
      kind: 'success',
      title: 'HTML dossier downloaded',
      description: `Saved ${filename}`,
    });
  };

  const handleDownloadJson = () => {
    const json = exportToJson(dossier);
    const filename = `${slugify(dossier.title)}.json`;
    downloadFile(json, filename, 'application/json');
    showToast({
      kind: 'success',
      title: 'JSON dossier downloaded',
      description: `Saved ${filename}`,
    });
  };

  const cameraText = dossier.camera
    ? `${dossier.camera.lat.toFixed(4)}°, ${dossier.camera.lon.toFixed(4)}° (${dossier.camera.heightM.toLocaleString('en-US')} m)`
    : 'Global View';

  return (
    <Dialog
      open={isOpen}
      onOpenChange={handleOpenChange}
      {...(defaultOpen !== undefined ? { defaultOpen } : {})}
      title="Intelligence Briefing Dossier"
      description="Compile, customize, preview, and export an OSINT situation report from active feeds, annotations, and camera state."
      className={className ?? styles.dialog}
      actions={
        <div className={styles.actions}>
          <Button variant="secondary" onClick={handleCopyMarkdown}>
            Copy Markdown
          </Button>
          <Button variant="secondary" onClick={handleDownloadHtml}>
            Download HTML Dossier
          </Button>
          <Button variant="secondary" onClick={handleDownloadJson}>
            Download JSON
          </Button>
          <DialogClose render={<Button variant="primary">Done</Button>} />
        </div>
      }
    >
      <div className={styles.body}>
        <section className={styles.formSection} aria-label="Briefing customization">
          <TextField
            label="Briefing title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="OSINT Situation Briefing — YYYY-MM-DD"
          />

          <div className={styles.notesField}>
            <label htmlFor={notesId} className={styles.fieldLabel}>
              Analyst notes
            </label>
            <textarea
              id={notesId}
              className={styles.textarea}
              placeholder="Add operational context, intelligence observations, or analysis notes..."
              value={analystNotes}
              onChange={(e) => setAnalystNotes(e.target.value)}
              rows={3}
            />
          </div>
        </section>

        <article className={styles.previewContainer} aria-label="Dossier preview">
          <header className={styles.previewHeader}>
            <span className={styles.previewBadge}>OSINT Situation Dossier</span>
            <h3 className={styles.previewTitle}>{dossier.title}</h3>
            <div className={styles.previewMeta}>
              <span>
                <strong>Generated:</strong> <span className="mono">{dossier.generatedAt}</span>
              </span>
              <span>
                <strong>Viewport:</strong> <span className="mono">{cameraText}</span>
              </span>
            </div>
          </header>

          {dossier.analystNotes ? (
            <section className={styles.section} aria-label="Analyst notes preview">
              <h4 className={styles.sectionHeading}>Analyst Notes</h4>
              <div className={styles.notesCard}>{dossier.analystNotes}</div>
            </section>
          ) : null}

          <section className={styles.section} aria-label="Intelligence summary preview">
            <h4 className={styles.sectionHeading}>Intelligence Summary</h4>

            <h5 className={styles.subHeading}>Active Feeds & Layers</h5>
            {dossier.activeLayers.length === 0 ? (
              <p className={styles.emptyText}>No active layers enabled at the time of export.</p>
            ) : (
              <div className={styles.tableContainer}>
                <table className={styles.previewTable}>
                  <thead>
                    <tr>
                      <th>Feed ID</th>
                      <th>Layer Name</th>
                      <th style={{ textAlign: 'right' }}>Count</th>
                    </tr>
                  </thead>
                  <tbody>
                    {dossier.activeLayers.map((l) => (
                      <tr key={l.id}>
                        <td>
                          <span className="mono">{l.id}</span>
                        </td>
                        <td>{l.name}</td>
                        <td style={{ textAlign: 'right' }} className="mono">
                          {l.count !== undefined ? l.count.toLocaleString('en-US') : '—'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            <h5 className={styles.subHeading}>Geofences & Monitored Perimeters</h5>
            {dossier.geofences.length === 0 ? (
              <p className={styles.emptyText}>No monitored geofences configured.</p>
            ) : (
              <div className={styles.tableContainer}>
                <table className={styles.previewTable}>
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th style={{ textAlign: 'right' }}>Vertices</th>
                      <th>ID</th>
                    </tr>
                  </thead>
                  <tbody>
                    {dossier.geofences.map((g) => (
                      <tr key={g.id}>
                        <td>
                          <strong>{g.name}</strong>
                        </td>
                        <td style={{ textAlign: 'right' }} className="mono">
                          {g.pointCount}
                        </td>
                        <td>
                          <span className="mono">{g.id}</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          <section className={styles.section} aria-label="Entities and annotations preview">
            <h4 className={styles.sectionHeading}>Entities & Annotations</h4>

            {dossier.selectedEntity ? (
              <div className={styles.entityCard}>
                <div className={styles.entityHeader}>
                  <span>Selected Target:</span>
                  <strong>{dossier.selectedEntity.id}</strong>
                  <span className="mono">({dossier.selectedEntity.feed})</span>
                </div>
                {dossier.selectedEntity.details &&
                Object.keys(dossier.selectedEntity.details).length > 0 ? (
                  <div className={styles.entityDetailsGrid}>
                    {Object.entries(dossier.selectedEntity.details).map(([k, v]) => (
                      <div key={k} className={styles.detailItem}>
                        <span className={styles.detailLabel}>{k}</span>
                        <span className={styles.detailValue}>{String(v)}</span>
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : (
              <p className={styles.emptyText}>No entity currently selected.</p>
            )}

            <h5 className={styles.subHeading}>Manual Annotations</h5>
            {dossier.annotations.length === 0 ? (
              <p className={styles.emptyText}>No manual annotations on current workspace.</p>
            ) : (
              <div className={styles.tableContainer}>
                <table className={styles.previewTable}>
                  <thead>
                    <tr>
                      <th>Kind</th>
                      <th>Label</th>
                      <th>Note</th>
                      <th>ID</th>
                    </tr>
                  </thead>
                  <tbody>
                    {dossier.annotations.map((a) => (
                      <tr key={a.id}>
                        <td>
                          <span className="mono">{a.kind}</span>
                        </td>
                        <td>
                          <strong>{a.label}</strong>
                        </td>
                        <td>{a.note}</td>
                        <td>
                          <span className="mono">{a.id}</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          <section
            className={styles.section}
            aria-label="Mandatory legal citations and attribution"
          >
            <h4 className={styles.sectionHeading}>Mandatory Legal Citations & Attribution</h4>
            {dossier.sources.length === 0 ? (
              <p className={styles.emptyText}>No source citations recorded.</p>
            ) : (
              <div className={styles.sourceList}>
                {dossier.sources.map((s) => (
                  <div key={s.id} className={styles.sourceCard}>
                    <div className={styles.sourceCardHeader}>
                      <span className={styles.sourceCardName}>{s.name}</span>
                      <span className="mono">({s.id})</span>
                    </div>
                    <div className={styles.sourceCardDetail}>
                      <strong>Attribution:</strong> {s.attribution}
                    </div>
                    <div className={styles.sourceCardDetail}>
                      <strong>Licence:</strong> {s.licence}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </section>
        </article>
      </div>
    </Dialog>
  );
}
