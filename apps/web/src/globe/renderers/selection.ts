/**
 * The selection halo (design-system §4): a 2 px `--map-selection` ring with `--map-outline`
 * separation rings, drawn as one billboard that follows the selected entity as snapshots and
 * propagation move it. Hidden when nothing is selected or the entity is not drawn.
 */
import { Cartesian3 } from '@cesium/core';
import { type Billboard, BillboardCollection, type PrimitiveCollection } from '@cesium/engine';
import type { EntityRef } from '../index';
import { drawHalo, GLYPH_SUPERSAMPLE, haloId } from './glyphs';
import type { Located, MapColors } from './types';

/** Draws the halo slightly towards the camera so it is never hidden by its own symbol. */
const EYE_OFFSET = new Cartesian3(0, 0, -10);

export class SelectionHalo {
  readonly #primitives: PrimitiveCollection;
  readonly #colors: MapColors;
  readonly #billboards: BillboardCollection;
  readonly #billboard: Billboard;
  readonly #images = new Map<string, HTMLCanvasElement>();
  #selection: EntityRef | null = null;
  #imageId: string | null = null;

  constructor(primitives: PrimitiveCollection, colors: MapColors) {
    this.#primitives = primitives;
    this.#colors = colors;
    this.#billboards = primitives.add(new BillboardCollection()) as BillboardCollection;
    this.#billboard = this.#billboards.add({
      position: Cartesian3.ZERO,
      show: false,
      scale: 1 / GLYPH_SUPERSAMPLE,
      eyeOffset: EYE_OFFSET,
    });
  }

  get selection(): EntityRef | null {
    return this.#selection;
  }

  get visible(): boolean {
    return this.#billboard.show;
  }

  set(selection: EntityRef | null): void {
    this.#selection = selection;
    this.#billboard.id = selection;
  }

  /** Moves the halo to where the selected entity is now (or hides it). */
  follow(located: Located | undefined): void {
    if (!this.#selection || !located) {
      this.#billboard.show = false;
      return;
    }
    const id = haloId(located.symbolPx);
    if (id !== this.#imageId) {
      this.#imageId = id;
      let image = this.#images.get(id);
      if (!image) {
        image = drawHalo(located.symbolPx, this.#colors.css.selection, this.#colors.css.outline);
        this.#images.set(id, image);
      }
      this.#billboard.setImage(id, image);
    }
    this.#billboard.position = located.position;
    this.#billboard.show = true;
  }

  destroy(): void {
    this.#primitives.remove(this.#billboards);
  }
}
