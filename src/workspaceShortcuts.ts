export const assetListRowHeight = 48;

export function isAssetKeyboardTarget(
  target: EventTarget | null,
  activeElement: Element | null = document.activeElement,
) {
  const element = target instanceof HTMLElement ? target : null;
  const interactive = element?.closest<HTMLElement>(
    "button, a, input, select, textarea, [role='button'], [contenteditable='true']",
  );
  if (interactive && interactive.getAttribute("role") !== "option") return false;
  return Boolean(element?.closest("[data-asset-browser], [data-asset-card]")) || activeElement === document.body;
}
