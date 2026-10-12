import { Menu as BaseMenu } from '@base-ui/react/menu';
import type { ReactNode } from 'react';
import { cx } from '../cx';
import { CheckIcon, type Icon } from '../icons';
import { Kbd } from './Kbd';
import styles from './Menu.module.css';
import { usePortalProps } from './portal';

/**
 * Menus (Base UI): `--popover` surface with `--float-edge` + `--float-shadow`, `--r-lg`; items
 * 32 px, `--r-sm`; arrow keys, Home/End and type-ahead; a checkmark marks the selected item.
 * Focus returns to the trigger on close.
 *
 *   <Menu>
 *     <MenuTrigger render={<Button iconEnd={CaretDownIcon}>Imagery</Button>} />
 *     <MenuContent>
 *       <MenuRadioGroup value={v} onValueChange={setV}>
 *         <MenuRadioItem value="satellite">Satellite</MenuRadioItem>
 *       </MenuRadioGroup>
 *     </MenuContent>
 *   </Menu>
 */
export const Menu = BaseMenu.Root;
export const MenuTrigger = BaseMenu.Trigger;

export interface MenuContentProps {
  children: ReactNode;
  side?: 'top' | 'bottom' | 'left' | 'right';
  align?: 'start' | 'center' | 'end';
  className?: string | undefined;
}

export function MenuContent({
  children,
  side = 'bottom',
  align = 'start',
  className,
}: MenuContentProps) {
  const portal = usePortalProps();
  return (
    <BaseMenu.Portal {...portal}>
      <BaseMenu.Positioner
        side={side}
        align={align}
        sideOffset={8}
        collisionPadding={8}
        className={styles.positioner}
      >
        <BaseMenu.Popup className={cx(styles.popup, className)}>{children}</BaseMenu.Popup>
      </BaseMenu.Positioner>
    </BaseMenu.Portal>
  );
}

interface ItemDecor {
  icon?: Icon;
  /** Key hint, e.g. "⌘C". Shown, not bound: shortcuts live in the command registry. */
  shortcut?: string;
}

export interface MenuItemProps extends ItemDecor, Omit<BaseMenu.Item.Props, 'className'> {
  /** Destructive action: label in --alert. */
  danger?: boolean;
  className?: string | undefined;
}

export function MenuItem({
  icon: Glyph,
  shortcut,
  danger,
  children,
  className,
  ...rest
}: MenuItemProps) {
  return (
    <BaseMenu.Item
      {...rest}
      data-danger={danger ? '' : undefined}
      className={cx(styles.item, className)}
    >
      <span className={styles.leading} aria-hidden>
        {Glyph ? <Glyph size={16} weight="bold" /> : null}
      </span>
      <span className={styles.text}>{children}</span>
      {shortcut ? <Kbd className={styles.shortcut}>{shortcut}</Kbd> : null}
    </BaseMenu.Item>
  );
}

export interface MenuCheckboxItemProps
  extends Pick<ItemDecor, 'shortcut'>,
    Omit<BaseMenu.CheckboxItem.Props, 'className'> {
  className?: string | undefined;
}

export function MenuCheckboxItem({
  shortcut,
  children,
  className,
  ...rest
}: MenuCheckboxItemProps) {
  return (
    <BaseMenu.CheckboxItem {...rest} className={cx(styles.item, className)}>
      <span className={styles.leading} aria-hidden>
        <BaseMenu.CheckboxItemIndicator className={styles.indicator}>
          <CheckIcon size={16} weight="bold" />
        </BaseMenu.CheckboxItemIndicator>
      </span>
      <span className={styles.text}>{children}</span>
      {shortcut ? <Kbd className={styles.shortcut}>{shortcut}</Kbd> : null}
    </BaseMenu.CheckboxItem>
  );
}

export const MenuRadioGroup = BaseMenu.RadioGroup;

export interface MenuRadioItemProps
  extends Pick<ItemDecor, 'shortcut'>,
    Omit<BaseMenu.RadioItem.Props, 'className'> {
  /** Secondary text after the label, e.g. the imagery provider. */
  detail?: string;
  className?: string | undefined;
}

export function MenuRadioItem({
  shortcut,
  detail,
  children,
  className,
  ...rest
}: MenuRadioItemProps) {
  return (
    <BaseMenu.RadioItem {...rest} className={cx(styles.item, className)}>
      <span className={styles.leading} aria-hidden>
        <BaseMenu.RadioItemIndicator className={styles.indicator}>
          <CheckIcon size={16} weight="bold" />
        </BaseMenu.RadioItemIndicator>
      </span>
      <span className={styles.text}>{children}</span>
      {detail ? <span className={styles.detail}>{detail}</span> : null}
      {shortcut ? <Kbd className={styles.shortcut}>{shortcut}</Kbd> : null}
    </BaseMenu.RadioItem>
  );
}

export function MenuSeparator() {
  return <BaseMenu.Separator className={styles.separator} />;
}

export const MenuGroup = BaseMenu.Group;

export interface MenuGroupLabelProps {
  children: ReactNode;
}

export function MenuGroupLabel({ children }: MenuGroupLabelProps) {
  return <BaseMenu.GroupLabel className={styles.groupLabel}>{children}</BaseMenu.GroupLabel>;
}
