import { type KeyboardEvent, type PointerEvent, useRef, useState } from 'react';
import type { ResultGroup } from './results';

/** Default ranking follows arriving results; arrow/pointer choices claim explicit intent. */
export function useResultSelection(query: string, groups: readonly ResultGroup[]) {
  const q = query.trim();
  const items = groups.flatMap((group) => group.items.filter((item) => !item.disabled));
  const [intent, setIntent] = useState<{ readonly query: string; readonly value: string | null }>({
    query: q,
    value: null,
  });
  const keyboardNavigation = useRef(false);
  if (
    intent.query !== q ||
    (intent.value !== null && !items.some((item) => item.id === intent.value))
  )
    setIntent({ query: q, value: null });
  const explicit =
    intent.query === q && items.some((item) => item.id === intent.value) ? intent.value : null;
  const value = explicit ?? items[0]?.id ?? '';

  return {
    value,
    onValueChange(next: string) {
      if (keyboardNavigation.current) setIntent({ query: q, value: next });
    },
    onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
      if (event.defaultPrevented || event.nativeEvent.isComposing || event.keyCode === 229) return;
      if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
        // cmdk calls onValueChange synchronously for this key. Its automatic registrations
        // and re-ranking later in layout effects must not masquerade as a user choice.
        if (value) setIntent({ query: q, value });
        keyboardNavigation.current = true;
        queueMicrotask(() => {
          keyboardNavigation.current = false;
        });
      }
      if (event.key === 'Enter') {
        const item = items.find((candidate) => candidate.id === value);
        if (item) {
          event.preventDefault();
          // React already committed this model. Avoid cmdk's passive native select listener
          // race when a newly visible item has just mounted.
          item.run();
        }
      }
    },
    onPointerMoveCapture(event: PointerEvent<HTMLDivElement>) {
      const target =
        event.target instanceof Element ? event.target.closest('[role="option"]') : null;
      const id = target?.getAttribute('data-value');
      if (id && items.some((item) => item.id === id)) setIntent({ query: q, value: id });
    },
  };
}
