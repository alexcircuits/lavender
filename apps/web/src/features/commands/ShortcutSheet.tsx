import { Button, Dialog, DialogClose, Kbd } from '../../design';
import { useUi } from '../../runtime';
import { COMMANDS, shortcutLabel } from './registry';
import styles from './ShortcutSheet.module.css';

const BOUND = COMMANDS.filter((c) => c.shortcut);

/** The `?` sheet: every shortcut in the registry, so the list can never drift from the keys. */
export function ShortcutSheet() {
  const open = useUi((s) => s.shortcutsOpen);
  const setOpen = useUi((s) => s.setShortcutsOpen);
  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title="Keyboard shortcuts"
      description="In lists, J and K or the arrow keys move between rows and Enter chooses."
      className={styles.sheet}
      // Initial focus lands on the first action. Without one it would land on the close glyph,
      // whose tooltip then takes the first Escape (Dialog primitive behaviour).
      actions={<DialogClose render={<Button variant="secondary">Done</Button>} />}
    >
      <dl className={styles.list}>
        {BOUND.map((command) => {
          const shortcut = command.shortcut;
          if (!shortcut) return null;
          return (
            <div key={command.id} className={styles.row}>
              <dt className={styles.title}>{command.title}</dt>
              <dd className={styles.keys}>
                <Kbd>{shortcutLabel(shortcut)}</Kbd>
              </dd>
            </div>
          );
        })}
      </dl>
    </Dialog>
  );
}
