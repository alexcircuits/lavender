import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { EvidencePack } from '../../contracts';
import { ScenesPanel } from './ScenesPanel';

const packs: EvidencePack[] = [
  {
    id: 'p1',
    title: 'Pack One',
    summary: 'Sum',
    region: 'Nepal',
    requiredLayers: [],
    shots: [
      {
        id: 's1',
        title: 'Shot A',
        durationS: 1,
        holdS: 1,
        mapStack: 'osm',
        pose: { lat: 28, lon: 85, altM: 5000, headingDeg: 0, pitchDeg: -45, rollDeg: 0 },
      },
    ],
  },
];

const harnesses: Harness[] = [];
afterEach(() => {
  for (const h of harnesses.splice(0)) h.stop();
});

describe('ScenesPanel', () => {
  it('lists packs, shows shots and flies the camera on play', async () => {
    const h = await startHarness({ globe: true });
    harnesses.push(h);
    renderWithRuntime(h.runtime, <ScenesPanel packs={packs} />);
    const user = userEvent.setup();
    await user.click(screen.getByText('Pack One'));
    expect(screen.getByText('Shot A')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Play' }));
    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'point',
      position: { lat: 28, lon: 85 },
      heightM: 5000,
    });
    expect(screen.getByRole('button', { name: 'Pause' })).toBeInTheDocument();
  });
});
