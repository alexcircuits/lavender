import type { MountGlobe } from '../globe';
import { AppShell } from '../shell';

export interface ShellProps {
  readonly mountGlobe: MountGlobe;
}

/** The workspace route (v1 has one): the shell around the globe. */
export function Shell({ mountGlobe }: ShellProps) {
  return <AppShell mountGlobe={mountGlobe} />;
}
