import type { Catalog, FeedId } from '../contracts';
import { showToast } from '../design';
import type { EntityRef } from '../globe';
import type { LayerState, Stores, WorkspaceState } from '../state/stores';
import {
  type WorkspaceDocument,
  WorkspaceLinkError,
  workspaceDocumentFromUrl,
  workspaceShareUrl,
} from '../state/workspaceDocument';

export interface WorkspaceLocation {
  currentUrl(): string;
  onNavigate(listener: () => void): () => void;
}

export function browserWorkspaceLocation(): WorkspaceLocation | null {
  if (typeof window === 'undefined') return null;
  return {
    currentUrl: () => window.location.href,
    onNavigate(listener) {
      window.addEventListener('hashchange', listener);
      window.addEventListener('popstate', listener);
      return () => {
        window.removeEventListener('hashchange', listener);
        window.removeEventListener('popstate', listener);
      };
    },
  };
}

interface PendingRestore {
  readonly document: WorkspaceDocument;
  readonly revisions: WorkspaceState['revisions'];
  readonly cameraRevision: number;
  workspaceApplied: boolean;
}

/** One pending document, with independent workspace/camera intent lanes. No entity data. */
export class WorkspaceShare {
  readonly #stores: Stores;
  readonly #location: WorkspaceLocation | null;
  #cameraRevision = 0;
  #pending: PendingRestore | null = null;
  #lastUrl: string | null = null;

  constructor(stores: Stores, location: WorkspaceLocation | null) {
    this.#stores = stores;
    this.#location = location;
  }

  start(): () => void {
    this.#lastUrl = null;
    const navigate = () => {
      const url = this.#location?.currentUrl();
      if (!url || url === this.#lastUrl) return;
      this.#lastUrl = url;
      this.restoreUrl(url);
    };
    navigate();
    const off = this.#location?.onNavigate(navigate);
    return () => {
      off?.();
      this.#pending = null;
    };
  }

  claimCamera(): void {
    this.#cameraRevision += 1;
  }

  restoreUrl(url: string): boolean {
    this.#pending = null;
    try {
      const document = workspaceDocumentFromUrl(url);
      if (!document) return false;
      this.#pending = {
        document,
        revisions: this.#stores.workspace.getState().revisions,
        cameraRevision: this.#cameraRevision,
        workspaceApplied: false,
      };
      this.apply(this.#stores.system.getState().catalog);
      return true;
    } catch (error) {
      showToast({
        kind: 'error',
        title: 'Could not open shared view',
        description:
          error instanceof WorkspaceLinkError
            ? error.message
            : 'This link is invalid or too large.',
      });
      return false;
    }
  }

  /** Called before catalog publication, so only the final layer choices open feeds. */
  apply(catalog: Catalog | null): void {
    const pending = this.#pending;
    if (!pending || !catalog) return;
    const { workspace, ui } = this.#stores;
    if (!pending.workspaceApplied) {
      pending.workspaceApplied = true;
      const state = workspace.getState();
      const known = new Set<string>(catalog.layers.map((layer) => layer.feed));
      let layers = state.layers;
      if (state.revisions.layers === pending.revisions.layers) {
        const restored: Partial<Record<FeedId, LayerState>> = {};
        for (const layer of pending.document.layers) {
          if (known.has(layer.id))
            restored[layer.id as FeedId] = { enabled: layer.enabled, filters: layer.options };
        }
        layers = restored;
      }
      const imageryId =
        state.revisions.imagery !== pending.revisions.imagery
          ? state.imageryId
          : catalog.imagery.some((imagery) => imagery.id === pending.document.imageryId)
            ? pending.document.imageryId
            : null;
      let selection = state.selection;
      if (state.revisions.selection === pending.revisions.selection) {
        const selected = pending.document.selection;
        selection =
          selected &&
          known.has(selected.feed) &&
          layers[selected.feed as FeedId]?.enabled &&
          catalog.layers.some(
            (layer) => layer.feed === selected.feed && layer.availability.state === 'available',
          )
            ? (selected as EntityRef)
            : null;
      }
      workspace
        .getState()
        .restore(
          layers,
          imageryId,
          selection,
          state.revisions.annotations === pending.revisions.annotations
            ? pending.document.annotations
            : state.annotations,
        );
    }
    const globe = ui.getState().globe;
    if (!globe) return;
    // Cut synchronously: the restore cannot complete later and steal a newer camera action.
    this.#pending = null;
    if (pending.cameraRevision === this.#cameraRevision)
      globe.restoreCamera(pending.document.camera);
  }

  async copyLink(): Promise<void> {
    try {
      const pose = this.#stores.ui.getState().globe?.cameraPose();
      if (!pose || !this.#location) throw new WorkspaceLinkError('Wait for the globe to load.');
      const state = this.#stores.workspace.getState();
      const catalog = this.#stores.system.getState().catalog;
      const document: WorkspaceDocument = {
        schemaVersion: 2,
        camera: pose,
        layers: (catalog?.layers ?? []).map(({ feed }) => ({
          id: feed,
          enabled: state.layers[feed]?.enabled ?? false,
          options: state.layers[feed]?.filters ?? {},
        })),
        imageryId:
          state.imageryId ??
          catalog?.imagery.find((imagery) => imagery.isDefault)?.id ??
          catalog?.imagery[0]?.id ??
          null,
        selection: state.selection,
        annotations: state.annotations,
      };
      const link = workspaceShareUrl(document, this.#location.currentUrl());
      await navigator.clipboard.writeText(link);
      showToast({ kind: 'success', title: 'Link copied' });
    } catch (error) {
      showToast({
        kind: 'error',
        title: 'Could not copy link',
        description:
          error instanceof WorkspaceLinkError
            ? error.message
            : 'Allow clipboard access and try again.',
      });
    }
  }
}
