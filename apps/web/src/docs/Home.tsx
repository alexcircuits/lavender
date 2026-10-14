import { ArrowRightIcon } from '@phosphor-icons/react/dist/csr/ArrowRight';
import { useState } from 'react';
import { SegmentedControl } from '../design';
import { CodeBlock } from './CodeBlock';
import { docHref } from './content';
import styles from './docs.module.css';

const quickstart = {
  install: 'cargo build --locked -p lavender-server\npnpm --dir apps/web install --frozen-lockfile',
  run: '# Terminal 1 — Rust API\ncargo run --locked -p lavender-server\n\n# Terminal 2 — web interface + docs\npnpm --dir apps/web run dev',
};

export function Home() {
  const [step, setStep] = useState<'install' | 'run'>('install');
  return (
    <>
      <div className={styles.homeIntro}>
        <p className={styles.eyebrow}>Lavender / Developer documentation</p>
        <h1 id="page-title" tabIndex={-1}>
          Build with Lavender.
        </h1>
        <p className={styles.lede}>
          One globe. A shared Rust core.
          <br />A clear path from your first run to your first contribution.
        </p>
        <div className={styles.heroActions}>
          <a href={docHref('start-here')} className={styles.primaryLink}>
            Start here <ArrowRightIcon size={16} weight="bold" aria-hidden />
          </a>
          <a href={docHref('getting-started')} className={styles.secondaryLink}>
            Run locally
          </a>
        </div>
      </div>
      <section className={styles.systemSection} aria-labelledby="how-it-fits">
        <div className={styles.sectionLabel}>
          <h2 id="how-it-fits">How it fits together</h2>
          <a href={docHref('architecture')}>
            Explore architecture <ArrowRightIcon size={14} aria-hidden />
          </a>
        </div>
        <div className={styles.systemDiagram}>
          <a href={docHref('web-and-desktop')} className={styles.systemNode}>
            <span className={styles.nodeNumber}>01 / Interface</span>
            <strong>React + Cesium</strong>
            <span>Web · Tauri desktop</span>
          </a>
          <ArrowRightIcon className={styles.flowArrow} size={18} aria-hidden />
          <a href={docHref('architecture')} className={`${styles.systemNode} ${styles.coreNode}`}>
            <span className={styles.nodeNumber}>02 / Shared core</span>
            <strong>LavenderCore</strong>
            <span>Typed records · shared feeds</span>
          </a>
          <ArrowRightIcon className={styles.flowArrow} size={18} aria-hidden />
          <a href={docHref('add-provider')} className={styles.systemNode}>
            <span className={styles.nodeNumber}>03 / Providers</span>
            <strong>Source → snapshot</strong>
            <span>USGS · adsb.lol · CelesTrak</span>
          </a>
        </div>
        <p className={styles.systemCaption}>
          Two platform adapters. The same provider logic and validated records.
        </p>
      </section>
      <section className={styles.quickstart} aria-labelledby="quickstart">
        <div className={styles.sectionLabel}>
          <h2 id="quickstart">A local workspace, in two steps</h2>
          <SegmentedControl
            aria-label="Quickstart step"
            value={step}
            onValueChange={setStep}
            options={[
              { value: 'install', label: '1. Install' },
              { value: 'run', label: '2. Run' },
            ]}
            size="sm"
          />
        </div>
        <CodeBlock
          code={quickstart[step]}
          language="bash"
          label="From the Lavender repository root"
        />
        <p className={styles.quickstartNote}>
          Rust 1.99 · Node 22.12+ · pnpm 12.11.2<span>No database or provider key needed.</span>
        </p>
        <a className={styles.textLink} href={docHref('getting-started')}>
          Prerequisites, fixtures & troubleshooting <ArrowRightIcon size={14} aria-hidden />
        </a>
      </section>
      <section className={styles.firstSteps} aria-labelledby="first-change">
        <p className={styles.eyebrow}>Your first 30 minutes</p>
        <h2 id="first-change">Get oriented. Then make a small change.</h2>
        {[
          [
            '01',
            'Run a predictable workspace',
            'Use recorded fixtures, inspect an earthquake and find FIN7KV.',
            'getting-started',
            'predictable-fixture-data',
          ],
          [
            '02',
            'Follow an aircraft record',
            'Trace the interface, WebSocket, shared hub and provider parser.',
            'architecture',
            'an-aircraft-record-end-to-end',
          ],
          [
            '03',
            'Choose your contribution',
            'Read the current task, its blockers and the review workflow.',
            'contributing',
            '',
          ],
        ].map(([number, title, description, id, section]) => (
          <a key={number} href={docHref(id ?? '', section)} className={styles.stepRow}>
            <span className={styles.stepNumber}>{number}</span>
            <span>
              <strong>{title}</strong>
              <span>{description}</span>
            </span>
            <ArrowRightIcon size={18} aria-hidden />
          </a>
        ))}
      </section>
      <section className={styles.referenceSection} aria-labelledby="keep-close">
        <h2 id="keep-close">Keep these close</h2>
        <div className={styles.referenceGrid}>
          {[
            [
              'codebase-map',
              'Find your way through the code',
              'Files, boundaries and responsibilities.',
            ],
            ['reference', 'Functions & modules', 'Inputs, results, errors and side effects.'],
            ['testing', 'Test with confidence', 'Fixtures, logs and transport parity.'],
            ['roadmap', 'See what comes next', 'Small milestones and existing tasks.'],
          ].map(([id, title, description]) => (
            <a key={id} href={docHref(id ?? '')}>
              <strong>
                {title}
                <ArrowRightIcon size={16} aria-hidden />
              </strong>
              <span>{description}</span>
            </a>
          ))}
        </div>
      </section>
    </>
  );
}
