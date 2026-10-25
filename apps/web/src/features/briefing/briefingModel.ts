import type { Catalog, FeedId, SourceInfo } from '../../contracts';
import type { EntityRef, GlobeController } from '../../globe';
import type { FeedSummary, LayerState } from '../../state/stores';
import type { MapAnnotation } from '../annotations/model';

export interface BriefingCamera {
  readonly lat: number;
  readonly lon: number;
  readonly heightM: number;
}

export interface BriefingLayer {
  readonly id: string;
  readonly name: string;
  readonly count?: number | undefined;
}

export interface BriefingEntity {
  readonly feed: string;
  readonly id: string;
  readonly details?: Record<string, string | number> | undefined;
}

export interface BriefingAnnotation {
  readonly id: string;
  readonly kind: string;
  readonly label: string;
  readonly note: string;
}

export interface BriefingGeofence {
  readonly id: string;
  readonly name: string;
  readonly pointCount: number;
}

export interface BriefingSource {
  readonly id: string;
  readonly name: string;
  readonly attribution: string;
  readonly licence: string;
}

export interface BriefingDossier {
  readonly generatedAt: string;
  readonly title: string;
  readonly analystNotes?: string | undefined;
  readonly camera: BriefingCamera | null;
  readonly activeLayers: readonly BriefingLayer[];
  readonly selectedEntity?: BriefingEntity | null;
  readonly annotations: readonly BriefingAnnotation[];
  readonly geofences: readonly BriefingGeofence[];
  readonly sources: readonly BriefingSource[];
}

export interface CompileBriefingOptions {
  readonly title?: string | undefined;
  readonly analystNotes?: string | undefined;
  readonly generatedAt?: string | undefined;
  readonly camera?: BriefingCamera | null | undefined;
  readonly globe?: GlobeController | null | undefined;
  readonly catalog?: Catalog | null | undefined;
  readonly layers?: Partial<Record<FeedId, LayerState>> | undefined;
  readonly summaries?: Partial<Record<FeedId, FeedSummary>> | undefined;
  readonly selection?: EntityRef | null | undefined;
  readonly selectedEntity?: unknown | undefined;
  readonly annotations?: readonly MapAnnotation[] | undefined;
}

export function defaultBriefingTitle(date: Date = new Date()): string {
  const dateStr = date.toISOString().slice(0, 10);
  return `OSINT Situation Briefing — ${dateStr}`;
}

export function extractEntityDetails(entity: unknown): Record<string, string | number> | undefined {
  if (!entity || typeof entity !== 'object' || Array.isArray(entity)) {
    return undefined;
  }
  const result: Record<string, string | number> = {};
  for (const [key, val] of Object.entries(entity)) {
    if (typeof val === 'string' || typeof val === 'number') {
      result[key] = val;
    }
  }
  return Object.keys(result).length > 0 ? result : undefined;
}

function resolveLicenceString(source: SourceInfo): string {
  if (typeof source.licence === 'string') {
    return source.licence;
  }
  if (source.licence && typeof source.licence.name === 'string') {
    return source.licence.name;
  }
  return 'Unknown';
}

