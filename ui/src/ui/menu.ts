import { PaperColor, PaperIntensity, PaperType, ThemePreference } from '../bridge/types.ts';
import {
  CAPTURE_DELIMITERS,
  CaptureDelimiter,
  DEFAULT_CAPTURE_DELIMITER,
  delimiterLabel,
} from '../capture/autoPaste.ts';
import { CALLOUT_TYPES, CalloutType } from '../editor/callout.ts';
import { CODE_LANGUAGES, codeLanguageLabel } from '../editor/codeBlock.ts';
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
  onToggleCodeBlock(): void;
  onSelectCodeLanguage(language: string | null): void;
  onToggleBlockquote(): void;
  onSelectCallout(type: CalloutType | null): void;
  onInsertComment(): void;
  onOpenGlobalSearch(): void;
  onOpenFind(): void;
  onOpenReplace(): void;
  /** The reader confirmed: move this note to the trash. */
  onTrashNote(): void;
  onOpenTrash(): void;
  onCreateBackup(): void;
  /** Asks the host for a file chooser and an image from it. */
  onInsertImage(): void;
  /** Asks for this note to start or stop capturing the clipboard. */
  onToggleAutoPaste(active: boolean): void;
  onSelectCaptureDelimiter(delimiter: CaptureDelimiter): void;
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

export type MenuPanel =
  | 'root'
  | 'paper'
  | 'paperType'
  | 'paperIntensity'
  | 'textSize'
  | 'textColor'
  | 'highlight'
  | 'zoom'
  | 'theme'
  | 'layer'
  | 'blocks'
  | 'codeLanguage'
  | 'callout'
  | 'search'
  | 'media'
  | 'capture'
  | 'captureDelimiter'
  | 'data'
  | 'trashConfirm';

export interface NoteMenuOptions {
  /** Button that toggles the menu; it stays outside the drag region. */
  trigger: HTMLElement;
  /** Element the popover is appended to, outside the drag region. */
  mount: HTMLElement;
  colors: readonly PaperColor[];
  handlers: NoteMenuHandlers;
  /**
   * Header buttons that open an existing panel directly.
   *
   * A quick action is a second way in, never a second implementation: the
   * button is bound to a panel this menu already builds, so Cor da nota and
   * the menu's own paper panel are the same panel and go through the same
   * handler.
   */
  quickTriggers?: Partial<Record<MenuPanel, HTMLElement>>;
  /** Defaults to the document owning the trigger. */
  document?: Document;
}

interface PanelEntry {
  element: HTMLElement;
  refresh?: () => void;
}

/** What the cursor is sitting in, as far as the Blocos section needs it. */
export interface BlockState {
  codeBlock: boolean;
  codeLanguage: string | null;
  blockquote: boolean;
  callout: CalloutType | null;
  comment: boolean;
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
  private readonly triggers = new Map<HTMLElement, MenuPanel>();
  private readonly collapseItem: HTMLButtonElement;
  private readonly paperTypeValue: HTMLElement;
  private readonly paperIntensityValue: HTMLElement;
  private readonly zoomValue: HTMLElement;
  private autoPasteItem!: HTMLButtonElement;
  private autoPasteValue!: HTMLElement;
  private captureDelimiterValue!: HTMLElement;
  private readonly themeValue: HTMLElement;
  private readonly layerValue: HTMLElement;
  private codeBlockItem!: HTMLButtonElement;
  private codeLanguageItem!: HTMLButtonElement;
  private codeLanguageValue!: HTMLElement;
  private calloutValue!: HTMLElement;
  private blockquoteItem!: HTMLButtonElement;
  private panel: MenuPanel = 'root';
  private invoker: HTMLElement;
  private open = false;
  private collapsed = false;
  private zoomPercent = 100;
  private layerMode: LayerMode = 'overlay';
  private selectedColor: PaperColor | null = null;
  private paperType: PaperType = DEFAULT_PAPER_TYPE;
  private paperIntensity: PaperIntensity = DEFAULT_PAPER_INTENSITY;
  private theme: ThemePreference = DEFAULT_THEME;
  private autoPaste = false;
  private captureDelimiter: CaptureDelimiter = DEFAULT_CAPTURE_DELIMITER;
  private currentTextSize: TextSize | null = null;
  private textSizeMixed = false;
  private currentTextColor: string | null = null;
  private currentHighlight: string | null = null;
  private block: BlockState = {
    codeBlock: false,
    codeLanguage: null,
    blockquote: false,
    callout: null,
    comment: false,
  };

