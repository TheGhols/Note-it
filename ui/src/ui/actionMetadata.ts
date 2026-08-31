export interface ActionShortcut {
  readonly display: string;
  readonly aria: string;
}

export interface ActionMetadata {
  readonly label: string;
  readonly shortcut?: ActionShortcut;
}

/**
 * The page-owned shortcuts a header action may advertise.
 *
 * This is deliberately small: a tooltip only names a chord when the same
 * WebView keyboard controller really handles that action. Composer-level
 * shortcuts and formatting chords that do something different are not
 * borrowed to make a button look more capable than it is.
 */
export const HEADER_ACTIONS = {
  search: {
    label: 'Buscar notas',
    shortcut: { display: 'Ctrl+K', aria: 'Control+K' },
  },
  zoomOut: {
    label: 'Diminuir zoom',
    shortcut: { display: 'Ctrl+-', aria: 'Control+-' },
  },
  zoomIn: {
    label: 'Aumentar zoom',
    shortcut: { display: 'Ctrl+=', aria: 'Control+=' },
  },
  close: {
    label: 'Fechar nota',
    shortcut: { display: 'Ctrl+W', aria: 'Control+W' },
  },
  collapse: {
    label: 'Recolher nota',
    shortcut: { display: 'Ctrl+Shift+M', aria: 'Control+Shift+M' },
  },
  resetZoom: {
    label: 'Restaurar zoom',
    shortcut: { display: 'Ctrl+0', aria: 'Control+0' },
  },
  layer: {
    label: 'Alternar camada',
    shortcut: { display: 'Ctrl+Shift+Space', aria: 'Control+Shift+Space' },
  },
} as const satisfies Record<string, ActionMetadata>;

export function actionTitle(action: ActionMetadata): string {
  return action.shortcut ? `${action.label} · ${action.shortcut.display}` : action.label;
}

export function applyActionMetadata(
  element: HTMLElement | null,
  action: ActionMetadata,
): void {
  if (!element) return;
  element.title = actionTitle(action);
  element.setAttribute('aria-label', action.label);
  if (action.shortcut) element.setAttribute('aria-keyshortcuts', action.shortcut.aria);
  else element.removeAttribute('aria-keyshortcuts');
}

export function applyHeaderActionMetadata(doc: Document): void {
  applyActionMetadata(doc.getElementById('btn-search'), HEADER_ACTIONS.search);
  applyActionMetadata(doc.getElementById('btn-search-pill'), HEADER_ACTIONS.search);
  applyActionMetadata(doc.getElementById('btn-zoom-out'), HEADER_ACTIONS.zoomOut);
  applyActionMetadata(doc.getElementById('btn-zoom-in'), HEADER_ACTIONS.zoomIn);
  applyActionMetadata(doc.getElementById('btn-close'), HEADER_ACTIONS.close);
}
