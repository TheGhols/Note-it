import { FindStatus } from '../editor/find.ts';

export interface FindBarHandlers {
  onQuery(query: string, caseSensitive: boolean): void;
  onStep(step: number): void;
  onReplaceOne(replacement: string): void;
  onReplaceAll(replacement: string): void;
  onClose(): void;
}

export interface FindBarOptions {
  mount: HTMLElement;
  handlers: FindBarHandlers;
  document?: Document;
}

/**
 * Find, and optionally replace, inside the note you are looking at.
 *
 * One bar with a second row that appears when replacing is asked for, rather
 * than two panels: they search the same text with the same query, and a reader
 * who opened Find and then wanted Replace should not have to retype it.
 *
 * The bar owns its keys. Nothing typed here reaches the document, and closing
 * it hands the keyboard back to the editor.
 */
export class FindBar {
  private readonly doc: Document;
  private readonly root: HTMLElement;
  private readonly findInput: HTMLInputElement;
  private readonly replaceInput: HTMLInputElement;
  private readonly replaceRow: HTMLElement;
  private readonly counter: HTMLElement;
  private readonly caseButton: HTMLButtonElement;
  private readonly handlers: FindBarHandlers;

  private open = false;
  private replacing = false;
  private caseSensitive = false;

  public constructor(options: FindBarOptions) {
    this.doc = options.document ?? options.mount.ownerDocument;
    this.handlers = options.handlers;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-find';
    this.root.hidden = true;
    this.root.setAttribute('role', 'search');

    this.findInput = this.input('Buscar nesta nota…', 'Buscar nesta nota');
    this.counter = this.doc.createElement('span');
    this.counter.className = 'note-find-counter';

    this.caseButton = this.doc.createElement('button');
    this.caseButton.type = 'button';
    this.caseButton.className = 'note-find-button note-find-case';
    this.caseButton.textContent = 'Aa';
    this.caseButton.title = 'Diferenciar maiúsculas de minúsculas';
    this.caseButton.setAttribute('aria-pressed', 'false');

    const previous = this.button('↑', 'Ocorrência anterior (Shift+Enter)');
    const next = this.button('↓', 'Próxima ocorrência (Enter)');
    const close = this.button('✕', 'Fechar (Esc)');

    const findRow = this.doc.createElement('div');
    findRow.className = 'note-find-row';
    findRow.append(this.findInput, this.counter, this.caseButton, previous, next, close);

    this.replaceInput = this.input('Substituir por…', 'Substituir por');
    const replaceOne = this.button('Substituir', 'Substituir esta ocorrência');
    replaceOne.classList.add('note-find-wide');
    const replaceAll = this.button('Todas', 'Substituir todas as ocorrências');
    replaceAll.classList.add('note-find-wide');

    this.replaceRow = this.doc.createElement('div');
    this.replaceRow.className = 'note-find-row';
    this.replaceRow.hidden = true;
    this.replaceRow.append(this.replaceInput, replaceOne, replaceAll);

    this.root.append(findRow, this.replaceRow);
    options.mount.append(this.root);

    this.findInput.addEventListener('input', this.handleInput);
    this.findInput.addEventListener('keydown', this.handleKeyDown);
    this.replaceInput.addEventListener('keydown', this.handleKeyDown);
    previous.addEventListener('click', () => this.handlers.onStep(-1));
    next.addEventListener('click', () => this.handlers.onStep(1));
    close.addEventListener('click', () => this.close());
    replaceOne.addEventListener('click', () =>
      this.handlers.onReplaceOne(this.replaceInput.value),
    );
    replaceAll.addEventListener('click', () =>
      this.handlers.onReplaceAll(this.replaceInput.value),
    );
    this.caseButton.addEventListener('click', () => this.toggleCase());
    // A click on the bar must not move the caret in the note behind it.
    this.root.addEventListener('mousedown', (event) => {
      if (event.target !== this.findInput && event.target !== this.replaceInput) {
        event.preventDefault();
      }
    });
  }

  public isOpen(): boolean {
    return this.open;
  }

  public element(): HTMLElement {
    return this.root;
  }

  /**
   * Opens the bar, optionally with the replace row and a query to start from.
   *
   * `seed` is what the reader had selected. A short single-line selection is
   * almost always the thing they meant to look for; a paragraph is not, so a
   * long one is ignored rather than dumped into the field.
   */
  public openBar(options: { replace: boolean; seed?: string; focus?: boolean }): void {
    this.open = true;
    this.root.hidden = false;
    this.setReplacing(options.replace);

    const seed = (options.seed ?? '').trim();
    if (seed !== '' && seed.length <= 80 && !seed.includes('\n')) {
      this.findInput.value = seed;
    }
    // A reader who asked for Find wants to type; one who arrived here from a
    // global search wants to read the note they were brought to. The bar opens
    // either way, so the highlight always has a visible cause and an obvious
    // way out.
    if (options.focus !== false) {
      this.findInput.focus();
      this.findInput.select();
    }
    this.handlers.onQuery(this.findInput.value, this.caseSensitive);
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.handlers.onQuery('', this.caseSensitive);
    this.handlers.onClose();
  }

  public setStatus(status: FindStatus): void {
    if (status.query === '') {
      this.counter.textContent = '';
      this.root.removeAttribute('data-no-match');
      return;
    }
    if (status.total === 0) {
      this.counter.textContent = 'nenhuma';
      this.root.setAttribute('data-no-match', '');
      return;
    }
    this.counter.textContent = `${status.current} de ${status.total}`;
    this.root.removeAttribute('data-no-match');
  }

  public destroy(): void {
    this.root.remove();
  }

  private setReplacing(replacing: boolean): void {
    this.replacing = replacing;
    this.replaceRow.hidden = !replacing;
    this.root.setAttribute('aria-label', replacing ? 'Localizar e substituir' : 'Buscar na nota');
  }

  private toggleCase(): void {
    this.caseSensitive = !this.caseSensitive;
    this.caseButton.setAttribute('aria-pressed', String(this.caseSensitive));
    this.caseButton.classList.toggle('active', this.caseSensitive);
    this.handlers.onQuery(this.findInput.value, this.caseSensitive);
  }

  private input(placeholder: string, label: string): HTMLInputElement {
    const element = this.doc.createElement('input');
    element.type = 'text';
    element.className = 'note-find-input';
    element.placeholder = placeholder;
    element.setAttribute('aria-label', label);
    element.autocomplete = 'off';
    element.spellcheck = false;
    return element;
  }

  private button(text: string, title: string): HTMLButtonElement {
    const element = this.doc.createElement('button');
    element.type = 'button';
    element.className = 'note-find-button';
    element.textContent = text;
    element.title = title;
    element.setAttribute('aria-label', title);
    return element;
  }

  private readonly handleInput = (): void => {
    this.handlers.onQuery(this.findInput.value, this.caseSensitive);
  };

  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (event.isComposing) return;

    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.close();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      if (this.replacing && event.target === this.replaceInput) {
        this.handlers.onReplaceOne(this.replaceInput.value);
        return;
      }
      this.handlers.onStep(event.shiftKey ? -1 : 1);
    }
  };
}
