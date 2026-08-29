import { SearchResult } from '../bridge/types.ts';

export interface SearchPaletteHandlers {
  /** Asks the host for results. The palette never reads the store itself. */
  onQuery(requestId: number, query: string): void;
  /** The reader chose a note. Addressed by identifier, never by label. */
  onOpen(noteId: string, query: string): void;
  /** The palette closed; the editor should have the keyboard back. */
  onClose(): void;
}

export interface SearchPaletteOptions {
  mount: HTMLElement;
  handlers: SearchPaletteHandlers;
  document?: Document;
}

/**
 * How long the palette waits before asking.
 *
 * Long enough that typing a word is one search rather than six, short enough
 * that it still feels like the list is following the keyboard.
 */
export const SEARCH_DEBOUNCE_MS = 120;

/**
 * Searching every note, from inside the note you are in.
 *
 * A panel in the page rather than a second window: a window would be a second
 * layer-shell surface to place, focus, stack and tear down, and everything it
 * would do is done here by an element that disappears when it is closed.
 *
 * It is not part of the document. The editor never sees these keystrokes, the
 * Markdown never hears about it, and closing it leaves the note exactly as it
 * was — searching is reading.
 */
export class SearchPalette {
  private readonly doc: Document;
  private readonly root: HTMLElement;
  private readonly input: HTMLInputElement;
  private readonly list: HTMLElement;
  private readonly status: HTMLElement;
  private readonly handlers: SearchPaletteHandlers;

  private results: SearchResult[] = [];
  private selected = 0;
  private open = false;

  /**
   * Which request the palette is waiting for.
   *
   * Every query carries a number, and the only answer allowed to change the
   * list is the answer to the question currently being asked. That covers both
   * ways an old reply can arrive: after the new one, and — the case a
   * "never go backwards" rule misses — *before* it, while the newer request is
   * still in flight. Once `biopsia` has been asked, the answer to `bio` is
   * stale whenever it turns up.
   */
  private lastRequestId = 0;
  private debounce: number | null = null;

  public constructor(options: SearchPaletteOptions) {
    this.doc = options.document ?? options.mount.ownerDocument;
    this.handlers = options.handlers;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-search';
    this.root.hidden = true;
    this.root.setAttribute('role', 'dialog');
    this.root.setAttribute('aria-label', 'Buscar em todas as notas');

    this.input = this.doc.createElement('input');
    this.input.type = 'text';
    this.input.className = 'note-search-input';
    this.input.placeholder = 'Buscar em todas as notas…';
    this.input.setAttribute('aria-label', 'Buscar em todas as notas');
    // A password manager or spell checker inside a search field is noise.
    this.input.autocomplete = 'off';
    this.input.spellcheck = false;

    this.status = this.doc.createElement('div');
    this.status.className = 'note-search-status';

    this.list = this.doc.createElement('ul');
    this.list.className = 'note-search-results';
    this.list.setAttribute('role', 'listbox');

    const header = this.doc.createElement('div');
    header.className = 'note-search-header';
    header.append(this.input, this.status);
    this.root.append(header, this.list);
    options.mount.append(this.root);

    this.input.addEventListener('input', this.handleInput);
    this.input.addEventListener('keydown', this.handleKeyDown);
    this.root.addEventListener('mousedown', this.handleMouseDown);
  }

  public isOpen(): boolean {
    return this.open;
  }

  public element(): HTMLElement {
    return this.root;
  }

  public openPalette(): void {
    if (this.open) {
      this.input.select();
      return;
    }
    this.open = true;
    this.root.hidden = false;
    this.input.value = '';
    this.results = [];
    this.selected = 0;
    this.renderResults();
    this.input.focus();
    // An empty box would be a dead end, so it opens as a list of the notes
    // most recently written in — the same control, used as a way to move.
    this.request('');
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.cancelPending();
    this.results = [];
    this.list.replaceChildren();
    this.handlers.onClose();
  }

  /**
   * Takes an answer from the host, if it is still the answer to the question
   * being asked.
   */
  public showResults(requestId: number, results: SearchResult[]): void {
    if (!this.open) return;
    if (requestId !== this.lastRequestId) return;
    this.results = results;
    this.selected = 0;
    this.renderResults();
  }

