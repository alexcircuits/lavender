/**
 * Native Cesium Wind Streamline Primitive Renderer (ENG-990).
 *
 * Renders global wind vector streamlines as native Cesium PolylineGeometry primitives
 * with animated GLSL flow materials, spherical Fibonacci equal-area spatial binning,
 * and camera frustum + horizon culling.
 */
import {
  ArcType,
  BoundingSphere,
  Cartesian3,
  Color,
  ComponentDatatype,
  Ellipsoid,
  GeometryInstance,
  GeometryInstanceAttribute,
  Intersect,
  Occluder,
  PolylineGeometry,
} from '@cesium/core';
import {
  Material,
  PolylineMaterialAppearance,
  Primitive,
  type PrimitiveCollection,
  SceneMode,
} from '@cesium/engine';
import type { WindGridData } from '../../platform';
import { bakeAllStreamlinesWasm, groupWindPaths } from './wasmWind';

export const WIND_DISPLAY_HEIGHT_METERS = 12000;
export const WIND_PATH_LIMIT = 7200;
export const WIND_NARROW_PATH_LIMIT = 1200;
export const WIND_FADE_LOW_METERS = 15000;
export const WIND_FADE_HIGH_METERS = 60000;

export const WIND_STREAMLINE_BASE_COLOR = new Color(0.5, 0.79, 0.9, 1.0);
export const WIND_STREAMLINE_TIP_COLOR = new Color(0.91, 0.99, 1.0, 1.0);

export const FLOW_MATERIAL = `
in float v_windFacing;
czm_material czm_getMaterial(czm_materialInput materialInput)
{
    czm_material material = czm_getDefaultMaterial(materialInput);
    float pathId = floor(materialInput.st.s);
    float along = fract(materialInput.st.s);
    float offset = fract((pathId + 1.0) * 0.61803398875);
    float behind = fract(phaseTime * 0.16 + offset - along);
    float tail = (1.0 - smoothstep(0.0, 0.32, behind));
    float tip = 1.0 - smoothstep(0.0, 0.045, behind);
    float ends = smoothstep(0.0, 0.055, along) * (1.0 - smoothstep(0.945, 1.0, along));
    float edge = 1.0 - smoothstep(0.26, 0.5, abs(materialInput.st.t - 0.5));
    float horizon = smoothstep(0.0, 0.065, v_windFacing);
    material.diffuse = mix(vec3(0.50, 0.79, 0.90), vec3(0.91, 0.99, 1.0), tip);
    material.alpha = (ghostAlpha + 0.64 * tail + 0.16 * tip) * ends * edge * horizon * heightFade;
    return material;
}`;

export interface WindStreamlineDiagnostics {
  readonly pathCount: number;
  readonly vertexCount: number;
  readonly buildMs: number;
  readonly error: string | null;
  readonly cellCount: number;
  readonly visibleCells: number;
  readonly visibleVertexCount: number;
  readonly heightFade: number;
  readonly ready: boolean;
  readonly mode: 'gpu-streamlines';
  readonly displayHeightMeters: number;
}

export interface WindCameraLike {
  readonly positionWC: Cartesian3;
  readonly positionCartographic: { readonly height: number };
  readonly directionWC: Cartesian3;
  readonly upWC: Cartesian3;
  readonly frustum: {
    computeCullingVolume(
      positionWC?: Cartesian3,
      directionWC?: Cartesian3,
      upWC?: Cartesian3,
    ): {
      computeVisibility(boundingVolume: unknown): unknown;
    };
  };
}

export interface WindSceneLike {
  readonly primitives?: PrimitiveCollection | undefined;
  readonly mode?: unknown;
  readonly camera?: { readonly positionWC?: Cartesian3 | undefined } | undefined;
  readonly canvas?: { readonly clientWidth?: number | undefined } | undefined;
}

export interface WindViewerLike {
  readonly scene?: WindSceneLike | undefined;
}

