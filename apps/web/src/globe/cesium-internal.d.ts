declare module '@cesium/engine' {
  import type { Matrix3, TimeInterval } from '@cesium/core';

  export class DrawCommand {
    constructor(options?: Record<string, unknown>);
    owner?: unknown;
    vertexArray?: VertexArray;
    primitiveType?: unknown;
    count?: number;
    renderState?: unknown;
    shaderProgram?: ShaderProgram;
    uniformMap?: Record<string, unknown>;
    derivedCommands?: Record<string, unknown>;
    execute(context: unknown, passState?: unknown): void;
  }

  export class VertexArray {
    constructor(options?: Record<string, unknown>);
    static fromGeometry(options: Record<string, unknown>): VertexArray;
    destroy(): undefined;
    isDestroyed(): boolean;
  }

  export class ShaderProgram {
    static fromCache(options: Record<string, unknown>): ShaderProgram;
    destroy(): undefined;
    isDestroyed(): boolean;
  }

  export class ShaderSource {
    constructor(options: { defines?: string[]; sources?: string[] });
  }

  export class CubeMap {
    constructor(options: Record<string, unknown>);
    generateMipmap(): void;
    sampler: unknown;
    destroy(): undefined;
    isDestroyed(): boolean;
  }

  export class Sampler {
    constructor(options?: Record<string, unknown>);
  }

  export namespace Buffer {
    export function createVertexBuffer(options: Record<string, unknown>): unknown;
  }

  export const BufferUsage: {
    STATIC_DRAW: unknown;
    DYNAMIC_DRAW: unknown;
    STREAM_DRAW: unknown;
  };

  export namespace RenderState {
    export function fromCache(options: Record<string, unknown>): unknown;
  }

  export namespace ContextLimits {
    export let _maximumCubeMapSize: number | undefined;
  }

  export namespace Transforms {
    export function preloadIcrfFixed(timeInterval: TimeInterval): Promise<void>;
    export function computeIcrfToFixedMatrix(time: unknown, result?: Matrix3): Matrix3 | undefined;
    export function computeTemeToPseudoFixedMatrix(time: unknown, result?: Matrix3): Matrix3;
  }
}
