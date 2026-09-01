import type {
  MetadataCatalog,
  MetadataView,
  NoteProperty,
  TagView,
} from '../bridge/types.ts';

export type MetadataSection = 'tags' | 'properties';

export interface MetadataDraft {
  tags: string[];
  properties: NoteProperty[];
}

export interface MetadataPanelHandlers {
  requestCatalog(requestId: number): void;
  requestSuggestions(requestId: number, kind: 'tag' | 'property_key', query: string): void;
  save(requestId: number, draft: MetadataDraft): void;
  onOpen?(): void;
  onClose?(): void;
}

const emptyCatalog: MetadataCatalog = { tags: [], propertyKeys: [] };

/**
 * The one semantic-metadata editor. It owns no persistence and never handles
 * YAML: confirmed drafts go to Rust, and only a successful host reply becomes
 * committed UI state.
 */
export class MetadataPanel {
  private readonly root: HTMLElement;
  private readonly title: HTMLElement;
  private readonly body: HTMLElement;
  private readonly status: HTMLElement;
  private readonly tagsTab: HTMLButtonElement;
  private readonly propertiesTab: HTMLButtonElement;
  private openState = false;
  private section: MetadataSection = 'tags';
  private invoker: HTMLElement | null = null;
  private metadata: MetadataView = { tags: [], properties: [] };
  private catalog: MetadataCatalog = emptyCatalog;
  private requestId = 0;
  private pendingSave: number | null = null;
  private suggestionRequestId = 0;
  private suggestionTarget: HTMLInputElement | null = null;

  public constructor(
    mount: HTMLElement,
    private readonly handlers: MetadataPanelHandlers,
    private readonly doc: Document = mount.ownerDocument,
  ) {
    this.root = doc.createElement('section');
    this.root.className = 'note-metadata note-menu';
    this.root.setAttribute('role', 'dialog');
    this.root.setAttribute('aria-modal', 'false');
    this.root.setAttribute('aria-label', 'Metadados da nota');
    this.root.hidden = true;

    const header = doc.createElement('header');
    header.className = 'note-metadata-header';
    this.title = doc.createElement('h2');
    this.title.textContent = 'Metadados';
    const close = doc.createElement('button');
    close.type = 'button';
    close.className = 'note-metadata-close';
    close.textContent = 'Fechar';
    close.setAttribute('aria-label', 'Fechar metadados');
    close.addEventListener('click', () => this.close());
    header.append(this.title, close);

    const tabs = doc.createElement('div');
    tabs.className = 'note-metadata-tabs';
    tabs.setAttribute('role', 'tablist');
    this.tagsTab = this.makeTab('Tags', 'tags');
    this.propertiesTab = this.makeTab('Propriedades', 'properties');
    tabs.append(this.tagsTab, this.propertiesTab);

    this.body = doc.createElement('div');
    this.body.className = 'note-metadata-body';
    this.status = doc.createElement('p');
    this.status.className = 'note-metadata-status';
    this.status.setAttribute('aria-live', 'polite');

    this.root.append(header, tabs, this.body, this.status);
    mount.append(this.root);
    this.root.addEventListener('pointerdown', (event) => event.stopPropagation());
    doc.addEventListener('pointerdown', this.onDocumentPointerDown, true);
    doc.addEventListener('keydown', this.onKeyDown);
  }

  public get element(): HTMLElement {
    return this.root;
  }

  public isOpen(): boolean {
    return this.openState;
  }

  public activeSection(): MetadataSection {
    return this.section;
  }

  public setMetadata(metadata: MetadataView): void {
    this.metadata = {
      tags: metadata.tags.map((tag) => ({ ...tag })),
      properties: metadata.properties.map((property) => ({ ...property })),
    };
    if (this.openState) this.render();
  }

  public setCatalog(requestId: number, catalog: MetadataCatalog): void {
    if (requestId !== this.requestId) return;
    this.catalog = catalog;
    if (!this.openState) return;
    const tagInput = this.root.querySelector<HTMLInputElement>('[aria-label="Nova tag"]');
    if (tagInput) tagInput.title = `${catalog.tags.length} tags usadas nas notas vivas`;
    for (const keyInput of this.root.querySelectorAll<HTMLInputElement>(
      '[aria-label="Chave da propriedade"]',
    )) {
      keyInput.title = `${catalog.propertyKeys.length} chaves usadas nas notas vivas`;
    }
  }

  public resolveSave(requestId: number, ok: boolean, message: string, metadata: MetadataView): void {
    if (requestId !== this.pendingSave) return;
    this.pendingSave = null;
    this.status.textContent = message;
    this.status.dataset.ok = String(ok);
    if (ok) this.setMetadata(metadata);
    this.setDisabled(false);
  }

