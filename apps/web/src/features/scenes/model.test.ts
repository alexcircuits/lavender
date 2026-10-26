import { describe, expect, it, vi } from 'vitest';
import type { SceneShot } from '../../contracts';
import { type Clock, plainCredit, ScenePlayer, sampleCameraMove } from './model';

const pose = (lon: number, heading = 0) => ({
  lat: 0,
  lon,
  altM: 1000,
  headingDeg: heading,
  pitchDeg: -30,
  rollDeg: 0,
});

describe('sampleCameraMove', () => {
  const move = { from: pose(0), to: { ...pose(100), altM: 3000 }, easing: 'linear' as const };
  it('returns endpoints at 0 and 1 and clamps', () => {
    expect(sampleCameraMove(move, -1)).toEqual(move.from);
    expect(sampleCameraMove(move, 2)).toEqual(move.to);
    expect(sampleCameraMove(move, Number.NaN)).toEqual(move.from);
  });
  it('interpolates linearly', () => {
    const m = sampleCameraMove(move, 0.5);
    expect(m.lon).toBeCloseTo(50);
    expect(m.altM).toBeCloseTo(2000);
  });
  it('eases cubic by default', () => {
    const cubicMove = { from: move.from, to: move.to };
    expect(sampleCameraMove(cubicMove, 0.5).lon).toBeCloseTo(50);
    expect(sampleCameraMove(cubicMove, 0.25).lon).toBeCloseTo(100 * 4 * 0.25 ** 3);
  });
  it('takes the short arc across the antimeridian', () => {
    const m = sampleCameraMove({ from: pose(170, 350), to: pose(-170, 10), easing: 'linear' }, 0.5);
    expect(Math.abs(m.lon)).toBeCloseTo(180);
    expect(((m.headingDeg % 360) + 360) % 360).toBeCloseTo(0);
  });
});

class FakeClock implements Clock {
  t = 0;
  timers = new Map<number, { at: number; fn: () => void }>();
  id = 0;
  now() {
    return this.t;
  }
  setTimeout(fn: () => void, ms: number) {
    this.timers.set(++this.id, { at: this.t + ms, fn });
    return this.id;
  }
  clearTimeout(h: unknown) {
    this.timers.delete(h as number);
  }
  advance(ms: number) {
    const end = this.t + ms;
    for (;;) {
      const next = [...this.timers.entries()].sort((a, b) => a[1].at - b[1].at)[0];
      if (!next || next[1].at > end) break;
      this.timers.delete(next[0]);
      this.t = next[1].at;
      next[1].fn();
    }
    this.t = end;
  }
}

const shot = (id: string, durationS = 2, holdS = 3): SceneShot => ({
  id,
  title: id,
  pose: pose(0),
  durationS,
  holdS,
  mapStack: 'osm',
});

describe('ScenePlayer', () => {
  const setup = () => {
    const clock = new FakeClock();
    const onShot = vi.fn();
    const p = new ScenePlayer({ shots: [shot('a'), shot('b')] }, onShot, clock);
    return { clock, onShot, p };
  };
  it('flies, holds, advances and finishes', () => {
    const { clock, onShot, p } = setup();
    p.play();
    expect(p.state.phase).toBe('flying');
    expect(onShot).toHaveBeenCalledTimes(1);
    clock.advance(2000);
    expect(p.state.phase).toBe('holding');
    clock.advance(3000);
    expect(p.state).toMatchObject({ phase: 'flying', index: 1 });
    expect(onShot).toHaveBeenCalledTimes(2);
    clock.advance(5000);
    expect(p.state.phase).toBe('finished');
  });
  it('pauses and resumes with remaining time', () => {
    const { clock, onShot, p } = setup();
    p.play();
    clock.advance(1500);
    p.pause();
    expect(p.state).toMatchObject({ phase: 'paused', remainingMs: 500 });
    clock.advance(10_000);
    expect(p.state.phase).toBe('paused');
    p.play();
    expect(onShot).toHaveBeenCalledTimes(1);
    clock.advance(500);
    expect(p.state.phase).toBe('holding');
  });
  it('next/prev jump shots and clamp', () => {
    const { p, onShot } = setup();
    p.next();
    expect(p.state.index).toBe(1);
    p.next();
    expect(p.state.index).toBe(1);
    p.prev();
    expect(p.state.index).toBe(0);
    p.prev();
    expect(p.state.index).toBe(0);
    expect(onShot).toHaveBeenCalledTimes(3);
  });
  it('replays from the start after finishing and handles empty packs', () => {
    const { clock, p } = setup();
    p.play();
    clock.advance(10_000);
    p.play();
    expect(p.state).toMatchObject({ phase: 'flying', index: 0 });
    const empty = new ScenePlayer({ shots: [] }, vi.fn(), clock);
    empty.play();
    expect(empty.state.phase).toBe('idle');
  });
});

describe('plainCredit', () => {
  it('strips markup', () => {
    expect(plainCredit('© <a href="x">OSM</a> &amp; co')).toBe('© OSM & co');
  });
});
