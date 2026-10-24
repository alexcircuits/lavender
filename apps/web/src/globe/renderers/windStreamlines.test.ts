import {
  BoundingSphere,
  Cartesian3,
  Color,
  Ellipsoid,
  Intersect,
  Occluder,
  PerspectiveFrustum,
} from '@cesium/core';
import type { PrimitiveCollection } from '@cesium/engine';
import { beforeAll, describe, expect, it } from 'vitest';
import type { WindGridData } from '../../platform';
import { bakeAllStreamlinesWasm, loadLavenderWasm } from './wasmWind';
import {
  buildWindVertexShaderSource,
  createWindGpuRendering,
  createWindStreamlineRenderer,
  FLOW_MATERIAL,
  WIND_DISPLAY_HEIGHT_METERS,
  WIND_FADE_HIGH_METERS,
  WIND_FADE_LOW_METERS,
  WIND_NARROW_PATH_LIMIT,
  WIND_PATH_LIMIT,
  WIND_STREAMLINE_BASE_COLOR,
  WIND_STREAMLINE_TIP_COLOR,
  WindStreamlineRenderer,
} from './windStreamlines';

interface MockPrimitiveInstance {
  show?: boolean | undefined;
  geometryInstances: Array<{
    geometry: { positions: Cartesian3[]; width?: number | undefined };
    attributes: {
      windSeed: { value: number[]; componentDatatype: number; componentsPerAttribute: number };
    };
  }>;
  appearance: {
    vertexShaderSource: string;
    material: {
      uniforms: { phaseTime: number; ghostAlpha: number; heightFade: number };
      source: string;
      destroyed: boolean;
    };
  };
  asynchronous?: boolean | undefined;
  allowPicking?: boolean | undefined;
  releaseGeometryInstances?: boolean | undefined;
  compressVertices?: boolean | undefined;
  ready: boolean;
  destroyed: boolean;
  destroy(): void;
  isDestroyed(): boolean;
}

interface TestHarness {
  C: Record<string, unknown>;
  scene: {
    globe: { show: boolean };
    camera: {
      positionWC: Cartesian3;
      positionCartographic: { height: number };
      directionWC: Cartesian3;
      upWC: Cartesian3;
      frustum: {
        computeCullingVolume: (
          position: Cartesian3,
          direction: Cartesian3,
          up: Cartesian3,
        ) => {
          computeVisibility(sphere: unknown): unknown;
        };
      };
    };
    canvas: { clientWidth: number };
    primitives: PrimitiveCollection;
  };
  members: Set<MockPrimitiveInstance>;
  builds: () => number;
  spheres: Array<{ sphere: BoundingSphere; points: Cartesian3[] }>;
  visibility: {
    horizon: (sphere: BoundingSphere, cameraPosition?: Cartesian3) => boolean;
    frustum: (sphere: BoundingSphere) => unknown;
    horizonTests: number;
    frustumTests: number;
  };
  occluderBuilds: () => number;
  owner: WindStreamlineRenderer;
}

