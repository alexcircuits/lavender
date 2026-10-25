import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, source, startHarness } from '../../app/testing';
import type { Catalog } from '../../contracts';
import { toastStore } from '../../design';
import { catalogOf, layer } from '../../test/fakes';
import { BriefingModal } from './BriefingModal';

const CATALOG: Catalog = catalogOf(
  [
    layer('earthquakes', { name: 'Earthquakes', sourceIds: ['usgs-earthquakes'] }),
    layer('aircraft', { name: 'Commercial Air', sourceIds: ['adsb-lol'] }),
  ],
  {
    sources: [
      source('usgs-earthquakes', {
        name: 'USGS Earthquakes',
        attribution: 'U.S. Geological Survey',
        licence: { name: 'Public Domain', url: 'https://usgs.gov' },
      }),
      source('adsb-lol', {
        name: 'adsb.lol',
        attribution: 'Community ADS-B Exchange',
        licence: { name: 'CC BY-SA 4.0', url: 'https://creativecommons.org' },
      }),
    ],
  },
);

let harness: Harness | null = null;

afterEach(() => {
  harness?.stop();
  harness = null;
  vi.restoreAllMocks();
});

describe('BriefingModal', () => {
  beforeEach(() => {
    toastStore.setState({ toasts: [] });
  });

  it('renders modal with form fields and action buttons when open', async () => {
    harness = await startHarness({ catalog: CATALOG, globe: true });
    renderWithRuntime(harness.runtime, <BriefingModal open={true} onOpenChange={vi.fn()} />);

    const dialog = await screen.findByRole('dialog', { name: 'Intelligence Briefing Dossier' });
    expect(dialog).toBeInTheDocument();

    expect(screen.getByRole('textbox', { name: 'Briefing title' })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Analyst notes' })).toBeInTheDocument();

    expect(screen.getByRole('button', { name: 'Copy Markdown' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Download HTML Dossier' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Download JSON' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
  });

  it('updates the live preview when title and analyst notes are modified', async () => {
    harness = await startHarness({ catalog: CATALOG, globe: true });
    renderWithRuntime(harness.runtime, <BriefingModal open={true} onOpenChange={vi.fn()} />);

    const titleField = screen.getByRole('textbox', { name: 'Briefing title' });
    fireEvent.change(titleField, { target: { value: 'Operation Blue Dawn' } });

    const notesField = screen.getByRole('textbox', { name: 'Analyst notes' });
    fireEvent.change(notesField, {
      target: { value: 'Surveillance sector fully locked down.' },
    });

    expect(
      screen.getByRole('heading', { level: 3, name: 'Operation Blue Dawn' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Analyst notes preview' })).toHaveTextContent(
      'Surveillance sector fully locked down.',
    );
  });

  it('renders live intelligence summary, geofences, entity details and attributions', async () => {
    harness = await startHarness({ catalog: CATALOG, globe: true });

    act(() => {
      harness?.runtime.stores.workspace.getState().setLayerEnabled('aircraft', true);
      harness?.runtime.stores.feeds.getState().setSummary('aircraft', {
        count: 88,
        phase: 'ready',
        sourceCount: 88,
        freshness: null,
        coverage: null,
        fetchedAt: null,
        pendingRegions: 0,
        notices: [],
        sources: [],
        error: null,
      });
      harness?.runtime.stores.workspace.getState().select({ feed: 'aircraft', id: 'FIN404' });
      harness?.runtime.stores.feeds
        .getState()
        .setSelectedEntity(
          { hex: 'FIN404', callsign: 'FINLAND404', altitudeM: 10200 },
          '2026-10-25T12:00:00Z',
          true,
        );
      harness?.runtime.stores.workspace.getState().setAnnotations([
        {
          id: 'post-1',
          kind: 'pin',
          label: 'Northern Station',
          note: 'Coastal Radar array',
          points: [{ lat: 60.1, lon: 24.5 }],
          createdAt: '2026-10-25T12:00:00.000Z',
          approximate: true,
        },
        {
          id: 'zone-1',
          kind: 'geofence',
          label: 'Maritime Border',
          note: 'Territorial water corridor',
          points: [
            { lat: 60, lon: 24 },
            { lat: 60, lon: 25 },
            { lat: 61, lon: 25 },
          ],
          createdAt: '2026-10-25T12:01:00.000Z',
          approximate: true,
        },
      ]);
    });

    renderWithRuntime(harness.runtime, <BriefingModal open={true} onOpenChange={vi.fn()} />);

    // Active layer in summary
    expect(screen.getByText('Commercial Air')).toBeInTheDocument();
    expect(screen.getByText('88')).toBeInTheDocument();

    // Geofence in summary
    expect(screen.getByText('Maritime Border')).toBeInTheDocument();

    // Target entity
    expect(screen.getAllByText('FIN404').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('FINLAND404')).toBeInTheDocument();
    expect(screen.getByText('10200')).toBeInTheDocument();

    // Manual annotation
    expect(screen.getByText('Northern Station')).toBeInTheDocument();
    expect(screen.getByText('Coastal Radar array')).toBeInTheDocument();

    // Mandatory attribution
    expect(screen.getByText('adsb.lol')).toBeInTheDocument();
    expect(screen.getByText(/Community ADS-B Exchange/)).toBeInTheDocument();
    expect(screen.getByText(/CC BY-SA 4.0/)).toBeInTheDocument();
  });

  it('copies Markdown to clipboard and displays success toast', async () => {
    const user = userEvent.setup();
    harness = await startHarness({ catalog: CATALOG, globe: true });

    let writtenText = '';
    const writeTextMock = vi.fn(async (text: string) => {
      writtenText = text;
    });
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: writeTextMock },
      configurable: true,
      writable: true,
    });

    renderWithRuntime(harness.runtime, <BriefingModal open={true} onOpenChange={vi.fn()} />);

    const copyBtn = screen.getByRole('button', { name: 'Copy Markdown' });
    await user.click(copyBtn);

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalled();
      expect(writtenText).toContain('# OSINT Situation Briefing');
      expect(writtenText).toContain('## Intelligence Summary');
    });

    const toasts = toastStore.getState().toasts;
    expect(toasts.some((t) => t.title === 'Markdown copied')).toBe(true);
  });

  it('triggers HTML dossier download and shows toast', async () => {
    const user = userEvent.setup();
    harness = await startHarness({ catalog: CATALOG, globe: true });

    const clickSpy = vi.fn();
    const origCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      const el = origCreateElement(tagName);
      if (tagName === 'a') {
        el.click = clickSpy;
      }
      return el;
    });

    renderWithRuntime(harness.runtime, <BriefingModal open={true} onOpenChange={vi.fn()} />);

    const downloadBtn = screen.getByRole('button', { name: 'Download HTML Dossier' });
    await user.click(downloadBtn);

    expect(clickSpy).toHaveBeenCalled();
    const toasts = toastStore.getState().toasts;
    expect(toasts.some((t) => t.title === 'HTML dossier downloaded')).toBe(true);
  });

  it('triggers JSON dossier download and shows toast', async () => {
    const user = userEvent.setup();
    harness = await startHarness({ catalog: CATALOG, globe: true });

    const clickSpy = vi.fn();
    const origCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      const el = origCreateElement(tagName);
      if (tagName === 'a') {
        el.click = clickSpy;
      }
      return el;
    });

    renderWithRuntime(harness.runtime, <BriefingModal open={true} onOpenChange={vi.fn()} />);

    const downloadJsonBtn = screen.getByRole('button', { name: 'Download JSON' });
    await user.click(downloadJsonBtn);

    expect(clickSpy).toHaveBeenCalled();
    const toasts = toastStore.getState().toasts;
    expect(toasts.some((t) => t.title === 'JSON dossier downloaded')).toBe(true);
  });

  it('controls modal visibility from store and allows closing', async () => {
    const user = userEvent.setup();
    harness = await startHarness({ catalog: CATALOG, globe: true });

    act(() => {
      harness?.runtime.stores.ui.getState().setBriefingOpen(true);
    });

    renderWithRuntime(harness.runtime, <BriefingModal />);

    expect(
      await screen.findByRole('dialog', { name: 'Intelligence Briefing Dossier' }),
    ).toBeInTheDocument();

    const doneBtn = screen.getByRole('button', { name: 'Done' });
    await user.click(doneBtn);

    expect(harness.runtime.stores.ui.getState().briefingOpen).toBe(false);
  });
});
