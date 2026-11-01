import {
  BoxGeometry,
  Cartesian3,
  ComponentDatatype,
  GeometryPipeline,
  JulianDate,
  Matrix3,
  PrimitiveType,
  TimeInterval,
  VertexFormat,
} from '@cesium/core';
import {
  BlendEquation,
  BlendFunction,
  BufferUsage,
  Buffer as CesiumBuffer,
  CubeMap,
  DrawCommand,
  RenderState,
  Resource,
  Sampler,
  SceneMode,
  ShaderProgram,
  ShaderSource,
  TextureMinificationFilter,
  Transforms,
  VertexArray,
} from '@cesium/engine';
import { buildStarVertices, STAR_VERTEX_FLOATS } from './wasmSkyKinematics';

/** Brightness of the Milky Way glow; the stars carry the sky. */
export const MILKY_WAY_INTENSITY = 0.4;
/** Clock change, in seconds, below which the sky rotation is reused. */
export const SKY_ROTATION_EPSILON_SECONDS = 1;

const base = import.meta.env?.BASE_URL ?? '/';
const normalizedBase = base.endsWith('/') ? base : `${base}/`;
const DEFAULT_STAR_PACK_URL = `${normalizedBase}assets/sky/bright_stars.json`;
const DEFAULT_MILKY_WAY_FACE_URLS: Record<string, string> = Object.freeze({
  positiveX: `${normalizedBase}assets/sky/milkyway_px.webp`,
  negativeX: `${normalizedBase}assets/sky/milkyway_nx.webp`,
  positiveY: `${normalizedBase}assets/sky/milkyway_py.webp`,
  negativeY: `${normalizedBase}assets/sky/milkyway_ny.webp`,
  positiveZ: `${normalizedBase}assets/sky/milkyway_pz.webp`,
  negativeZ: `${normalizedBase}assets/sky/milkyway_nz.webp`,
});

// Directions, not points: w = 0 drops the camera translation, and z just
// under w keeps every fragment at the far plane, inside any frustum.
const SKY_DIRECTION_GLSL = `
uniform mat3 u_icrfToFixed;
vec4 skyClipPosition(vec3 icrfDirection)
{
    vec3 eye = czm_viewRotation * (u_icrfToFixed * icrfDirection);
    vec4 clip = czm_projection * vec4(eye, 0.0);
    clip.z = clip.w * 0.99999;
    return clip;
}
`;

const MILKY_WAY_VS = `${SKY_DIRECTION_GLSL}
in vec3 position;
out vec3 v_direction;
void main()
{
    gl_Position = skyClipPosition(position);
    v_direction = position;
}
`;

const MILKY_WAY_FS = `
uniform samplerCube u_milkyWay;
uniform float u_intensity;
in vec3 v_direction;
void main()
{
    vec3 color = czm_textureCube(u_milkyWay, normalize(v_direction)).rgb * u_intensity;
    // Interleaved-gradient dither: the dim glow spans only a few 8-bit steps.
    float noise = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
    color = max(color + (noise - 0.5) / 255.0, 0.0);
    out_FragColor = vec4(czm_gammaCorrect(color), 1.0);
}
`;

const STAR_VS = `${SKY_DIRECTION_GLSL}
in vec3 position;
in float sigma;
in vec4 color;
out vec4 v_color;
out float v_sigma;
out float v_radius;
void main()
{
    gl_Position = skyClipPosition(position);
    // σ is in CSS pixels; the sprite holds ±3σ plus a pixel of margin.
    v_sigma = sigma * czm_pixelRatio;
    v_radius = ceil(3.0 * v_sigma) + 1.0;
    gl_PointSize = 2.0 * v_radius;
    v_color = color;
}
`;

const STAR_FS = `
in vec4 v_color;
in float v_sigma;
in float v_radius;
void main()
{
    vec2 offset = (gl_PointCoord - 0.5) * (2.0 * v_radius);
    float alpha = v_color.a *
        exp(-0.5 * dot(offset, offset) / (v_sigma * v_sigma));
    if (alpha < 1.0 / 512.0) discard;
    out_FragColor = vec4(czm_gammaCorrect(v_color.rgb) * alpha, alpha);
}
`;