  public constructor(private readonly options: NoteMenuOptions) {
    this.doc = options.document ?? options.trigger.ownerDocument;
    this.invoker = options.trigger;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-menu';
    this.root.id = 'note-menu';
    this.root.setAttribute('role', 'menu');
    this.root.hidden = true;

    const rootPanel = this.doc.createElement('div');
    rootPanel.className = 'note-menu-panel';

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

    const zoomItem = this.createSubmenuItem('Zoom', 'zoom');
    this.zoomValue = this.doc.createElement('span');
    this.zoomValue.className = 'note-menu-value';
    zoomItem.insertBefore(this.zoomValue, zoomItem.lastElementChild);

    const themeItem = this.createSubmenuItem('Tema', 'theme');
    this.themeValue = this.doc.createElement('span');
    this.themeValue.className = 'note-menu-value';
    themeItem.insertBefore(this.themeValue, themeItem.lastElementChild);

    const mediaItem = this.createSubmenuItem('Mídia', 'media');
    const captureItem = this.createSubmenuItem('Captura', 'capture');
    const dataItem = this.createSubmenuItem('Dados', 'data');

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

    // Built before the root panel is assembled: the Blocos rows are fields on
    // the menu, because the section reflects the cursor and has to be able to
    // update them without rebuilding anything.
    const blocksPanel = this.buildBlocksPanel();

    // The root holds what the header bar does not. Cor da nota, Tamanho do
    // texto, Cor do texto, Marca-texto, Blocos and Buscar are one click away
    // in the bar, so repeating them here would be two places to find the same
    // panel and two places to keep in step. The panels themselves are still
    // built above and are still the only ones any of those buttons opens.
    rootPanel.append(
      paperTypeItem,
      paperIntensityItem,
      mediaItem,
      captureItem,
      dataItem,
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
    this.panels.set('search', this.buildSearchPanel());
    this.panels.set('media', this.buildMediaPanel());
    this.panels.set('capture', this.buildCapturePanel());
    this.panels.set(
      'captureDelimiter',
      this.buildChoicePanel(
        'Separar capturas',
        'note-menu-capture-delimiter',
        CAPTURE_DELIMITERS,
        () => this.captureDelimiter,
        (delimiter) => this.options.handlers.onSelectCaptureDelimiter(delimiter),
      ),
    );
    this.panels.set('data', this.buildDataPanel());
    this.panels.set('trashConfirm', this.buildTrashConfirmPanel());
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
    this.panels.set('blocks', blocksPanel);
    this.panels.set(
      'codeLanguage',
      this.buildChoicePanel(
        'Linguagem',
        'note-menu-code-language',
        [{ id: '', label: 'Sem linguagem' }, ...CODE_LANGUAGES],
        () => this.block.codeLanguage ?? '',
        (language) => this.options.handlers.onSelectCodeLanguage(language === '' ? null : language),
      ),
    );
    this.panels.set(
      'callout',
      this.buildChoicePanel(
        'Callout',
        'note-menu-callout',
        [{ id: '', label: 'Nenhum (citação)' }, ...CALLOUT_TYPES],
        () => this.block.callout ?? '',
        (type) =>
          this.options.handlers.onSelectCallout(type === '' ? null : (type as CalloutType)),
      ),
    );

    for (const entry of this.panels.values()) {
      entry.element.hidden = true;
      this.root.append(entry.element);
    }
    options.mount.append(this.root);

    // Belt and braces: the popover sits outside the drag region already, but a
    // pointerdown here must never be read as the start of a window drag.
    this.root.addEventListener('pointerdown', (event) => event.stopPropagation());

    this.bindTrigger(options.trigger, 'root');
    for (const [panel, trigger] of Object.entries(options.quickTriggers ?? {})) {
      if (trigger) this.bindTrigger(trigger, panel as MenuPanel);
    }

    this.doc.addEventListener('pointerdown', this.handleDocumentPointerDown, true);
    this.doc.addEventListener('keydown', this.handleKeyDown);

    this.setZoomPercent(100);
    this.setLayerMode('overlay');
    this.setPaper(DEFAULT_PAPER_TYPE, DEFAULT_PAPER_INTENSITY);
    this.setTheme(DEFAULT_THEME);
    this.setAutoPaste(false, DEFAULT_CAPTURE_DELIMITER);
    this.setCollapsed(false);
    this.setBlockState(this.block);
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

  /**
   * Reflects whether this note is the one capturing the clipboard.
   *
   * Both halves come from the host together, because both are its to decide:
   * the target is exclusive across the application, so a note that has just
   * lost it must stop claiming it, and the delimiter is one preference shared
   * by every note's menu.
   */
  public setAutoPaste(active: boolean, delimiter: CaptureDelimiter): void {
    this.autoPaste = active;
    this.captureDelimiter = delimiter;
    this.autoPasteItem.setAttribute('aria-pressed', String(active));
    // Said in words as well as marked, so the state does not depend on
    // noticing a tick or a colour.
    this.autoPasteValue.textContent = active ? 'Ativo' : 'Desativado';
    this.autoPasteItem.setAttribute(
      'aria-label',
      active ? 'AutoPaste ativo. Desativar' : 'AutoPaste desativado. Ativar',
    );
    this.captureDelimiterValue.textContent = delimiterLabel(delimiter);
    this.panels.get('captureDelimiter')?.refresh?.();
  }

  public isAutoPasteActive(): boolean {
    return this.autoPaste;
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

  /**
   * Reflects the block under the cursor.
   *
   * The Blocos rows show what is there rather than a fixed list: the language
   * row only means something inside a code block, and the callout row marks
   * the kind the quote already has.
   */
  public setBlockState(state: BlockState): void {
    this.block = state;
    this.codeBlockItem.setAttribute('aria-checked', String(state.codeBlock));
    this.blockquoteItem.setAttribute('aria-checked', String(state.blockquote));
    this.codeLanguageItem.disabled = !state.codeBlock;
    this.codeLanguageValue.textContent = state.codeBlock
      ? codeLanguageLabel(state.codeLanguage)
      : '—';
    this.calloutValue.textContent = state.callout
      ? CALLOUT_TYPES.find((entry) => entry.id === state.callout)!.label
      : 'Nenhum';
    this.panels.get('codeLanguage')?.refresh?.();
    this.panels.get('callout')?.refresh?.();
  }

  /**
   * Media: putting a picture in the note.
   *
   * Here rather than in the header bar, which is already carrying the menu,
   * six quick actions, the timer and the close cross. Inserting an image is
   * something done occasionally and deliberately, so it costs a menu rather
   * than a permanent control — and the two gestures most people will actually
   * use, pasting and dropping, need no control at all.
   */
  private buildMediaPanel(): PanelEntry {
    const { panel, body } = this.createPanel('Mídia', 'note-menu-media');

    const insert = this.createItem('Inserir imagem…', 'note-menu-item');
    insert.addEventListener('click', () => {
      // The chooser is the host's, so the menu gets out of the way first.
      this.close();
      this.options.handlers.onInsertImage();
    });

    const hint = this.doc.createElement('p');
    hint.className = 'note-menu-hint note-menu-media-hint';
    hint.textContent = 'Também é possível colar uma imagem ou arrastá-la para a nota.';

    body.append(insert, hint);
    return { element: panel };
  }

  /**
   * Capture: the switch, how captures are separated, and what the switch means.
   *
   * It lives in the menu rather than as an eighth button in the bar because
   * turning clipboard observation on is a decision, not a quick action — and
   * because the bar has no room for another permanent control. What the bar
   * gets instead is the indicator, and only while capture is on.
   *
   * The sentence is not a warning dialog and asks for no confirmation. It is
   * one line saying exactly what the switch does, which is what a reader needs
   * before agreeing to have their clipboard watched.
   */
  private buildCapturePanel(): PanelEntry {
    const { panel, body } = this.createPanel('Captura', 'note-menu-capture');

    this.autoPasteItem = this.createItem('AutoPaste', 'note-menu-item note-menu-option');
    this.autoPasteItem.setAttribute('role', 'menuitemcheckbox');
    this.autoPasteItem.setAttribute('aria-pressed', 'false');
    this.autoPasteValue = this.doc.createElement('span');
    this.autoPasteValue.className = 'note-menu-value';
    this.autoPasteItem.append(this.autoPasteValue);
    this.autoPasteItem.addEventListener('click', () => {
      const next = !this.autoPaste;
      // The panel stays open: the reader has just switched clipboard
      // observation on, and the sentence saying what that means should still
      // be in front of them.
      this.options.handlers.onToggleAutoPaste(next);
    });

    const delimiterItem = this.createSubmenuItem('Separar capturas', 'captureDelimiter');
    this.captureDelimiterValue = this.doc.createElement('span');
    this.captureDelimiterValue.className = 'note-menu-value';
    delimiterItem.insertBefore(this.captureDelimiterValue, delimiterItem.lastElementChild);

    const hint = this.doc.createElement('p');
    hint.className = 'note-menu-hint note-menu-capture-hint';
    hint.textContent =
      'Enquanto ativo, todo novo texto copiado será adicionado a esta nota.';

    body.append(this.autoPasteItem, delimiterItem, hint);
    return { element: panel };
  }

  private buildBlocksPanel(): PanelEntry {
    const { panel, body } = this.createPanel('Blocos', 'note-menu-blocks');

    this.codeBlockItem = this.createItem('Bloco de código', 'note-menu-item note-menu-option');
    this.codeBlockItem.setAttribute('role', 'menuitemcheckbox');
    this.codeBlockItem.setAttribute('aria-checked', 'false');
    this.codeBlockItem.addEventListener('click', () => {
      this.close();
      this.options.handlers.onToggleCodeBlock();
    });

    this.codeLanguageItem = this.createSubmenuItem('Linguagem', 'codeLanguage');
    this.codeLanguageValue = this.doc.createElement('span');
    this.codeLanguageValue.className = 'note-menu-value';
    this.codeLanguageItem.insertBefore(
      this.codeLanguageValue,
      this.codeLanguageItem.lastElementChild,
    );

    const calloutItem = this.createSubmenuItem('Callout', 'callout');
    this.calloutValue = this.doc.createElement('span');
    this.calloutValue.className = 'note-menu-value';
    calloutItem.insertBefore(this.calloutValue, calloutItem.lastElementChild);

    this.blockquoteItem = this.createItem('Citação', 'note-menu-item note-menu-option');
    this.blockquoteItem.setAttribute('role', 'menuitemcheckbox');
    this.blockquoteItem.setAttribute('aria-checked', 'false');
    this.blockquoteItem.addEventListener('click', () => {
      this.close();
      this.options.handlers.onToggleBlockquote();
    });

    const commentItem = this.createItem('Comentário', 'note-menu-item');
    commentItem.addEventListener('click', () => {
      this.close();
      this.options.handlers.onInsertComment();
    });

    body.append(
      this.codeBlockItem,
      this.codeLanguageItem,
      calloutItem,
      this.blockquoteItem,
      commentItem,
    );
    return { element: panel };
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
      this.invoker = this.options.trigger;
      this.syncTriggerState();
      this.showPanel('root');
      return;
    }
    this.openAt('root', this.options.trigger);
  }

  private openAt(panel: MenuPanel, invoker: HTMLElement): void {
    this.open = true;
    this.invoker = invoker;
    this.root.hidden = false;
    this.syncTriggerState();
    this.options.handlers.onOpen?.();
    this.showPanel(panel);
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.showPanel('root');
    this.syncTriggerState();
    this.options.handlers.onClose?.();
  }

  private bindTrigger(trigger: HTMLElement, panel: MenuPanel): void {
    this.triggers.set(trigger, panel);
    trigger.setAttribute('aria-haspopup', 'true');
    trigger.setAttribute('aria-expanded', 'false');
    trigger.setAttribute('aria-controls', this.root.id);
    trigger.addEventListener('pointerdown', (event) => event.stopPropagation());
    trigger.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      if (this.open && this.invoker === trigger) {
        this.close();
        return;
      }
      if (this.open) {
        this.invoker = trigger;
        this.syncTriggerState();
        this.showPanel(panel);
        return;
      }
      this.openAt(panel, trigger);
    });
  }

