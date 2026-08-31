/** Wires the central pill and its compact fallback to one existing search. */
export function bindSearchEntries(
  triggers: readonly (HTMLElement | null)[],
  openSearch: () => void,
): () => void {
  const bindings: Array<[HTMLElement, EventListener, EventListener]> = [];
  for (const trigger of triggers) {
    if (!trigger) continue;
    const pointer: EventListener = (event) => event.stopPropagation();
    const click: EventListener = (event) => {
      event.preventDefault();
      event.stopPropagation();
      openSearch();
    };
    trigger.addEventListener('pointerdown', pointer);
    trigger.addEventListener('click', click);
    bindings.push([trigger, pointer, click]);
  }
  return () => {
    for (const [trigger, pointer, click] of bindings) {
      trigger.removeEventListener('pointerdown', pointer);
      trigger.removeEventListener('click', click);
    }
  };
}
