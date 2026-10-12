import { Radio } from '@base-ui/react/radio';
import { RadioGroup } from '@base-ui/react/radio-group';
import { cx } from '../cx';
import type { Icon } from '../icons';
import styles from './SegmentedControl.module.css';

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  icon?: Icon;
  disabled?: boolean;
}

export interface SegmentedControlProps<T extends string> {
  /** Accessible name of the group, e.g. "Globe mode". */
  'aria-label': string;
  options: ReadonlyArray<SegmentedOption<T>>;
  value?: T;
  defaultValue?: T;
  onValueChange?: (value: T) => void;
  size?: 'sm' | 'md';
  disabled?: boolean;
  className?: string | undefined;
}

/**
 * One-of-N choice: a pill track in `--well`; the selected segment is `--control` + `--ink`.
 * Radio semantics (Base UI RadioGroup): Tab enters the group, arrow keys move the selection.
 */
export function SegmentedControl<T extends string>({
  options,
  value,
  defaultValue,
  onValueChange,
  size = 'md',
  disabled,
  className,
  'aria-label': ariaLabel,
}: SegmentedControlProps<T>) {
  return (
    <RadioGroup<T>
      aria-label={ariaLabel}
      value={value}
      defaultValue={defaultValue}
      onValueChange={(next) => onValueChange?.(next)}
      disabled={disabled}
      data-size={size}
      className={cx(styles.track, className)}
    >
      {options.map(({ value: optionValue, label, icon: Glyph, disabled: optionDisabled }) => (
        <Radio.Root
          key={optionValue}
          value={optionValue}
          disabled={optionDisabled}
          className={styles.segment}
        >
          {Glyph ? <Glyph size={14} weight="bold" aria-hidden /> : null}
          <span>{label}</span>
        </Radio.Root>
      ))}
    </RadioGroup>
  );
}
