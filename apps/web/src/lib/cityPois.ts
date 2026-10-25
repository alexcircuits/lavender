/**
 * Preset tactical cities and their primary landmarks — ported from legacy/src/cityPois.js.
 * Plain data with zero Cesium dependencies so controls and search can import it directly.
 */
import type { BBox, GeoPoint } from '../contracts';

export interface CityPoi {
  readonly name: string;
  readonly position: GeoPoint;
  /** Range (distance from target in meters). */
  readonly alt: number;
  /** Camera tilt in degrees (negative = looking down). */
  readonly pitch: number;
  /** Optimal camera heading in degrees (0=N, 90=E, 180=S, 270=W). */
  readonly heading: number;
  /** Estimated height of landmark centre above ground (meters). */
  readonly buildingHeight: number;
}

export interface CityPreset {
  readonly id: string;
  readonly name: string;
  readonly groundElevation: number;
  readonly bbox: BBox;
  readonly pois: readonly CityPoi[];
}

export const CITY_PRESETS: readonly CityPreset[] = [
  {
    id: 'austin',
    name: 'Austin',
    groundElevation: 150,
    bbox: { west: -97.95, south: 30.1, east: -97.55, north: 30.52 },
    pois: [
      {
        name: 'Texas State Capitol',
        position: { lat: 30.2747, lon: -97.7403 },
        alt: 550,
        pitch: -28,
        heading: 180,
        buildingHeight: 35,
      },
      {
        name: 'Frost Bank Tower',
        position: { lat: 30.2674, lon: -97.7434 },
        alt: 550,
        pitch: -22,
        heading: 30,
        buildingHeight: 80,
      },
      {
        name: 'Pennybacker Bridge',
        position: { lat: 30.3451, lon: -97.7951 },
        alt: 500,
        pitch: -25,
        heading: 90,
        buildingHeight: 40,
      },
      {
        name: 'The Jenga Tower',
        position: { lat: 30.2642, lon: -97.75 },
        alt: 500,
        pitch: -18,
        heading: 45,
        buildingHeight: 60,
      },
      {
        name: 'UT Tower',
        position: { lat: 30.2862, lon: -97.7394 },
        alt: 500,
        pitch: -22,
        heading: 180,
        buildingHeight: 50,
      },
    ],
  },
  {
    id: 'sf',
    name: 'San Francisco',
    groundElevation: 15,
    bbox: { west: -122.53, south: 37.7, east: -122.35, north: 37.84 },
    pois: [
      {
        name: 'Golden Gate Bridge',
        position: { lat: 37.8199, lon: -122.4783 },
        alt: 1400,
        pitch: -20,
        heading: 45,
        buildingHeight: 100,
      },
      {
        name: 'Transamerica Pyramid',
        position: { lat: 37.7952, lon: -122.4028 },
        alt: 500,
        pitch: -25,
        heading: 30,
        buildingHeight: 85,
      },
      {
        name: 'Salesforce Tower',
        position: { lat: 37.7897, lon: -122.3972 },
        alt: 680,
        pitch: -25,
        heading: 330,
        buildingHeight: 100,
      },
      {
        name: 'Alcatraz Island',
        position: { lat: 37.8267, lon: -122.423 },
        alt: 800,
        pitch: -30,
        heading: 0,
        buildingHeight: 20,
      },
      {
        name: 'Coit Tower',
        position: { lat: 37.8024, lon: -122.4058 },
        alt: 420,
        pitch: -30,
        heading: 45,
        buildingHeight: 30,
      },
    ],
  },
  {
    id: 'nyc',
    name: 'New York',
    groundElevation: 10,
    bbox: { west: -74.259, south: 40.477, east: -73.7, north: 40.918 },
    pois: [
      {
        name: 'Statue of Liberty',
        position: { lat: 40.6892, lon: -74.0445 },
        alt: 450,
        pitch: -25,
        heading: 315,
        buildingHeight: 45,
      },
      {
        name: 'Empire State Building',
        position: { lat: 40.7484, lon: -73.9857 },
        alt: 850,
        pitch: -12,
        heading: 30,
        buildingHeight: 130,
      },
      {
        name: 'One World Trade Center',
        position: { lat: 40.7127, lon: -74.0134 },
        alt: 850,
        pitch: -25,
        heading: 0,
        buildingHeight: 170,
      },
      {
        name: 'Brooklyn Bridge',
        position: { lat: 40.7061, lon: -73.9969 },
        alt: 850,
        pitch: -25,
        heading: 45,
        buildingHeight: 40,
      },
      {
        name: 'Chrysler Building',
        position: { lat: 40.7516, lon: -73.9755 },
        alt: 700,
        pitch: -20,
        heading: 225,
        buildingHeight: 100,
      },
    ],
  },
  {
    id: 'tokyo',
    name: 'Tokyo',
    groundElevation: 40,
    bbox: { west: 139.55, south: 35.52, east: 139.92, north: 35.9 },
    pois: [
      {
        name: 'Tokyo Tower',
        position: { lat: 35.6586, lon: 139.7454 },
        alt: 850,
        pitch: -25,
        heading: 0,
        buildingHeight: 110,
      },
      {
        name: 'Tokyo Skytree',
        position: { lat: 35.7101, lon: 139.8107 },
        alt: 900,
        pitch: -25,
        heading: 30,
        buildingHeight: 200,
      },
      {
        name: 'Imperial Palace',
        position: { lat: 35.6852, lon: 139.7528 },
        alt: 900,
        pitch: -35,
        heading: 0,
        buildingHeight: 20,
      },
      {
        name: 'Senso-ji Temple',
        position: { lat: 35.7148, lon: 139.7967 },
        alt: 400,
        pitch: -30,
        heading: 180,
        buildingHeight: 25,
      },
      {
        name: 'Mode Gakuen Cocoon Tower',
        position: { lat: 35.6929, lon: 139.6925 },
        alt: 350,
        pitch: -20,
        heading: 30,
        buildingHeight: 70,
      },
    ],
  },
  {
    id: 'london',
    name: 'London',
    groundElevation: 15,
    bbox: { west: -0.51, south: 51.28, east: 0.33, north: 51.7 },
    pois: [
      {
        name: 'Tower Bridge',
        position: { lat: 51.5055, lon: -0.0754 },
        alt: 400,
        pitch: -25,
        heading: 270,
        buildingHeight: 65,
      },
      {
        name: 'The Shard',
        position: { lat: 51.5045, lon: -0.0865 },
        alt: 850,
        pitch: -20,
        heading: 0,
        buildingHeight: 100,
      },
      {
        name: 'Big Ben / Parliament',
        position: { lat: 51.5007, lon: -0.1246 },
        alt: 600,
        pitch: -25,
        heading: 180,
        buildingHeight: 50,
      },
      {
        name: "St. Paul's Cathedral",
        position: { lat: 51.5138, lon: -0.0984 },
        alt: 400,
        pitch: -30,
        heading: 270,
        buildingHeight: 55,
      },
      {
        name: 'The Gherkin',
        position: { lat: 51.5145, lon: -0.0803 },
        alt: 350,
        pitch: -20,
        heading: 30,
        buildingHeight: 60,
      },
    ],
  },
  {
    id: 'paris',
    name: 'Paris',
    groundElevation: 35,
    bbox: { west: 2.224, south: 48.815, east: 2.47, north: 48.902 },
    pois: [
      {
        name: 'Eiffel Tower',
        position: { lat: 48.8584, lon: 2.2945 },
        alt: 750,
        pitch: -25,
        heading: 315,
        buildingHeight: 150,
      },
      {
        name: 'Arc de Triomphe',
        position: { lat: 48.8738, lon: 2.295 },
        alt: 400,
        pitch: -28,
        heading: 45,
        buildingHeight: 25,
      },
      {
        name: 'Notre-Dame',
        position: { lat: 48.853, lon: 2.3499 },
        alt: 400,
        pitch: -25,
        heading: 225,
        buildingHeight: 35,
      },
      {
        name: 'Sacré-Cœur',
        position: { lat: 48.8867, lon: 2.3431 },
        alt: 400,
        pitch: -30,
        heading: 180,
        buildingHeight: 40,
      },
      {
        name: 'Louvre Pyramid',
        position: { lat: 48.8606, lon: 2.3376 },
        alt: 500,
        pitch: -35,
        heading: 0,
        buildingHeight: 10,
      },
    ],
  },
  {
    id: 'dubai',
    name: 'Dubai',
    groundElevation: 5,
    bbox: { west: 54.9, south: 24.95, east: 55.55, north: 25.35 },
    pois: [
      {
        name: 'Burj Khalifa',
        position: { lat: 25.1972, lon: 55.2744 },
        alt: 600,
        pitch: -20,
        heading: 200,
        buildingHeight: 270,
      },
      {
        name: 'Burj Al Arab',
        position: { lat: 25.1412, lon: 55.1853 },
        alt: 500,
        pitch: -25,
        heading: 90,
        buildingHeight: 100,
      },
      {
        name: 'Palm Jumeirah',
        position: { lat: 25.1124, lon: 55.139 },
        alt: 1200,
        pitch: -40,
        heading: 0,
        buildingHeight: 20,
      },
      {
        name: 'Dubai Frame',
        position: { lat: 25.235, lon: 55.3003 },
        alt: 400,
        pitch: -25,
        heading: 270,
        buildingHeight: 75,
      },
      {
        name: 'Museum of the Future',
        position: { lat: 25.2197, lon: 55.2806 },
        alt: 350,
        pitch: -20,
        heading: 30,
        buildingHeight: 35,
      },
    ],
  },
  {
    id: 'dc',
    name: 'Washington DC',
    groundElevation: 10,
    bbox: { west: -77.12, south: 38.79, east: -76.91, north: 38.995 },
    pois: [
      {
        name: 'US Capitol',
        position: { lat: 38.8897, lon: -77.0091 },
        alt: 550,
        pitch: -25,
        heading: 270,
        buildingHeight: 45,
      },
      {
        name: 'Washington Monument',
        position: { lat: 38.8895, lon: -77.0353 },
        alt: 500,
        pitch: -30,
        heading: 0,
        buildingHeight: 85,
      },
      {
        name: 'Lincoln Memorial',
        position: { lat: 38.8893, lon: -77.0502 },
        alt: 400,
        pitch: -25,
        heading: 90,
        buildingHeight: 20,
      },
      {
        name: 'Pentagon',
        position: { lat: 38.8711, lon: -77.0559 },
        alt: 800,
        pitch: -40,
        heading: 0,
        buildingHeight: 20,
      },
      {
        name: 'Jefferson Memorial',
        position: { lat: 38.8814, lon: -77.0365 },
        alt: 400,
        pitch: -30,
        heading: 0,
        buildingHeight: 25,
      },
    ],
  },
  {
    id: 'tallinn',
    name: 'Tallinn',
    groundElevation: 15,
    bbox: { west: 24.6, south: 59.36, east: 24.92, north: 59.52 },
    pois: [
      {
        name: 'Viru Square',
        position: { lat: 59.4366, lon: 24.7527 },
        alt: 450,
        pitch: -28,
        heading: 60,
        buildingHeight: 25,
      },
      {
        name: 'Old Town / Raekoja plats',
        position: { lat: 59.4372, lon: 24.7452 },
        alt: 400,
        pitch: -30,
        heading: 180,
        buildingHeight: 20,
      },
      {
        name: 'Teatri väljak',
        position: { lat: 59.4344, lon: 24.7514 },
        alt: 400,
        pitch: -25,
        heading: 220,
        buildingHeight: 25,
      },
      {
        name: 'Port of Tallinn',
        position: { lat: 59.4445, lon: 24.7675 },
        alt: 700,
        pitch: -30,
        heading: 90,
        buildingHeight: 20,
      },
      {
        name: 'Ülemiste',
        position: { lat: 59.421, lon: 24.792 },
        alt: 600,
        pitch: -28,
        heading: 45,
        buildingHeight: 30,
      },
    ],
  },
];

export function cityPresetById(id: string): CityPreset | undefined {
  return CITY_PRESETS.find((city) => city.id === id);
}
