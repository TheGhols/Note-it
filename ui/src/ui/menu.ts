import { PaperColor } from '../bridge/types.ts';

export interface NoteMenuLabels {
  colors: string;
  collapse: string;
  expand: string;
  colorNames: Record<PaperColor, string>;
}

export interface NoteMenuHandlers {
  onSelectColor(color: PaperColor): void;
  onToggleCollapsed(collapsed: boolean): void;
  onOpen?(): void;
  onClose?(): void;
}

export const DEFAULT_MENU_LABELS: NoteMenuLabels = {
  colors: 'Cor da nota',
  collapse: 'Recolher nota',
  expand: 'Expandir nota',
  colorNames: {
    yellow: 'Amarelo',
    blue: 'Azul',
    green: 'Verde',
    pink: 'Rosa',
    purple: 'Roxo',
    gray: 'Cinza',
    black: 'Preto',
  },
};

type MenuPanel = 'root' | 'colors';

export interface NoteMenuOptions {
  /** Button that toggles the menu; it stays outside the drag region. */
  trigger: HTMLElement;
  /** Element the popover is appended to, outside the drag region. */
  mount: HTMLElement;
  colors: readonly PaperColor[];
  handlers: NoteMenuHandlers;
  labels?: NoteMenuLabels;
  /** Defaults to the document owning the trigger. */
  document?: Document;
}

/**
 * The note's settings popover.
 *
 * It lives beside the drag region rather than inside it, so a pointer event
 * that lands on the menu can never bubble into the window drag handler. Only
 * one popover exists per note: opening simply swaps the visible panel.
 */
export class NoteMenu {
  private readonly doc: Document;
  private readonly root: HTMLElement;
  private readonly rootPanel: HTMLElement;
  private readonly colorPanel: HTMLElement;
  private readonly collapseItem: HTMLButtonElement;
  private readonly colorsItem: HTMLButtonElement;
  private panel: MenuPanel = 'root';
  private open = false;
  private collapsed = false;
  private selectedColor: PaperColor | null = null;

