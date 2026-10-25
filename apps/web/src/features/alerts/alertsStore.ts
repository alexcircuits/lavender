import { create } from 'zustand';
import type { GeoPoint } from '../../contracts';
import type { EntitySource } from '../../globe';

export type AlertType = 'enter' | 'exit';

export interface AlertEntity {
  readonly feed: EntitySource;
  readonly id: string;
  readonly label?: string;
}

export interface AlertItem {
  readonly id: string;
  readonly timestamp: number;
  readonly type: AlertType;
  readonly entity: AlertEntity;
  readonly geofenceId: string;
  readonly geofenceName: string;
  readonly position: GeoPoint;
  readonly dismissed: boolean;
}

export interface AlertInput {
  readonly id?: string;
  readonly timestamp?: number;
  readonly type: AlertType;
  readonly entity: AlertEntity;
  readonly geofenceId: string;
  readonly geofenceName: string;
  readonly position: GeoPoint;
  readonly dismissed?: boolean;
}

export const MAX_ALERTS = 100;

let alertCounter = 0;
function generateAlertId(): string {
  alertCounter += 1;
  return `alert-${Date.now()}-${alertCounter}`;
}

export interface AlertsState {
  readonly alerts: readonly AlertItem[];
  readonly unreadCount: number;
  addAlert(alert: AlertInput): void;
  dismissAlert(id: string): void;
  clearAll(): void;
  setUnreadCount(count: number): void;
}

export const useAlertsStore = create<AlertsState>((set) => ({
  alerts: [],
  unreadCount: 0,

  addAlert: (alertInput) =>
    set((state) => {
      const id = alertInput.id ?? generateAlertId();
      const dismissed = alertInput.dismissed ?? false;
      const timestamp = alertInput.timestamp ?? Date.now();
      const item: AlertItem = {
        ...alertInput,
        id,
        timestamp,
        dismissed,
      };

      const nextAlerts = [item, ...state.alerts.filter((a) => a.id !== id)].slice(0, MAX_ALERTS);
      const unreadCount = nextAlerts.filter((a) => !a.dismissed).length;
      return { alerts: nextAlerts, unreadCount };
    }),

  dismissAlert: (id) =>
    set((state) => {
      const nextAlerts = state.alerts.map((a) => (a.id === id ? { ...a, dismissed: true } : a));
      const unreadCount = nextAlerts.filter((a) => !a.dismissed).length;
      return { alerts: nextAlerts, unreadCount };
    }),

  clearAll: () =>
    set({
      alerts: [],
      unreadCount: 0,
    }),

  setUnreadCount: (count) =>
    set({
      unreadCount: Math.max(0, count),
    }),
}));
