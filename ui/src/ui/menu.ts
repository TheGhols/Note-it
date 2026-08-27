import { PaperColor, PaperIntensity, PaperType, ThemePreference } from '../bridge/types.ts';
import { TEXT_SIZES, TextSize } from '../editor/textSize.ts';
import { HIGHLIGHT_COLORS, PaletteEntry, TEXT_COLORS } from './palettes.ts';
import {
  DEFAULT_PAPER_INTENSITY,
  DEFAULT_PAPER_TYPE,
  PAPER_INTENSITIES,
  PAPER_TYPES,
  paperIntensityLabel,
  paperTypeLabel,
} from './paper.ts';
import { DEFAULT_THEME, THEMES, themeLabel } from './theme.ts';

export type LayerMode = 'overlay' | 'desktop' | 'hidden';

export interface NoteMenuHandlers {
  onSelectColor(color: PaperColor): void;
  onSelectPaperType(type: PaperType): void;
  onSelectPaperIntensity(intensity: PaperIntensity): void;
  onSelectTheme(theme: ThemePreference): void;
  onToggleCollapsed(collapsed: boolean): void;
  onSelectTextSize(size: TextSize | null): void;
  onSelectTextColor(color: string | null): void;
  onSelectHighlight(color: string | null): void;
  onZoomIn(): void;
  onZoomOut(): void;
  onResetZoom(): void;
  onSelectLayerMode(mode: LayerMode): void;
  onOpen?(): void;
  onClose?(): void;
}

const PAPER_COLOR_NAMES: Record<PaperColor, string> = {
  yellow: 'Amarelo',
  blue: 'Azul',
  green: 'Verde',
  pink: 'Rosa',
  purple: 'Roxo',
  gray: 'Cinza',
  black: 'Preto',
};

type MenuPanel =
  | 'root'
  | 'paper'
  | 'paperType'
  | 'paperIntensity'
  | 'textSize'
  | 'textColor'
  | 'highlight'
  | 'zoom'
  | 'theme'
  | 'layer';

export interface NoteMenuOptions {
  /** Button that toggles the menu; it stays outside the drag region. */
  trigger: HTMLElement;
  /** Element the popover is appended to, outside the drag region. */
  mount: HTMLElement;
  colors: readonly PaperColor[];
  handlers: NoteMenuHandlers;
  /** Defaults to the document owning the trigger. */
  document?: Document;
}

interface PanelEntry {
  element: HTMLElement;
  refresh?: () => void;
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
  private readonly panels = new Map<MenuPanel, PanelEntry>();
  private readonly collapseItem: HTMLButtonElement;
  private readonly paperTypeValue: HTMLElement;
  private readonly paperIntensityValue: HTMLElement;
  private readonly zoomValue: HTMLElement;
  private readonly themeValue: HTMLElement;
  private readonly layerValue: HTMLElement;
  private panel: MenuPanel = 'root';
  private open = false;
  private collapsed = false;
  private zoomPercent = 100;
  private layerMode: LayerMode = 'overlay';
  private selectedColor: PaperColor | null = null;
  private paperType: PaperType = DEFAULT_PAPER_TYPE;
  private paperIntensity: PaperIntensity = DEFAULT_PAPER_INTENSITY;
  private theme: ThemePreference = DEFAULT_THEME;
  private currentTextSize: TextSize | null = null;
  private textSizeMixed = false;
  private currentTextColor: string | null = null;
  private currentHighlight: string | null = null;

