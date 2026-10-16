/**
 * Chooses the platform once, at the composition root. Nothing else in the app branches on
 * the environment (ADR 0005).
 */

import type { LavenderPlatform } from './types';
import { WebPlatform } from './web/WebPlatform';

export type {
  CallOptions,
  DiagnosticsSink,
  FeedEvent,
  FeedSubscription,
  LavenderPlatform,
  PreferenceStore,
  TransportState,
  WindGridRequest,
  WindRequest,
} from './types';
export { PlatformError, toApiError } from './types';
export { decodeWindGrid, type WindGridData } from './windGrid';

/** Whether the page runs inside the Lavender desktop app (Tauri 2 sets this global). */
export function isDesktop(): boolean {
  return (
    typeof globalThis === 'object' &&
    'isTauri' in globalThis &&
    Boolean((globalThis as { isTauri?: unknown }).isTauri)
  );
}

export async function createPlatform(): Promise<LavenderPlatform> {
  if (isDesktop()) {
    // Loaded only on desktop so the web bundle carries no Tauri code.
    const [{ TauriPlatform }, core] = await Promise.all([
      import('./tauri/TauriPlatform'),
      import('@tauri-apps/api/core'),
    ]);
    return new TauriPlatform({
      invoke: (command, args) => core.invoke(command, args),
      channel: <T>() => new core.Channel<T>(),
    });
  }
  return new WebPlatform();
}