function createHarness({ width = 1000 }: { width?: number } = {}): TestHarness {
  const members = new Set<MockPrimitiveInstance>();
  let geometryBuilds = 0;
  const spheres: Array<{ sphere: BoundingSphere; points: Cartesian3[] }> = [];
  const visibility = {
    horizon: (_sphere: BoundingSphere, _cameraPosition?: Cartesian3) => true,
    frustum: (_sphere: BoundingSphere) => Intersect.INSIDE,
    horizonTests: 0,
    frustumTests: 0,
  };
  let occluderBuilds = 0;

  class MockMaterial {
    uniforms: { phaseTime: number; ghostAlpha: number; heightFade: number };
    source: string;
    destroyed = false;
    constructor(options: {
      fabric: {
        uniforms: { phaseTime: number; ghostAlpha: number; heightFade: number };
        source: string;
      };
    }) {
      this.uniforms = options.fabric.uniforms;
      this.source = options.fabric.source;
    }
    destroy() {
      if (this.destroyed) {
        throw new Error('owned resource destroyed once');
      }
      this.destroyed = true;
    }
  }

  class MockAppearance {
    static VERTEX_FORMAT = {};
    vertexShaderSource: string;
    material?: MockMaterial | undefined;
    translucent?: boolean | undefined;
    renderState?: unknown;
    constructor(options: {
      vertexShaderSource?: string | undefined;
      material?: MockMaterial | undefined;
      translucent?: boolean | undefined;
      renderState?: unknown;
    }) {
      this.vertexShaderSource =
        options.vertexShaderSource ??
        'in vec3 position3DHigh; in vec3 position3DLow; void main() { gl_Position = vec4(0.0); v_st.s = st.s; }';
      this.material = options.material;
      this.translucent = options.translucent;
      this.renderState = options.renderState;
    }
  }

  class MockPolyline {
    positions: Cartesian3[];
    width?: number | undefined;
    arcType?: number | undefined;
    vertexFormat?: unknown;
    constructor(options: {
      positions: Cartesian3[];
      width?: number | undefined;
      arcType?: number | undefined;
      vertexFormat?: unknown;
    }) {
      this.positions = options.positions;
      this.width = options.width;
      this.arcType = options.arcType;
      this.vertexFormat = options.vertexFormat;
    }
    static createGeometry(options: { positions: Cartesian3[] }) {
      geometryBuilds++;
      const st = new Float32Array(options.positions.length * 2);
      for (let i = 0; i < options.positions.length; i++) {
        st[i * 2] = i / (options.positions.length - 1);
      }
      return { attributes: { st: { values: st } } };
    }
  }

  class MockPrimitive implements MockPrimitiveInstance {
    show?: boolean | undefined;
    geometryInstances: Array<{
      geometry: { positions: Cartesian3[]; width?: number | undefined };
      attributes: {
        windSeed: { value: number[]; componentDatatype: number; componentsPerAttribute: number };
      };
    }>;
    appearance: {
      vertexShaderSource: string;
      material: {
        uniforms: { phaseTime: number; ghostAlpha: number; heightFade: number };
        source: string;
        destroyed: boolean;
      };
    };
    asynchronous?: boolean | undefined;
    allowPicking?: boolean | undefined;
    releaseGeometryInstances?: boolean | undefined;
    compressVertices?: boolean | undefined;
    ready = true;
    destroyed = false;
    constructor(options: {
      show?: boolean | undefined;
      geometryInstances: Array<{
        geometry: { positions: Cartesian3[]; width?: number | undefined };
        attributes: {
          windSeed: { value: number[]; componentDatatype: number; componentsPerAttribute: number };
        };
      }>;
      appearance: {
        vertexShaderSource: string;
        material: {
          uniforms: { phaseTime: number; ghostAlpha: number; heightFade: number };
          source: string;
          destroyed: boolean;
        };
      };
      asynchronous?: boolean | undefined;
      allowPicking?: boolean | undefined;
      releaseGeometryInstances?: boolean | undefined;
      compressVertices?: boolean | undefined;
    }) {
      this.show = options.show;
      this.geometryInstances = options.geometryInstances;
      this.appearance = options.appearance;
      this.asynchronous = options.asynchronous;
      this.allowPicking = options.allowPicking;
      this.releaseGeometryInstances = options.releaseGeometryInstances;
      this.compressVertices = options.compressVertices;
    }
    destroy() {
      if (this.destroyed) {
        throw new Error('owned resource destroyed once');
      }
      this.destroyed = true;
    }
    isDestroyed() {
      return this.destroyed;
    }
  }

  const C: Record<string, unknown> = {
    ComponentDatatype: { FLOAT: 5126 },
    GeometryInstanceAttribute: class {
      componentDatatype: number;
      componentsPerAttribute: number;
      value: number[];
      constructor(options: {
        componentDatatype: number;
        componentsPerAttribute: number;
        value: number[];
      }) {
        this.componentDatatype = options.componentDatatype;
        this.componentsPerAttribute = options.componentsPerAttribute;
        this.value = options.value;
      }
    },
    Material: MockMaterial,
    PolylineMaterialAppearance: MockAppearance,
    PolylineGeometry: MockPolyline,
    Primitive: MockPrimitive,
    GeometryInstance: class {
      geometry: unknown;
      attributes: Record<string, unknown>;
      constructor(options: { geometry: unknown; attributes: Record<string, unknown> }) {
        this.geometry = options.geometry;
        this.attributes = options.attributes;
      }
    },
    Cartesian3,
    Ellipsoid,
    Intersect,
    BoundingSphere: class extends BoundingSphere {
      static override fromPoints(points: Cartesian3[]) {
        const sphere = BoundingSphere.fromPoints(points);
        spheres.push({ sphere, points });
        return sphere;
      }
    },
    Occluder: class {
      cameraPosition: Cartesian3;
      constructor(_sphere: unknown, cameraPosition: Cartesian3) {
        occluderBuilds++;
        this.cameraPosition = cameraPosition;
      }
      isBoundingSphereVisible(sphere: unknown) {
        visibility.horizonTests++;
        return visibility.horizon(sphere as BoundingSphere, this.cameraPosition);
      }
    },
    ArcType: { NONE: 0 },
    Color,
    SceneMode: { SCENE3D: 3, SCENE2D: 2, COLUMBUS_VIEW: 1 },
  };

  const volume = {
    computeVisibility(sphere: unknown) {
      visibility.frustumTests++;
      return visibility.frustum(sphere as BoundingSphere);
    },
  };

  const primitives = {
    add(item: MockPrimitiveInstance) {
      members.add(item);
    },
    remove(item: MockPrimitiveInstance) {
      const removed = members.delete(item);
      if (removed) item.destroy();
      return removed;
    },
  };

  const scene = {
    globe: { show: false },
    camera: {
      positionWC: Cartesian3.fromDegrees(0, 0, 1e7),
      positionCartographic: { height: 1e7 },
      directionWC: new Cartesian3(-1, 0, 0),
      upWC: new Cartesian3(0, 0, 1),
      frustum: {
        computeCullingVolume: (_position: Cartesian3, _direction: Cartesian3, _up: Cartesian3) =>
          volume,
      },
    },
    canvas: { clientWidth: width },
    primitives: primitives as unknown as PrimitiveCollection,
  };

  return {
    C,
    scene,
    members,
    builds: () => geometryBuilds,
    spheres,
    visibility,
    occluderBuilds: () => occluderBuilds,
    owner: new WindStreamlineRenderer(primitives as unknown as PrimitiveCollection, {
      cesium: C,
      getViewer: () => ({ scene }),
    }),
  };
}