/** Pick the ICRF→fixed rotation for `time`, or TEME while tables load. */
export function computeSkyRotation(
  time: JulianDate,
  result: Matrix3 = new Matrix3(),
): { matrix: Matrix3; precise: boolean } {
  const icrf = Transforms.computeIcrfToFixedMatrix(time, result);
  if (icrf) return { matrix: icrf, precise: true };
  const teme = Transforms.computeTemeToPseudoFixedMatrix(time, result);
  if (teme) return { matrix: teme, precise: false };
  return { matrix: Matrix3.clone(Matrix3.IDENTITY, result), precise: false };
}

/** Whether a cached rotation at `cachedTime` still serves `time`. */
export function isSkyRotationCurrent(
  cachedTime: JulianDate | undefined,
  time: JulianDate,
): boolean {
  return (
    cachedTime !== undefined &&
    Math.abs(JulianDate.secondsDifference(time, cachedTime)) < SKY_ROTATION_EPSILON_SECONDS
  );
}

export interface StarrySkyOptions {
  readonly requestRender?: () => void;
  readonly starPackUrl?: string;
  readonly milkyWayFaceUrls?: Record<string, string>;
}

export class StarrySky {
  show = true;
  milkyWayIntensity = MILKY_WAY_INTENSITY;
  milkyWayVisibility = 1;

  _requestRender: () => void;
  _rotation = Matrix3.clone(Matrix3.IDENTITY);
  _rotationTime: JulianDate | undefined = undefined;
  _rotationPrecise = false;
  _icrfRequest: Promise<void> | undefined = undefined;
  _icrfUnavailable = false;
  _icrfMissing: TimeInterval | undefined = undefined;
  _loading = false;
  _starData: { count: number; vertices: Float32Array } | undefined = undefined;
  _faceImages: Record<string, unknown> | undefined = undefined;
  _cubeMap: CubeMap | undefined = undefined;
  _milkyWayCubeMap: CubeMap | undefined = undefined;
  _milkyWayCommand: DrawCommand | undefined = undefined;
  _milkyWayAttributes: Record<string, number> | undefined = undefined;
  _starCommand: DrawCommand | undefined = undefined;
  _useHdr: boolean | undefined = undefined;
  _destroyed = false;

  readonly _starPackUrl: string;
  readonly _milkyWayFaceUrls: Record<string, string>;
  readonly _milkyWayUniforms: Record<string, () => unknown>;
  readonly _starUniforms: Record<string, () => unknown>;
  readonly _commands: DrawCommand[] = [];
  readonly _compositeCommand: {
    derivedCommands: Record<string, unknown>;
    execute: (context: unknown, passState: unknown) => void;
  };

  constructor({
    requestRender = () => {},
    starPackUrl = DEFAULT_STAR_PACK_URL,
    milkyWayFaceUrls = DEFAULT_MILKY_WAY_FACE_URLS,
  }: StarrySkyOptions = {}) {
    this._requestRender = requestRender;
    this._starPackUrl = starPackUrl;
    this._milkyWayFaceUrls = milkyWayFaceUrls;

    const uniforms = {
      u_icrfToFixed: () => this._rotation,
    };
    this._milkyWayUniforms = {
      ...uniforms,
      u_milkyWay: () => this._milkyWayCubeMap,
      u_intensity: () => this.milkyWayIntensity * this.milkyWayVisibility,
    };
    this._starUniforms = uniforms;

    const commands = this._commands;
    this._compositeCommand = {
      derivedCommands: {},
      execute(context, passState) {
        for (const command of commands) command.execute(context, passState);
      },
    };
  }

  _load(): void {
    if (this._loading) return;
    this._loading = true;

    Resource.fetchJson({ url: this._starPackUrl })
      ?.then((pack: { stars?: number[] } | number[] | undefined) => {
        if (this._destroyed || !pack) return;
        const stars = Array.isArray(pack) ? pack : (pack?.stars ?? []);
        this._starData = buildStarVertices(stars);
        this._requestRender();
      })
      .catch((error: unknown) => {
        console.warn('[sky] star catalogue unavailable', error);
      });

    const faces = Object.entries(this._milkyWayFaceUrls);
    Promise.all(faces.map(([, url]) => Resource.fetchImage({ url })))
      .then((images) => {
        if (this._destroyed) return;
        this._faceImages = Object.fromEntries(faces.map(([face], index) => [face, images[index]]));
        this._requestRender();
      })
      .catch((error) => {
        console.warn('[sky] Milky Way unavailable', error);
      });
  }

