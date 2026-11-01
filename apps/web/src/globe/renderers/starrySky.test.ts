import { Cartesian3, JulianDate, Matrix3 } from '@cesium/core';
import { SceneMode, Transforms } from '@cesium/engine';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  computeSkyRotation,
  isSkyRotationCurrent,
  MILKY_WAY_INTENSITY,
  SKY_ROTATION_EPSILON_SECONDS,
  StarrySky,
} from './starrySky';

describe('starrySky', () => {
  describe('rotation math', () => {
    const time = JulianDate.fromIso8601('2026-10-19T00:00:00Z');

    it('prefers ICRF rotation when available', () => {
      const icrf = Matrix3.fromRotationZ(0.35);
      const spyIcrf = vi
        .spyOn(Transforms, 'computeIcrfToFixedMatrix')
        .mockImplementation((_t, result) => Matrix3.clone(icrf, result as Matrix3));
      const spyTeme = vi.spyOn(Transforms, 'computeTemeToPseudoFixedMatrix');

      try {
        const { matrix, precise } = computeSkyRotation(time);
        expect(precise).toBe(true);
        expect(Matrix3.equals(matrix, icrf)).toBe(true);
        expect(spyTeme).not.toHaveBeenCalled();
      } finally {
        spyIcrf.mockRestore();
        spyTeme.mockRestore();
      }
    });

    it('falls back to TEME pseudo-fixed when ICRF tables are not loaded', () => {
      const teme = Matrix3.fromRotationZ(0.22);
      const spyIcrf = vi
        .spyOn(Transforms, 'computeIcrfToFixedMatrix')
        .mockReturnValue(undefined as unknown as Matrix3);
      const spyTeme = vi
        .spyOn(Transforms, 'computeTemeToPseudoFixedMatrix')
        .mockImplementation((_t, result) => Matrix3.clone(teme, result as Matrix3));

      try {
        const { matrix, precise } = computeSkyRotation(time);
        expect(precise).toBe(false);
        expect(Matrix3.equals(matrix, teme)).toBe(true);
      } finally {
        spyIcrf.mockRestore();
        spyTeme.mockRestore();
      }
    });

    it('falls back to identity matrix when both ICRF and TEME return nothing', () => {
      const spyIcrf = vi
        .spyOn(Transforms, 'computeIcrfToFixedMatrix')
        .mockReturnValue(undefined as unknown as Matrix3);
      const spyTeme = vi
        .spyOn(Transforms, 'computeTemeToPseudoFixedMatrix')
        .mockReturnValue(undefined as unknown as Matrix3);

      try {
        const { matrix, precise } = computeSkyRotation(time);
        expect(precise).toBe(false);
        expect(Matrix3.equals(matrix, Matrix3.IDENTITY)).toBe(true);
      } finally {
        spyIcrf.mockRestore();
        spyTeme.mockRestore();
      }
    });

    it('keeps celestial pole on the Earth rotation axis under TEME', () => {
      const teme = Transforms.computeTemeToPseudoFixedMatrix(time);
      const pole = Matrix3.multiplyByVector(teme, Cartesian3.UNIT_Z, new Cartesian3());
      expect(Math.abs(pole.z - 1)).toBeLessThan(1e-9);
    });

    it('evaluates whether cached rotation is still current within epsilon', () => {
      const later = (seconds: number) => JulianDate.addSeconds(time, seconds, new JulianDate());

      expect(isSkyRotationCurrent(undefined, time)).toBe(false);
      expect(isSkyRotationCurrent(time, later(0.4))).toBe(true);
      expect(isSkyRotationCurrent(time, later(-0.4))).toBe(true);
      expect(isSkyRotationCurrent(time, later(SKY_ROTATION_EPSILON_SECONDS))).toBe(false);
      expect(isSkyRotationCurrent(time, later(SKY_ROTATION_EPSILON_SECONDS + 0.1))).toBe(false);
      expect(isSkyRotationCurrent(time, later(-SKY_ROTATION_EPSILON_SECONDS))).toBe(false);
    });
  });

  describe('StarrySky lifecycle and update protocol', () => {
    let sky: StarrySky | null = null;

    afterEach(() => {
      if (sky && !sky.isDestroyed()) {
        sky.destroy();
      }
      sky = null;
    });

    it('initializes with default settings and tracks destruction lifecycle', () => {
      sky = new StarrySky();
      expect(sky.show).toBe(true);
      expect(sky.milkyWayIntensity).toBe(MILKY_WAY_INTENSITY);
      expect(sky.milkyWayVisibility).toBe(1);
      expect(sky.isDestroyed()).toBe(false);

      sky.destroy();
      expect(sky.isDestroyed()).toBe(true);

      // Idempotent destroy
      sky.destroy();
      expect(sky.isDestroyed()).toBe(true);
    });

    it('returns undefined when hidden or non-3D mode or non-render pass', () => {
      sky = new StarrySky({ requestRender: vi.fn() });
      sky._loading = true; // prevent actual network fetching in unit test

      const baseFrameState = {
        mode: SceneMode.SCENE3D,
        passes: { render: true },
        time: JulianDate.fromIso8601('2026-10-19T00:00:00Z'),
        context: {},
      };

      // 1. When show is false
      sky.show = false;
      expect(sky.update(baseFrameState)).toBeUndefined();

      // 2. When mode is SCENE2D
      sky.show = true;
      expect(sky.update({ ...baseFrameState, mode: SceneMode.SCENE2D })).toBeUndefined();

      // 3. When mode is COLUMBUS_VIEW
      expect(sky.update({ ...baseFrameState, mode: SceneMode.COLUMBUS_VIEW })).toBeUndefined();

      // 4. When pass is not render (e.g. pick pass)
      expect(
        sky.update({ ...baseFrameState, passes: { render: false, pick: true } }),
      ).toBeUndefined();

      // 5. When passes is missing render
      expect(sky.update({ ...baseFrameState, passes: {} })).toBeUndefined();

      // 6. When render pass but no commands are loaded yet (mock cubeMap so no WebGL needed)
      sky._cubeMap = { destroy: () => undefined } as unknown as typeof sky._cubeMap;
      expect(sky.update(baseFrameState)).toBeUndefined();
    });

    it('scales Milky Way uniform intensity by visibility multiplier', () => {
      sky = new StarrySky();
      const intensityFn = sky._milkyWayUniforms.u_intensity;
      expect(typeof intensityFn).toBe('function');
      expect(intensityFn?.()).toBeCloseTo(MILKY_WAY_INTENSITY, 6);

      sky.milkyWayVisibility = 0.5;
      expect(intensityFn?.()).toBeCloseTo(MILKY_WAY_INTENSITY * 0.5, 6);

      sky.milkyWayVisibility = 0;
      expect(intensityFn?.()).toBe(0);

      sky.milkyWayIntensity = 0.8;
      sky.milkyWayVisibility = 1;
      expect(intensityFn?.()).toBeCloseTo(0.8, 6);
    });

    it('disposes GL resources cleanly on destroy', () => {
      sky = new StarrySky();
      let cubeMapDestroyed = 0;
      let milkyWayDestroyed = 0;
      let mwVaDestroyed = 0;
      let starVaDestroyed = 0;

      sky._cubeMap = {
        destroy: () => {
          cubeMapDestroyed++;
          return undefined;
        },
      } as unknown as typeof sky._cubeMap;

      sky._milkyWayCubeMap = {
        destroy: () => {
          milkyWayDestroyed++;
          return undefined;
        },
      } as unknown as typeof sky._milkyWayCubeMap;

      sky._milkyWayCommand = {
        vertexArray: {
          destroy: () => {
            mwVaDestroyed++;
            return undefined;
          },
        },
        shaderProgram: { destroy: () => undefined },
      } as unknown as typeof sky._milkyWayCommand;

      sky._starCommand = {
        vertexArray: {
          destroy: () => {
            starVaDestroyed++;
            return undefined;
          },
        },
        shaderProgram: { destroy: () => undefined },
      } as unknown as typeof sky._starCommand;

      sky.destroy();
      expect(sky.isDestroyed()).toBe(true);
      expect(cubeMapDestroyed).toBe(1);
      expect(milkyWayDestroyed).toBe(1);
      expect(mwVaDestroyed).toBe(1);
      expect(starVaDestroyed).toBe(1);
      expect(sky._cubeMap).toBeUndefined();
      expect(sky._milkyWayCubeMap).toBeUndefined();
      expect(sky._milkyWayCommand).toBeUndefined();
      expect(sky._starCommand).toBeUndefined();
      expect(sky._commands.length).toBe(0);
    });
  });
});
