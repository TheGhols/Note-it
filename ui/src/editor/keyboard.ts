export interface NoteKeyboardActions {
  newNote(): void;
  closeNote(): void;
  increaseFontSize(): void;
  decreaseFontSize(): void;
  toggleStrike(): void;
}

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

    switch (event.key.toLowerCase()) {
      case 'n':
        event.preventDefault();
        this.actions.newNote();
        break;
      case 'w':
        event.preventDefault();
        this.actions.closeNote();
        break;
      case 'r':
        event.preventDefault();
        this.actions.toggleStrike();
        break;
      case '+':
      case '=':
        event.preventDefault();
        this.actions.increaseFontSize();
        break;
      case '-':
      case '_':
        event.preventDefault();
        this.actions.decreaseFontSize();
        break;
    }
  };
}