  _updateRotation(time: JulianDate): void {
    const settled = this._rotationPrecise || this._icrfRequest || !this._canPreloadIcrf(time);
    if (settled && isSkyRotationCurrent(this._rotationTime, time)) return;
    const { precise } = computeSkyRotation(time, this._rotation);
    this._rotationPrecise = precise;
    this._rotationTime = JulianDate.clone(time, this._rotationTime);
    if (!precise) this._preloadIcrf(time);
  }

  _canPreloadIcrf(time: JulianDate): boolean {
    return (
      !this._icrfUnavailable &&
      !(this._icrfMissing && TimeInterval.contains(this._icrfMissing, time))
    );
  }

  _preloadIcrf(time: JulianDate): void {
    if (this._icrfRequest || !this._canPreloadIcrf(time)) return;
    const interval = new TimeInterval({
      start: JulianDate.addDays(time, -1, new JulianDate()),
      stop: JulianDate.addDays(time, 1, new JulianDate()),
    });
    if (typeof Transforms.preloadIcrfFixed !== 'function') return;
    this._icrfRequest = Promise.resolve(Transforms.preloadIcrfFixed(interval))
      .then(
        () => {
          if (!Transforms.computeIcrfToFixedMatrix(time)) {
            this._icrfMissing = interval;
            return;
          }
          this._rotationTime = undefined;
          if (!this._destroyed) this._requestRender();
        },
        (error) => {
          this._icrfUnavailable = true;
          console.warn('[sky] ICRF tables unavailable, using TEME', error);
        },
      )
      .finally(() => {
        this._icrfRequest = undefined;
      });
  }

  _createMilkyWayCommand(context: unknown): void {
    const cubeMap = new CubeMap({
      context,
      source: this._faceImages,
      flipY: false,
    });
    cubeMap.generateMipmap();
    cubeMap.sampler = new Sampler({
      minificationFilter: TextureMinificationFilter.LINEAR_MIPMAP_LINEAR,
    });
    this._milkyWayCubeMap = cubeMap;
    this._faceImages = undefined;

    const geometry = BoxGeometry.createGeometry(
      BoxGeometry.fromDimensions({
        dimensions: new Cartesian3(2, 2, 2),
        vertexFormat: VertexFormat.POSITION_ONLY,
      }),
    );
    if (!geometry) return;
    const attributeLocations = GeometryPipeline.createAttributeLocations(geometry);
    this._milkyWayCommand = new DrawCommand({
      owner: this,
      vertexArray: VertexArray.fromGeometry({
        context: context as Record<string, unknown>,
        geometry,
        attributeLocations,
        bufferUsage: BufferUsage.STATIC_DRAW,
      }),
      renderState: RenderState.fromCache({}),
      uniformMap: this._milkyWayUniforms,
    });
    this._milkyWayAttributes = attributeLocations;
  }

  _createStarCommand(context: unknown): void {
    if (!this._starData) return;
    const { count, vertices } = this._starData;
    this._starData = undefined;
    if (count === 0) return;

    const vertexBuffer = CesiumBuffer.createVertexBuffer({
      context,
      typedArray: vertices,
      usage: BufferUsage.STATIC_DRAW,
    });
    const stride = STAR_VERTEX_FLOATS * Float32Array.BYTES_PER_ELEMENT;
    const attribute = (index: number, components: number, offsetFloats: number) => ({
      index,
      vertexBuffer,
      componentsPerAttribute: components,
      componentDatatype: ComponentDatatype.FLOAT,
      offsetInBytes: offsetFloats * Float32Array.BYTES_PER_ELEMENT,
      strideInBytes: stride,
    });

    this._starCommand = new DrawCommand({
      owner: this,
      primitiveType: PrimitiveType.POINTS,
      count,
      vertexArray: new VertexArray({
        context,
        attributes: [attribute(0, 3, 0), attribute(1, 1, 3), attribute(2, 4, 4)],
      }),
      renderState: RenderState.fromCache({
        blending: {
          enabled: true,
          equationRgb: BlendEquation.ADD,
          equationAlpha: BlendEquation.ADD,
          functionSourceRgb: BlendFunction.ONE,
          functionDestinationRgb: BlendFunction.ONE,
          functionSourceAlpha: BlendFunction.ZERO,
          functionDestinationAlpha: BlendFunction.ONE,
        },
      }),
      uniformMap: this._starUniforms,
    });
  }

