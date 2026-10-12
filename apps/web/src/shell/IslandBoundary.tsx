import { Component, type ErrorInfo, type ReactNode } from 'react';
import { Button, ErrorState, Panel } from '../design';

export interface IslandBoundaryProps {
  /** The island's name, for the fallback's title ("Layers", "Inspector"). */
  readonly title: string;
  /** A change (another panel, another selection) clears a previous failure. */
  readonly resetKey: string;
  readonly className?: string | undefined;
  readonly children: ReactNode;
}

interface IslandBoundaryState {
  readonly failedKey: string | null;
}

/**
 * Contains a rendering failure to one island: the globe and the rest of the chrome keep
 * working, and the island says what happened and offers to try again (product spec §6:
 * "the app never blanks").
 */
export class IslandBoundary extends Component<IslandBoundaryProps, IslandBoundaryState> {
  override state: IslandBoundaryState = { failedKey: null };

  static getDerivedStateFromError(): Partial<IslandBoundaryState> {
    return { failedKey: '' };
  }

  override componentDidCatch(error: unknown, info: ErrorInfo): void {
    console.error(`${this.props.title} island failed`, error, info.componentStack);
    this.setState({ failedKey: this.props.resetKey });
  }

  override render(): ReactNode {
    const { failedKey } = this.state;
    const failed = failedKey !== null && (failedKey === '' || failedKey === this.props.resetKey);
    if (!failed) return this.props.children;
    return (
      <Panel title={this.props.title} dense className={this.props.className}>
        <ErrorState
          live
          title={`${this.props.title} stopped working`}
          action={
            <Button variant="secondary" onClick={() => this.setState({ failedKey: null })}>
              Try again
            </Button>
          }
        >
          Something in this panel failed to display. The globe and other panels still work.
        </ErrorState>
      </Panel>
    );
  }
}
