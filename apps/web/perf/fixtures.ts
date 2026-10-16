/**
 * Dense SYNTHETIC fixtures for the performance harness (ENG-1000), replayed by
 * `lavender-server` in fixture mode (`crates/lavender-core/src/providers/fixture.rs`).
 *
 * Every number comes from a seeded generator, so two runs write identical files. Nothing
 * here is a real flight, satellite or fire: callsigns start with `SYN`, satellites are named
 * `SYNTHETIC nnnnn` and the fires use the `SYN-N20` satellite name. Earthquakes are the
 * recorded USGS feed and cyclones the synthetic `TEST ALPHA` advisory, both copied from
 * `e2e/fixtures/`.
 *
 * CLI (Node >= 22.12): `node --experimental-strip-types perf/fixtures.ts --out <dir>
 * [--aircraft 10000] [--satellites 5000] [--fires 100000]`
 */
import { copyFileSync, mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crc32, deflateSync } from 'node:zlib';

/** Spec scenarios: ENG-974 (10 000 aircraft), ENG-976 (5 000 satellites), GEV's FIRMS pass. */
export const DEFAULT_COUNTS = { aircraft: 10_000, satellites: 5_000, fires: 100_000 } as const;
export const SEED = 1000;
const E2E_FIXTURES = fileURLToPath(new URL('../e2e/fixtures/', import.meta.url));
/** Copied unchanged: the recorded USGS feed and the synthetic cyclone advisory. */
const COPIED = [
  'usgs.geojson',
  ...['status', 'points', 'track', 'cone'].map((n) => `cyclones-${n}.json`),
];