  public constructor(private readonly options: NoteMenuOptions) {
    this.doc = options.document ?? options.trigger.ownerDocument;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-menu';
    this.root.id = 'note-menu';
    this.root.setAttribute('role', 'menu');
    this.root.hidden = true;

    const rootPanel = this.doc.createElement('div');
    rootPanel.className = 'note-menu-panel';

    const paperItem = this.createSubmenuItem('Cor da nota', 'paper');

    const paperTypeItem = this.createSubmenuItem('Tipo de papel', 'paperType');
    this.paperTypeValue = this.doc.createElement('span');
    this.paperTypeValue.className = 'note-menu-value';
    paperTypeItem.insertBefore(this.paperTypeValue, paperTypeItem.lastElementChild);

    const paperIntensityItem = this.createSubmenuItem('Intensidade', 'paperIntensity');
    this.paperIntensityValue = this.doc.createElement('span');
    this.paperIntensityValue.className = 'note-menu-value';
    paperIntensityItem.insertBefore(
      this.paperIntensityValue,
      paperIntensityItem.lastElementChild,
    );

    const textSizeItem = this.createSubmenuItem('Tamanho do texto', 'textSize');
    const textColorItem = this.createSubmenuItem('Cor do texto', 'textColor');
    const highlightItem = this.createSubmenuItem('Marca-texto', 'highlight');

    const zoomItem = this.createSubmenuItem('Zoom', 'zoom');
    this.zoomValue = this.doc.createElement('span');
    this.zoomValue.className = 'note-menu-value';
    zoomItem.insertBefore(this.zoomValue, zoomItem.lastElementChild);

    const themeItem = this.createSubmenuItem('Tema', 'theme');
    this.themeValue = this.doc.createElement('span');
    this.themeValue.className = 'note-menu-value';
    themeItem.insertBefore(this.themeValue, themeItem.lastElementChild);

    const layerItem = this.createSubmenuItem('Camada', 'layer');
    this.layerValue = this.doc.createElement('span');
    this.layerValue.className = 'note-menu-value';
    layerItem.insertBefore(this.layerValue, layerItem.lastElementChild);

    this.collapseItem = this.createItem('Recolher nota', 'note-menu-item');
    this.collapseItem.append(this.createShortcutHint('Ctrl+Shift+M'));
    this.collapseItem.addEventListener('click', () => {
      const next = !this.collapsed;
      this.close();
      this.options.handlers.onToggleCollapsed(next);
    });

    rootPanel.append(
      paperItem,
      paperTypeItem,
      paperIntensityItem,
      textSizeItem,
      textColorItem,
      highlightItem,
      zoomItem,
      themeItem,
      layerItem,
      this.collapseItem,
    );
    this.panels.set('root', { element: rootPanel });

    this.panels.set('paper', this.buildPaperPanel(options.colors));
    this.panels.set(
      'paperType',
      this.buildChoicePanel(
        'Tipo de papel',
        'note-menu-paper-type',
        PAPER_TYPES,
        () => this.paperType,
        (type) => this.options.handlers.onSelectPaperType(type),
      ),
    );
    this.panels.set(
      'paperIntensity',
      this.buildChoicePanel(
        'Intensidade',
        'note-menu-paper-intensity',
        PAPER_INTENSITIES,
        () => this.paperIntensity,
        (intensity) => this.options.handlers.onSelectPaperIntensity(intensity),
      ),
    );
    this.panels.set('textSize', this.buildTextSizePanel());
    this.panels.set('textColor', this.buildSwatchPanel(
      'Cor do texto',
      TEXT_COLORS,
      (entry) => this.options.handlers.onSelectTextColor(entry.value),
      () => this.currentTextColor,
      'color',
    ));
    this.panels.set('highlight', this.buildSwatchPanel(
      'Marca-texto',
      HIGHLIGHT_COLORS,
      (entry) => this.options.handlers.onSelectHighlight(entry.value),
      () => this.currentHighlight,
      'highlight',
    ));
    this.panels.set('zoom', this.buildZoomPanel());
    this.panels.set(
      'theme',
      this.buildChoicePanel(
        'Tema',
        'note-menu-theme',
        THEMES,
        () => this.theme,
        (theme) => this.options.handlers.onSelectTheme(theme),
      ),
    );
    this.panels.set('layer', this.buildLayerPanel());

    for (const entry of this.panels.values()) {
      entry.element.hidden = true;
      this.root.append(entry.element);
    }
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

    this.setZoomPercent(100);
    this.setLayerMode('overlay');
    this.setPaper(DEFAULT_PAPER_TYPE, DEFAULT_PAPER_INTENSITY);
    this.setTheme(DEFAULT_THEME);
    this.setCollapsed(false);
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
    this.collapseItem.firstChild!.textContent = collapsed ? 'Expandir nota' : 'Recolher nota';
  }