export interface CesiumWindOverrides {
  readonly ArcType?: { readonly NONE: number } | undefined;
  readonly BoundingSphere?:
    | (typeof BoundingSphere & { fromPoints(points: unknown[]): unknown })
    | undefined;
  readonly Cartesian3?: typeof Cartesian3 | undefined;
  readonly Color?: typeof Color | undefined;
  readonly ComponentDatatype?: typeof ComponentDatatype | undefined;
  readonly Ellipsoid?: typeof Ellipsoid | undefined;
  readonly GeometryInstance?: typeof GeometryInstance | undefined;
  readonly GeometryInstanceAttribute?: typeof GeometryInstanceAttribute | undefined;
  readonly Intersect?: typeof Intersect | undefined;
  readonly Occluder?: typeof Occluder | undefined;
  readonly PolylineGeometry?: typeof PolylineGeometry | undefined;
  readonly Material?: typeof Material | undefined;
  readonly PolylineMaterialAppearance?: typeof PolylineMaterialAppearance | undefined;
  readonly Primitive?: typeof Primitive | undefined;
  readonly PrimitiveCollection?: typeof PrimitiveCollection | undefined;
  readonly SceneMode?: typeof SceneMode | undefined;
  readonly [key: string]: unknown;
}

export interface WindStreamlineRendererOptions {
  readonly getViewer?: (() => WindViewerLike | unknown) | undefined;
  readonly getScene?: (() => WindSceneLike | unknown) | undefined;
  readonly cesium?: CesiumWindOverrides | Record<string, unknown> | undefined;
  readonly canvasWidth?: number | (() => number) | undefined;
  readonly paused?: boolean | undefined;
  readonly collection?: PrimitiveCollection | undefined;
}

interface WindCell {
  readonly primitive: unknown;
  readonly sphere: unknown;
  readonly vertexCount: number;
}

interface MutableDiagnostics {
  pathCount: number;
  vertexCount: number;
  buildMs: number;
  error: string | null;
  cellCount: number;
  visibleCells: number;
  visibleVertexCount: number;
  heightFade: number;
}

function emptyDiagnostics(): MutableDiagnostics {
  return {
    pathCount: 0,
    vertexCount: 0,
    buildMs: 0,
    error: null,
    cellCount: 0,
    visibleCells: 0,
    visibleVertexCount: 0,
    heightFade: 0,
  };
}

const DEFAULT_CESIUM: Record<string, unknown> = {
  ArcType,
  BoundingSphere,
  Cartesian3,
  Color,
  ComponentDatatype,
  Ellipsoid,
  GeometryInstance,
  GeometryInstanceAttribute,
  Intersect,
  Material,
  Occluder,
  PolylineGeometry,
  PolylineMaterialAppearance,
  Primitive,
  SceneMode,
};

