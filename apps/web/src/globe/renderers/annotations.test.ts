import { Cartesian2, Cartesian3, Cartographic, EllipsoidGeodesic } from '@cesium/core';
import {
  Label,
  type PointPrimitiveCollection,
  type PolylineCollection,
  PrimitiveCollection,
  type Scene,
} from '@cesium/engine';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { geometryError } from '../../features/annotations/geometry';
import { pin } from '../../test/annotation';
import { DEFAULT_PALETTE } from '../tokens';
import { AnnotationRenderer } from './annotations';
import { toMapColors } from './types';

afterEach(() => vi.restoreAllMocks());
function setup() {
  const root = new PrimitiveCollection();
  const scene = { canvas: { clientWidth: 1_000, clientHeight: 800 } } as Scene;
  vi.spyOn(Label.prototype, 'computeScreenSpacePosition').mockReturnValue(new Cartesian2(500, 400));
  return {
    root,
    renderer: new AnnotationRenderer(
      root,
      {
        colors: toMapColors(DEFAULT_PALETTE),
        requestRender: vi.fn(),
        cameraPosition: () => Cartesian3.fromDegrees(0, 0, 1_000_000),
        cameraHeightM: () => 1_000_000,
      },
      scene,
    ),
  };
}
describe('annotation geometry guards and failure isolation', () => {
  it('rejects the installed solver failure case with a wider safe margin', () => {
    expect(
      () =>
        new EllipsoidGeodesic(Cartographic.fromDegrees(0, 0), Cartographic.fromDegrees(179.5, 0)),
    ).toThrow();
    expect(
      geometryError('line', [
        { lat: 0, lon: 0 },
        { lat: 0, lon: 179.5 },
      ]),
    ).toMatch(/antipodal/);
    expect(
      geometryError('line', [
        { lat: 0, lon: 0 },
        { lat: 0, lon: 178 },
      ]),
    ).toBeNull();
    expect(
      () => new EllipsoidGeodesic(Cartographic.fromDegrees(0, 0), Cartographic.fromDegrees(178, 0)),
    ).not.toThrow();
  });
  it('does not pass an unsafe draft edge into the geodesic solver', () => {
    const { root, renderer } = setup();
    expect(() =>
      renderer.show([pin], {
        kind: 'line',
        points: [
          { lat: 0, lon: 0 },
          { lat: 0, lon: 179.5 },
        ],
      }),
    ).not.toThrow();
    expect((root.get(1) as PolylineCollection).length).toBe(0);
    renderer.destroy();
    root.destroy();
  });
  it('continues drawing later annotations when one mark throws', () => {
    const { root, renderer } = setup();
    const report = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(root.get(0) as PointPrimitiveCollection, 'add').mockImplementationOnce(() => {
      throw new Error('One primitive failed');
    });
    expect(() => renderer.show([pin, { ...pin, id: 'second' }], null)).not.toThrow();
    expect((root.get(0) as PointPrimitiveCollection).length).toBe(1);
    expect(report).toHaveBeenCalledWith('Globe could not draw one annotation', expect.any(Error));
    renderer.destroy();
    root.destroy();
  });
});