  public setSelectedColor(color: PaperColor): void {
    this.selectedColor = color;
    this.panels.get('paper')?.refresh?.();
  }

  /**
   * Reflects the note's own paper. Both halves arrive together because they
   * describe one surface, and the root rows show the current choice so it is
   * readable without opening either submenu.
   */
  public setPaper(type: PaperType, intensity: PaperIntensity): void {
    this.paperType = type;
    this.paperIntensity = intensity;
    this.paperTypeValue.textContent = paperTypeLabel(type);
    this.paperIntensityValue.textContent = paperIntensityLabel(intensity);
    this.panels.get('paperType')?.refresh?.();
    this.panels.get('paperIntensity')?.refresh?.();
  }

  /** Reflects the shared interface theme, which every note's menu agrees on. */
  public setTheme(theme: ThemePreference): void {
    this.theme = theme;
    this.themeValue.textContent = themeLabel(theme);
    this.panels.get('theme')?.refresh?.();
  }

  public setZoomPercent(percent: number): void {
    this.zoomPercent = percent;
    this.zoomValue.textContent = `${percent}%`;
    this.panels.get('zoom')?.refresh?.();
  }

  public setLayerMode(mode: LayerMode): void {
    this.layerMode = mode;
    this.layerValue.textContent =
      mode === 'desktop' ? 'Área de trabalho' : 'Sempre no topo';
    this.panels.get('layer')?.refresh?.();
  }

