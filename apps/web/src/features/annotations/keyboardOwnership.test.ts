import { afterEach, describe, expect, it } from 'vitest';
import { visiblePopupOwnsKeys } from './keyboardOwnership';

const fixture = document.createElement('div');
afterEach(() => fixture.remove());
describe('annotation keyboard ownership', () => {
  it.each(['display:none', 'visibility:hidden', 'content-visibility:hidden', 'opacity:0'])(
    'ignores popup DOM hidden by an ancestor with %s',
    (style) => {
      fixture.innerHTML = `<div style="${style}"><div role="listbox"></div></div>`;
      document.body.append(fixture);
      expect(visiblePopupOwnsKeys(fixture)).toBe(false);
    },
  );
  it('keeps visible popup ownership and ignores inert or hidden responsive surfaces', () => {
    fixture.innerHTML =
      '<div inert><div role="menu"></div></div><div aria-hidden="true"><div role="dialog"></div></div>';
    document.body.append(fixture);
    expect(visiblePopupOwnsKeys(fixture)).toBe(false);
    fixture.insertAdjacentHTML('beforeend', '<div role="dialog"></div>');
    expect(visiblePopupOwnsKeys(fixture)).toBe(true);
  });
});