const createUniformField = (u = 15, v = 0): WindGridData => ({
  nx: 4,
  ny: 3,
  lo1: -180,
  la1: 90,
  dx: 90,
  dy: 90,
  di: 90,
  dj: 90,
  u: new Float32Array(12).fill(u),
  v: new Float32Array(12).fill(v),
});

describe('windStreamlines', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('constants and exports', () => {
    it('exports expected physical bounds and colors', () => {
      expect(WIND_DISPLAY_HEIGHT_METERS).toBe(12000);
      expect(WIND_PATH_LIMIT).toBe(7200);
      expect(WIND_NARROW_PATH_LIMIT).toBe(1200);
      expect(WIND_FADE_LOW_METERS).toBe(15000);
      expect(WIND_FADE_HIGH_METERS).toBe(60000);
      expect(WIND_STREAMLINE_BASE_COLOR).toBeInstanceOf(Color);
      expect(WIND_STREAMLINE_TIP_COLOR).toBeInstanceOf(Color);
      expect(FLOW_MATERIAL).toContain('v_windFacing');
      expect(FLOW_MATERIAL).toContain('phaseTime');
      expect(FLOW_MATERIAL).toContain('ghostAlpha');
      expect(FLOW_MATERIAL).toContain('heightFade');
    });

    it('buildWindVertexShaderSource injects batch seed and facing normal dot product', () => {
      const baseVS =
        'in vec3 position3DHigh; in vec3 position3DLow; void main() {\n  gl_Position = vec4(0.0);\n  v_st.s = st.s;\n}';
      const modified = buildWindVertexShaderSource(baseVS);
      expect(modified).toContain('out float v_windFacing;');
      expect(modified).toContain('v_st.s = st.s * 0.999 + czm_batchTable_windSeed(batchId);');
      expect(modified).toContain(
        'v_windFacing = dot(normalize(windWorld), normalize(czm_viewerPositionWC - windWorld));',
      );
    });

    it('buildWindVertexShaderSource throws if required hooks are missing', () => {
      expect(() => buildWindVertexShaderSource('void other() {}')).toThrow(
        /Wind polyline shader entry unavailable/,
      );
    });
  });

  describe('grid loading, batching and uniforms', () => {
    it(
      'builds regional native batches and ticks update uniforms without rebuilding geometry',
      () => {
        const h = createHarness();
        expect(h.owner.setGrid(createUniformField())).toBe(true);
        expect(h.members.size).toBe(72);

        const batch = Array.from(h.members)[0];
        expect(batch).toBeDefined();
        if (!batch) return;

        expect(batch.allowPicking).toBe(false);
        expect(batch.asynchronous).toBe(true);
        expect(batch.appearance.vertexShaderSource).toContain('v_windFacing = dot');
        expect(batch.appearance.material.source).toContain('smoothstep(0.0, 0.065, v_windFacing)');
        expect(batch.appearance.vertexShaderSource).toContain('czm_batchTable_windSeed(batchId)');

        const builds = h.builds();
      expect(builds).toBe(0); // geometry expansion deferred to workers

      h.owner.tick(3);
      expect(batch.appearance.material.uniforms.phaseTime).toBe(3);

      h.owner.setOptions({ paused: true });
      h.owner.tick(8);
      expect(batch.appearance.material.uniforms.phaseTime).toBe(3);

      h.owner.setOptions({ paused: false });
      h.owner.tick(9);
      expect(batch.appearance.material.uniforms.phaseTime).toBe(9);
      expect(h.builds()).toBe(builds);

      const diagnostics = h.owner.getDiagnostics();
      expect(diagnostics.pathCount).toBeLessThanOrEqual(WIND_PATH_LIMIT);
      expect(diagnostics.pathCount).toBeGreaterThan(7000);
      expect(diagnostics.vertexCount).toBeGreaterThan(0);
      expect(diagnostics.vertexCount).toBeLessThanOrEqual(7200 * 128);
      expect(diagnostics.ready).toBe(true);
      expect(diagnostics.displayHeightMeters).toBe(12000);
      expect(diagnostics.cellCount).toBe(72);
      expect(diagnostics.mode).toBe('gpu-streamlines');
      expect(h.spheres.length).toBe(72);

      const appearances = new Set(Array.from(h.members).map((p) => p.appearance));
      expect(appearances.size).toBe(72);

      for (const cell of h.members) {
        expect(cell.appearance.material).toBe(batch.appearance.material);
        expect(cell.appearance.material.uniforms.phaseTime).toBe(9);
        expect(cell.appearance.material.uniforms.ghostAlpha).toBe(0.25);
        expect(cell.compressVertices).toBe(false);
        expect(cell.releaseGeometryInstances).toBe(true);
      }

      batch.ready = false;
      expect(h.owner.getDiagnostics().ready).toBe(false);
      batch.ready = true;

      const instances = Array.from(h.members).flatMap((p) => p.geometryInstances);
      const baked = bakeAllStreamlinesWasm(createUniformField(), { count: WIND_PATH_LIMIT });
      expect(instances.length).toBe(baked.length);

      const bySeed = new Map(instances.map((i) => [i.attributes.windSeed.value[0], i]));
      for (const path of baked) {
        const inst = bySeed.get(path.seed);
        expect(inst).toBeDefined();
        if (!inst) continue;
        const expectedPositions = path.coordinates.map(([lon, lat]) =>
          Cartesian3.fromDegrees(lon, lat, 12000),
        );
        expect(inst.geometry.positions).toEqual(expectedPositions);
      }

      for (const { sphere, points } of h.spheres) {
        expect(sphere).toEqual(BoundingSphere.fromPoints(points));
        for (const point of points) {
          expect(Cartesian3.distance(point, sphere.center)).toBeLessThanOrEqual(
            sphere.radius + 1e-5,
          );
        }
      }

      for (const instance of instances) {
        expect(instance.geometry.width).toBe(2.4);
        expect(instance.attributes.windSeed.componentsPerAttribute).toBe(1);
        expect(Number.isInteger(instance.attributes.windSeed.value[0])).toBe(true);
      }
    }, 15000);

    it('aliases setField to setGrid', () => {
      const h = createHarness();
      expect(h.owner.setField(createUniformField())).toBe(true);
      expect(h.owner.getParticleCount()).toBeGreaterThan(7000);
    });
  });

  describe('replacement, clear and destruction lifecycle', () => {
    it('replacement, clear and destroy dispose every owned cell and the shared material', () => {
      const h = createHarness();
      const assertDisposed = (batches: MockPrimitiveInstance[]) => {
        expect(batches.length).toBe(72);
        for (const batch of batches) {
          expect(batch.destroyed).toBe(true);
          expect(batch.appearance.material.destroyed).toBe(true);
        }
      };

      h.owner.setGrid(createUniformField());
      const first = Array.from(h.members);
      h.owner.setGrid(createUniformField());
      assertDisposed(first);

      const second = Array.from(h.members);
      h.owner.clear();
      assertDisposed(second);
      expect(h.members.size).toBe(0);
      expect(h.owner.getParticleCount()).toBe(0);
      expect(h.owner.getDiagnostics().ready).toBe(false);

      h.owner.setGrid(createUniformField());
      const third = Array.from(h.members);
      h.owner.destroy();
      assertDisposed(third);
      expect(h.members.size).toBe(0);
      expect(h.owner.supported()).toBe(false);
      expect(h.owner.isDestroyed()).toBe(true);
      expect(h.owner.setGrid(createUniformField())).toBe(false);
    });
  });

  describe('unsupported environments and build error recovery', () => {
    it('unsupported or failed geometry cleanly requests the existing fallback', () => {
      const missing = createWindStreamlineRenderer({
        cesium: {},
        getViewer: () => ({ scene: {} }),
      });
      expect(missing.setGrid(createUniformField())).toBe(false);

      const h = createHarness();
      h.C.PolylineGeometry = class {
        constructor() {
          throw new Error('test build failure');
        }
      };
      expect(h.owner.setGrid(createUniformField())).toBe(false);
      expect(h.members.size).toBe(0);
      expect(h.owner.getDiagnostics().error).toMatch(/test build failure/);
    });

    it('a failed later cell build releases already installed cells and shared material', () => {
      const h = createHarness();
      const NativePrimitive = h.C.Primitive as new (options: unknown) => MockPrimitiveInstance;
      const built: MockPrimitiveInstance[] = [];

      class FailingPrimitive extends NativePrimitive {
        constructor(options: unknown) {
          if (built.length === 2) {
            throw new Error('later cell failed');
          }
          super(options);
          built.push(this);
        }
      }

      h.C.Primitive = FailingPrimitive;
      expect(h.owner.setGrid(createUniformField())).toBe(false);
      expect(h.members.size).toBe(0);
      expect(built.length).toBe(2);

      for (const batch of built) {
        expect(batch.destroyed).toBe(true);
        expect(batch.appearance.material.destroyed).toBe(true);
      }
      expect(h.owner.getDiagnostics().error).toMatch(/later cell failed/);
    });

    it('returns false for calm wind fields (< 0.15 m/s)', () => {
      const h = createHarness();
      const calmField = createUniformField(0, 0);
      expect(h.owner.setGrid(calmField)).toBe(false);
      expect(h.owner.getParticleCount()).toBe(0);
    });
  });

  describe('narrow viewport path budgeting', () => {
    it('narrow canvases use a lower bounded path budget', () => {
      const h = createHarness({ width: 390 });
      expect(h.owner.setGrid(createUniformField())).toBe(true);
      expect(h.owner.getParticleCount()).toBeLessThanOrEqual(WIND_NARROW_PATH_LIMIT);
      expect(h.owner.getParticleCount()).toBeGreaterThan(1000);
    });
  });

  describe('culling and visibility', () => {
    it('cell visibility combines horizon, frustum and height with shared fade uniforms', () => {
      const h = createHarness();
      h.owner.setGrid(createUniformField());
      const batches = Array.from(h.members);
      const camera = h.scene.camera;

      h.visibility.horizon = (sphere) => h.spheres.findIndex((s) => s.sphere === sphere) % 2 === 0;
      h.visibility.frustum = (sphere) =>
        h.spheres.findIndex((s) => s.sphere === sphere) % 3 === 0
          ? Intersect.INTERSECTING
          : Intersect.OUTSIDE;

      expect(h.owner.updateVisibility(camera)).toBe(true);
      expect(h.owner.getDiagnostics().visibleCells).toBe(12);

      let vertices = 0;
      for (let i = 0; i < batches.length; i++) {
        const batch = batches[i];
        expect(batch).toBeDefined();
        if (!batch) continue;
        expect(batch.show).toBe(i % 6 === 0);
        if (batch.show) {
          vertices += batch.geometryInstances.reduce(
            (sum, p) => sum + p.geometry.positions.length * 4 - 4,
            0,
          );
        }
      }
      expect(h.owner.getDiagnostics().visibleVertexCount).toBe(vertices);
      expect(h.visibility.horizonTests).toBe(72);
      expect(h.visibility.frustumTests).toBe(36);

      for (const [height, fade] of [
        [60000, 1],
        [37500, 0.5],
        [15000, 0],
        [1200, 0],
      ] as const) {
        camera.positionCartographic.height = height;
        expect(h.owner.updateVisibility(camera)).toBe(fade > 0);
        expect(h.owner.getDiagnostics().heightFade).toBe(fade);
        for (const batch of batches) {
          expect(batch.appearance.material.uniforms.heightFade).toBe(fade);
          expect(batch.appearance.material.source).toMatch(/horizon \* heightFade/);
          if (!fade) expect(batch.show).toBe(false);
        }
      }

      expect(h.owner.getDiagnostics().visibleVertexCount).toBe(0);
      expect(h.owner.getDiagnostics().visibleCells).toBe(0);
      expect(h.visibility.horizonTests).toBe(216); // zero fade skips all sphere tests
      expect(h.visibility.frustumTests).toBe(108);
      expect(h.occluderBuilds()).toBe(1); // occluder reused across updates
    });

    it('real Cesium sphere culling hides far hemisphere with globe hidden and rejects a sky-facing view', () => {
      const h = createHarness();
      h.C.Occluder = Occluder;
      h.owner.setGrid(createUniformField());
      const camera = h.scene.camera;
      camera.frustum = new PerspectiveFrustum({
        fov: Math.PI / 3,
        aspectRatio: 1.5,
        near: 1,
        far: 5e7,
      });

      expect(h.owner.updateVisibility(camera)).toBe(true);
      const facingEarth = h.owner.getDiagnostics();
      expect(facingEarth.visibleCells).toBeGreaterThan(0);
      expect(facingEarth.visibleCells).toBeLessThan(72);

      const batches = Array.from(h.members);
      for (let i = 0; i < h.spheres.length; i++) {
        const sphereItem = h.spheres[i];
        if (!sphereItem) continue;
        const sphere = sphereItem.sphere;
        if (sphere.center.x < -5e6) {
          const batch = batches[i];
          expect(batch?.show).toBe(false);
        }
      }

      camera.directionWC.x = 1;
      expect(h.owner.updateVisibility(camera)).toBe(false);
      expect(h.owner.getDiagnostics().visibleCells).toBe(0);

      camera.positionWC = Cartesian3.fromDegrees(180, 0, 1e7);
      expect(h.owner.updateVisibility(camera)).toBe(true);
      h.owner.destroy();
    });
  });

  describe('factories and options', () => {
    it('createWindStreamlineRenderer and createWindGpuRendering construct instances', () => {
      const h = createHarness();
      const r1 = createWindStreamlineRenderer(h.scene.primitives, {
        cesium: h.C,
        getViewer: () => ({ scene: h.scene }),
      });
      expect(r1).toBeInstanceOf(WindStreamlineRenderer);

      const r2 = createWindGpuRendering({
        cesium: h.C,
        getViewer: () => ({ scene: h.scene }),
      });
      expect(r2).toBeInstanceOf(WindStreamlineRenderer);
      expect(r2.supported()).toBe(true);
    });

    it('tick clamps negative elapsedSeconds to zero and wraps modulo 10000', () => {
      const h = createHarness();
      h.owner.setGrid(createUniformField());
      const batch = Array.from(h.members)[0];
      expect(batch).toBeDefined();
      if (!batch) return;

      h.owner.tick(-5);
      expect(batch.appearance.material.uniforms.phaseTime).toBe(0);

      h.owner.tick(12345);
      expect(batch.appearance.material.uniforms.phaseTime).toBe(2345);

      h.owner.tick(Number.NaN);
      expect(batch.appearance.material.uniforms.phaseTime).toBe(2345);
    });
  });
});