export function buildWindVertexShaderSource(baseShaderSource: string): string {
  if (
    !/void\s+main\s*\(\s*\)/.test(baseShaderSource) ||
    !/v_st\.s\s*=\s*st\.s\s*;/.test(baseShaderSource)
  ) {
    throw new Error('Wind polyline shader entry unavailable');
  }
  const seededSource = baseShaderSource.replace(
    /v_st\.s\s*=\s*st\.s\s*;/,
    'v_st.s = st.s * 0.999 + czm_batchTable_windSeed(batchId);',
  );
  return `out float v_windFacing;\n${seededSource}`.replace(
    /void\s+main\s*\(\s*\)\s*\{/,
    'void main() {\nvec3 windWorld = (czm_model * vec4(position3DHigh + position3DLow, 1.0)).xyz;\nv_windFacing = dot(normalize(windWorld), normalize(czm_viewerPositionWC - windWorld));\n',
  );
}

export class WindStreamlineRenderer {
  private collection: PrimitiveCollection | null;
  private readonly options?: WindStreamlineRendererOptions | undefined;
  private cells: WindCell[] = [];
  private occluder: unknown = null;
  private material: unknown = null;
  private paused: boolean;
  private destroyed = false;
  private diagnostics: MutableDiagnostics = emptyDiagnostics();

  constructor(collection?: PrimitiveCollection | null, options?: WindStreamlineRendererOptions) {
    this.collection = collection ?? null;
    this.options = options;
    this.paused = Boolean(options?.paused);
  }

  private get C(): Record<string, unknown> {
    if (this.options?.cesium) {
      return this.options.cesium;
    }
    return DEFAULT_CESIUM;
  }

  private resolveViewer(): WindViewerLike | undefined {
    return typeof this.options?.getViewer === 'function'
      ? (this.options.getViewer() as WindViewerLike)
      : undefined;
  }

  private resolveScene(): WindSceneLike | undefined {
    if (typeof this.options?.getScene === 'function') {
      return this.options.getScene() as WindSceneLike;
    }
    const viewer = this.resolveViewer();
    return viewer?.scene;
  }

  private resolveCollection(): PrimitiveCollection | undefined {
    if (this.collection) {
      return this.collection;
    }
    if (this.options?.collection) {
      return this.options.collection;
    }
    const scene = this.resolveScene();
    return scene?.primitives;
  }

  private getCanvasWidth(): number | undefined {
    if (typeof this.options?.canvasWidth === 'number') {
      return this.options.canvasWidth;
    }
    if (typeof this.options?.canvasWidth === 'function') {
      return this.options.canvasWidth();
    }
    const scene = this.resolveScene();
    return scene?.canvas?.clientWidth;
  }

  supported(): boolean {
    if (this.destroyed) return false;
    const scene = this.resolveScene();
    const C = this.C;
    const collection = this.resolveCollection();
    const hasPrimitives = Boolean(collection || scene?.primitives);
    const hasClasses = Boolean(
      hasPrimitives &&
        C.Primitive &&
        C.GeometryInstance &&
        C.GeometryInstanceAttribute &&
        (C.ComponentDatatype as { FLOAT?: unknown } | undefined)?.FLOAT !== undefined &&
        C.PolylineGeometry &&
        C.PolylineMaterialAppearance &&
        C.Material &&
        (C.BoundingSphere as { fromPoints?: unknown } | undefined)?.fromPoints &&
        C.Occluder &&
        (C.Ellipsoid as { WGS84?: unknown } | undefined)?.WGS84 &&
        C.Intersect &&
        (C.Cartesian3 as { fromDegrees?: unknown } | undefined)?.fromDegrees,
    );
    const sceneMode = scene?.mode;
    const sceneMode3D = (C.SceneMode as { SCENE3D?: unknown } | undefined)?.SCENE3D;
    const sceneModeOk =
      sceneMode3D === undefined || sceneMode === undefined || sceneMode === sceneMode3D;
    return hasClasses && sceneModeOk;
  }

  setGrid(grid: WindGridData): boolean {
    this.clear();
    if (!this.supported() || !grid) return false;
    const started = globalThis.performance?.now?.() ?? Date.now();
    try {
      const width = this.getCanvasWidth();
      const budget =
        width !== undefined && width > 0 && width < 700 ? WIND_NARROW_PATH_LIMIT : WIND_PATH_LIMIT;

      const paths = bakeAllStreamlinesWasm(grid, { count: budget });
      if (paths.length === 0) return false;

      const C = this.C;
      const MatClass = C.Material as new (options: unknown) => unknown;
      const PolyAppClass = C.PolylineMaterialAppearance as (new (
        options: unknown,
      ) => unknown) & {
        VERTEX_FORMAT?: unknown;
      };
      const PolyGeomClass = C.PolylineGeometry as new (options: unknown) => unknown;
      const GeomInstClass = C.GeometryInstance as new (options: unknown) => unknown;
      const GeomInstAttrClass = C.GeometryInstanceAttribute as new (options: unknown) => unknown;
      const PrimClass = C.Primitive as new (options: unknown) => unknown;
      const BSphereClass = C.BoundingSphere as (new (
        center: unknown,
        radius: number,
      ) => unknown) & {
        fromPoints(points: unknown[]): unknown;
      };
      const OccluderClass = C.Occluder as new (sphere: unknown, cameraPosition: unknown) => unknown;
      const Cartesian3Class = C.Cartesian3 as typeof Cartesian3;
      const EllipsoidClass = C.Ellipsoid as typeof Ellipsoid;
      const CompDataClass = C.ComponentDatatype as typeof ComponentDatatype;
      const ArcTypeObj = (C.ArcType as typeof ArcType | undefined) ?? ArcType;

      const material = new MatClass({
        fabric: {
          type: 'GevWindStreamline',
          uniforms: {
            phaseTime: 0,
            ghostAlpha: this.paused ? 0.34 : 0.25,
            heightFade: 0,
          },
          source: FLOW_MATERIAL,
        },
        translucent: () => true,
      });
      this.material = material;

      const baseAppearance = new PolyAppClass({ material });
      const baseVS = (baseAppearance as { vertexShaderSource?: string }).vertexShaderSource;
      if (!baseVS) {
        throw new Error('Wind polyline shader entry unavailable');
      }
      const vertexShaderSource = buildWindVertexShaderSource(baseVS);

      const collection = this.resolveCollection();
      if (!collection) {
        throw new Error('Primitive collection unavailable');
      }
      this.collection = collection;

      const scene = this.resolveScene();
      const initialCameraPosition = scene?.camera?.positionWC ?? Cartesian3Class.ZERO;
      this.occluder = new OccluderClass(
        new BSphereClass(Cartesian3Class.ZERO, EllipsoidClass.WGS84.minimumRadius),
        initialCameraPosition,
      );

      let vertexCount = 0;
      for (const group of groupWindPaths(paths)) {
        const cellPositions: unknown[] = [];
        let cellVertexCount = 0;

        const instances = group.paths.map((path) => {
          const positions = path.coordinates.map(([lon, lat]) =>
            Cartesian3Class.fromDegrees(lon, lat, WIND_DISPLAY_HEIGHT_METERS),
          );
          const geometry = new PolyGeomClass({
            positions,
            width: 2.4,
            arcType: ArcTypeObj.NONE,
            vertexFormat: PolyAppClass.VERTEX_FORMAT,
          });
          cellPositions.push(...positions);
          cellVertexCount += positions.length * 4 - 4;
          return new GeomInstClass({
            geometry,
            attributes: {
              windSeed: new GeomInstAttrClass({
                componentDatatype: CompDataClass.FLOAT,
                componentsPerAttribute: 1,
                value: [path.seed ?? 0],
              }),
            },
          });
        });

        const appearance = new PolyAppClass({
          material,
          vertexShaderSource,
          translucent: true,
          renderState: { depthTest: { enabled: true }, depthMask: false },
        });

        const primitive = new PrimClass({
          show: false,
          geometryInstances: instances,
          appearance,
          asynchronous: true,
          allowPicking: false,
          releaseGeometryInstances: true,
          compressVertices: false,
        });

        const cell: WindCell = {
          primitive,
          sphere: BSphereClass.fromPoints(cellPositions),
          vertexCount: cellVertexCount,
        };

        this.cells.push(cell);
        collection.add(primitive as Primitive);
        vertexCount += cellVertexCount;
      }

      this.diagnostics = {
        ...emptyDiagnostics(),
        cellCount: this.cells.length,
        pathCount: paths.length,
        vertexCount,
        buildMs: (globalThis.performance?.now?.() ?? Date.now()) - started,
        error: null,
      };

      return true;
    } catch (error) {
      this.clear();
      this.diagnostics.error = error instanceof Error ? error.message : 'Wind geometry unavailable';
      return false;
    }
  }

  setField(field: WindGridData): boolean {
    return this.setGrid(field);
  }

  updateVisibility(camera: WindCameraLike): boolean {
    const height = camera?.positionCartographic?.height ?? 0;
    const heightFade = Math.max(
      0,
      Math.min(1, (height - WIND_FADE_LOW_METERS) / (WIND_FADE_HIGH_METERS - WIND_FADE_LOW_METERS)),
    );
    this.diagnostics.heightFade = heightFade;

    const mat = this.material as { uniforms?: { heightFade?: number } } | null;
    if (mat?.uniforms) {
      mat.uniforms.heightFade = heightFade;
    }

    this.diagnostics.visibleCells = 0;
    this.diagnostics.visibleVertexCount = 0;

    let volume: { computeVisibility(sphere: unknown): unknown } | undefined;
    const occluder = this.occluder as {
      cameraPosition?: unknown;
      isBoundingSphereVisible(sphere: unknown): boolean;
    } | null;

    if (heightFade > 0 && this.cells.length > 0 && occluder) {
      occluder.cameraPosition = camera.positionWC;
      volume = camera.frustum.computeCullingVolume(
        camera.positionWC,
        camera.directionWC,
        camera.upWC,
      );
    }

    const IntersectObj = (this.C.Intersect as typeof Intersect | undefined) ?? Intersect;

    for (let i = 0; i < this.cells.length; i++) {
      const cell = this.cells[i];
      if (!cell) continue;

      const isVisible =
        heightFade > 0 &&
        Boolean(occluder?.isBoundingSphereVisible(cell.sphere)) &&
        Boolean(volume?.computeVisibility(cell.sphere) !== IntersectObj.OUTSIDE);

      const prim = cell.primitive as { show?: boolean };
      prim.show = isVisible;

      if (isVisible) {
        this.diagnostics.visibleCells++;
        this.diagnostics.visibleVertexCount += cell.vertexCount;
      }
    }

    return this.diagnostics.visibleCells > 0 && heightFade > 0;
  }

  tick(elapsedSeconds: number): void {
    if (this.material && !this.paused && Number.isFinite(elapsedSeconds)) {
      const mat = this.material as { uniforms?: { phaseTime?: number } };
      if (mat.uniforms) {
        mat.uniforms.phaseTime = Math.max(0, elapsedSeconds) % 10000;
      }
    }
  }

  setOptions(options: { paused?: boolean } = {}): void {
    if (typeof options.paused === 'boolean') {
      this.paused = options.paused;
    }
    if (this.material) {
      const mat = this.material as { uniforms?: { ghostAlpha?: number } };
      if (mat.uniforms) {
        mat.uniforms.ghostAlpha = this.paused ? 0.34 : 0.25;
      }
    }
  }

  clear(): void {
    const collection = this.resolveCollection();
    for (const cell of this.cells) {
      if (collection) {
        collection.remove(cell.primitive as Primitive);
      }
      const prim = cell.primitive as {
        isDestroyed?: () => boolean;
        destroy?: () => void;
        destroyed?: boolean;
      };
      if (typeof prim.isDestroyed === 'function') {
        if (!prim.isDestroyed()) {
          prim.destroy?.();
        }
      } else if (!prim.destroyed) {
        prim.destroy?.();
      }
    }
    this.cells = [];
    this.occluder = null;

    if (this.material) {
      const mat = this.material as {
        isDestroyed?: () => boolean;
        destroy?: () => void;
        destroyed?: boolean;
      };
      if (typeof mat.isDestroyed === 'function') {
        if (!mat.isDestroyed()) {
          mat.destroy?.();
        }
      } else if (!mat.destroyed) {
        mat.destroy?.();
      }
      this.material = null;
    }

    this.diagnostics = emptyDiagnostics();
  }

  destroy(): void {
    this.clear();
    this.destroyed = true;
  }

  isDestroyed(): boolean {
    return this.destroyed;
  }

  getParticleCount(): number {
    return this.diagnostics.pathCount;
  }

  getDiagnostics(): WindStreamlineDiagnostics {
    const ready =
      this.cells.length > 0 &&
      this.cells.every((cell) => {
        const prim = cell.primitive as { ready?: boolean };
        return Boolean(prim.ready);
      });
    return {
      ...this.diagnostics,
      ready,
      mode: 'gpu-streamlines',
      displayHeightMeters: WIND_DISPLAY_HEIGHT_METERS,
    };
  }
}

export function createWindStreamlineRenderer(
  collectionOrOptions?: PrimitiveCollection | WindStreamlineRendererOptions | null,
  maybeOptions?: WindStreamlineRendererOptions,
): WindStreamlineRenderer {
  if (
    collectionOrOptions &&
    typeof (collectionOrOptions as { add?: unknown }).add === 'function' &&
    typeof (collectionOrOptions as { remove?: unknown }).remove === 'function'
  ) {
    return new WindStreamlineRenderer(collectionOrOptions as PrimitiveCollection, maybeOptions);
  }
  return new WindStreamlineRenderer(
    null,
    (collectionOrOptions ?? maybeOptions) as WindStreamlineRendererOptions | undefined,
  );
}

export const createWindGpuRendering = createWindStreamlineRenderer;
