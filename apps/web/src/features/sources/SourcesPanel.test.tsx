import { screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, source, startHarness } from '../../app/testing';
import type { Catalog, SystemStatus } from '../../contracts';
import { catalogOf, layer } from '../../test/fakes';
import { SourcesPanel } from './SourcesPanel';

const SOURCES = [
  source('usgs-earthquakes', {
    name: 'USGS Earthquake Hazards Program',
    homepage: 'https://earthquake.usgs.gov/',
    attribution: 'Earthquake data: U.S. Geological Survey (public domain)',
    licence: { name: 'Public domain', url: 'https://www.usgs.gov/information-policies' },
  }),
  source('adsb-lol', {
    name: 'adsb.lol',
    homepage: 'http://adsb.lol/',
    attribution: 'Aircraft data © adsb.lol contributors, ODbL 1.0',
    licence: { name: 'ODbL 1.0', url: 'javascript:alert(1)' },
    commercial: 'requiresAgreement',
    requiresKey: true,
  }),
  source('celestrak', {
    name: 'CelesTrak',
    attribution: 'Orbital elements courtesy of CelesTrak — “as is”, <no warranty>',
    commercial: 'nonCommercialOnly',
    privacy: 'sensitive',
  }),
];

const CATALOG: Catalog = catalogOf(
  [
    layer('earthquakes', { name: 'Earthquakes', sourceIds: ['usgs-earthquakes'] }),
    layer('aircraft', {
      name: 'Aircraft',
      sourceIds: ['adsb-lol'],
      availability: { state: 'unconfigured', missing: ['ADSB_LOL_KEY'] },
    }),
    layer('satellites', {
      name: 'Satellites',
      sourceIds: ['celestrak'],
      availability: { state: 'disabledByLicence' },
    }),
  ],
  { sources: SOURCES, licenceMode: 'commercial' },
);

const STATUS: SystemStatus = {
  version: '0.1.0',
  licenceMode: 'commercial',
  startedAt: '2026-10-11T00:00:00Z',
  feeds: [],
  sources: [
    {
      sourceId: 'usgs-earthquakes',
      state: 'healthy',
      lastSuccess: new Date(Date.now() - 95_000).toISOString(),
      consecutiveFailures: 0,
    },
    {
      sourceId: 'adsb-lol',
      state: 'failing',
      lastError: 'The provider answered 503 Service Unavailable.',
      consecutiveFailures: 4,
    },
  ],
};

let harness: Harness | null = null;

async function setup(catalog: Catalog = CATALOG, status: SystemStatus = STATUS) {
  harness = await startHarness({ catalog, status });
  renderWithRuntime(harness.runtime, <SourcesPanel catalog={catalog} />);
}

const card = (name: string) => {
  const heading = screen.getByRole('heading', { level: 3, name });
  const article = heading.closest('article');
  if (!article) throw new Error(`no card for ${name}`);
  return article;
};

afterEach(() => {
  harness?.stop();
  harness = null;
});

describe('SourcesPanel', () => {
  it('renders every source’s attribution verbatim', async () => {
    await setup();
    for (const s of CATALOG.sources) {
      const text = within(card(s.name)).getByText(s.attribution);
      expect(text.textContent).toBe(s.attribution);
    }
  });

  it('links only https homepages and licences, in a new tab without an opener', async () => {
    await setup();
    const usgs = card('USGS Earthquake Hazards Program');
    const home = within(usgs).getByRole('link', { name: /earthquake\.usgs\.gov/ });
    expect(home).toHaveAttribute('href', 'https://earthquake.usgs.gov/');
    expect(home).toHaveAttribute('target', '_blank');
    expect(home).toHaveAttribute('rel', 'noopener noreferrer');

    const adsb = card('adsb.lol');
    expect(within(adsb).queryAllByRole('link')).toEqual([]);
    expect(within(adsb).getByText('http://adsb.lol/')).toBeInTheDocument();
    expect(within(adsb).getByText('ODbL 1.0')).toBeInTheDocument();
  });

  it('writes licence terms, keys and privacy in words', async () => {
    await setup();
    const adsb = card('adsb.lol');
    expect(within(adsb).getByText('Needs an agreement')).toBeInTheDocument();
    expect(within(adsb).getByText('Needs a key')).toBeInTheDocument();
    expect(within(card('CelesTrak')).getByText('Non-commercial only')).toBeInTheDocument();
    expect(within(card('CelesTrak')).getByText('Sensitive: off by default')).toBeInTheDocument();
    expect(
      within(card('USGS Earthquake Hazards Program')).getByText('No key needed'),
    ).toBeVisible();
  });

  it('explains the licence policy for sources whose layers it turned off', async () => {
    await setup();
    expect(
      within(card('CelesTrak')).getByText(
        'Turned off by the licence policy: this source does not allow commercial use, and Lavender is in commercial mode.',
      ),
    ).toBeInTheDocument();
    expect(
      within(card('USGS Earthquake Hazards Program')).queryByText(/licence policy/),
    ).not.toBeInTheDocument();
    expect(within(card('adsb.lol')).getByText(/the server needs ADSB_LOL_KEY/)).toBeInTheDocument();
  });

  it('shows each source’s health from the system status', async () => {
    await setup();
    const usgs = card('USGS Earthquake Hazards Program');
    expect(within(usgs).getByText('Healthy')).toBeInTheDocument();
    expect(within(usgs).getByText('1 min ago')).toBeInTheDocument();
    const adsb = card('adsb.lol');
    expect(within(adsb).getByText('Failing')).toBeInTheDocument();
    expect(
      within(adsb).getByText(
        'The provider answered 503 Service Unavailable. (4 failures in a row)',
      ),
    ).toBeInTheDocument();
    expect(within(card('CelesTrak')).getByText('Health unknown')).toBeInTheDocument();
  });

  it('explains the licence mode', async () => {
    await setup();
    expect(screen.getByText('Commercial mode')).toBeInTheDocument();
  });

  it('explains evaluation mode', async () => {
    await setup(
      { ...CATALOG, licenceMode: 'evaluation' },
      { ...STATUS, licenceMode: 'evaluation' },
    );
    expect(screen.getByText('Evaluation mode')).toBeInTheDocument();
    expect(screen.getByText(/not in a product or service you sell/)).toBeInTheDocument();
  });
});