export function compileBriefing(options: CompileBriefingOptions = {}): BriefingDossier {
  const generatedAt = options.generatedAt ?? new Date().toISOString();
  const dateObj = new Date(generatedAt);
  const fallbackDate = Number.isNaN(dateObj.getTime()) ? new Date() : dateObj;
  const title = options.title?.trim() || defaultBriefingTitle(fallbackDate);
  const analystNotes = options.analystNotes?.trim() || undefined;

  let camera: BriefingCamera | null = null;
  if (options.camera !== undefined) {
    camera = options.camera;
  } else if (options.globe) {
    const pose = options.globe.cameraPose();
    if (pose) {
      camera = {
        lat: Number(pose.lat.toFixed(4)),
        lon: Number(pose.lon.toFixed(4)),
        heightM: Math.round(pose.heightM),
      };
    }
  }

  const catalogLayers = options.catalog?.layers ?? [];
  const layersState = options.layers ?? {};
  const summariesState = options.summaries ?? {};

  const activeLayers: BriefingLayer[] = catalogLayers
    .filter((l) => layersState[l.feed]?.enabled)
    .map((l) => {
      const summary = summariesState[l.feed];
      return {
        id: l.feed,
        name: l.name,
        ...(summary !== undefined ? { count: summary.count } : {}),
      };
    });

  let selectedEntity: BriefingEntity | null = null;
  if (options.selection) {
    const details = extractEntityDetails(options.selectedEntity);
    selectedEntity = {
      feed: options.selection.feed,
      id: options.selection.id,
      ...(details ? { details } : {}),
    };
  }

  const allAnnotations = options.annotations ?? [];
  const annotations: BriefingAnnotation[] = allAnnotations
    .filter((a) => a.kind !== 'geofence')
    .map((a) => ({
      id: a.id,
      kind: a.kind,
      label: a.label,
      note: a.note,
    }));

  const geofences: BriefingGeofence[] = allAnnotations
    .filter((a) => a.kind === 'geofence')
    .map((a) => ({
      id: a.id,
      name: a.label,
      pointCount: a.points.length,
    }));

  const catalogSources = options.catalog?.sources ?? [];
  const enabledFeedSet = new Set(activeLayers.map((l) => l.id));
  const activeSourceIds = new Set(
    catalogLayers.filter((l) => enabledFeedSet.has(l.feed)).flatMap((l) => l.sourceIds),
  );

  const sourcesList =
    activeSourceIds.size > 0
      ? catalogSources.filter((s) => activeSourceIds.has(s.id))
      : catalogSources;

  const sources: BriefingSource[] = sourcesList.map((s) => ({
    id: s.id,
    name: s.name,
    attribution: s.attribution,
    licence: resolveLicenceString(s),
  }));

  return {
    generatedAt,
    title,
    ...(analystNotes ? { analystNotes } : {}),
    camera,
    activeLayers,
    selectedEntity,
    annotations,
    geofences,
    sources,
  };
}

export function exportToJson(dossier: BriefingDossier): string {
  return JSON.stringify(dossier, null, 2);
}

