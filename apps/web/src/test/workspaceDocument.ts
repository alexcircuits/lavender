import type { WorkspaceDocument } from '../state/workspaceDocument';

export const document: WorkspaceDocument = {
  schemaVersion: 2,
  camera: {
    lon: 24.94,
    lat: 60.17,
    heightM: 400_000,
    headingRad: 0.3,
    pitchRad: -1.2,
    rollRad: 0.1,
  },
  layers: [
    { id: 'aircraft', enabled: true, options: { militaryOnly: false } },
    { id: 'satellites', enabled: true, options: { groups: ['stations'] } },
    { id: 'earthquakes', enabled: false, options: { minMagnitude: 4.5 } },
  ],
  imageryId: 'offline',
  selection: { feed: 'aircraft', id: 'Київ ✈️' },
  annotations: [],
};