/** mulberry32: a tiny, well-known 32-bit generator. */
export function generator(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

/** A latitude drawn uniformly by area between ±limit degrees. */
function areaLatitude(random: () => number, limit: number): number {
  const s = Math.sin((limit * Math.PI) / 180);
  return (Math.asin((2 * random() - 1) * s) * 180) / Math.PI;
}

const round = (value: number, digits: number) => Number(value.toFixed(digits));
const wrapLon = (lon: number) => ((((lon + 180) % 360) + 360) % 360) - 180;

/**
 * An adsb.lol v2 document of `count` worldwide contacts. `variant` moves every contact a
 * little along its track (about 2 km per step), so variant 1 → 2 is a 100 % position update.
 */
export function aircraftDocument(count: number, variant = 0): string {
  const random = generator(SEED + 1);
  const ac = Array.from({ length: count }, (_, i) => {
    const lat0 = areaLatitude(random, 70);
    const lon0 = random() * 360 - 180;
    const track = round(random() * 360, 1);
    const altitude = Math.round(1_000 + random() * 40_000);
    const onGround = random() < 0.03;
    const military = random() < 0.02;
    const stepDeg = variant * 0.02;
    const heading = (track * Math.PI) / 180;
    const lat = lat0 + stepDeg * Math.cos(heading);
    const lon = wrapLon(lon0 + (stepDeg * Math.sin(heading)) / Math.cos((lat0 * Math.PI) / 180));
    const n = String(i).padStart(5, '0');
    return {
      hex: (0x100000 + i).toString(16),
      type: 'adsb_icao',
      flight: `SYN${n} `,
      r: `SY-${n}`,
      t: 'A320',
      category: 'A3',
      lat: round(lat, 5),
      lon: round(lon, 5),
      alt_baro: onGround ? 'ground' : altitude,
      alt_geom: onGround ? undefined : altitude + 175,
      gs: round(onGround ? random() * 20 : 120 + random() * 380, 1),
      track,
      baro_rate: Math.round(random() * 4_000 - 2_000),
      squawk: String(Math.floor(random() * 7_777)).padStart(4, '0'),
      emergency: 'none',
      nic: 8,
      rc: 186,
      version: 2,
      nac_p: 9,
      sil: 3,
      messages: 1_000 + i,
      seen_pos: round(random() * 2, 1),
      seen: round(random(), 1),
      rssi: round(-30 + random() * 15, 1),
      ...(military ? { dbFlags: 1 } : {}),
    };
  });
  return JSON.stringify({ ac, msg: 'No error', now: 1_791_633_600_000, total: count });
}

/** CelesTrak OMM element sets: mostly low Earth orbits, some navigation-like and GEO. */
export function satelliteDocument(count: number): string {
  const random = generator(SEED + 2);
  const sets = Array.from({ length: count }, (_, i) => {
    const kind = random();
    const meanMotion = kind < 0.85 ? 13 + random() * 3 : kind < 0.95 ? 2.0057 : 1.0027;
    return {
      OBJECT_NAME: `SYNTHETIC ${String(i).padStart(5, '0')}`,
      OBJECT_ID: `2026-${String(100 + Math.floor(i / 26)).padStart(3, '0')}${String.fromCharCode(65 + (i % 26))}`,
      EPOCH: '2026-10-10T12:00:00.000000',
      MEAN_MOTION: round(meanMotion, 8),
      ECCENTRICITY: round(0.0001 + random() * 0.01, 7),
      INCLINATION: round(random() * 99, 4),
      RA_OF_ASC_NODE: round(random() * 360, 4),
      ARG_OF_PERICENTER: round(random() * 360, 4),
      MEAN_ANOMALY: round(random() * 360, 4),
      EPHEMERIS_TYPE: 0,
      CLASSIFICATION_TYPE: 'U',
      NORAD_CAT_ID: 100_000 + i,
      ELEMENT_SET_NO: 999,
      REV_AT_EPOCH: 1_000,
      BSTAR: round(random() * 0.0001, 8),
      MEAN_MOTION_DOT: 0,
      MEAN_MOTION_DDOT: 0,
    };
  });
  return JSON.stringify(sets);
}

/** FIRMS-shaped detections in about one cluster per 28 detections, like GEV's 100 430 in 3 557 cells. */
export function firesCsv(count: number): string {
  const random = generator(SEED + 3);
  const clusters = Array.from({ length: Math.max(1, Math.round(count / 28)) }, () => ({
    lat: -40 + random() * 105,
    lon: random() * 360 - 180,
  }));
  const rows = [
    'latitude,longitude,bright_ti4,acq_date,acq_time,satellite,confidence,frp,daynight',
  ];
  for (let i = 0; i < count; i += 1) {
    const centre = clusters[i % clusters.length] ?? { lat: 0, lon: 0 };
    const lat = round(centre.lat + (random() - 0.5) * 0.4, 4);
    const lon = round(wrapLon(centre.lon + (random() - 0.5) * 0.4), 4);
    const minute = String(Math.floor(random() * 60)).padStart(2, '0');
    const confidence = random() < 0.2 ? 'h' : random() < 0.8 ? 'n' : 'l';
    const frp = round(1 + random() ** 3 * 400, 1);
    rows.push(
      `${lat},${lon},${Math.round(300 + random() * 80)},2026-10-14,11${minute},SYN-N20,${confidence},${frp},D`,
    );
  }
  return `${rows.join('\n')}\n`;
}

function chunk(type: string, data: Buffer): Buffer {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'latin1'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

/**
 * A 512×512 RGB PNG standing in for one GIBS 500 m tile (same pixel size; night-lights-like
 * dark noise with sparse lights, a few hundred KB like real imagery, not a 1-pixel stub).
 */
export function imageryTile(size = 512): Buffer {
  const random = generator(SEED + 4);
  const row = size * 3 + 1;
  const raw = Buffer.alloc(row * size);
  for (let y = 0; y < size; y += 1) {
    raw[y * row] = 0;
    for (let x = 0; x < size; x += 1) {
      const lit = random() < 0.01 ? 120 + Math.floor(random() * 135) : 0;
      const noise = Math.floor(random() * 6);
      const offset = y * row + 1 + x * 3;
      raw[offset] = 8 + noise + lit;
      raw[offset + 1] = 10 + noise + Math.floor(lit * 0.85);
      raw[offset + 2] = 22 + noise + Math.floor(lit * 0.5);
    }
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(raw, { level: 6 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/** Writes atomically: the server re-reads fixtures every 30 s and must never see half a file. */
function writeAtomic(path: string, data: string | Buffer): void {
  writeFileSync(`${path}.tmp`, data);
  renameSync(`${path}.tmp`, path);
}

export function writeAircraft(dir: string, count: number, variant = 0): void {
  writeAtomic(join(dir, 'adsb-lol.json'), aircraftDocument(count, variant));
}

export interface FixtureCounts {
  readonly aircraft: number;
  readonly satellites: number;
  readonly fires: number;
}

export function writeFixtures(dir: string, counts: FixtureCounts = DEFAULT_COUNTS): void {
  mkdirSync(dir, { recursive: true });
  for (const name of COPIED) {
    copyFileSync(join(E2E_FIXTURES, name), join(dir, name));
  }
  writeAircraft(dir, counts.aircraft);
  writeAtomic(join(dir, 'celestrak-synthetic.json'), satelliteDocument(counts.satellites));
  writeAtomic(join(dir, 'firms.csv'), firesCsv(counts.fires));
  writeAtomic(join(dir, 'tile.png'), imageryTile());
  writeAtomic(
    join(dir, 'README.md'),
    [
      '# SYNTHETIC performance fixtures',
      '',
      `Generated by apps/web/perf/fixtures.ts (seed ${SEED}). Not live data and not real objects:`,
      `${counts.aircraft} aircraft (adsb-lol.json), ${counts.satellites} satellites`,
      `(celestrak-synthetic.json), ${counts.fires} fire detections (firms.csv), one 512×512`,
      'imagery tile (tile.png). usgs.geojson is the recorded USGS 2.5_day feed (45 events);',
      'the cyclone files are the synthetic TEST ALPHA advisory from e2e/fixtures.',
      '',
    ].join('\n'),
  );
}

function main(argv: readonly string[]): void {
  const value = (flag: string) => {
    const index = argv.indexOf(flag);
    return index >= 0 ? argv[index + 1] : undefined;
  };
  const out = value('--out');
  if (!out)
    throw new Error(
      'usage: node --experimental-strip-types perf/fixtures.ts --out <dir> [--aircraft N] ...',
    );
  const count = (flag: string, fallback: number) => {
    const n = Number(value(flag) ?? fallback);
    if (!Number.isInteger(n) || n < 0 || n > 200_000) throw new Error(`${flag}: 0–200000`);
    return n;
  };
  const counts = {
    aircraft: count('--aircraft', DEFAULT_COUNTS.aircraft),
    satellites: count('--satellites', DEFAULT_COUNTS.satellites),
    fires: count('--fires', DEFAULT_COUNTS.fires),
  };
  writeFixtures(out, counts);
  console.log(`perf fixtures: ${JSON.stringify(counts)} → ${out}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main(process.argv);
