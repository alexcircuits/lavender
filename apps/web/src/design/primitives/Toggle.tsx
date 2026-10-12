import { Switch } from '@base-ui/react/switch';
import { cx } from '../cx';
import styles from './Toggle.module.css';

export interface ToggleProps
  extends Omit<Switch.Root.Props, 'className' | 'render' | 'nativeButton' | 'children'> {
  /** Name the switch by what it turns on ("Aircraft"); state is announced as on/off. */
  'aria-label'?: string;
  'aria-labelledby'?: string;
  className?: string | undefined;
}

/** On/off switch: 28 × 16 pill, on = `--accent`. Base UI provides role="switch" and keyboard. */
export function Toggle({ className, ...rest }: ToggleProps) {
  return (
    <Switch.Root {...rest} className={cx(styles.track, className)}>
      <Switch.Thumb className={styles.thumb} />
    </Switch.Root>
  );
}