  public setSuggestions(requestId: number, suggestions: string[]): void {
    if (requestId !== this.suggestionRequestId || !this.suggestionTarget?.isConnected) return;
    this.root.querySelector('.note-metadata-suggestions')?.remove();
    if (suggestions.length === 0) return;
    const list = this.doc.createElement('div');
    list.className = 'note-metadata-suggestions';
    list.setAttribute('role', 'listbox');
    for (const suggestion of suggestions) {
      const option = this.doc.createElement('button');
      option.type = 'button';
      option.setAttribute('role', 'option');
      option.textContent = suggestion;
      option.addEventListener('click', () => {
        if (!this.suggestionTarget) return;
        this.suggestionTarget.value = suggestion;
        this.suggestionTarget.focus();
        list.remove();
      });
      list.append(option);
    }
    this.suggestionTarget.parentElement?.append(list);
  }

  public open(section: MetadataSection, invoker: HTMLElement | null = null): void {
    this.section = section;
    this.invoker = invoker;
    this.openState = true;
    this.root.hidden = false;
    this.status.textContent = '';
    this.render();
    this.requestId += 1;
    this.handlers.requestCatalog(this.requestId);
    this.handlers.onOpen?.();
    this.root.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
  }

  public close(): void {
    if (!this.openState) return;
    this.openState = false;
    this.root.hidden = true;
    this.root.querySelector('.note-metadata-suggestions')?.remove();
    this.handlers.onClose?.();
    this.invoker?.focus();
    this.invoker = null;
  }

  public destroy(): void {
    this.doc.removeEventListener('pointerdown', this.onDocumentPointerDown, true);
    this.doc.removeEventListener('keydown', this.onKeyDown);
    this.root.remove();
  }

  private makeTab(label: string, section: MetadataSection): HTMLButtonElement {
    const button = this.doc.createElement('button');
    button.type = 'button';
    button.className = 'note-metadata-tab';
    button.setAttribute('role', 'tab');
    button.textContent = label;
    button.addEventListener('click', () => {
      this.section = section;
      this.render();
    });
    return button;
  }

  private render(): void {
    this.tagsTab.setAttribute('aria-selected', String(this.section === 'tags'));
    this.propertiesTab.setAttribute('aria-selected', String(this.section === 'properties'));
    this.body.replaceChildren();
    if (this.section === 'tags') this.renderTags();
    else this.renderProperties();
  }

