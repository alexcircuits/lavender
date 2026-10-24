/**
 * WebAssembly-accelerated Screen-Space Label Collision Arbiter (ENG-979).
 *
 * Provides ultra-fast 2D bounding box collision detection and priority arbitration
 * using 32px spatial hash grid buckets in the freestanding Zig WebAssembly kernel.
 */
import {
  getActiveLavenderWasm,
  type LavenderWasmContext,
  loadLavenderWasm,
} from './wasmPropagation';

export const LABEL_CANDIDATE_BYTE_SIZE = 24;
export const DEFAULT_MAX_LABELS = 500;

/** Reserve scratch memory above heapBase:
 * 0 - 4MB: SGP4 orbit propagator scratch
 * 4MB - 8MB: Wind grid and streamline scratch
 * 8MB - 9MB: Label candidate & mask scratch
 */
const LABELS_SCRATCH_OFFSET = 8 * 1024 * 1024;
const MAX_CANDIDATES = 10_000;

export interface LabelCandidateInput {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
  readonly priority?: number | undefined;
  readonly isSelected?: boolean | undefined;
  readonly isTracked?: boolean | undefined;
}

export interface LabelArbitrationResult {
  readonly visible: boolean[];
  readonly visibleCount: number;
}

function getWasmContext(): LavenderWasmContext {
  const wasm = getActiveLavenderWasm();
  if (!wasm) {
    throw new Error('Lavender WebAssembly module is not loaded. Call loadLavenderWasm() first.');
  }
  return wasm;
}

/**
 * Arbitrates screen-space label placements using the Zig WebAssembly kernel.
 * Resolves 2D AABB bounding box overlaps, prioritizing selected labels, tracked labels,
 * and higher priority weights, within a maximum visible label budget.
 *
 * @param candidates Array of label candidates in screen coordinates
 * @param screenWidth Viewport width in pixels
 * @param screenHeight Viewport height in pixels
 * @param maxLabels Maximum number of labels to place (defaults to 500)
 * @returns Result containing a boolean mask indicating visibility for each candidate and total placed count
 */
export function arbitrateLabelsWasm(
  candidates: readonly LabelCandidateInput[],
  screenWidth: number,
  screenHeight: number,
  maxLabels: number = DEFAULT_MAX_LABELS,
): LabelArbitrationResult {
  const count = candidates.length;
  if (count === 0 || screenWidth <= 0 || screenHeight <= 0 || maxLabels <= 0) {
    return { visible: new Array<boolean>(count).fill(false), visibleCount: 0 };
  }

  const wasm = getWasmContext();
  const clampedCount = Math.min(count, MAX_CANDIDATES);

  // Layout scratch pointers
  const labelsBase = wasm.heapBase + LABELS_SCRATCH_OFFSET;
  const labelsPtr = labelsBase;
  const maskPtr = labelsBase + clampedCount * LABEL_CANDIDATE_BYTE_SIZE;

  // Ensure WASM linear memory has enough pages
  const requiredBytes = maskPtr + clampedCount;
  if (requiredBytes > wasm.memory.buffer.byteLength) {
    const bytesNeeded = requiredBytes - wasm.memory.buffer.byteLength;
    const pagesNeeded = Math.ceil(bytesNeeded / 65536);
    wasm.memory.grow(pagesNeeded);
  }

  // Write candidate structs into WASM memory
  const dataView = new DataView(
    wasm.memory.buffer,
    labelsPtr,
    clampedCount * LABEL_CANDIDATE_BYTE_SIZE,
  );
  for (let i = 0; i < clampedCount; i++) {
    const c = candidates[i];
    if (!c) continue;
    const offset = i * LABEL_CANDIDATE_BYTE_SIZE;
    dataView.setFloat32(offset, c.x, true);
    dataView.setFloat32(offset + 4, c.y, true);
    dataView.setFloat32(offset + 8, c.w, true);
    dataView.setFloat32(offset + 12, c.h, true);
    dataView.setUint32(offset + 16, c.priority ?? 0, true);
    dataView.setUint8(offset + 20, c.isSelected ? 1 : 0);
    dataView.setUint8(offset + 21, c.isTracked ? 1 : 0);
    dataView.setUint8(offset + 22, 0); // padding
    dataView.setUint8(offset + 23, 0); // padding
  }

  // Execute Zig WebAssembly kernel
  const visibleCount = wasm.exports.lavender_arbitrate_labels(
    labelsPtr,
    clampedCount,
    screenWidth,
    screenHeight,
    maxLabels,
    maskPtr,
  );

  // Read output mask from WASM memory
  const maskView = new Uint8Array(wasm.memory.buffer, maskPtr, count);
  const visible: boolean[] = new Array(count);
  for (let i = 0; i < count; i++) {
    if (i < clampedCount) {
      visible[i] = maskView[i] === 1;
    } else {
      visible[i] = false;
    }
  }

  return { visible, visibleCount };
}

export { loadLavenderWasm };