  /** The note behind a result is gone. Say so, and ask again. */
  public reportMissing(noteId: string): void {
    if (!this.open) return;
    this.results = this.results.filter((result) => result.noteId !== noteId);
    this.selected = 0;
    this.renderResults();
    this.status.textContent = 'nota não encontrada';
    this.request(this.input.value);
  }

  public destroy(): void {
    this.cancelPending();
    this.input.removeEventListener('input', this.handleInput);
    this.input.removeEventListener('keydown', this.handleKeyDown);
    this.root.removeEventListener('mousedown', this.handleMouseDown);
    this.root.remove();
  }

  private cancelPending(): void {
    if (this.debounce !== null) {
      this.doc.defaultView?.clearTimeout(this.debounce);
      this.debounce = null;
    }
  }

  private readonly handleInput = (): void => {
    this.cancelPending();
    const query = this.input.value;
    this.debounce = this.doc.defaultView?.setTimeout(() => {
      this.debounce = null;
      this.request(query);
    }, SEARCH_DEBOUNCE_MS) as unknown as number;
  };

  private request(query: string): void {
    this.lastRequestId += 1;
    this.handlers.onQuery(this.lastRequestId, query);
  }

  /**
   * Keys the palette owns.
   *
   * Everything handled here is stopped from reaching the editor: a note must
   * not gain the letters of a search. `Ctrl+Shift+Space` is deliberately not
   * among them — the layer belongs to the whole application and stays reachable
   * with the palette open.
   */
  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (event.isComposing) return;

    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.close();
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      event.stopPropagation();
      this.move(1);
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      event.stopPropagation();
      this.move(-1);
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      this.choose(this.selected);
    }
  };

  /** A click inside the palette must not move the caret in the note behind it. */
  private readonly handleMouseDown = (event: MouseEvent): void => {
    const target = event.target as HTMLElement | null;
    const row = target?.closest('[data-note-id]') as HTMLElement | null;
    if (!row) return;
    event.preventDefault();
    const index = this.results.findIndex((result) => result.noteId === row.dataset.noteId);
    if (index >= 0) this.choose(index);
  };

  private move(step: number): void {
    if (this.results.length === 0) return;
    const total = this.results.length;
    this.selected = ((this.selected + step) % total + total) % total;
    this.renderResults();
  }

  private choose(index: number): void {
    const result = this.results[index];
    if (!result) return;
    // What the note will be told to look for is the spelling that matched in
    // *that* note, not what was typed: `biopsia` found `Biópsia`, and the
    // editor's own find does not fold accents.
    const query = result.matchedText || this.input.value.trim();
    this.close();
    this.handlers.onOpen(result.noteId, query);
  }

  private renderResults(): void {
    this.list.replaceChildren();

    const query = this.input.value.trim();
    if (this.results.length === 0) {
      this.status.textContent = query === '' ? 'nenhuma nota' : 'nenhum resultado';
      return;
    }
    this.status.textContent =
      query === '' ? 'notas recentes' : `${this.results.length} nota(s)`;

    this.results.forEach((result, index) => {
      const row = this.doc.createElement('li');
      row.className = index === this.selected ? 'note-search-row selected' : 'note-search-row';
      row.setAttribute('role', 'option');
      row.setAttribute('aria-selected', String(index === this.selected));
      // The identifier is what the action uses. Two notes may open with the
      // same words and still be two notes.
      row.dataset.noteId = result.noteId;

      const label = this.doc.createElement('span');
      label.className = 'note-search-label';
      // `textContent`, always. A note is a text file a person controls, and a
      // snippet of one is text — never markup, and never anything that could
      // run.
      label.textContent = result.label;

      const snippet = this.doc.createElement('span');
      snippet.className = 'note-search-snippet';
      snippet.textContent = result.snippet;

      row.append(label, snippet);

      if (result.matchCount > 1) {
        const count = this.doc.createElement('span');
        count.className = 'note-search-count';
        count.textContent = `${result.matchCount}`;
        row.append(count);
      }

      this.list.append(row);
    });

    this.list.children[this.selected]?.scrollIntoView({ block: 'nearest' });
  }
}
