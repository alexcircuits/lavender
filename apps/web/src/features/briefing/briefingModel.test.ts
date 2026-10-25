import { describe, expect, it } from 'vitest';
import { source } from '../../app/testing';
import type { Catalog } from '../../contracts';
import { catalogOf, fakeGlobe, layer } from '../../test/fakes';
import type { MapAnnotation } from '../annotations/model';
import {
  compileBriefing,
  defaultBriefingTitle,
  exportToHtml,
  exportToJson,
  exportToMarkdown,
  extractEntityDetails,
  slugify,
} from './briefingModel';

describe('briefingModel', () => {
  const SOURCE_USGS = source('usgs-earthquakes', {
    name: 'USGS Earthquakes',
    attribution: 'U.S. Geological Survey (USGS)',
    licence: { name: 'Public Domain', url: 'https://www.usgs.gov' },
  });
  const SOURCE_ADSB = source('adsb-lol', {
    name: 'adsb.lol',
    attribution: 'Community ADS-B Exchange',
    licence: { name: 'CC BY-SA 4.0', url: 'https://creativecommons.org' },
  });

  const CATALOG: Catalog = catalogOf(
    [
      layer('earthquakes', { name: 'Earthquakes', sourceIds: ['usgs-earthquakes'] }),
      layer('aircraft', { name: 'Commercial Air', sourceIds: ['adsb-lol'] }),
      layer('satellites', { name: 'Orbiting Satellites', sourceIds: [] }),
    ],
    { sources: [SOURCE_USGS, SOURCE_ADSB] },
  );

  const ANNOTATIONS: readonly MapAnnotation[] = [
    {
      id: 'ann-1',
      kind: 'pin',
      points: [{ lat: 60.1, lon: 24.9 }],
      label: 'Forward Observation Post',
      note: 'Monitored radar array',
      createdAt: '2026-10-25T12:00:00.000Z',
      approximate: true,
    },
    {
      id: 'fence-1',
      kind: 'geofence',
      points: [
        { lat: 60.0, lon: 24.0 },
        { lat: 60.5, lon: 24.0 },
        { lat: 60.5, lon: 25.0 },
        { lat: 60.0, lon: 25.0 },
      ],
      label: 'Gulf of Finland Sector A',
      note: 'Restricted maritime corridor',
      createdAt: '2026-10-25T12:05:00.000Z',
      approximate: true,
    },
  ];

  describe('extractEntityDetails', () => {
    it('extracts primitive number and string fields', () => {
      const entity = {
        hex: '4B821A',
        callsign: 'FIN902',
        altitudeM: 10600,
        speedMps: 240,
        nested: { foo: 'bar' },
        tags: ['military'],
      };
      const details = extractEntityDetails(entity);
      expect(details).toEqual({
        hex: '4B821A',
        callsign: 'FIN902',
        altitudeM: 10600,
        speedMps: 240,
      });
    });

    it('returns undefined for invalid or non-object entities', () => {
      expect(extractEntityDetails(null)).toBeUndefined();
      expect(extractEntityDetails(undefined)).toBeUndefined();
      expect(extractEntityDetails('string')).toBeUndefined();
      expect(extractEntityDetails([1, 2, 3])).toBeUndefined();
      expect(extractEntityDetails({})).toBeUndefined();
    });
  });

  describe('compileBriefing', () => {
    it('uses default title with date when title is omitted or blank', () => {
      const fixedDate = '2026-10-25T15:30:00.000Z';
      const dossier = compileBriefing({ generatedAt: fixedDate });
      expect(dossier.title).toBe('OSINT Situation Briefing — 2026-10-25');
      expect(dossier.generatedAt).toBe(fixedDate);
    });

    it('preserves custom title and analyst notes', () => {
      const dossier = compileBriefing({
        title: 'Operation Northern Sentinel — SITREP 04',
        analystNotes: 'Immediate escalation observed in coastal sector.',
      });
      expect(dossier.title).toBe('Operation Northern Sentinel — SITREP 04');
      expect(dossier.analystNotes).toBe('Immediate escalation observed in coastal sector.');
    });

    it('extracts camera pose from globe controller', () => {
      const { globe } = fakeGlobe();
      const dossier = compileBriefing({ globe });
      expect(dossier.camera).toEqual({
        lat: 20,
        lon: 10,
        heightM: 20_000_000,
      });
    });

    it('accepts explicit camera parameters', () => {
      const dossier = compileBriefing({
        camera: { lat: 51.5074, lon: -0.1278, heightM: 50_000 },
      });
      expect(dossier.camera).toEqual({
        lat: 51.5074,
        lon: -0.1278,
        heightM: 50_000,
      });
    });

    it('gathers active layers with item counts from summaries', () => {
      const dossier = compileBriefing({
        catalog: CATALOG,
        layers: {
          earthquakes: { enabled: true, filters: {} },
          satellites: { enabled: false, filters: {} },
          aircraft: { enabled: true, filters: {} },
        },
        summaries: {
          earthquakes: {
            count: 14,
            phase: 'ready',
            sourceCount: 14,
            freshness: null,
            coverage: null,
            fetchedAt: null,
            pendingRegions: 0,
            notices: [],
            sources: [],
            error: null,
          },
          aircraft: {
            count: 350,
            phase: 'ready',
            sourceCount: 350,
            freshness: null,
            coverage: null,
            fetchedAt: null,
            pendingRegions: 0,
            notices: [],
            sources: [],
            error: null,
          },
        },
      });

      expect(dossier.activeLayers).toEqual([
        { id: 'earthquakes', name: 'Earthquakes', count: 14 },
        { id: 'aircraft', name: 'Commercial Air', count: 350 },
      ]);
    });

    it('correctly partitions annotations into manual annotations and geofences', () => {
      const dossier = compileBriefing({ annotations: ANNOTATIONS });

      expect(dossier.annotations).toEqual([
        {
          id: 'ann-1',
          kind: 'pin',
          label: 'Forward Observation Post',
          note: 'Monitored radar array',
        },
      ]);

      expect(dossier.geofences).toEqual([
        {
          id: 'fence-1',
          name: 'Gulf of Finland Sector A',
          pointCount: 4,
        },
      ]);
    });

    it('includes selected entity and extracts attributes', () => {
      const dossier = compileBriefing({
        selection: { feed: 'aircraft', id: '4B821A' },
        selectedEntity: {
          hex: '4B821A',
          callsign: 'FIN902',
          speed: 480,
        },
      });

      expect(dossier.selectedEntity).toEqual({
        feed: 'aircraft',
        id: '4B821A',
        details: {
          hex: '4B821A',
          callsign: 'FIN902',
          speed: 480,
        },
      });
    });

    it('compiles source attribution and licence for active layers', () => {
      const dossier = compileBriefing({
        catalog: CATALOG,
        layers: {
          earthquakes: { enabled: true, filters: {} },
        },
      });

      expect(dossier.sources).toEqual([
        {
          id: 'usgs-earthquakes',
          name: 'USGS Earthquakes',
          attribution: 'U.S. Geological Survey (USGS)',
          licence: 'Public Domain',
        },
      ]);
    });
  });

  describe('exportToMarkdown', () => {
    it('generates rich markdown dossier with all sections', () => {
      const dossier = compileBriefing({
        title: 'Tactical Reconnaissance Dossier',
        analystNotes: 'High priority maritime activity detected.',
        generatedAt: '2026-10-25T14:00:00.000Z',
        camera: { lat: 59.9, lon: 24.5, heightM: 150_000 },
        catalog: CATALOG,
        layers: { earthquakes: { enabled: true, filters: {} } },
        summaries: {
          earthquakes: {
            count: 7,
            phase: 'ready',
            sourceCount: 7,
            freshness: null,
            coverage: null,
            fetchedAt: null,
            pendingRegions: 0,
            notices: [],
            sources: [],
            error: null,
          },
        },
        selection: { feed: 'aircraft', id: 'FIN101' },
        selectedEntity: { callsign: 'FIN101', altitude: 9000 },
        annotations: ANNOTATIONS,
      });

      const md = exportToMarkdown(dossier);

      expect(md).toContain('# Tactical Reconnaissance Dossier');
      expect(md).toContain('**Generated:** 2026-10-25T14:00:00.000Z');
      expect(md).toContain('**Camera Viewport:** 59.9000°, 24.5000° (Altitude: 150,000 m)');
      expect(md).toContain('## Analyst Notes');
      expect(md).toContain('High priority maritime activity detected.');
      expect(md).toContain('## Intelligence Summary');
      expect(md).toContain('`earthquakes`');
      expect(md).toContain('Earthquakes');
      expect(md).toContain('Gulf of Finland Sector A');
      expect(md).toContain('## Selected Target / Entity');
      expect(md).toContain('`FIN101`');
      expect(md).toContain('**altitude:** `9000`');
      expect(md).toContain('## Annotations & Observations');
      expect(md).toContain('Forward Observation Post');
      expect(md).toContain('## Mandatory Legal Citations & Attribution');
      expect(md).toContain('USGS Earthquakes (`usgs-earthquakes`)');
      expect(md).toContain('U.S. Geological Survey (USGS)');
      expect(md).toContain('Public Domain');
    });

    it('handles empty state sections cleanly', () => {
      const dossier = compileBriefing({
        title: 'Empty Briefing',
        camera: null,
      });
      const md = exportToMarkdown(dossier);

      expect(md).toContain('**Camera Viewport:** Global View');
      expect(md).not.toContain('## Analyst Notes');
      expect(md).toContain('*No active layers enabled at the time of export.*');
      expect(md).toContain('*No monitored geofences configured.*');
      expect(md).not.toContain('## Selected Target / Entity');
      expect(md).toContain('*No manual annotations on current workspace.*');
      expect(md).toContain('*No source citations recorded.*');
    });
  });

  describe('exportToHtml', () => {
    it('produces a complete standalone HTML document with Noir styling', () => {
      const dossier = compileBriefing({
        title: 'OSINT SITREP Baltic <Alpha & Beta>',
        analystNotes: 'Note with <tags> & "quotes"',
        catalog: CATALOG,
        layers: { earthquakes: { enabled: true, filters: {} } },
        selection: { feed: 'aircraft', id: 'TARGET-1' },
        annotations: ANNOTATIONS,
      });

      const html = exportToHtml(dossier);

      expect(html).toContain('<!DOCTYPE html>');
      expect(html).toContain('<html lang="en">');
      expect(html).toContain('<title>OSINT SITREP Baltic &lt;Alpha &amp; Beta&gt;</title>');
      expect(html).toContain('color-scheme: dark;');
      expect(html).toContain('--ground: #0b0c10;');
      expect(html).toContain('--island: #16171d;');
      expect(html).toContain('Note with &lt;tags&gt; &amp; &quot;quotes&quot;');
      expect(html).toContain('Gulf of Finland Sector A');
      expect(html).toContain('Forward Observation Post');
      expect(html).toContain('Mandatory Legal Citations &amp; Attribution');
      expect(html).toContain('@media print');
      expect(html).toContain('</html>');
    });
  });

  describe('exportToJson', () => {
    it('produces parseable JSON matching the dossier structure', () => {
      const dossier = compileBriefing({
        title: 'JSON Test',
        camera: { lat: 10, lon: 20, heightM: 5000 },
        annotations: ANNOTATIONS,
      });

      const json = exportToJson(dossier);
      const parsed = JSON.parse(json);

      expect(parsed.title).toBe('JSON Test');
      expect(parsed.camera).toEqual({ lat: 10, lon: 20, heightM: 5000 });
      expect(parsed.annotations[0].label).toBe('Forward Observation Post');
      expect(parsed.geofences[0].name).toBe('Gulf of Finland Sector A');
    });
  });

  describe('slugify & defaultBriefingTitle', () => {
    it('creates web-safe filenames from dossier titles', () => {
      expect(slugify('OSINT Situation Briefing — 2026-10-25')).toBe(
        'osint-situation-briefing-2026-10-25',
      );
      expect(slugify('Special Op: Operation / Black Bear!#')).toBe(
        'special-op-operation-black-bear',
      );
      expect(slugify('   ')).toBe('briefing');
    });

    it('generates expected default title', () => {
      const title = defaultBriefingTitle(new Date('2026-10-25T00:00:00.000Z'));
      expect(title).toBe('OSINT Situation Briefing — 2026-10-25');
    });
  });
});
