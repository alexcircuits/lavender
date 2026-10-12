import { useStore } from 'zustand';
import { CheckCircleIcon, InfoIcon, WarningCircleIcon, XIcon } from '../icons';
import { Button } from './Button';
import { IconButton } from './IconButton';
import styles from './Toast.module.css';
import { dismissToast, type Toast, toastStore, visibleToasts } from './toast-store';

const GLYPHS = {
  info: InfoIcon,
  success: CheckCircleIcon,
  error: WarningCircleIcon,
} as const;

/**
 * Renders the toast store: bottom centre above the status bar, at most two visible. Info and
 * success are polite status messages; errors are alerts and stay until acted on or dismissed.
 */
export function Toaster() {
  // Select the stable array, then slice: a selector returning a new array each call would
  // make useSyncExternalStore re-render forever.
  const toasts = visibleToasts(useStore(toastStore));
  return (
    <section className={styles.viewport} aria-label="Notifications">
      {/* Live regions exist before content arrives so additions are announced. */}
      <div role="status" aria-live="polite" className={styles.stack}>
        {toasts
          .filter((toast) => toast.kind !== 'error')
          .map((toast) => (
            <ToastCard key={toast.id} toast={toast} />
          ))}
      </div>
      <div role="alert" aria-live="assertive" className={styles.stack}>
        {toasts
          .filter((toast) => toast.kind === 'error')
          .map((toast) => (
            <ToastCard key={toast.id} toast={toast} />
          ))}
      </div>
    </section>
  );
}

function ToastCard({ toast }: { toast: Toast }) {
  const Glyph = GLYPHS[toast.kind];
  return (
    <div className={styles.toast} data-kind={toast.kind}>
      <Glyph size={16} weight="fill" aria-hidden className={styles.glyph} />
      <div className={styles.text}>
        <p className={styles.title}>{toast.title}</p>
        {toast.description ? <p className={styles.description}>{toast.description}</p> : null}
      </div>
      {toast.action ? (
        <Button
          size="sm"
          variant="secondary"
          onClick={() => {
            toast.action?.onAction();
            dismissToast(toast.id);
          }}
        >
          {toast.action.label}
        </Button>
      ) : null}
      <IconButton label="Dismiss" icon={XIcon} size="sm" onClick={() => dismissToast(toast.id)} />
    </div>
  );
}