  _updateShaders(context: unknown, useHdr: boolean): void {
    if (this._useHdr === useHdr) return;
    this._useHdr = useHdr;
    const defines = useHdr ? ['HDR'] : [];
    const targets: Array<{
      command: DrawCommand | undefined;
      vs: string;
      fs: string;
      attributeLocations?: Record<string, number> | undefined;
    }> = [
      {
        command: this._milkyWayCommand,
        vs: MILKY_WAY_VS,
        fs: MILKY_WAY_FS,
        attributeLocations: this._milkyWayAttributes,
      },
      {
        command: this._starCommand,
        vs: STAR_VS,
        fs: STAR_FS,
        attributeLocations: { position: 0, sigma: 1, color: 2 },
      },
    ];

    for (const { command, vs, fs, attributeLocations } of targets) {
      if (!command) continue;
      command.shaderProgram?.destroy();
      command.shaderProgram = ShaderProgram.fromCache({
        context: context as Record<string, unknown>,
        vertexShaderSource: vs,
        fragmentShaderSource: new ShaderSource({
          defines,
          sources: [fs],
        }),
        attributeLocations,
      });
    }
  }

  update(
    frameState: {
      mode?: number | undefined;
      passes?: { render?: boolean; [key: string]: unknown } | undefined;
      context?: unknown;
      time?: JulianDate | undefined;
      [key: string]: unknown;
    },
    useHdr?: boolean,
  ): DrawCommand | undefined {
    if (!this.show) return undefined;
    const mode = frameState.mode;
    const passes = frameState.passes;
    const context = frameState.context;
    if (mode !== SceneMode.SCENE3D) return undefined;
    if (!passes?.render) return undefined;

    if (!this._cubeMap && context) {
      const face = {
        width: 1,
        height: 1,
        arrayBufferView: new Uint8Array([0, 0, 0, 255]),
      };
      this._cubeMap = new CubeMap({
        context,
        source: {
          positiveX: face,
          negativeX: face,
          positiveY: face,
          negativeY: face,
          positiveZ: face,
          negativeZ: face,
        },
        flipY: false,
      });
    }

    this._load();

    let rebuilt = false;
    if (this._faceImages && !this._milkyWayCommand && context) {
      this._createMilkyWayCommand(context);
      rebuilt = true;
    }
    if (this._starData && !this._starCommand && context) {
      this._createStarCommand(context);
      rebuilt = true;
    }
    if (rebuilt) this._useHdr = undefined;

    if (context) {
      this._updateShaders(context, Boolean(useHdr));
    }

    this._commands.length = 0;
    if (this._milkyWayCommand) this._commands.push(this._milkyWayCommand);
    if (this._starCommand) this._commands.push(this._starCommand);
    if (this._commands.length === 0) return undefined;

    if (frameState.time) {
      this._updateRotation(frameState.time);
    }
    return this._compositeCommand as unknown as DrawCommand;
  }

  destroy(): void {
    for (const command of [this._milkyWayCommand, this._starCommand]) {
      if (!command) continue;
      command.vertexArray?.destroy();
      command.shaderProgram?.destroy();
    }
    this._milkyWayCubeMap?.destroy();
    this._cubeMap?.destroy();
    this._milkyWayCubeMap = undefined;
    this._cubeMap = undefined;
    this._milkyWayCommand = undefined;
    this._starCommand = undefined;
    this._commands.length = 0;
    this._destroyed = true;
  }

  isDestroyed(): boolean {
    return this._destroyed;
  }
}
