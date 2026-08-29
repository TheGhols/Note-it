export interface NoteKeyboardActions {
  newNote(): void;
  closeNote(): void;
  toggleStrike(): void;
  zoomIn(): void;
  zoomOut(): void;
  resetZoom(): void;
  toggleCollapsed(): void;
  toggleLayerMode(): void;
  increaseTextSize(): void;
  decreaseTextSize(): void;
  openGlobalSearch(): void;
  openFind(): void;
  openReplace(): void;
}

/**
 * The single keyboard entry point for the note WebView.
 *
 * Every shortcut is registered here rather than in scattered listeners, so
 * conflicts are visible in one place. Composition is always allowed through
 * untouched: pt-BR dead keys and AltGr must reach the editor intact, so a
 * shortcut never runs while a composition is active and `Ctrl+Alt` — how AltGr
 * is reported — is never treated as a plain `Ctrl` chord.
 */
export class NoteKeyboardController {
  private compositionInProgress = false;

  public constructor(
    private readonly target: Window,
    private readonly actions: NoteKeyboardActions,
  ) {
    target.addEventListener('compositionstart', this.handleCompositionStart);
    target.addEventListener('compositionend', this.handleCompositionEnd);
    target.addEventListener('keydown', this.handleKeyDown);
  }

  public destroy(): void {
    this.target.removeEventListener('compositionstart', this.handleCompositionStart);
    this.target.removeEventListener('compositionend', this.handleCompositionEnd);
    this.target.removeEventListener('keydown', this.handleKeyDown);
  }

  private readonly handleCompositionStart = (): void => {
    this.compositionInProgress = true;
  };

  private readonly handleCompositionEnd = (): void => {
    this.compositionInProgress = false;
  };

  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (
      event.isComposing ||
      this.compositionInProgress ||
      event.altKey ||
      (!event.ctrlKey && !event.metaKey)
    ) {
      return;
    }

    const key = event.key;

    if (event.shiftKey) {
      const handled = this.handleShiftChord(key, event.code, event.repeat);
      if (handled) event.preventDefault();
      return;
    }

    switch (key.toLowerCase()) {
      case 'n':
        event.preventDefault();
        if (!event.repeat) this.actions.newNote();
        break;
      case 'w':
        event.preventDefault();
        if (!event.repeat) this.actions.closeNote();
        break;
      case 'r':
        event.preventDefault();
        if (!event.repeat) this.actions.toggleStrike();
        break;
      case '+':
      case '=':
        event.preventDefault();
        this.actions.zoomIn();
        break;
      case '-':
      case '_':
        event.preventDefault();
        this.actions.zoomOut();
        break;
      case '0':
        event.preventDefault();
        if (!event.repeat) this.actions.resetZoom();
        break;
      // `Ctrl+K`, `Ctrl+F` and `Ctrl+H` were all free: nothing in Note-it
      // claimed them before Phase 3.8, and none of them collides with the
      // chords above.
      case 'k':
        event.preventDefault();
        if (!event.repeat) this.actions.openGlobalSearch();
        break;
      case 'f':
        event.preventDefault();
        if (!event.repeat) this.actions.openFind();
        break;
      case 'h':
        event.preventDefault();
        if (!event.repeat) this.actions.openReplace();
        break;
    }
  };

  /**
   * `Ctrl+Shift` chords. The physical keys for `<` and `>` are `,` and `.`, and
   * which of the two a layout reports depends on the layout itself, so both the
   * produced character and the physical `code` are accepted.
   */
  private handleShiftChord(key: string, code: string, repeat: boolean): boolean {
    if (key === 'M' || key === 'm' || code === 'KeyM') {
      if (!repeat) this.actions.toggleCollapsed();
      return true;
    }
    if (key === ' ' || code === 'Space') {
      if (!repeat) this.actions.toggleLayerMode();
      return true;
    }
    if (key === '>' || key === '.' || code === 'Period') {
      if (!repeat) this.actions.increaseTextSize();
      return true;
    }
    if (key === '<' || key === ',' || code === 'Comma') {
      if (!repeat) this.actions.decreaseTextSize();
      return true;
    }
    return false;
  }
}
