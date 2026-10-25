import { useCallback, useState } from 'react';
import {
  Button,
  cx,
  EmptyState,
  IconButton,
  Panel,
  SegmentedControl,
  WarningCircleIcon,
  XIcon,
} from '../../design';
import { formatUtcTime } from '../../lib/time';
import { useGlobe, useUi, useWorkspace } from '../../runtime';
import { annotationCentre } from '../annotations/geometry';
import styles from './alerts.module.css';
import { type AlertItem, useAlertsStore } from './alertsStore';
import { SavedSearchesTab } from './SavedSearchesTab';

export interface AlertsPanelProps {
  readonly className?: string | undefined;
}

const ALERT_TABS = [
  { value: 'inbox', label: 'Inbox' },
  { value: 'searches', label: 'Saved Searches' },
] as const;

type AlertTabValue = (typeof ALERT_TABS)[number]['value'];

/**
 * Alert Inbox panel (LB-140):
 * Displays recent and live geofence containment alerts (enter/exit events),
 * allows focusing entities or geofences on the globe, dismissing alerts,
 * clearing the inbox, and managing persistent saved search monitors.
 */
export function AlertsPanel({ className }: AlertsPanelProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const annotations = useWorkspace((s) => s.annotations);
  const close = useUi((s) => s.setSidePanel);
  const [activeTab, setActiveTab] = useState<AlertTabValue>('inbox');

  const alerts = useAlertsStore((s) => s.alerts);
  const unreadCount = useAlertsStore((s) => s.unreadCount);
  const dismissAlert = useAlertsStore((s) => s.dismissAlert);
  const clearAll = useAlertsStore((s) => s.clearAll);

  const handleFocusEntity = useCallback(
    (alert: AlertItem) => {
      select({ feed: alert.entity.feed, id: alert.entity.id });
      globe?.flyTo({ kind: 'point', position: alert.position });
    },
    [globe, select],
  );

  const handleFocusGeofence = useCallback(
    (alert: AlertItem) => {
      const fence = annotations.find((a) => a.id === alert.geofenceId);
      const centre = fence ? annotationCentre(fence.points) : alert.position;
      if (centre) {
        globe?.flyTo({ kind: 'point', position: centre });
      }
    },
    [globe, annotations],
  );

  return (
    <Panel
      title="Alert Inbox"
      dense
      className={cx(styles.panel, className)}
      actions={
        <IconButton icon={XIcon} label="Close alerts" onClick={() => close(null)} variant="ghost" />
      }
    >
      <SegmentedControl
        aria-label="Alert view"
        size="sm"
        options={ALERT_TABS}
        value={activeTab}
        onValueChange={setActiveTab}
      />

      <div hidden={activeTab !== 'inbox'} className={styles.tabSection}>
        <div className={styles.toolbar}>
          <span className={styles.count}>
            {alerts.length} {alerts.length === 1 ? 'alert' : 'alerts'} ({unreadCount} unread)
          </span>
          <Button size="sm" variant="secondary" disabled={alerts.length === 0} onClick={clearAll}>
            Clear All
          </Button>
        </div>

        {alerts.length === 0 ? (
          <EmptyState icon={WarningCircleIcon} title="No alerts">
            No geofence alert events recorded.
          </EmptyState>
        ) : (
          <ol className={styles.list} aria-label="Alerts">
            {alerts.map((alert) => (
              <li key={alert.id} className={styles.item} data-dismissed={alert.dismissed}>
                <div className={styles.itemHeader}>
                  <div className={styles.badges}>
                    <span className={alert.type === 'enter' ? styles.kindEnter : styles.kindExit}>
                      {alert.type === 'enter' ? 'ENTER' : 'EXIT'}
                    </span>
                    <span className={styles.entityBadge}>{alert.entity.feed}</span>
                    <span className={styles.geofenceBadge}>{alert.geofenceName}</span>
                  </div>
                  <time
                    className={styles.timestamp}
                    dateTime={new Date(alert.timestamp).toISOString()}
                  >
                    {formatUtcTime(alert.timestamp)}
                  </time>
                </div>

                <div className={styles.itemBody}>
                  <h3 className={styles.entityLabel}>{alert.entity.label || alert.entity.id}</h3>
                  <p className={styles.details}>
                    {alert.position.lat.toFixed(4)}°, {alert.position.lon.toFixed(4)}°
                  </p>
                </div>

                <div className={styles.actions}>
                  <Button size="sm" variant="secondary" onClick={() => handleFocusEntity(alert)}>
                    Focus Entity
                  </Button>
                  <Button size="sm" variant="secondary" onClick={() => handleFocusGeofence(alert)}>
                    Focus Geofence
                  </Button>
                  {!alert.dismissed ? (
                    <Button size="sm" variant="ghost" onClick={() => dismissAlert(alert.id)}>
                      Dismiss
                    </Button>
                  ) : (
                    <Button size="sm" variant="ghost" disabled>
                      Dismissed
                    </Button>
                  )}
                </div>
              </li>
            ))}
          </ol>
        )}
      </div>

      <div hidden={activeTab !== 'searches'}>
        <SavedSearchesTab />
      </div>
    </Panel>
  );
}
