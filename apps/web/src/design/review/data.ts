/**
 * Sample content for the design review. Realistic in shape and voice (sources, cadences, units,
 * identifiers), but not live data: nothing here is fetched.
 */
import type { FeedStatus } from '../primitives/StatusPill';

export type Family = 'air' | 'maritime' | 'space' | 'hazard' | 'context';

export interface ReviewLayer {
  id: string;
  name: string;
  family: Family;
  source: string;
  status: FeedStatus;
  statusDetail?: string;
  count: number | null;
  ageSeconds: number | null;
  cadenceSeconds: number;
  on: boolean;
  /** Toggle unavailable (not configured, licence policy). */
  locked?: boolean;
}

export interface LayerGroup {
  label: string;
  layers: ReadonlyArray<ReviewLayer>;
}

export const LAYER_GROUPS: ReadonlyArray<LayerGroup> = [
  {
    label: 'Air',
    layers: [
      {
        id: 'aircraft',
        name: 'Aircraft',
        family: 'air',
        source: 'adsb.lol',
        status: 'live',
        count: 12486,
        ageSeconds: 4,
        cadenceSeconds: 10,
        on: true,
      },
    ],
  },
  {
    label: 'Maritime',
    layers: [
      {
        id: 'vessels',
        name: 'Vessels',
        family: 'maritime',
        source: 'AISStream.io',
        status: 'licence',
        count: null,
        ageSeconds: null,
        cadenceSeconds: 60,
        on: false,
        locked: true,
      },
    ],
  },
  {
    label: 'Space',
    layers: [
      {
        id: 'satellites',
        name: 'Satellites',
        family: 'space',
        source: 'CelesTrak',
        status: 'stale',
        statusDetail: '4 min',
        count: 9214,
        ageSeconds: 250,
        cadenceSeconds: 120,
        on: true,
      },
    ],
  },
  {
    label: 'Hazards',
    layers: [
      {
        id: 'earthquakes',
        name: 'Earthquakes',
        family: 'hazard',
        source: 'USGS',
        status: 'live',
        count: 37,
        ageSeconds: 95,
        cadenceSeconds: 60,
        on: true,
      },
      {
        id: 'fires',
        name: 'Active fires',
        family: 'hazard',
        source: 'NASA FIRMS',
        status: 'not-configured',
        count: null,
        ageSeconds: null,
        cadenceSeconds: 600,
        on: false,
        locked: true,
      },
      {
        id: 'cyclones',
        name: 'Tropical cyclones',
        family: 'hazard',
        source: 'NOAA NHC',
        status: 'unavailable',
        count: 2,
        ageSeconds: 2460,
        cadenceSeconds: 300,
        on: true,
      },
    ],
  },
  {
    label: 'Infrastructure',
    layers: [
      {
        id: 'cables',
        name: 'Submarine cables',
        family: 'context',
        source: 'TeleGeography',
        status: 'degraded',
        statusDetail: 'polling',
        count: 552,
        ageSeconds: 3000,
        cadenceSeconds: 86400,
        on: false,
      },
    ],
  },
];

export interface Fact {
  label: string;
  value: string;
  mono?: boolean;
}

export const INSPECTED_AIRCRAFT = {
  callsign: 'SAS1462',
  summary: 'Airbus A320neo · Helsinki → Stockholm',
  facts: [
    { label: 'Callsign', value: 'SAS1462', mono: true },
    { label: 'ICAO address', value: '4aca2d', mono: true },
    { label: 'Registration', value: 'SE-ROM', mono: true },
    { label: 'Type', value: 'A20N', mono: true },
    { label: 'Squawk', value: '2341', mono: true },
    { label: 'Altitude', value: '36 000 ft' },
    { label: 'Ground speed', value: '451 kn' },
    { label: 'Track', value: '214°' },
    { label: 'Vertical rate', value: '−64 ft/min' },
    { label: 'Position', value: '60.1699° N, 24.9384° E', mono: true },
    { label: 'MGRS', value: '35VLG 85390 72861', mono: true },
  ] satisfies ReadonlyArray<Fact>,
  provenance: [
    { label: 'Source', value: 'adsb.lol' },
    { label: 'Observed', value: '14:02:03 UTC', mono: true },
    { label: 'Fetched', value: '14:02:05 UTC', mono: true },
    { label: 'Age', value: '8 s' },
  ] satisfies ReadonlyArray<Fact>,
};

export interface Contact {
  callsign: string;
  type: string;
  altitude: string;
  age: string;
}

export const NEARBY_CONTACTS: ReadonlyArray<Contact> = [
  { callsign: 'SAS1462', type: 'A20N', altitude: '36 000 ft', age: '8 s' },
  { callsign: 'FIN7LY', type: 'A359', altitude: '38 975 ft', age: '3 s' },
  { callsign: 'NAX5KM', type: 'B38M', altitude: '24 300 ft', age: '12 s' },
  { callsign: 'DLH2TA', type: 'A321', altitude: '11 450 ft', age: '5 s' },
  { callsign: 'OHJLM', type: 'C172', altitude: '2 100 ft', age: '41 s' },
];

export interface Quake {
  id: string;
  time: string;
  magnitude: number;
  depthKm: number;
  place: string;
}

const PLACES = [
  'Off the east coast of Honshu, Japan',
  'Kermadec Islands region',
  'Central Mid-Atlantic Ridge',
  '38 km NE of Hualien City, Taiwan',
  'Southern Peru',
  'Fiji region',
  'Andreanof Islands, Aleutian Islands, Alaska',
  'Reykjanes Peninsula, Iceland',
  'Antofagasta, Chile',
  '12 km SSW of Ridgecrest, CA',
  'Central Italy',
  'Banda Sea',
];

/** 1 000 deterministic earthquakes for the virtualised table. */
export function sampleQuakes(count = 1000): Quake[] {
  const start = Date.UTC(2026, 9, 12, 14, 0, 0);
  return Array.from({ length: count }, (_, i) => {
    const seconds = i * 397;
    const at = new Date(start - seconds * 1000);
    return {
      id: `us7000${(1_200_000 + i * 7919).toString(36).slice(-4)}`,
      time: `${at.toISOString().slice(0, 10)} ${at.toISOString().slice(11, 19)}`,
      magnitude: Math.round((2.5 + ((i * 37) % 47) / 10) * 10) / 10,
      depthKm: Math.round(((i * 53) % 620) + 5),
      place: PLACES[i % PLACES.length] ?? 'Unknown region',
    };
  });
}
