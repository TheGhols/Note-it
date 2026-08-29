import { TrashEntry } from '../bridge/types.ts';
import { formatNoteTimestamp } from '../format/datetime.ts';

export interface TrashPanelHandlers {
  /** Asks the host for the contents of the trash. The panel never reads disk. */
  onList(requestId: number): void;
  /** The reader chose a note to bring back. Addressed by identifier, never by label. */
  onRestore(noteId: string): void;
  /** The panel closed; the editor should have the keyboard back. */
  onClose(): void;
}

export interface TrashPanelOptions {
  mount: HTMLElement;
  handlers: TrashPanelHandlers;
  document?: Document;
}

/**
 * The trash, as a panel in the note you are already in.
 *
 * The same shape the search palette has, and for the same reason: a second
 * layer-shell window would have to be placed, focused, stacked and torn down,
 * and everything it would do is done here by an element that disappears when it
 * is closed. See ADR-028.
 *
 * It is a **list of what can be brought back**, not a file manager. There is
 * one action on a row, and it is Restore: this phase is about recovery, and an
 * interface that offers permanent deletion beside it is an interface where the
 * wrong click is unrecoverable.
 *
 * Nothing here is part of the document. Opening it writes nothing, and every
 * label and snippet is written with `textContent` — a note is a text file a
 * person controls, and a preview of one is text, never markup.
 */
export class TrashPanel {
  private readonly doc: Document;
  private readonly root: HTMLElement;
  private readonly list: HTMLElement;
  private readonly status: HTMLElement;
  private readonly handlers: TrashPanelHandlers;

  private entries: TrashEntry[] = [];
  private selected = 0;
  private open = false;

  /**
   * Which request the panel is waiting for.
   *
   * The same rule the search palette follows: only the answer to the question
   * currently being asked may change the list, so a reply that crossed a
   * refresh cannot put a restored note back on screen.
   */
  private lastRequestId = 0;

  public constructor(options: TrashPanelOptions) {
    this.doc = options.document ?? options.mount.ownerDocument;
    this.handlers = options.handlers;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-trash';
    this.root.hidden = true;
    this.root.setAttribute('role', 'dialog');
    this.root.setAttribute('aria-label', 'Lixeira');

    const title = this.doc.createElement('span');
    title.className = 'note-trash-title';
    title.textContent = 'Lixeira';

    this.status = this.doc.createElement('div');
    this.status.className = 'note-trash-status';
    this.status.setAttribute('role', 'status');

    this.list = this.doc.createElement('ul');
    this.list.className = 'note-trash-list';

    const header = this.doc.createElement('div');
    header.className = 'note-trash-header';
    header.append(title, this.status);
    this.root.append(header, this.list);
    options.mount.append(this.root);

    this.root.addEventListener('keydown', this.handleKeyDown);
  }

  public isOpen(): boolean {
    return this.open;
  }

  public element(): HTMLElement {
    return this.root;
  }

  public openPanel(): void {
    this.open = true;
    this.root.hidden = false;
    this.entries = [];
    this.selected = 0;
    this.status.textContent = 'carregando…';
    this.render();
    this.request();
    this.focusFirst();
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.entries = [];
    this.list.replaceChildren();
    this.handlers.onClose();
  }

  /** Asks again — after a restore, so the list stops showing what came back. */
  public refresh(): void {
    if (!this.open) return;
    this.request();
  }

  /** Takes an answer from the host, if it is still the one being waited for. */
  public showEntries(requestId: number, entries: TrashEntry[]): void {
    if (!this.open) return;
    if (requestId !== this.lastRequestId) return;
    this.entries = entries;
    this.selected = Math.min(this.selected, Math.max(entries.length - 1, 0));
    this.render();
  }

  /** A sentence from the host about the last action, shown as it arrived. */
  public setStatus(message: string): void {
    if (!this.open) return;
    this.status.textContent = message;
  }

  public destroy(): void {
    this.root.removeEventListener('keydown', this.handleKeyDown);
    this.root.remove();
  }

  private request(): void {
    this.lastRequestId += 1;
    this.handlers.onList(this.lastRequestId);
  }

  /**
   * Keys the panel owns.
   *
   * Everything handled here is stopped from reaching the editor: the trash
   * must not type into the note behind it. `Ctrl+Shift+Space` is deliberately
   * not among them — the layer belongs to the whole application.
   */
  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (event.isComposing) return;

    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.close();
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      event.stopPropagation();
      this.move(event.key === 'ArrowDown' ? 1 : -1);
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      this.restore(this.selected);
    }
  };

  private move(step: number): void {
    if (this.entries.length === 0) return;
    const total = this.entries.length;
    this.selected = (((this.selected + step) % total) + total) % total;
    this.render();
    this.focusSelected();
  }

  private restore(index: number): void {
    const entry = this.entries[index];
    if (!entry) return;
    this.status.textContent = 'restaurando…';
    this.handlers.onRestore(entry.noteId);
  }

  private focusFirst(): void {
    this.root.querySelector<HTMLElement>('button')?.focus();
  }

  private focusSelected(): void {
    this.list
      .querySelectorAll<HTMLElement>('.note-trash-restore')
      [this.selected]?.focus();
  }

  private render(): void {
    this.list.replaceChildren();

    if (this.entries.length === 0) {
      if (this.status.textContent === 'carregando…' || this.status.textContent === '') {
        this.status.textContent = 'a lixeira está vazia';
      }
      return;
    }
    this.status.textContent = `${this.entries.length} nota(s)`;

    this.entries.forEach((entry, index) => {
      const row = this.doc.createElement('li');
      row.className = index === this.selected ? 'note-trash-row selected' : 'note-trash-row';
      // The identifier is what the action uses. Two notes may read the same and
      // still be two notes.
      row.dataset.noteId = entry.noteId;

      const label = this.doc.createElement('span');
      label.className = 'note-trash-label';
      // `textContent`, always. Nothing from a note is ever parsed as markup.
      label.textContent = entry.label;

      const snippet = this.doc.createElement('span');
      snippet.className = 'note-trash-snippet';
      snippet.textContent = entry.snippet;

      const date = this.doc.createElement('span');
      date.className = 'note-trash-date';
      date.textContent = formatNoteTimestamp(entry.deletedAt);

      const restore = this.doc.createElement('button');
      restore.type = 'button';
      restore.className = 'note-trash-restore';
      restore.textContent = 'Restaurar';
      // Named for a screen reader, so the row is not four identical buttons.
      restore.setAttribute('aria-label', `Restaurar ${entry.label}`);
      restore.addEventListener('click', (event) => {
        event.preventDefault();
        event.stopPropagation();
        this.selected = index;
        this.restore(index);
      });

      row.append(label, snippet, date, restore);
      this.list.append(row);
    });

    this.list.children[this.selected]?.scrollIntoView({ block: 'nearest' });
  }
}
