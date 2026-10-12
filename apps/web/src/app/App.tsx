import { lazy, Suspense } from 'react';
import { DesignProvider } from '../design';
import type { MountGlobe } from '../globe';
import { type Runtime, RuntimeProvider } from '../runtime';
import { Shell } from './Shell';
import { useHash } from './useHash';

export const DESIGN_REVIEW_HASH = '#design-review';

// Dev-only: the constant condition is folded away in production builds, so neither the review
// surface nor its chunk ships.
const DesignReview = import.meta.env.DEV
  ? lazy(() => import('../design/review/DesignReview'))
  : null;

export interface AppProps {
  readonly runtime: Runtime;
  readonly mountGlobe: MountGlobe;
}

export function App({ runtime, mountGlobe }: AppProps) {
  const hash = useHash();
  return (
    <RuntimeProvider runtime={runtime}>
      <DesignProvider>
        {DesignReview && hash === DESIGN_REVIEW_HASH ? (
          <Suspense fallback={null}>
            <DesignReview />
          </Suspense>
        ) : (
          <Shell mountGlobe={mountGlobe} />
        )}
      </DesignProvider>
    </RuntimeProvider>
  );
}