  private syncTriggerState(): void {
    for (const trigger of this.triggers.keys()) {
      trigger.setAttribute('aria-expanded', String(this.open && trigger === this.invoker));
    }
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

  /**
   * The three ways of looking for something, in one place.
   *
   * They are here because a reader who has not learned the chords still has to
   * be able to find them, and each row carries its shortcut so they learn them
   * once. The menu closes first: every one of these opens something that wants
   * the keyboard.
   */
  private buildSearchPanel(): PanelEntry {
    const { panel, body } = this.createPanel('Buscar', 'note-menu-search');

    const rows: Array<[string, string, () => void]> = [
      ['Buscar em todas as notas', 'Ctrl+K', () => this.options.handlers.onOpenGlobalSearch()],
      ['Buscar nesta nota', 'Ctrl+F', () => this.options.handlers.onOpenFind()],
      ['Localizar e substituir', 'Ctrl+H', () => this.options.handlers.onOpenReplace()],
    ];

    for (const [label, shortcut, action] of rows) {
      const item = this.createItem(label, 'note-menu-item');
      item.append(this.createShortcutHint(shortcut));
      item.addEventListener('click', () => {
        this.close();
        action();
      });
      body.append(item);
    }

    return { element: panel };
  }

  /**
   * Everything that acts on the store rather than on the text.
   *
   * One section, three rows, and no second toolbar. Deleting a note lives here
   * rather than beside the close button on purpose: `×` closes a window and
   * has always closed a window, and putting a deletion next to it would make
   * the difference a matter of aim.
   */
  private buildDataPanel(): PanelEntry {
    const { panel, body } = this.createPanel('Dados', 'note-menu-data');

    // The one destructive row, and it does not act — it asks.
    const trash = this.createItem('Mover esta nota para a lixeira', 'note-menu-item');
    trash.addEventListener('click', () => this.showPanel('trashConfirm'));

    const openTrash = this.createItem('Lixeira', 'note-menu-item');
    openTrash.addEventListener('click', () => {
      this.close();
      this.options.handlers.onOpenTrash();
    });

    const backup = this.createItem('Fazer backup agora', 'note-menu-item');
    backup.addEventListener('click', () => {
      this.close();
      this.options.handlers.onCreateBackup();
    });

    body.append(trash, openTrash, backup);
    return { element: panel };
  }

  /**
   * The confirmation, in the menu that asked for it.
   *
   * Not a dialog and not a second surface: the popover already owns the
   * keyboard, already closes on Escape and already closes on a click outside,
   * and all three of those mean "no" here. The wording says the deletion is
   * recoverable, because it is, and a reader who is told only "Excluir?" will
   * make a different decision than the one the software actually offers.
   *
   * Cancel is first, so it is what the panel focuses and what Enter takes.
   */
  private buildTrashConfirmPanel(): PanelEntry {
    const { panel, body } = this.createPanel('Mover para a lixeira', 'note-menu-confirm');

    const question = this.doc.createElement('p');
    question.className = 'note-menu-hint';
    question.textContent =
      'Mover esta nota para a lixeira? Você poderá restaurá-la depois em Dados › Lixeira.';

    const cancel = this.createItem('Cancelar', 'note-menu-item');
    cancel.addEventListener('click', () => this.showPanel('data'));

    const confirm = this.createItem('Mover', 'note-menu-item note-menu-destructive');
    confirm.addEventListener('click', () => {
      this.close();
      this.options.handlers.onTrashNote();
    });

    body.append(question, cancel, confirm);
    return { element: panel };
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
    this.root.scrollTop = 0;
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
    if (
      target &&
      (
        this.root.contains(target) ||
        Array.from(this.triggers.keys()).some((trigger) => trigger.contains(target))
      )
    ) return;
    this.close();
  };

  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (!this.open) return;

    if (event.key === 'Escape') {
      event.preventDefault();
      const invoker = this.invoker;
      this.close();
      invoker.focus?.();
      return;
    }

    if (event.key === 'ArrowLeft' && this.panel !== 'root') {
      event.preventDefault();
      // The confirmation was reached from Dados, so back is Dados. Everything
      // else is one level down from the root panel.
      const back: Partial<Record<MenuPanel, MenuPanel>> = {
        trashConfirm: 'data',
        captureDelimiter: 'capture',
      };
      this.showPanel(back[this.panel] ?? 'root');
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