export function exportToMarkdown(dossier: BriefingDossier): string {
  const lines: string[] = [];

  lines.push(`# ${dossier.title}`);
  lines.push('');
  lines.push(`**Generated:** ${dossier.generatedAt}`);

  if (dossier.camera) {
    const latStr = `${dossier.camera.lat.toFixed(4)}°`;
    const lonStr = `${dossier.camera.lon.toFixed(4)}°`;
    const heightStr = `${dossier.camera.heightM.toLocaleString('en-US')} m`;
    lines.push(`**Camera Viewport:** ${latStr}, ${lonStr} (Altitude: ${heightStr})`);
  } else {
    lines.push('**Camera Viewport:** Global View');
  }

  if (dossier.analystNotes) {
    lines.push('');
    lines.push('---');
    lines.push('');
    lines.push('## Analyst Notes');
    lines.push('');
    lines.push(dossier.analystNotes);
  }

  lines.push('');
  lines.push('---');
  lines.push('');
  lines.push('## Intelligence Summary');
  lines.push('');

  lines.push('### Active Feeds & Layers');
  lines.push('');
  if (dossier.activeLayers.length === 0) {
    lines.push('*No active layers enabled at the time of export.*');
  } else {
    lines.push('| Feed ID | Layer Name | Visible Count |');
    lines.push('|---|---|---|');
    for (const layer of dossier.activeLayers) {
      const countStr = layer.count !== undefined ? String(layer.count) : '—';
      lines.push(`| \`${layer.id}\` | ${layer.name} | ${countStr} |`);
    }
  }

  lines.push('');
  lines.push('### Geofences & Monitored Perimeters');
  lines.push('');
  if (dossier.geofences.length === 0) {
    lines.push('*No monitored geofences configured.*');
  } else {
    lines.push('| Geofence Name | Vertices | Identity |');
    lines.push('|---|---|---|');
    for (const fence of dossier.geofences) {
      lines.push(`| ${fence.name} | ${fence.pointCount} | \`${fence.id}\` |`);
    }
  }

  if (dossier.selectedEntity) {
    lines.push('');
    lines.push('---');
    lines.push('');
    lines.push('## Selected Target / Entity');
    lines.push('');
    lines.push(`- **Feed:** \`${dossier.selectedEntity.feed}\``);
    lines.push(`- **Identity:** \`${dossier.selectedEntity.id}\``);

    if (dossier.selectedEntity.details && Object.keys(dossier.selectedEntity.details).length > 0) {
      lines.push('- **Telemetry & Attributes:**');
      for (const [key, val] of Object.entries(dossier.selectedEntity.details)) {
        lines.push(`  - **${key}:** \`${val}\``);
      }
    }
  }

  lines.push('');
  lines.push('---');
  lines.push('');
  lines.push('## Annotations & Observations');
  lines.push('');
  if (dossier.annotations.length === 0) {
    lines.push('*No manual annotations on current workspace.*');
  } else {
    lines.push('| Kind | Label | Note | Identity |');
    lines.push('|---|---|---|---|');
    for (const ann of dossier.annotations) {
      const noteClean = ann.note.replace(/\r?\n/g, ' ');
      lines.push(`| ${ann.kind} | ${ann.label} | ${noteClean} | \`${ann.id}\` |`);
    }
  }

  lines.push('');
  lines.push('---');
  lines.push('');
  lines.push('## Mandatory Legal Citations & Attribution');
  lines.push('');
  if (dossier.sources.length === 0) {
    lines.push('*No source citations recorded.*');
  } else {
    for (const source of dossier.sources) {
      lines.push(`### ${source.name} (\`${source.id}\`)`);
      lines.push(`- **Attribution:** ${source.attribution}`);
      lines.push(`- **Licence:** ${source.licence}`);
      lines.push('');
    }
  }

  return lines.join('\n');
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export function exportToHtml(dossier: BriefingDossier): string {
  const cameraText = dossier.camera
    ? `${dossier.camera.lat.toFixed(4)}°, ${dossier.camera.lon.toFixed(4)}° (Altitude: ${dossier.camera.heightM.toLocaleString('en-US')} m)`
    : 'Global View';

  const notesHtml = dossier.analystNotes
    ? `
      <section class="section">
        <h2>Analyst Notes</h2>
        <div class="analyst-notes">${escapeHtml(dossier.analystNotes)}</div>
      </section>
    `
    : '';

  const layersHtml =
    dossier.activeLayers.length === 0
      ? '<p class="empty-text">No active layers enabled at the time of export.</p>'
      : `
        <table>
          <thead>
            <tr>
              <th>Feed ID</th>
              <th>Layer Name</th>
              <th style="text-align: right;">Visible Count</th>
            </tr>
          </thead>
          <tbody>
            ${dossier.activeLayers
              .map(
                (l) => `
              <tr>
                <td><code>${escapeHtml(l.id)}</code></td>
                <td>${escapeHtml(l.name)}</td>
                <td style="text-align: right;" class="mono">${l.count !== undefined ? l.count.toLocaleString('en-US') : '—'}</td>
              </tr>
            `,
              )
              .join('')}
          </tbody>
        </table>
      `;

  const geofencesHtml =
    dossier.geofences.length === 0
      ? '<p class="empty-text">No monitored geofences configured.</p>'
      : `
        <table>
          <thead>
            <tr>
              <th>Geofence Name</th>
              <th style="text-align: right;">Vertices</th>
              <th>ID</th>
            </tr>
          </thead>
          <tbody>
            ${dossier.geofences
              .map(
                (g) => `
              <tr>
                <td><strong>${escapeHtml(g.name)}</strong></td>
                <td style="text-align: right;" class="mono">${g.pointCount}</td>
                <td><code>${escapeHtml(g.id)}</code></td>
              </tr>
            `,
              )
              .join('')}
          </tbody>
        </table>
      `;

  const entityHtml = dossier.selectedEntity
    ? `
      <section class="section">
        <h2>Selected Target / Entity</h2>
        <div class="card">
          <p><strong>Feed:</strong> <code>${escapeHtml(dossier.selectedEntity.feed)}</code></p>
          <p><strong>Identity:</strong> <code>${escapeHtml(dossier.selectedEntity.id)}</code></p>
          ${
            dossier.selectedEntity.details && Object.keys(dossier.selectedEntity.details).length > 0
              ? `
            <div class="details-grid">
              ${Object.entries(dossier.selectedEntity.details)
                .map(
                  ([k, v]) => `
                <div class="detail-cell">
                  <span class="detail-key">${escapeHtml(k)}</span>
                  <span class="detail-val mono">${escapeHtml(String(v))}</span>
                </div>
              `,
                )
                .join('')}
            </div>
          `
              : ''
          }
        </div>
      </section>
    `
    : '';

  const annotationsHtml =
    dossier.annotations.length === 0
      ? '<p class="empty-text">No manual annotations on current workspace.</p>'
      : `
        <table>
          <thead>
            <tr>
              <th>Kind</th>
              <th>Label</th>
              <th>Note</th>
              <th>ID</th>
            </tr>
          </thead>
          <tbody>
            ${dossier.annotations
              .map(
                (a) => `
              <tr>
                <td><span class="badge badge-subtle">${escapeHtml(a.kind)}</span></td>
                <td><strong>${escapeHtml(a.label)}</strong></td>
                <td>${escapeHtml(a.note)}</td>
                <td><code>${escapeHtml(a.id)}</code></td>
              </tr>
            `,
              )
              .join('')}
          </tbody>
        </table>
      `;

  const sourcesHtml =
    dossier.sources.length === 0
      ? '<p class="empty-text">No source citations recorded.</p>'
      : `
        <div class="sources-list">
          ${dossier.sources
            .map(
              (s) => `
            <div class="source-card">
              <div class="source-header">
                <strong>${escapeHtml(s.name)}</strong>
                <code>${escapeHtml(s.id)}</code>
              </div>
              <p class="source-attr"><strong>Attribution:</strong> ${escapeHtml(s.attribution)}</p>
              <p class="source-licence"><strong>Licence:</strong> ${escapeHtml(s.licence)}</p>
            </div>
          `,
            )
            .join('')}
        </div>
      `;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(dossier.title)}</title>
  <style>
    :root {
      color-scheme: dark;
      --ground: #0b0c10;
      --well: #1e2027;
      --island: #16171d;
      --hairline: #2b2a39;
      --hairline-strong: #3a3847;
      --ink: #f2f3f6;
      --ink-2: #a3a8b4;
      --ink-3: #8f96a5;
      --accent: #1e3fd8;
      --accent-ink: #9db1ff;
      --font-sans: 'Geist Variable', 'Geist', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
      --font-mono: 'Geist Mono Variable', 'Geist Mono', ui-monospace, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background-color: var(--ground);
      color: var(--ink);
      font-family: var(--font-sans);
      font-size: 14px;
      line-height: 1.5;
      padding: 32px 16px;
      display: flex;
      justify-content: center;
    }
    .dossier {
      max-width: 860px;
      width: 100%;
      background: var(--island);
      border: 1px solid var(--hairline);
      border-radius: 12px;
      padding: 32px;
      box-shadow: 0 12px 32px -12px rgba(0, 0, 0, 0.7);
    }
    header {
      border-bottom: 1px solid var(--hairline);
      padding-bottom: 20px;
      margin-bottom: 24px;
    }
    .badge {
      display: inline-block;
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: var(--accent-ink);
      background: rgba(30, 63, 216, 0.2);
      border: 1px solid rgba(157, 177, 255, 0.3);
      padding: 2px 8px;
      border-radius: 4px;
      margin-bottom: 12px;
    }
    .badge-subtle {
      color: var(--ink-2);
      background: var(--well);
      border: 1px solid var(--hairline);
    }
    h1 {
      font-size: 24px;
      font-weight: 600;
      color: var(--ink);
      margin-bottom: 8px;
      letter-spacing: -0.012em;
    }
    .meta {
      display: flex;
      flex-wrap: wrap;
      gap: 16px;
      font-size: 12px;
      color: var(--ink-2);
    }
    .mono { font-family: var(--font-mono); }
    code {
      font-family: var(--font-mono);
      font-size: 12px;
      background: var(--well);
      padding: 2px 4px;
      border-radius: 4px;
      border: 1px solid var(--hairline);
      color: var(--ink);
    }
    .section {
      margin-top: 24px;
      padding-top: 20px;
      border-top: 1px solid var(--hairline);
    }
    h2 {
      font-size: 16px;
      font-weight: 600;
      color: var(--ink);
      margin-bottom: 12px;
      letter-spacing: -0.006em;
    }
    h3 {
      font-size: 13px;
      font-weight: 600;
      color: var(--ink-2);
      text-transform: uppercase;
      letter-spacing: 0.04em;
      margin: 16px 0 8px;
    }
    .analyst-notes {
      background: var(--well);
      border: 1px solid var(--hairline);
      border-left: 3px solid var(--accent);
      border-radius: 6px;
      padding: 14px 18px;
      white-space: pre-wrap;
      font-size: 13px;
      color: var(--ink);
    }
    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 13px;
      margin-bottom: 16px;
      border: 1px solid var(--hairline);
      border-radius: 6px;
      overflow: hidden;
    }
    th, td {
      text-align: left;
      padding: 8px 12px;
      border-bottom: 1px solid var(--hairline);
    }
    th {
      color: var(--ink-2);
      font-weight: 500;
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      background: var(--well);
    }
    tr:last-child td { border-bottom: none; }
    .empty-text {
      color: var(--ink-3);
      font-style: italic;
      margin-bottom: 12px;
    }
    .card {
      background: var(--well);
      border: 1px solid var(--hairline);
      border-radius: 6px;
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .details-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
      gap: 8px;
      margin-top: 8px;
      padding-top: 8px;
      border-top: 1px solid var(--hairline);
    }
    .detail-cell {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }
    .detail-key {
      font-size: 11px;
      color: var(--ink-3);
      text-transform: uppercase;
    }
    .detail-val {
      font-size: 13px;
      color: var(--ink);
    }
    .sources-list {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .source-card {
      background: var(--well);
      border: 1px solid var(--hairline);
      border-radius: 6px;
      padding: 12px 14px;
    }
    .source-header {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 6px;
    }
    .source-attr, .source-licence {
      font-size: 12px;
      color: var(--ink-2);
      margin-top: 2px;
    }
    footer {
      margin-top: 32px;
      padding-top: 16px;
      border-top: 1px solid var(--hairline);
      font-size: 11px;
      color: var(--ink-3);
      display: flex;
      justify-content: space-between;
    }
    @media print {
      body { background: #ffffff; color: #111111; padding: 0; }
      .dossier { border: none; box-shadow: none; max-width: 100%; padding: 0; background: #ffffff; }
      th { background: #f4f4f4; color: #333333; }
      .source-card, .card, .analyst-notes { background: #fafafa; border-color: #e0e0e0; }
      h1, h2, h3, strong { color: #111111; }
      .meta, .source-attr, .source-licence, th, td { color: #444444; }
      code { background: #f0f0f0; border-color: #dddddd; color: #111111; }
      .badge { border-color: #999999; color: #111111; background: transparent; }
    }
  </style>
</head>
<body>
  <article class="dossier">
    <header>
      <span class="badge">NoirSystems OSINT Intelligence Dossier</span>
      <h1>${escapeHtml(dossier.title)}</h1>
      <div class="meta">
        <span><strong>Generated:</strong> <span class="mono">${escapeHtml(dossier.generatedAt)}</span></span>
        <span><strong>Viewport:</strong> <span class="mono">${escapeHtml(cameraText)}</span></span>
      </div>
    </header>

    ${notesHtml}

    <section class="section">
      <h2>Intelligence Summary</h2>
      <h3>Active Feeds & Layers</h3>
      ${layersHtml}

      <h3>Geofences & Monitored Perimeters</h3>
      ${geofencesHtml}
    </section>

    ${entityHtml}

    <section class="section">
      <h2>Annotations & Observations</h2>
      ${annotationsHtml}
    </section>

    <section class="section">
      <h2>Mandatory Legal Citations &amp; Attribution</h2>
      ${sourcesHtml}
    </section>

    <footer>
      <span>NoirSystems Lavender OSINT Platform</span>
      <span>Confidential Intelligence Assessment</span>
    </footer>
  </article>
</body>
</html>`;
}

export function slugify(text: string): string {
  return (
    text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/(^-|-$)+/g, '') || 'briefing'
  );
}

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

export function downloadFile(content: string, filename: string, mimeType: string): void {
  if (typeof document === 'undefined') return;
  const blob = new Blob([content], { type: mimeType });
  const createUrl = typeof URL.createObjectURL === 'function' ? URL.createObjectURL : () => '';
  const revokeUrl = typeof URL.revokeObjectURL === 'function' ? URL.revokeObjectURL : () => {};
  const url = createUrl(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  revokeUrl(url);
}
