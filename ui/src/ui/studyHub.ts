import { DOMSerializer } from '@tiptap/pm/model';
import {
  currentStreak,
  heatmapDays,
  localDay,
  longestStreak,
  reviewNow,
  statusOf,
} from '../study/stats.ts';
import type { GlobalCatalog, GlobalReviewItem, StudyState } from '../study/types.ts';

export type StudyFilter = 'review' | 'all' | 'current';

export interface StudyHubHandlers {
  onRequestCatalog(requestId: number): void;
  onStart(items: readonly GlobalReviewItem[], cards: number, schema: GlobalCatalog['schema']): void;
  onClose(): void;
}

export interface StudyHubOptions {
  mount: HTMLElement;
  handlers: StudyHubHandlers;
  document?: Document;
  now?: () => Date;
}

export class StudyHub {
  private readonly doc: Document;
  private readonly handlers: StudyHubHandlers;
  private readonly now: () => Date;
  private readonly root: HTMLElement;
  private readonly status: HTMLElement;
  private readonly stats: HTMLElement;
  private readonly heatmap: HTMLElement;
  private readonly list: HTMLElement;
  private readonly start: HTMLButtonElement;
  private readonly filters: Record<StudyFilter, HTMLButtonElement>;
  private requestId = 0;
  private open = false;
  private currentNoteId = '';
  private filter: StudyFilter = 'review';
  private catalog: GlobalCatalog | null = null;
  private study: StudyState | null = null;
  private invoker: HTMLElement | null = null;

