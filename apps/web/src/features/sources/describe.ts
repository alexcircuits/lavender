/**
 * Words for the Sources panel. Every licence and health value is written out (never a bare
 * enum), and states are icon + word, never colour alone.
 */
import type {
  CommercialUse,
  HealthState,
  LayerDescriptor,
  LicenceMode,
  SourceInfo,
} from '../../contracts';
import { lookup } from '../../lib/lookup';

export const COMMERCIAL_WORDS: Readonly<Record<CommercialUse, string>> = {
  allowed: 'Allowed',
  requiresAgreement: 'Needs an agreement',
  nonCommercialOnly: 'Non-commercial only',
  unknown: 'Unknown',
};

export type HealthTone = 'good' | 'warning' | 'alert' | 'neutral';

export const HEALTH: Readonly<Record<HealthState, { word: string; tone: HealthTone }>> = {
  healthy: { word: 'Healthy', tone: 'good' },
  idle: { word: 'Idle', tone: 'neutral' },
  degraded: { word: 'Degraded', tone: 'warning' },
  failing: { word: 'Failing', tone: 'alert' },
  disabled: { word: 'Off', tone: 'neutral' },
};

export const LICENCE_MODE_NOTICE: Readonly<Record<LicenceMode, { title: string; body: string }>> = {
  evaluation: {
    title: 'Evaluation mode',
    body: 'Every registered source is on, including sources that do not allow commercial use. Use this deployment to evaluate Lavender, not in a product or service you sell.',
  },
  commercial: {
    title: 'Commercial mode',
    body: 'Sources whose terms do not allow commercial use, or that need an agreement Lavender does not have, are turned off. Their layers say so, and each source below explains why.',
  },
};

// Server enums may be newer than this client; unknown values read as unknown.

export const UNKNOWN_HEALTH: { word: string; tone: HealthTone } = {
  word: 'Health unknown',
  tone: 'neutral',
};

export function healthSpec(state: string): { word: string; tone: HealthTone } {
  return lookup(HEALTH, state, UNKNOWN_HEALTH);
}

export function commercialWords(value: string): string {
  return lookup(COMMERCIAL_WORDS, value, COMMERCIAL_WORDS.unknown);
}

/** An unknown mode is explained as the stricter one would be, never as evaluation. */
export function licenceModeNotice(mode: string): { title: string; body: string } {
  return lookup(LICENCE_MODE_NOTICE, mode, LICENCE_MODE_NOTICE.commercial);
}

/** Why the licence policy turned off layers that use this source. */
export function licencePolicyReason(source: SourceInfo): string {
  switch (source.commercial) {
    case 'nonCommercialOnly':
      return 'Turned off by the licence policy: this source does not allow commercial use, and Lavender is in commercial mode.';
    case 'requiresAgreement':
      return 'Turned off by the licence policy: commercial use needs an agreement with the provider, and Lavender is in commercial mode.';
    case 'unknown':
      return 'Turned off by the licence policy: the terms for commercial use are unknown, and Lavender is in commercial mode.';
    default:
      return 'Turned off by the licence policy in commercial mode.';
  }
}

export interface SourceUse {
  /** Layers that draw from this source. */
  readonly layers: readonly LayerDescriptor[];
  /** Some of them are off because of the licence policy. */
  readonly licenceDisabled: boolean;
  /** Configuration the server is missing for this source's layers. */
  readonly missing: readonly string[];
}

export function sourceUse(source: SourceInfo, layers: readonly LayerDescriptor[]): SourceUse {
  const using = layers.filter((l) => l.sourceIds.includes(source.id));
  const missing = using.flatMap((l) =>
    l.availability.state === 'unconfigured' ? l.availability.missing : [],
  );
  return {
    layers: using,
    licenceDisabled: using.some((l) => l.availability.state === 'disabledByLicence'),
    missing: [...new Set(missing)],
  };
}