  private renderTags(): void {
    const chips = this.doc.createElement('div');
    chips.className = 'note-metadata-chips';
    if (this.metadata.tags.length === 0) {
      const empty = this.doc.createElement('p');
      empty.className = 'note-metadata-empty';
      empty.textContent = 'Nenhuma tag';
      chips.append(empty);
    }
    for (const tag of this.metadata.tags) {
      chips.append(this.removableTag(tag));
    }

    const form = this.doc.createElement('form');
    form.className = 'note-metadata-add';
    const input = this.doc.createElement('input');
    input.type = 'text';
    input.maxLength = 65;
    input.placeholder = 'Adicionar tag';
    input.setAttribute('aria-label', 'Nova tag');
    input.title = `${this.catalog.tags.length} tags usadas nas notas vivas`;
    input.addEventListener('input', () => this.requestSuggestions(input, 'tag'));
    const add = this.doc.createElement('button');
    add.type = 'submit';
    add.textContent = 'Adicionar';
    form.append(input, add);
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      const value = input.value;
      if (!value.trim()) return;
      this.save({
        tags: [...this.metadata.tags.map((tag) => tag.value), value],
        properties: this.metadata.properties,
      });
    });
    this.body.append(chips, form);
  }

  private removableTag(tag: TagView): HTMLElement {
    const chip = this.doc.createElement('span');
    chip.className = 'metadata-chip';
    chip.dataset.colour = String(tag.colour);
    const label = this.doc.createElement('span');
    label.textContent = tag.value;
    const remove = this.doc.createElement('button');
    remove.type = 'button';
    remove.textContent = '×';
    remove.setAttribute('aria-label', `Remover tag ${tag.value}`);
    remove.addEventListener('click', () => {
      this.save({
        tags: this.metadata.tags.filter((candidate) => candidate !== tag).map((item) => item.value),
        properties: this.metadata.properties,
      });
    });
    chip.append(label, remove);
    return chip;
  }

  private renderProperties(): void {
    const form = this.doc.createElement('form');
    form.className = 'note-metadata-properties';
    const rows = this.metadata.properties.map((property) => this.propertyRow(property));
    for (const row of rows) form.append(row.element);

    const add = this.doc.createElement('button');
    add.type = 'button';
    add.className = 'note-metadata-add-property';
    add.textContent = '+ Adicionar propriedade';
    add.addEventListener('click', () => {
      const row = this.propertyRow({ key: '', value: '' });
      rows.push(row);
      form.insertBefore(row.element, actions);
      row.key.focus();
    });

    const actions = this.doc.createElement('div');
    actions.className = 'note-metadata-actions';
    const save = this.doc.createElement('button');
    save.type = 'submit';
    save.textContent = 'Salvar propriedades';
    actions.append(add, save);
    form.append(actions);
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      this.save({
        tags: this.metadata.tags.map((tag) => tag.value),
        properties: rows
          .filter((row) => row.element.isConnected)
          .map((row) => ({ key: row.key.value, value: row.value.value })),
      });
    });
    this.body.append(form);
  }

  private propertyRow(property: NoteProperty): {
    element: HTMLElement;
    key: HTMLInputElement;
    value: HTMLInputElement;
  } {
    const element = this.doc.createElement('div');
    element.className = 'note-metadata-property';
    const key = this.doc.createElement('input');
    key.type = 'text';
    key.maxLength = 64;
    key.value = property.key;
    key.placeholder = 'Chave';
    key.setAttribute('aria-label', 'Chave da propriedade');
    key.title = `${this.catalog.propertyKeys.length} chaves usadas nas notas vivas`;
    key.addEventListener('input', () => this.requestSuggestions(key, 'property_key'));
    const value = this.doc.createElement('input');
    value.type = 'text';
    value.maxLength = 512;
    value.value = property.value;
    value.placeholder = 'Valor';
    value.setAttribute('aria-label', `Valor de ${property.key || 'nova propriedade'}`);
    const remove = this.doc.createElement('button');
    remove.type = 'button';
    remove.textContent = 'Remover';
    remove.setAttribute('aria-label', `Remover propriedade ${property.key || 'nova'}`);
    remove.addEventListener('click', () => element.remove());
    element.append(key, value, remove);
    return { element, key, value };
  }

  private save(draft: MetadataDraft): void {
    if (this.pendingSave !== null) return;
    this.requestId += 1;
    this.pendingSave = this.requestId;
    this.status.textContent = 'Salvando…';
    this.status.dataset.ok = '';
    this.setDisabled(true);
    this.handlers.save(this.requestId, draft);
  }

  private requestSuggestions(
    target: HTMLInputElement,
    kind: 'tag' | 'property_key',
  ): void {
    this.suggestionRequestId += 1;
    this.suggestionTarget = target;
    this.handlers.requestSuggestions(this.suggestionRequestId, kind, target.value);
  }

  private setDisabled(disabled: boolean): void {
    for (const control of this.root.querySelectorAll<HTMLInputElement | HTMLButtonElement>(
      'input, button',
    )) {
      control.disabled = disabled;
    }
  }

  private readonly onDocumentPointerDown = (event: PointerEvent): void => {
    if (!this.openState) return;
    const target = event.target as Node | null;
    if (target && !this.root.contains(target)) this.close();
  };

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    if (!this.openState || event.key !== 'Escape') return;
    event.preventDefault();
    this.close();
  };
}

/** A single responsive row; its click route opens the panel above. */
export class NoteTagStrip {
  private metadata: MetadataView = { tags: [], properties: [] };

  public constructor(
    private readonly root: HTMLElement,
    onOpen: () => void,
  ) {
    root.addEventListener('click', () => onOpen());
    root.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        onOpen();
      }
    });
  }

  public setMetadata(
    metadata: MetadataView,
    width = this.root.clientWidth,
    height = this.root.ownerDocument.defaultView?.innerHeight ?? 0,
  ): void {
    this.metadata = metadata;
    this.root.ownerDocument.body.dataset.hasTags = String(metadata.tags.length > 0);
    this.render(width, height);
  }

  public render(
    width: number,
    height = this.root.ownerDocument.defaultView?.innerHeight ?? 0,
  ): void {
    const tags = this.metadata.tags;
    this.root.replaceChildren();
    this.root.hidden = tags.length === 0;
    this.root.tabIndex = tags.length === 0 ? -1 : 0;
    if (tags.length === 0) return;

    if ((width > 0 && width < 230) || (height > 0 && height < 220)) {
      const count = this.root.ownerDocument.createElement('span');
      count.className = 'note-tag-count';
      count.textContent = `${tags.length} ${tags.length === 1 ? 'tag' : 'tags'}`;
      this.root.append(count);
      return;
    }

    const visible = width > 0 && width < 320 ? 1 : width > 0 && width < 420 ? 2 : 4;
    for (const tag of tags.slice(0, visible)) {
      const chip = this.root.ownerDocument.createElement('span');
      chip.className = 'metadata-chip';
      chip.dataset.colour = String(tag.colour);
      chip.textContent = tag.value;
      this.root.append(chip);
    }
    if (tags.length > visible) {
      const overflow = this.root.ownerDocument.createElement('span');
      overflow.className = 'note-tag-overflow';
      overflow.textContent = `+${tags.length - visible}`;
      this.root.append(overflow);
    }
  }
}