  /** Reflects the formatting under the cursor so the menu marks what is active. */
  public setInlineFormatting(state: {
    textSize: TextSize | null;
    textSizeMixed: boolean;
    textColor: string | null;
    highlight: string | null;
  }): void {
    this.currentTextSize = state.textSize;
    this.textSizeMixed = state.textSizeMixed;
    this.currentTextColor = state.textColor;
    this.currentHighlight = state.highlight;
    this.panels.get('textSize')?.refresh?.();
    this.panels.get('textColor')?.refresh?.();
    this.panels.get('highlight')?.refresh?.();
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

  private buildPaperPanel(colors: readonly PaperColor[]): PanelEntry {
    const { panel, body } = this.createPanel('Cor da nota', 'note-menu-paper');
    const swatches = this.doc.createElement('div');
    swatches.className = 'note-menu-swatches';
    swatches.setAttribute('role', 'group');

    for (const color of colors) {
      const swatch = this.doc.createElement('button');
      swatch.type = 'button';
      swatch.className = 'note-menu-swatch';
      swatch.dataset.color = color;
      swatch.setAttribute('role', 'menuitemradio');
      swatch.setAttribute('aria-checked', 'false');
      swatch.setAttribute('aria-label', PAPER_COLOR_NAMES[color] ?? color);
      swatch.title = PAPER_COLOR_NAMES[color] ?? color;
      swatch.addEventListener('click', () => {
        this.close();
        this.options.handlers.onSelectColor(color);
      });
      swatches.append(swatch);
    }
    body.append(swatches);

    return {
      element: panel,
      refresh: () => {
        for (const swatch of swatches.querySelectorAll<HTMLElement>('.note-menu-swatch')) {
          swatch.setAttribute(
            'aria-checked',
            String(swatch.dataset.color === this.selectedColor),
          );
        }
      },
    };
  }

  /**
   * A list of mutually exclusive choices, marked with the active one.
   *
   * Paper type, pattern intensity and theme are all the same shape, so they
   * share one builder rather than three near-identical panels.
   */
  private buildChoicePanel<T extends string>(
    heading: string,
    className: string,
    choices: readonly { id: T; label: string }[],
    currentValue: () => T,
    onSelect: (value: T) => void,
  ): PanelEntry {
    const { panel, body } = this.createPanel(heading, className);
    const options: Array<{ button: HTMLButtonElement; id: T }> = [];

    for (const choice of choices) {
      const button = this.createItem(choice.label, 'note-menu-item note-menu-option');
      button.dataset.value = choice.id;
      button.setAttribute('role', 'menuitemradio');
      button.setAttribute('aria-checked', 'false');
      button.addEventListener('click', () => {
        this.close();
        onSelect(choice.id);
      });
      body.append(button);
      options.push({ button, id: choice.id });
    }

    return {
      element: panel,
      refresh: () => {
        const active = currentValue();
        for (const { button, id } of options) {
          button.setAttribute('aria-checked', String(id === active));
        }
      },
    };
  }

  private buildTextSizePanel(): PanelEntry {
    const { panel, body } = this.createPanel('Tamanho do texto', 'note-menu-sizes');
    const entries: Array<{ button: HTMLButtonElement; size: TextSize | null }> = [];

    const addOption = (label: string, size: TextSize | null) => {
      const button = this.createItem(label, 'note-menu-item note-menu-option');
      button.setAttribute('role', 'menuitemradio');
      button.setAttribute('aria-checked', 'false');
      button.addEventListener('click', () => {
        this.close();
        this.options.handlers.onSelectTextSize(size);
      });
      body.append(button);
      entries.push({ button, size });
    };

    addOption('Padrão', null);
    for (const size of TEXT_SIZES) addOption(String(size), size);

    const mixedHint = this.doc.createElement('p');
    mixedHint.className = 'note-menu-hint';
    mixedHint.textContent = 'Misto';
    mixedHint.hidden = true;
    body.append(mixedHint);

    return {
      element: panel,
      refresh: () => {
        mixedHint.hidden = !this.textSizeMixed;
        for (const entry of entries) {
          const active = !this.textSizeMixed && entry.size === this.currentTextSize;
          entry.button.setAttribute('aria-checked', String(active));
        }
      },
    };
  }

  private buildSwatchPanel(
    heading: string,
    palette: readonly PaletteEntry[],
    onSelect: (entry: PaletteEntry) => void,
    currentValue: () => string | null,
    kind: 'color' | 'highlight',
  ): PanelEntry {
    const { panel, body } = this.createPanel(heading, `note-menu-${kind}s`);
    const swatches = this.doc.createElement('div');
    swatches.className = 'note-menu-swatches';
    swatches.setAttribute('role', 'group');
    const buttons: Array<{ button: HTMLButtonElement; entry: PaletteEntry }> = [];

    for (const entry of palette) {
      const swatch = this.doc.createElement('button');
      swatch.type = 'button';
      swatch.className = entry.value === null
        ? 'note-menu-swatch note-menu-swatch-none'
        : 'note-menu-swatch';
      swatch.dataset.value = entry.value ?? '';
      swatch.setAttribute('role', 'menuitemradio');
      swatch.setAttribute('aria-checked', 'false');
      swatch.setAttribute('aria-label', entry.label);
      swatch.title = entry.label;
      if (entry.value !== null) {
        if (kind === 'color') {
          // Only the letter is coloured. The ground it sits on is a stylesheet
          // concern, because it has to stay pale in both themes: the palette is
          // tuned to be read on paper, and paper is never the popover's colour.
          swatch.style.color = entry.value;
          swatch.textContent = 'A';
        } else {
          swatch.style.backgroundColor = entry.value;
        }
      }
      swatch.addEventListener('click', () => {
        this.close();
        onSelect(entry);
      });
      swatches.append(swatch);
      buttons.push({ button: swatch, entry });
    }
    body.append(swatches);

    return {
      element: panel,
      refresh: () => {
        const active = currentValue();
        for (const { button, entry } of buttons) {
          button.setAttribute('aria-checked', String(entry.value === active));
        }
      },
    };
  }

  private buildZoomPanel(): PanelEntry {
    const { panel, body } = this.createPanel('Zoom', 'note-menu-zoom');

    const row = this.doc.createElement('div');
    row.className = 'note-menu-zoom-row';

    const minus = this.doc.createElement('button');
    minus.type = 'button';
    minus.className = 'note-menu-step';
    minus.setAttribute('role', 'menuitem');
    minus.setAttribute('aria-label', 'Diminuir zoom');
    minus.textContent = '−';
    minus.addEventListener('click', () => this.options.handlers.onZoomOut());

    const value = this.doc.createElement('span');
    value.className = 'note-menu-zoom-value';

    const plus = this.doc.createElement('button');
    plus.type = 'button';
    plus.className = 'note-menu-step';
    plus.setAttribute('role', 'menuitem');
    plus.setAttribute('aria-label', 'Aumentar zoom');
    plus.textContent = '+';
    plus.addEventListener('click', () => this.options.handlers.onZoomIn());

    row.append(minus, value, plus);

    const reset = this.createItem('Restaurar 100%', 'note-menu-item');
    reset.append(this.createShortcutHint('Ctrl+0'));
    reset.addEventListener('click', () => {
      this.close();
      this.options.handlers.onResetZoom();
    });

    body.append(row, reset);

    return {
      element: panel,
      refresh: () => {
        value.textContent = `${this.zoomPercent}%`;
      },
    };
  }

  private buildLayerPanel(): PanelEntry {
    const { panel, body } = this.createPanel('Camada', 'note-menu-layer');
    const options: Array<{ button: HTMLButtonElement; mode: LayerMode }> = [];

    for (const [label, mode] of [
      ['Sempre no topo', 'overlay'],
      ['Área de trabalho', 'desktop'],
    ] as const) {
      const button = this.createItem(label, 'note-menu-item note-menu-option');
      button.setAttribute('role', 'menuitemradio');
      button.setAttribute('aria-checked', 'false');
      button.append(this.createShortcutHint('Ctrl+Shift+Space'));
      button.addEventListener('click', () => {
        this.close();
        this.options.handlers.onSelectLayerMode(mode);
      });
      body.append(button);
      options.push({ button, mode });
    }

    return {
      element: panel,
      refresh: () => {
        for (const { button, mode } of options) {
          // A hidden application is not one of the two visible layers, so
          // nothing is marked rather than marking the wrong entry.
          button.setAttribute('aria-checked', String(this.layerMode === mode));
        }
      },
    };
  }

  private createPanel(heading: string, className: string): {
    panel: HTMLElement;
    body: HTMLElement;
  } {
    const panel = this.doc.createElement('div');
    panel.className = `note-menu-panel ${className}`;

    const title = this.doc.createElement('p');
    title.className = 'note-menu-heading';
    title.textContent = heading;
    panel.append(title);

    const body = this.doc.createElement('div');
    body.className = 'note-menu-body';
    panel.append(body);

    return { panel, body };
  }

  private createSubmenuItem(label: string, target: MenuPanel): HTMLButtonElement {
    const item = this.createItem(label, 'note-menu-item note-menu-submenu');
    item.dataset.panel = target;
    item.setAttribute('aria-haspopup', 'true');
    item.setAttribute('aria-expanded', 'false');
    const chevron = this.doc.createElement('span');
    chevron.className = 'note-menu-chevron';
    chevron.setAttribute('aria-hidden', 'true');
    chevron.textContent = '›';
    item.append(chevron);
    item.addEventListener('click', () => this.showPanel(target));
    return item;
  }

  private createShortcutHint(text: string): HTMLElement {
    const hint = this.doc.createElement('span');
    hint.className = 'note-menu-shortcut';
    hint.textContent = text;
    return hint;
  }

  private showPanel(panel: MenuPanel): void {
    this.panel = panel;
    for (const [name, entry] of this.panels) {
      entry.element.hidden = name !== panel;
    }
    for (const item of this.root.querySelectorAll<HTMLElement>('.note-menu-submenu')) {
      item.setAttribute('aria-expanded', String(item.dataset.panel === panel));
    }
    this.panels.get(panel)?.refresh?.();
    if (this.open) this.focusFirstItem();
  }

  private focusFirstItem(): void {
    this.focusableItems()[0]?.focus();
  }

  private focusableItems(): HTMLElement[] {
    const entry = this.panels.get(this.panel);
    if (!entry) return [];
    return Array.from(entry.element.querySelectorAll<HTMLElement>('button'));
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

    if (event.key === 'ArrowLeft' && this.panel !== 'root') {
      event.preventDefault();
      this.showPanel('root');
      return;
    }

    if (event.key === 'ArrowRight' && this.panel === 'root') {
      const target = (event.target as HTMLElement | null)?.dataset?.panel as
        | MenuPanel
        | undefined;
      if (target) {
        event.preventDefault();
        this.showPanel(target);
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
