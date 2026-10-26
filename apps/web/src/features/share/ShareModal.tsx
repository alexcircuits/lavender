import { useMemo, useState } from 'react';
import type { FeedId } from '../../contracts';
import { Button, CheckIcon, CopyIcon, Dialog, DialogClose, showToast } from '../../design';
import { useGlobe, usePreferences, useUi, useWorkspace } from '../../runtime';
import styles from './share.module.css';
import { copyShareUrl, encodeShareUrl, type ShareState } from './shareCodec';

export interface ShareModalProps {
  readonly open?: boolean | undefined;
  readonly onOpenChange?: ((open: boolean) => void) | undefined;
}

export function ShareModal({ open, onOpenChange }: ShareModalProps) {
  const storeOpen = useUi((s) => s.shareOpen);
  const setStoreOpen = useUi((s) => s.setShareOpen);
  const isOpen = open !== undefined ? open : storeOpen;
  const handleOpenChange = onOpenChange ?? setStoreOpen;

  const globe = useGlobe();
  const layers = useWorkspace((s) => s.layers);
  const selection = useWorkspace((s) => s.selection);
  const imageryId = useWorkspace((s) => s.imageryId);
  const visualPreset = usePreferences((s) => s.visualPreset);
  const hudEnabled = usePreferences((s) => s.hudEnabled);
  const scopeMaskEnabled = usePreferences((s) => s.scopeMaskEnabled);
  const sonarEnabled = usePreferences((s) => s.sonarEnabled);

  const [includeCamera, setIncludeCamera] = useState(true);
  const [includeLayers, setIncludeLayers] = useState(true);
  const [includeSelection, setIncludeSelection] = useState(true);
  const [includeOverlays, setIncludeOverlays] = useState(true);
  const [copied, setCopied] = useState(false);

  const enabledFeedIds = useMemo(() => {
    return (Object.entries(layers) as [FeedId, { enabled: boolean }][])
      .filter(([, state]) => state?.enabled)
      .map(([feed]) => feed);
  }, [layers]);

  const cameraPose = useMemo(() => {
    return globe?.cameraPose() ?? null;
  }, [globe]);

  const shareState = useMemo<ShareState>(() => {
    return {
      camera: includeCamera ? cameraPose : null,
      enabledLayers: includeLayers ? enabledFeedIds : [],
      visualPreset: includeOverlays ? visualPreset : 'normal',
      hudEnabled: includeOverlays ? hudEnabled : false,
      scopeMaskEnabled: includeOverlays ? scopeMaskEnabled : false,
      sonarEnabled: includeOverlays ? sonarEnabled : false,
      selection: includeSelection ? selection : null,
      imageryId: includeLayers ? imageryId : null,
    };
  }, [
    includeCamera,
    cameraPose,
    includeLayers,
    enabledFeedIds,
    includeOverlays,
    visualPreset,
    hudEnabled,
    scopeMaskEnabled,
    sonarEnabled,
    includeSelection,
    selection,
    imageryId,
  ]);

  const shareUrl = useMemo(() => {
    return encodeShareUrl(shareState);
  }, [shareState]);

  const handleCopy = async () => {
    const success = await copyShareUrl(shareState);
    if (success) {
      setCopied(true);
      showToast({ title: 'Workspace link copied to clipboard' });
      setTimeout(() => setCopied(false), 2000);
    }
  };

  return (
    <Dialog
      open={isOpen}
      onOpenChange={handleOpenChange}
      title="Share Workspace View"
      description="Share your camera position, active layers, and selected contacts via a deep link."
      className={styles.modal}
      actions={
        <div className={styles.footer}>
          <DialogClose render={<Button variant="secondary">Close</Button>} />
          <Button variant="primary" icon={CopyIcon} onClick={handleCopy}>
            Copy Link
          </Button>
        </div>
      }
    >
      <div className={styles.body}>
        <div className={styles.urlBox}>
          <input
            type="text"
            readOnly
            value={shareUrl}
            className={styles.urlInput}
            aria-label="Shareable workspace URL"
            onFocus={(e) => e.target.select()}
          />
          <Button
            size="sm"
            variant="secondary"
            icon={copied ? CheckIcon : CopyIcon}
            onClick={handleCopy}
          >
            {copied ? 'Copied' : 'Copy'}
          </Button>
        </div>

        <div className={styles.summary}>
          {cameraPose && (
            <div className={styles.summaryRow}>
              <span className={styles.summaryLabel}>Coordinates:</span>
              <span className={styles.summaryValue}>
                {cameraPose.lat.toFixed(4)}°, {cameraPose.lon.toFixed(4)}° (
                {Math.round(cameraPose.heightM).toLocaleString()}m)
              </span>
            </div>
          )}
          <div className={styles.summaryRow}>
            <span className={styles.summaryLabel}>Active Layers:</span>
            <span className={styles.summaryValue}>
              {enabledFeedIds.length > 0 ? enabledFeedIds.join(', ') : 'None'}
            </span>
          </div>
          {selection && (
            <div className={styles.summaryRow}>
              <span className={styles.summaryLabel}>Selected Contact:</span>
              <span className={styles.summaryValue}>
                {selection.feed}:{selection.id}
              </span>
            </div>
          )}
          <div className={styles.summaryRow}>
            <span className={styles.summaryLabel}>Visual Preset:</span>
            <span className={styles.summaryValue}>{visualPreset.toUpperCase()}</span>
          </div>
        </div>

        <div className={styles.toggles}>
          <label className={styles.checkboxLabel}>
            <input
              type="checkbox"
              checked={includeCamera}
              onChange={(e) => setIncludeCamera(e.target.checked)}
              className={styles.checkbox}
            />
            Include Camera Position & Heading
          </label>
          <label className={styles.checkboxLabel}>
            <input
              type="checkbox"
              checked={includeLayers}
              onChange={(e) => setIncludeLayers(e.target.checked)}
              className={styles.checkbox}
            />
            Include Active Layers & Basemap
          </label>
          <label className={styles.checkboxLabel}>
            <input
              type="checkbox"
              checked={includeSelection}
              onChange={(e) => setIncludeSelection(e.target.checked)}
              className={styles.checkbox}
            />
            Include Selected Contact
          </label>
          <label className={styles.checkboxLabel}>
            <input
              type="checkbox"
              checked={includeOverlays}
              onChange={(e) => setIncludeOverlays(e.target.checked)}
              className={styles.checkbox}
            />
            Include HUD & Optical Overlays
          </label>
        </div>
      </div>
    </Dialog>
  );
}
