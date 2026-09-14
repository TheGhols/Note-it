import { SHORTCUT_GROUPS, ShortcutGroup } from './shortcuts.ts';

export interface ShortcutsPanelOptions {
  /** The header button that opens it. */
  trigger: HTMLElement;
  /** Element the popover is appended to, outside the drag region. */
  mount: HTMLElement;
  handlers?: {
    onOpen?(): void;
    onClose?(): void;
  };
  document?: Document;
}

/**
 * The shortcut reference, as a popover in the note you are already in.
 *
 * The same shape the timer popover has, and for the same reasons: a second
 * layer-shell window would have to be placed, focused, stacked and torn down,
 * and everything it would do is done here by an element that disappears when
 * it is closed.
 *
 * It exists because the manual test could not tell why `Ctrl+Shift+Space`
 * answered in one situation and not in another, and nothing on screen said.
 * The answer is scope, so scope is what the panel is organised by — the three
 * groups in `shortcuts.ts`, each with the reason its members behave as they
 * do, rather than one flat list that would hide the only distinction that
 * matters.
 *
 * It writes nothing. Opening it does not touch the store, the note's Markdown,
 * its modification date or its geometry, and every string is written with
 * `textContent`.
 */
export class ShortcutsPanel {
  private readonly doc: Document;
  private readonly root: HTMLElement;
  private readonly trigger: HTMLElement;
  private readonly handlers: NonNullable<ShortcutsPanelOptions['handlers']>;
  private open = false;

  public constructor(options: ShortcutsPanelOptions) {
    this.doc = options.document ?? options.trigger.ownerDocument;
    this.trigger = options.trigger;
    this.handlers = options.handlers ?? {};

    this.root = this.doc.createElement('div');
    this.root.id = 'note-shortcuts';
    this.root.className = 'note-shortcuts';
    this.root.hidden = true;
    this.root.setAttribute('role', 'dialog');
    this.root.setAttribute('aria-label', 'Atalhos do Note-it');
    this.root.tabIndex = -1;

    const heading = this.doc.createElement('div');
    heading.className = 'note-shortcuts-heading';
    heading.textContent = 'Atalhos';
    this.root.append(heading);

    for (const group of SHORTCUT_GROUPS) {
      this.root.append(this.renderGroup(group));
    }

    options.mount.append(this.root);

    // A pointerdown here must never be read as the start of a window drag, for
    // the same reason the menu and the timer stop one.
    this.root.addEventListener('pointerdown', (event) => event.stopPropagation());
    this.trigger.addEventListener('pointerdown', (event) => event.stopPropagation());
    this.trigger.setAttribute('aria-haspopup', 'true');
    this.trigger.setAttribute('aria-expanded', 'false');
    this.trigger.setAttribute('aria-controls', this.root.id);
    this.trigger.addEventListener('click', this.handleTriggerClick);

    this.doc.addEventListener('pointerdown', this.handleDocumentPointerDown, true);
    this.doc.addEventListener('keydown', this.handleKeyDown);
  }

  public element(): HTMLElement {
    return this.root;
  }

  public isOpen(): boolean {
    return this.open;
  }

  public openPanel(): void {
    if (this.open) return;
    this.open = true;
    this.root.hidden = false;
    this.trigger.setAttribute('aria-expanded', 'true');
    this.handlers.onOpen?.();
    // The panel itself takes focus rather than a control inside it: there is
    // nothing here to activate, and Escape has to reach it.
    this.root.focus?.();
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.trigger.setAttribute('aria-expanded', 'false');
    this.handlers.onClose?.();
  }

  public toggle(): void {
    if (this.open) {
      this.close();
    } else {
      this.openPanel();
    }
  }

  public destroy(): void {
    this.doc.removeEventListener('pointerdown', this.handleDocumentPointerDown, true);
    this.doc.removeEventListener('keydown', this.handleKeyDown);
    this.trigger.removeEventListener('click', this.handleTriggerClick);
    this.root.remove();
  }

  private renderGroup(group: ShortcutGroup): HTMLElement {
    const section = this.doc.createElement('section');
    section.className = 'note-shortcuts-group';
    section.dataset.scope = group.scope;

    const title = this.doc.createElement('h2');
    title.className = 'note-shortcuts-group-title';
    title.textContent = group.title;
    section.append(title);

    const note = this.doc.createElement('p');
    note.className = 'note-shortcuts-group-note';
    note.textContent = group.note;
    section.append(note);

    const list = this.doc.createElement('dl');
    list.className = 'note-shortcuts-list';
    for (const shortcut of group.shortcuts) {
      const keys = this.doc.createElement('dt');
      keys.className = 'note-shortcuts-keys';
      keys.textContent = shortcut.keys;

      const description = this.doc.createElement('dd');
      description.className = 'note-shortcuts-description';
      description.textContent = shortcut.description;
      if (shortcut.runs) {
        const runs = this.doc.createElement('code');
        runs.className = 'note-shortcuts-runs';
        runs.textContent = shortcut.runs;
        description.append(runs);
      }

      list.append(keys, description);
    }
    section.append(list);
    return section;
  }

  private readonly handleTriggerClick = (event: Event): void => {
    event.preventDefault();
    event.stopPropagation();
    this.toggle();
  };

  private readonly handleDocumentPointerDown = (event: Event): void => {
    if (!this.open) return;
    const target = event.target as Node | null;
    if (target && (this.root.contains(target) || this.trigger.contains(target))) return;
    this.close();
  };

  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (!this.open || event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    this.close();
    this.trigger.focus?.();
  };
}