  public constructor(private readonly options: NoteMenuOptions) {
    this.doc = options.document ?? options.trigger.ownerDocument;
    const labels = options.labels ?? DEFAULT_MENU_LABELS;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-menu';
    this.root.id = 'note-menu';
    this.root.setAttribute('role', 'menu');
    this.root.hidden = true;

    this.rootPanel = this.doc.createElement('div');
    this.rootPanel.className = 'note-menu-panel';

    this.colorsItem = this.createItem(`${labels.colors}`, 'note-menu-item note-menu-submenu');
    this.colorsItem.setAttribute('aria-haspopup', 'true');
    this.colorsItem.setAttribute('aria-expanded', 'false');
    const chevron = this.doc.createElement('span');
    chevron.className = 'note-menu-chevron';
    chevron.setAttribute('aria-hidden', 'true');
    chevron.textContent = '›';
    this.colorsItem.append(chevron);
    this.colorsItem.addEventListener('click', () => this.showPanel('colors'));

    this.collapseItem = this.createItem(labels.collapse, 'note-menu-item');
    this.collapseItem.addEventListener('click', () => {
      const next = !this.collapsed;
      this.close();
      this.options.handlers.onToggleCollapsed(next);
    });

    this.rootPanel.append(this.colorsItem, this.collapseItem);

    this.colorPanel = this.doc.createElement('div');
    this.colorPanel.className = 'note-menu-panel note-menu-colors';
    this.colorPanel.hidden = true;

    const colorHeading = this.doc.createElement('p');
    colorHeading.className = 'note-menu-heading';
    colorHeading.id = 'note-menu-colors-heading';
    colorHeading.textContent = labels.colors;

    const swatches = this.doc.createElement('div');
    swatches.className = 'note-menu-swatches';
    swatches.setAttribute('role', 'group');
    swatches.setAttribute('aria-labelledby', colorHeading.id);

    for (const color of options.colors) {
      const swatch = this.doc.createElement('button');
      swatch.type = 'button';
      swatch.className = 'note-menu-swatch';
      swatch.dataset.color = color;
      swatch.setAttribute('role', 'menuitemradio');
      swatch.setAttribute('aria-checked', 'false');
      swatch.setAttribute('aria-label', labels.colorNames[color] ?? color);
      swatch.title = labels.colorNames[color] ?? color;
      swatch.addEventListener('click', () => {
        this.close();
        this.options.handlers.onSelectColor(color);
      });
      swatches.append(swatch);
    }

    this.colorPanel.append(colorHeading, swatches);
    this.root.append(this.rootPanel, this.colorPanel);
    options.mount.append(this.root);

    // Belt and braces: the popover sits outside the drag region already, but a
    // pointerdown here must never be read as the start of a window drag.
    this.root.addEventListener('pointerdown', (event) => event.stopPropagation());

    options.trigger.setAttribute('aria-haspopup', 'true');
    options.trigger.setAttribute('aria-expanded', 'false');
    options.trigger.setAttribute('aria-controls', this.root.id);
    options.trigger.addEventListener('pointerdown', (event) => event.stopPropagation());
    options.trigger.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      this.toggle();
    });

    this.doc.addEventListener('pointerdown', this.handleDocumentPointerDown, true);
    this.doc.addEventListener('keydown', this.handleKeyDown);
  }

  public destroy(): void {
    this.doc.removeEventListener('pointerdown', this.handleDocumentPointerDown, true);
    this.doc.removeEventListener('keydown', this.handleKeyDown);
    this.root.remove();
  }

  public get element(): HTMLElement {
    return this.root;
  }

  public isOpen(): boolean {
    return this.open;
  }

  public activePanel(): MenuPanel {
    return this.panel;
  }

  public setCollapsed(collapsed: boolean): void {
    this.collapsed = collapsed;
    const labels = this.options.labels ?? DEFAULT_MENU_LABELS;
    this.collapseItem.firstChild!.textContent = collapsed ? labels.expand : labels.collapse;
  }

  public setSelectedColor(color: PaperColor): void {
    this.selectedColor = color;
    for (const swatch of this.colorPanel.querySelectorAll<HTMLElement>('.note-menu-swatch')) {
      swatch.setAttribute('aria-checked', String(swatch.dataset.color === color));
    }
  }

  public selected(): PaperColor | null {
    return this.selectedColor;
  }

  public toggle(): void {
    if (this.open) {
      this.close();
    } else {
      this.openMenu();
    }
  }

  public openMenu(): void {
    if (this.open) {
      // Never stack a second popover: reset the existing one to its root panel.
      this.showPanel('root');
      return;
    }
    this.open = true;
    this.root.hidden = false;
    this.showPanel('root');
    this.options.trigger.setAttribute('aria-expanded', 'true');
    this.options.handlers.onOpen?.();
    this.focusFirstItem();
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.showPanel('root');
    this.options.trigger.setAttribute('aria-expanded', 'false');
    this.options.handlers.onClose?.();
  }

  private showPanel(panel: MenuPanel): void {
    this.panel = panel;
    this.rootPanel.hidden = panel !== 'root';
    this.colorPanel.hidden = panel !== 'colors';
    this.colorsItem.setAttribute('aria-expanded', String(panel === 'colors'));
    if (this.open) this.focusFirstItem();
  }

  private focusFirstItem(): void {
    const items = this.focusableItems();
    items[0]?.focus();
  }

  private focusableItems(): HTMLElement[] {
    const panel = this.panel === 'colors' ? this.colorPanel : this.rootPanel;
    return Array.from(panel.querySelectorAll<HTMLElement>('button'));
  }

  private createItem(label: string, className: string): HTMLButtonElement {
    const item = this.doc.createElement('button');
    item.type = 'button';
    item.className = className;
    item.setAttribute('role', 'menuitem');
    item.append(this.doc.createTextNode(label));
    return item;
  }

  private readonly handleDocumentPointerDown = (event: Event): void => {
    if (!this.open) return;
    const target = event.target as Node | null;
    if (target && (this.root.contains(target) || this.options.trigger.contains(target))) return;
    this.close();
  };

  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (!this.open) return;

    if (event.key === 'Escape') {
      event.preventDefault();
      this.close();
      (this.options.trigger as HTMLElement).focus?.();
      return;
    }

    if (event.key === 'ArrowLeft' && this.panel === 'colors') {
      event.preventDefault();
      this.showPanel('root');
      return;
    }

    if (event.key === 'ArrowRight' && this.panel === 'root') {
      const items = this.focusableItems();
      if (items.indexOf(event.target as HTMLElement) === 0) {
        event.preventDefault();
        this.showPanel('colors');
      }
      return;
    }

    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;

    const items = this.focusableItems();
    if (items.length === 0) return;
    event.preventDefault();
    const current = items.indexOf(event.target as HTMLElement);
    const step = event.key === 'ArrowDown' ? 1 : -1;
    const next = (current + step + items.length) % items.length;
    items[next].focus();
  };
}
