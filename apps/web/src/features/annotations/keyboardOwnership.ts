/** Hidden popup DOM can remain mounted during animation or in another responsive surface. */
export function visiblePopupOwnsKeys(root: ParentNode = document): boolean {
  const popups = root.querySelectorAll<HTMLElement>(
    '[role="menu"], [role="listbox"], [role="dialog"], [role="alertdialog"]',
  );
  return Array.from(popups).some((popup) => {
    if (popup.closest('[hidden], [inert], [aria-hidden="true"]')) return false;
    let element: HTMLElement | null = popup;
    while (element) {
      const style = getComputedStyle(element);
      if (
        style.display === 'none' ||
        style.visibility === 'hidden' ||
        style.visibility === 'collapse' ||
        style.contentVisibility === 'hidden' ||
        style.opacity === '0'
      )
        return false;
      element = element.parentElement;
    }
    return true;
  });
}
