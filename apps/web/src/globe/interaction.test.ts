import { Cartesian2, Cartesian3, Ellipsoid } from '@cesium/core';
import { type CesiumWidget, ScreenSpaceEventType } from '@cesium/engine';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Interaction } from './interaction';

const host = document.createElement('div');
afterEach(() => host.remove());
it('a drawing click moves keyboard focus from the label field to the existing globe region', () => {
  host.innerHTML =
    '<input aria-label="Label"><main aria-label="Globe" tabindex="-1"><canvas></canvas></main>';
  document.body.append(host);
  const input = host.querySelector('input');
  const canvas = host.querySelector('canvas');
  const main = host.querySelector('main');
  if (!input || !canvas || !main) throw new Error('Missing interaction fixture.');
  input.focus();
  let click: ((event: { position: Cartesian2 }) => void) | undefined;
  const point = vi.fn();
  const pick = vi.fn();
  const widget = {
    canvas,
    scene: {
      globe: { ellipsoid: Ellipsoid.WGS84 },
      camera: { pickEllipsoid: () => Cartesian3.fromDegrees(24.94, 60.17) },
    },
    screenSpaceEventHandler: {
      setInputAction: (action: typeof click, type: ScreenSpaceEventType) => {
        if (type === ScreenSpaceEventType.LEFT_CLICK) click = action;
      },
      removeInputAction: vi.fn(),
      isDestroyed: () => false,
    },
  } as unknown as CesiumWidget;
  const interaction = new Interaction(widget, {
    cursor: vi.fn(),
    pick,
    point,
    drawing: () => true,
  });
  click?.({ position: new Cartesian2(100, 100) });
  expect(document.activeElement).toBe(main);
  expect(point).toHaveBeenCalledWith(
    expect.objectContaining({ lat: expect.closeTo(60.17, 5), lon: expect.closeTo(24.94, 5) }),
  );
  expect(pick).not.toHaveBeenCalled();
  interaction.destroy();
});

describe('picking with pick assists', () => {
  const ring = { id: { feed: 'earthquakes', id: 'small' } };
  const wash = { id: { feed: 'earthquakes', id: 'big' } };

  /** A scene whose pick finds `drawn` under the cursor, or the assist while it is shown. */
  function setup(drawn: unknown, assistUnder = true) {
    const canvas = document.createElement('canvas');
    let click: ((event: { position: Cartesian2 }) => void) | undefined;
    const assist = { show: true };
    const shownAtPick: boolean[] = [];
    const scenePick = vi.fn(() => {
      shownAtPick.push(assist.show);
      // A nearer open centre covers the drawn symbol whenever the assists are shown.
      if (assist.show && assistUnder) return wash;
      return drawn;
    });
    const widget = {
      canvas,
      scene: { pick: scenePick },
      screenSpaceEventHandler: {
        setInputAction: (action: typeof click, type: ScreenSpaceEventType) => {
          if (type === ScreenSpaceEventType.LEFT_CLICK) click = action;
        },
        removeInputAction: vi.fn(),
        isDestroyed: () => false,
      },
    } as unknown as CesiumWidget;
    const pick = vi.fn();
    const interaction = new Interaction(widget, { cursor: vi.fn(), pick, pickAssists: [assist] });
    const clickAt = () => click?.({ position: new Cartesian2(10, 10) });
    return { assist, clickAt, interaction, pick, scenePick, shownAtPick };
  }

  it('selects the symbol drawn under the cursor before any open centre covering it', () => {
    const { assist, clickAt, interaction, pick, shownAtPick } = setup(ring);
    clickAt();
    expect(pick).toHaveBeenCalledWith({ feed: 'earthquakes', id: 'small' });
    expect(shownAtPick).toEqual([false]);
    expect(assist.show).toBe(true);
    interaction.destroy();
  });

  it('falls back to the open centre when nothing drawn is under the cursor', () => {
    const { assist, clickAt, interaction, pick, shownAtPick } = setup(undefined);
    clickAt();
    expect(pick).toHaveBeenCalledWith({ feed: 'earthquakes', id: 'big' });
    expect(shownAtPick).toEqual([false, true]);
    expect(assist.show).toBe(true);
    interaction.destroy();
  });

  it('reports empty globe when neither a symbol nor an open centre is under the cursor', () => {
    const { assist, clickAt, interaction, pick } = setup(undefined, false);
    clickAt();
    expect(pick).toHaveBeenCalledWith(null);
    expect(assist.show).toBe(true);
    interaction.destroy();
  });

  it('restores the assists when the pick fails', () => {
    const { assist, clickAt, interaction, pick, scenePick } = setup(ring);
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    scenePick.mockImplementationOnce(() => {
      throw new Error('context lost');
    });
    clickAt();
    expect(pick).toHaveBeenCalledWith(null);
    expect(assist.show).toBe(true);
    expect(error).toHaveBeenCalled();
    error.mockRestore();
    interaction.destroy();
  });

  it('picks once, and leaves hidden, an assist its renderer hid', () => {
    const { assist, clickAt, interaction, pick, shownAtPick } = setup(undefined, false);
    assist.show = false;
    clickAt();
    expect(shownAtPick).toEqual([false]);
    expect(assist.show).toBe(false);
    expect(pick).toHaveBeenCalledWith(null);
    interaction.destroy();
  });
});
