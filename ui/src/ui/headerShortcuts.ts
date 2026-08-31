export interface HeaderShortcutButtons {
  study: HTMLElement | null;
  zoomOut: HTMLElement | null;
  zoomIn: HTMLElement | null;
  trash: HTMLElement | null;
}

export interface HeaderShortcutActions {
  openStudyHub(invoker: HTMLElement): void;
  zoomOut(): void;
  zoomIn(): void;
  openTrashConfirmation(invoker: HTMLElement): void;
}

export function updateZoomShortcutState(
  zoomOut: HTMLButtonElement | null,
  zoomIn: HTMLButtonElement | null,
  current: number,
  minimum: number,
  maximum: number,
): void {
  if (zoomOut) zoomOut.disabled = current <= minimum;
  if (zoomIn) zoomIn.disabled = current >= maximum;
}

/**
 * Wires the four Phase 3.14 shortcuts to existing application actions.
 *
 * This module owns no study, zoom, or trash behavior. In particular, the
 * trash shortcut can only open the established confirmation; it has no
 * callback capable of moving a note itself.
 */
export function bindHeaderShortcuts(
  buttons: HeaderShortcutButtons,
  actions: HeaderShortcutActions,
): () => void {
  const bindings: Array<[HTMLElement, EventListener]> = [];
  const bind = (button: HTMLElement | null, action: (button: HTMLElement) => void): void => {
    if (!button) return;
    const stopPointer: EventListener = (event) => event.stopPropagation();
    const click: EventListener = (event) => {
      event.preventDefault();
      event.stopPropagation();
      action(button);
    };
    button.addEventListener('pointerdown', stopPointer);
    button.addEventListener('click', click);
    bindings.push([button, stopPointer], [button, click]);
  };

  bind(buttons.study, actions.openStudyHub);
  bind(buttons.zoomOut, actions.zoomOut);
  bind(buttons.zoomIn, actions.zoomIn);
  bind(buttons.trash, actions.openTrashConfirmation);

  return () => {
    for (let index = 0; index < bindings.length; index += 2) {
      const [button, pointer] = bindings[index];
      const [, click] = bindings[index + 1];
      button.removeEventListener('pointerdown', pointer);
      button.removeEventListener('click', click);
    }
  };
}
