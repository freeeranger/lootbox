const editableSelector = 'input, textarea, [contenteditable="true"]';
const appContextMenuSelector = '[data-slot="context-menu-trigger"]';
const appDragSelector = '[data-asset-card], [data-native-drag="true"]';

function closest(target: EventTarget | null, selector: string) {
  return target instanceof Element ? target.closest(selector) : null;
}

export function installNativeShellBehavior() {
  const preventBrowserContextMenu = (event: MouseEvent) => {
    if (
      closest(event.target, editableSelector) ||
      closest(event.target, appContextMenuSelector)
    ) {
      return;
    }
    event.preventDefault();
  };

  const preventBrowserDrag = (event: DragEvent) => {
    if (closest(event.target, appDragSelector)) return;
    event.preventDefault();
  };

  const preventBrowserShortcut = (event: KeyboardEvent) => {
    const modifier = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    if (
      event.key === "F5" ||
      (modifier && ["r", "p", "u", "l", "0", "+", "-", "="].includes(key))
    ) {
      event.preventDefault();
    }
  };

  const preventPageZoom = (event: WheelEvent) => {
    if (event.ctrlKey || event.metaKey) event.preventDefault();
  };

  document.addEventListener("contextmenu", preventBrowserContextMenu);
  document.addEventListener("dragstart", preventBrowserDrag);
  window.addEventListener("keydown", preventBrowserShortcut, { capture: true });
  window.addEventListener("wheel", preventPageZoom, { passive: false });

  return () => {
    document.removeEventListener("contextmenu", preventBrowserContextMenu);
    document.removeEventListener("dragstart", preventBrowserDrag);
    window.removeEventListener("keydown", preventBrowserShortcut, { capture: true });
    window.removeEventListener("wheel", preventPageZoom);
  };
}