  public constructor(options: StudyHubOptions) {
    this.doc = options.document ?? options.mount.ownerDocument;
    this.handlers = options.handlers;
    this.now = options.now ?? (() => new Date());
    this.root = this.doc.createElement('section');
    this.root.className = 'note-study-hub';
    this.root.hidden = true;
    this.root.tabIndex = -1;
    this.root.setAttribute('role', 'dialog');
    this.root.setAttribute('aria-label', 'Central de estudos');

    const header = this.doc.createElement('header');
    header.className = 'note-study-hub-header';
    const title = this.doc.createElement('h1');
    title.textContent = 'Estudos';
    const refresh = this.button('Atualizar', 'note-study-hub-refresh');
    refresh.addEventListener('click', () => this.load());
    const close = this.button('Fechar', 'note-study-hub-close');
    close.addEventListener('click', () => this.close());
    header.append(title, refresh, close);

    this.status = this.doc.createElement('p');
    this.status.className = 'note-study-hub-status';
    this.status.setAttribute('role', 'status');

    this.stats = this.doc.createElement('div');
    this.stats.className = 'note-study-hub-stats';

    const heatmapScroll = this.doc.createElement('div');
    heatmapScroll.className = 'note-study-heatmap-scroll';
    heatmapScroll.tabIndex = 0;
    heatmapScroll.setAttribute('aria-label', 'Atividade de estudo nos últimos 365 dias');
    this.heatmap = this.doc.createElement('div');
    this.heatmap.className = 'note-study-heatmap';
    heatmapScroll.append(this.heatmap);

    const filterBar = this.doc.createElement('div');
    filterBar.className = 'note-study-filters';
    this.filters = {
      review: this.filterButton('Revisar agora', 'review'),
      all: this.filterButton('Todos', 'all'),
      current: this.filterButton('Esta nota', 'current'),
    };
    filterBar.append(this.filters.review, this.filters.all, this.filters.current);

    this.list = this.doc.createElement('ol');
    this.list.className = 'note-study-global-list';

    this.start = this.button('Começar revisão', 'note-study-start');
    this.start.disabled = true;
    this.start.addEventListener('click', () => {
      const selection = this.selection();
      if (!this.catalog || selection.length === 0) return;
      this.handlers.onStart(selection, this.sourceCountFor(selection), this.catalog.schema);
    });

    this.root.append(header, this.status, this.stats, heatmapScroll, filterBar, this.list, this.start);
    options.mount.append(this.root);
    this.root.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      this.close();
    });
  }

  public isOpen(): boolean {
    return this.open;
  }

  public element(): HTMLElement {
    return this.root;
  }

  public openHub(currentNoteId: string, invoker?: HTMLElement | null, filter: StudyFilter = 'review'): void {
    this.open = true;
    this.currentNoteId = currentNoteId;
    this.invoker = invoker ?? null;
    this.filter = filter;
    this.root.hidden = false;
    this.root.focus();
    this.load();
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    const invoker = this.invoker;
    this.invoker = null;
    if (invoker && invoker.isConnected && invoker.offsetParent !== null) invoker.focus();
    else this.handlers.onClose();
  }

  public currentRequestId(): number {
    return this.requestId;
  }

  private load(): void {
    this.requestId += 1;
    this.catalog = null;
    this.study = null;
    this.status.textContent = 'Carregando flashcards…';
    this.stats.replaceChildren();
    this.heatmap.replaceChildren();
    this.list.replaceChildren();
    this.start.disabled = true;
    this.handlers.onRequestCatalog(this.requestId);
  }

  public showCatalog(requestId: number, catalog: GlobalCatalog, study: StudyState): boolean {
    if (!this.open || requestId !== this.requestId) return false;
    this.catalog = catalog;
    this.study = study;
    this.status.textContent = catalog.items.length === 0 ? 'Nenhum flashcard nas notas.' : '';
    this.render();
    return true;
  }

  public showError(requestId: number, message: string): boolean {
    if (!this.open || requestId !== this.requestId) return false;
    this.catalog = null;
    this.study = null;
    this.status.textContent = message;
    this.start.disabled = true;
    return true;
  }

  public updateStudyState(study: StudyState): void {
    this.study = study;
    if (this.catalog) {
      this.catalog = {
        ...this.catalog,
        items: this.catalog.items.map((item) => ({
          ...item,
          schedule: study.cards[item.reviewKey] ?? null,
        })),
      };
    }
    if (this.open) this.render();
  }

  private button(label: string, className: string): HTMLButtonElement {
    const button = this.doc.createElement('button');
    button.type = 'button';
    button.className = `note-study-button ${className}`;
    button.textContent = label;
    return button;
  }

  private filterButton(label: string, filter: StudyFilter): HTMLButtonElement {
    const button = this.button(label, `note-study-filter note-study-filter-${filter}`);
    button.addEventListener('click', () => {
      this.filter = filter;
      this.render();
    });
    return button;
  }

  private selection(): GlobalReviewItem[] {
    if (!this.catalog) return [];
    if (this.filter === 'review') return reviewNow(this.catalog.items, this.now());
    if (this.filter === 'current') {
      return this.catalog.items.filter((item) => item.noteId === this.currentNoteId);
    }
    return [...this.catalog.items].sort((left, right) => left.documentOrder - right.documentOrder);
  }

  private sourceCountFor(items: readonly GlobalReviewItem[]): number {
    return new Set(items.map((item) => `${item.noteId}:${item.source}`)).size;
  }

  private render(): void {
    if (!this.catalog || !this.study) return;
    const now = this.now();
    const due = this.catalog.items.filter((item) => statusOf(item, now) === 'due').length;
    const fresh = this.catalog.items.filter((item) => statusOf(item, now) === 'new').length;
    const today = this.study.days[localDay(now)]?.reviews ?? 0;
    const values: Array<[string, number]> = [
      ['Para revisar', due],
      ['Novos', fresh],
      ['Total de cartões', this.catalog.items.length],
      ['Notas com cartões', this.catalog.notesWithCards],
      ['Revisões hoje', today],
      ['Sequência atual', currentStreak(this.study.days, now)],
      ['Maior sequência', longestStreak(this.study.days)],
    ];
    this.stats.replaceChildren(
      ...values.map(([label, value]) => {
        const item = this.doc.createElement('div');
        item.className = 'note-study-stat';
        const number = this.doc.createElement('strong');
        number.textContent = String(value);
        const name = this.doc.createElement('span');
        name.textContent = label;
        item.append(number, name);
        return item;
      }),
    );

    const dateFormatter = new Intl.DateTimeFormat('pt-BR');
    this.heatmap.replaceChildren(
      ...heatmapDays(this.study, now).map((day) => {
        const cell = this.doc.createElement('span');
        cell.className = 'note-study-heat-cell';
        cell.dataset.level = String(day.level);
        const date = new Date(`${day.key}T12:00:00`);
        const label = `${dateFormatter.format(date)} · ${day.reviews} ${day.reviews === 1 ? 'revisão' : 'revisões'}`;
        cell.title = label;
        cell.setAttribute('aria-label', label);
        cell.setAttribute('role', 'img');
        return cell;
      }),
    );

    (Object.keys(this.filters) as StudyFilter[]).forEach((filter) => {
      const active = this.filter === filter;
      this.filters[filter].setAttribute('aria-pressed', String(active));
    });

    const serializer = DOMSerializer.fromSchema(this.catalog.schema);
    const selected = this.selection();
    this.list.replaceChildren(
      ...selected.map((item) => {
        const row = this.doc.createElement('li');
        row.className = 'note-study-global-item';
        const preview = this.doc.createElement('div');
        preview.className = 'note-study-global-preview note-study-side';
        preview.append(serializer.serializeFragment(item.question.content, { document: this.doc }));
        const meta = this.doc.createElement('div');
        meta.className = 'note-study-global-meta';
        const source = this.doc.createElement('span');
        source.textContent = item.noteTitle;
        const status = this.doc.createElement('span');
        status.textContent = this.statusLabel(item, now);
        meta.append(source, status);
        row.append(preview, meta);
        return row;
      }),
    );
    this.start.disabled = selected.length === 0;
  }

  private statusLabel(item: GlobalReviewItem, now: Date): string {
    const status = statusOf(item, now);
    if (status === 'new') return 'Novo';
    if (status === 'due') return 'Revisar agora';
    const days = Math.max(1, Math.ceil((Date.parse(item.schedule!.due_at) - now.getTime()) / 86_400_000));
    return days === 1 ? 'em 1 dia' : `em ${days} dias`;
  }
}
