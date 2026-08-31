import { afterEach, describe, expect, it, vi } from 'vitest';
import { QUICK_ACTIONS } from '../src/ui/icons.ts';
import { MenuPanel, NoteMenu } from '../src/ui/menu.ts';
import { PaperColor } from '../src/bridge/types.ts';
import { declarationIn } from './support/stylesheet.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];

function buildHeader() {
  const header = document.createElement('div');
  header.className = 'note-header';

  const left = document.createElement('div');
  left.id = 'note-controls-left';
  const trigger = document.createElement('button');
  trigger.id = 'btn-menu';
  left.append(trigger);

  // The six header actions, built from the same list the page is built from.
  const quickTriggers: Partial<Record<MenuPanel, HTMLElement>> = {};
  for (const action of QUICK_ACTIONS) {
    const button = document.createElement('button');
    button.id = action.buttonId;
    button.className = 'icon-btn header-quick-action';
    button.setAttribute('aria-label', action.label);
    left.append(button);
    quickTriggers[action.panel] = button;
  }

  const dragRegion = document.createElement('div');
  dragRegion.className = 'drag-region';

  header.append(left, dragRegion);
  document.body.append(header);
  return {
    header,
    left,
    trigger,
    quickTriggers,
    colorTrigger: quickTriggers.paper as HTMLElement,
    textSizeTrigger: quickTriggers.textSize as HTMLElement,
    textColorTrigger: quickTriggers.textColor as HTMLElement,
    highlightTrigger: quickTriggers.highlight as HTMLElement,
    dragRegion,
  };
}

function mountMenu() {
  const header = buildHeader();
  const { left, trigger, colorTrigger, textSizeTrigger, dragRegion } = header;
  const handlers = {
    onSelectColor: vi.fn(),
    onSelectPaperType: vi.fn(),
    onSelectPaperIntensity: vi.fn(),
    onSelectTheme: vi.fn(),
    onToggleCollapsed: vi.fn(),
    onSelectTextSize: vi.fn(),
    onSelectTextColor: vi.fn(),
    onSelectHighlight: vi.fn(),
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn(),
    onResetZoom: vi.fn(),
    onUiScaleIn: vi.fn(),
    onUiScaleOut: vi.fn(),
    onResetUiScale: vi.fn(),
    onSelectLayerMode: vi.fn(),
    onToggleCodeBlock: vi.fn(),
    onSelectCodeLanguage: vi.fn(),
    onToggleBlockquote: vi.fn(),
    onSelectCallout: vi.fn(),
    onInsertComment: vi.fn(),
    onOpenGlobalSearch: vi.fn(),
    onOpenFind: vi.fn(),
    onOpenReplace: vi.fn(),
    onTrashNote: vi.fn(),
    onOpenTrash: vi.fn(),
    onCreateBackup: vi.fn(),
    onInsertImage: vi.fn(),
    onOpenStudy: vi.fn(),
    onOpenStudyHub: vi.fn(),
    onToggleAutoPaste: vi.fn(),
    onSelectCaptureDelimiter: vi.fn(),
    onOpen: vi.fn(),
    onClose: vi.fn(),
  };
  const menu = new NoteMenu({
    trigger,
    mount: left,
    colors: COLORS,
    handlers,
    quickTriggers: header.quickTriggers,
  });
  return {
    menu,
    trigger,
    colorTrigger,
    textSizeTrigger,
    textColorTrigger: header.textColorTrigger,
    highlightTrigger: header.highlightTrigger,
    dragRegion,
    handlers,
  };
}

/** The value chip shown on a root row, addressed by that row's label. */
function rowValue(menu: NoteMenu, panel: string): string {
  const row = menu.element.querySelector<HTMLElement>(`[data-panel="${panel}"]`);
  return row?.querySelector('.note-menu-value')?.textContent ?? '';
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

function pointerDown(element: Element): PointerEvent {
  const event = new PointerEvent('pointerdown', {
    pointerId: 1,
    button: 0,
    buttons: 1,
    bubbles: true,
    cancelable: true,
  });
  element.dispatchEvent(event);
  return event;
}

function key(name: string): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true });
  document.dispatchEvent(event);
  return event;
}

describe('NoteMenu', () => {
  let active: NoteMenu | null = null;

  afterEach(() => {
    active?.destroy();
    active = null;
    document.body.innerHTML = '';
  });

  it('opens from the header button', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    expect(menu.isOpen()).toBe(false);
    expect(menu.element.hidden).toBe(true);

    click(trigger);

    expect(menu.isOpen()).toBe(true);
    expect(menu.element.hidden).toBe(false);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    expect(handlers.onOpen).toHaveBeenCalledTimes(1);
  });

  it('Escape closes the menu and returns focus to the button', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    click(trigger);
    const event = key('Escape');

    expect(event.defaultPrevented).toBe(true);
    expect(menu.isOpen()).toBe(false);
    expect(menu.element.hidden).toBe(true);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(handlers.onClose).toHaveBeenCalledTimes(1);
    expect(document.activeElement).toBe(trigger);
  });

  it('a click outside closes the menu', () => {
    const { menu, trigger, dragRegion } = mountMenu();
    active = menu;

    click(trigger);
    expect(menu.isOpen()).toBe(true);

    pointerDown(dragRegion);
    expect(menu.isOpen()).toBe(false);
  });

  it('a pointerdown inside the menu neither closes it nor reaches the drag region', () => {
    const { menu, trigger } = mountMenu();
    active = menu;
    click(trigger);

    const event = pointerDown(menu.element.querySelector('.note-menu-item')!);

    expect(menu.isOpen()).toBe(true);
    // Propagation is stopped, so no ancestor drag handler can ever see it.
    expect(event.cancelBubble).toBe(true);
  });

  it('only one popover instance exists, even when opened repeatedly', () => {
    const { menu, trigger } = mountMenu();
    active = menu;

    click(trigger);
    menu.openMenu();
    menu.openMenu();

    expect(document.querySelectorAll('.note-menu')).toHaveLength(1);
    expect(document.querySelectorAll('#note-menu')).toHaveLength(1);
    expect(menu.activePanel()).toBe('root');
  });

  it('the colour palette is reachable from its header action and applies a colour', () => {
    const { menu, colorTrigger, handlers } = mountMenu();
    active = menu;

    click(colorTrigger);
    expect(menu.activePanel()).toBe('paper');

    const swatches = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-paper .note-menu-swatch',
    );
    expect(swatches).toHaveLength(COLORS.length);
    expect(Array.from(swatches).map((s) => s.dataset.color)).toEqual(COLORS);

    click(swatches[3]);

    expect(handlers.onSelectColor).toHaveBeenCalledWith('pink');
    // Choosing a colour dismisses the menu naturally.
    expect(menu.isOpen()).toBe(false);
  });

  it('marks the note current colour in the palette', () => {
    const { menu, colorTrigger } = mountMenu();
    active = menu;

    menu.setSelectedColor('green');
    click(colorTrigger);

    const checked = menu.element.querySelectorAll(
      '.note-menu-paper .note-menu-swatch[aria-checked="true"]',
    );
    expect(checked).toHaveLength(1);
    expect((checked[0] as HTMLElement).dataset.color).toBe('green');
  });

  it('swaps the collapse entry for an expand entry', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    const item = () =>
      Array.from(menu.element.querySelectorAll<HTMLElement>('.note-menu-item')).find((node) =>
        /Recolher nota|Expandir nota/.test(node.textContent ?? ''),
      )!;

    click(trigger);
    expect(item().textContent).toContain('Recolher nota');

    click(item());
    expect(handlers.onToggleCollapsed).toHaveBeenCalledWith(true);
    expect(menu.isOpen()).toBe(false);

    menu.setCollapsed(true);
    click(trigger);
    expect(item().textContent).toContain('Expandir nota');

    click(item());
    expect(handlers.onToggleCollapsed).toHaveBeenLastCalledWith(false);
  });

  it('is navigable with the keyboard', () => {
    const { menu, trigger } = mountMenu();
    active = menu;

    click(trigger);
    const items = menu.element.querySelectorAll<HTMLElement>('.note-menu-item');
    expect(document.activeElement).toBe(items[0]);

    items[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
    expect(document.activeElement).toBe(items[1]);

    items[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true }));
    expect(document.activeElement).toBe(items[0]);

    items[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
    expect(menu.activePanel()).toBe('paperType');

    key('ArrowLeft');
    expect(menu.activePanel()).toBe('root');
  });

  it('gives every menu trigger an accessible popup relationship', () => {
    const { menu, trigger, colorTrigger, textSizeTrigger } = mountMenu();
    active = menu;

    trigger.setAttribute('aria-label', 'Configurações da nota');
    expect(trigger.getAttribute('aria-label')).toBeTruthy();
    expect(trigger.getAttribute('aria-haspopup')).toBe('true');
    expect(trigger.getAttribute('aria-controls')).toBe(menu.element.id);
    for (const quickTrigger of [colorTrigger, textSizeTrigger]) {
      expect(quickTrigger.getAttribute('aria-label')).toBeTruthy();
      expect(quickTrigger.getAttribute('aria-haspopup')).toBe('true');
      expect(quickTrigger.getAttribute('aria-controls')).toBe(menu.element.id);
    }

    click(trigger);
    for (const item of menu.element.querySelectorAll('.note-menu-item')) {
      expect(item.textContent?.trim().length).toBeGreaterThan(0);
    }
    for (const swatch of menu.element.querySelectorAll('.note-menu-swatch')) {
      expect(swatch.getAttribute('aria-label')).toBeTruthy();
    }
    for (const step of menu.element.querySelectorAll('.note-menu-step')) {
      expect(step.getAttribute('aria-label')).toBeTruthy();
    }
  });

  it('keeps every quick action out of the root menu and in the header bar', () => {
    const { menu, trigger } = mountMenu();
    active = menu;
    click(trigger);

    const rootPanel = menu.element.querySelector<HTMLElement>(
      '.note-menu-panel:not([class*=" "])',
    );
    const panels = Array.from(
      rootPanel!.querySelectorAll<HTMLElement>('.note-menu-submenu'),
    ).map((item) => item.dataset.panel);
    // The paper entries sit together, the note's own blocks sit after the
    // inline formatting they belong beside, and the theme sits with the other
    // application-wide switch rather than among the note's own settings.
    // Searching comes after the things a note is made of and before the ones
    // that describe how it is shown: it is what you do *with* a note. Mídia,
    // Captura and Dados sit beside it, because putting a picture in a note,
    // filling it from the clipboard, moving it to the trash, opening the trash
    // and taking a backup are the other things you do with one rather than to
    // it — and studying the cards a note holds is one more of them, which is
    // why Estudo sits with that group rather than among the settings.
    expect(panels).toEqual([
      'paperType',
      'paperIntensity',
      'media',
      'study',
      'capture',
      'data',
      'zoom',
      'interface',
      'theme',
      'layer',
    ]);
    // The six the header bar carries are not repeated here: one function, one
    // place to reach it. Their panels are still built, and are still the only
    // panels those buttons open.
    for (const label of [
      'Cor da nota',
      'Tamanho do texto',
      'Cor do texto',
      'Marca-texto',
      'Blocos',
      'Buscar',
    ]) {
      expect(rootPanel?.textContent).not.toContain(label);
    }
    // What is left is what the bar cannot hold, plus the collapse entry.
    expect(rootPanel?.textContent).toContain('Recolher nota');
  });

  it('shows the current zoom and steps it from the submenu', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setZoomPercent(130);
    click(trigger);
    expect(rowValue(menu, 'zoom')).toBe('130%');

    click(menu.element.querySelector('[data-panel="zoom"]')!);
    expect(menu.activePanel()).toBe('zoom');
    expect(menu.element.querySelector('.note-menu-zoom-value')!.textContent).toBe('130%');

    const steps = menu.element.querySelectorAll<HTMLElement>('.note-menu-step');
    click(steps[0]);
    click(steps[1]);
    expect(handlers.onZoomOut).toHaveBeenCalledTimes(1);
    expect(handlers.onZoomIn).toHaveBeenCalledTimes(1);

    const reset = Array.from(menu.element.querySelectorAll<HTMLElement>('.note-menu-item')).find(
      (node) => node.textContent?.includes('Restaurar 100%'),
    )!;
    click(reset);
    expect(handlers.onResetZoom).toHaveBeenCalledTimes(1);
  });

  it('keeps interface scale separate from note zoom and exposes its limits', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setZoomPercent(250);
    menu.setUiScalePercent(140);
    click(trigger);
    expect(rowValue(menu, 'zoom')).toBe('250%');
    expect(rowValue(menu, 'interface')).toBe('140%');

    click(menu.element.querySelector('[data-panel="interface"]')!);
    expect(menu.element.querySelector('.note-menu-interface .note-menu-zoom-value')?.textContent)
      .toBe('140%');
    const steps = menu.element.querySelectorAll<HTMLButtonElement>(
      '.note-menu-interface .note-menu-step',
    );
    click(steps[0]);
    click(steps[1]);
    expect(handlers.onUiScaleOut).toHaveBeenCalledTimes(1);
    expect(handlers.onUiScaleIn).toHaveBeenCalledTimes(1);

    menu.setUiScalePercent(160);
    expect(steps[0].disabled).toBe(false);
    expect(steps[1].disabled).toBe(true);
  });

  it('shows the active layer and offers the other one', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setLayerMode('overlay');
    click(trigger);
    expect(rowValue(menu, 'layer')).toBe('Sempre no topo');

    click(menu.element.querySelector('[data-panel="layer"]')!);
    const options = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-layer .note-menu-option',
    );
    expect(options).toHaveLength(2);
    expect(options[0].getAttribute('aria-checked')).toBe('true');
    expect(options[1].getAttribute('aria-checked')).toBe('false');
    // The shortcut is shown, so the feature is not shortcut-only knowledge.
    expect(options[0].textContent).toContain('Ctrl+Shift+Space');

    click(options[1]);
    expect(handlers.onSelectLayerMode).toHaveBeenCalledWith('desktop');

    menu.setLayerMode('desktop');
    click(trigger);
    expect(rowValue(menu, 'layer')).toBe('Área de trabalho');
  });

  it('offers the five papers and marks the note’s own', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setPaper('dotted', 'normal');
    click(trigger);
    expect(rowValue(menu, 'paperType')).toBe('Pontilhado');

    click(menu.element.querySelector('[data-panel="paperType"]')!);
    const options = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-paper-type .note-menu-option',
    );
    expect(Array.from(options).map((option) => option.textContent?.trim())).toEqual([
      'Liso',
      'Pautado',
      'Pontilhado',
      'Quadriculado pequeno',
      'Quadriculado grande',
    ]);
    expect(
      Array.from(options).map((option) => option.getAttribute('aria-checked')),
    ).toEqual(['false', 'false', 'true', 'false', 'false']);

    click(options[1]);
    expect(handlers.onSelectPaperType).toHaveBeenCalledWith('lined');
    // Choosing a paper closes the menu, like every other choice in it.
    expect(menu.isOpen()).toBe(false);
  });

  it('offers the three intensities and marks the current one', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setPaper('lined', 'strong');
    click(trigger);
    expect(rowValue(menu, 'paperIntensity')).toBe('Forte');

    click(menu.element.querySelector('[data-panel="paperIntensity"]')!);
    const options = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-paper-intensity .note-menu-option',
    );
    expect(Array.from(options).map((option) => option.textContent?.trim())).toEqual([
      'Suave',
      'Normal',
      'Forte',
    ]);
    expect(options[2].getAttribute('aria-checked')).toBe('true');

    click(options[0]);
    expect(handlers.onSelectPaperIntensity).toHaveBeenCalledWith('subtle');
  });

  it('keeps the intensity readable on the root row for plain paper', () => {
    const { menu, trigger } = mountMenu();
    active = menu;

    menu.setPaper('blank', 'subtle');
    click(trigger);
    // Plain paper has no pattern for it to act on, but the choice is still
    // the note's and is still shown.
    expect(rowValue(menu, 'paperType')).toBe('Liso');
    expect(rowValue(menu, 'paperIntensity')).toBe('Suave');
  });

  it('offers the three themes and marks the active one', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setTheme('dark');
    click(trigger);
    expect(rowValue(menu, 'theme')).toBe('Escuro');

    click(menu.element.querySelector('[data-panel="theme"]')!);
    const options = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-theme .note-menu-option',
    );
    expect(Array.from(options).map((option) => option.textContent?.trim())).toEqual([
      'Sistema',
      'Claro',
      'Escuro',
    ]);
    expect(
      Array.from(options).map((option) => option.getAttribute('aria-checked')),
    ).toEqual(['false', 'false', 'true']);

    click(options[0]);
    expect(handlers.onSelectTheme).toHaveBeenCalledWith('system');
  });

  it('starts on plain paper following the system, before a note loads', () => {
    const { menu, trigger } = mountMenu();
    active = menu;
    click(trigger);

    expect(rowValue(menu, 'paperType')).toBe('Liso');
    expect(rowValue(menu, 'paperIntensity')).toBe('Normal');
    expect(rowValue(menu, 'theme')).toBe('Sistema');
  });

  it('leaves the text colour swatch ground to the stylesheet', () => {
    const { menu, textColorTrigger } = mountMenu();
    active = menu;
    click(textColorTrigger);

    const swatches = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-colors .note-menu-swatch:not(.note-menu-swatch-none)',
    );
    expect(swatches.length).toBeGreaterThan(0);
    for (const swatch of swatches) {
      // Only the letter is coloured inline. An inline background would beat
      // the rule that keeps the sample on a pale ground in the dark theme.
      expect(swatch.style.color).not.toBe('');
      expect(swatch.style.backgroundColor).toBe('');
    }
  });

  it('marks the current text size and reports a mixed selection', () => {
    const { menu, textSizeTrigger, handlers } = mountMenu();
    active = menu;

    menu.setInlineFormatting({
      textSize: 22,
      textSizeMixed: false,
      textColor: null,
      highlight: null,
    });
    click(textSizeTrigger);

    const options = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-sizes .note-menu-option',
    );
    const checked = Array.from(options).filter(
      (node) => node.getAttribute('aria-checked') === 'true',
    );
    expect(checked).toHaveLength(1);
    expect(checked[0].textContent).toContain('22');

    click(options[0]);
    expect(handlers.onSelectTextSize).toHaveBeenCalledWith(null);

    menu.setInlineFormatting({
      textSize: null,
      textSizeMixed: true,
      textColor: null,
      highlight: null,
    });
    click(textSizeTrigger);
    expect(menu.element.querySelector<HTMLElement>('.note-menu-hint')!.hidden).toBe(false);
    expect(
      menu.element.querySelectorAll('.note-menu-sizes .note-menu-option[aria-checked="true"]'),
    ).toHaveLength(0);
  });

  it('applies a text colour and a highlight from their palettes', () => {
    const { menu, textColorTrigger, highlightTrigger, handlers } = mountMenu();
    active = menu;

    click(textColorTrigger);
    const colors = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-colors .note-menu-swatch',
    );
    click(colors[2]);
    expect(handlers.onSelectTextColor).toHaveBeenCalledWith('#DC2626');

    click(highlightTrigger);
    const highlights = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-highlights .note-menu-swatch',
    );
    click(highlights[1]);
    expect(handlers.onSelectHighlight).toHaveBeenCalledWith('#FDE68A');

    // The first entry of each palette clears the mark.
    click(textColorTrigger);
    click(menu.element.querySelectorAll<HTMLElement>('.note-menu-colors .note-menu-swatch')[0]);
    expect(handlers.onSelectTextColor).toHaveBeenLastCalledWith(null);
  });

  it('shows the collapse shortcut alongside the entry', () => {
    const { menu, trigger } = mountMenu();
    active = menu;
    click(trigger);

    const collapse = Array.from(
      menu.element.querySelectorAll<HTMLElement>('.note-menu-item'),
    ).find((node) => /Recolher nota/.test(node.textContent ?? ''))!;
    expect(collapse.textContent).toContain('Ctrl+Shift+M');
  });

  it('caps only a menu that exceeds the WebView and leaves native vertical scrolling enabled', () => {
    const { menu, trigger } = mountMenu();
    active = menu;

    expect(declarationIn('.note-menu', 'max-height')).toBe(
      'calc(100vh - var(--note-header-height) - 8px)',
    );
    expect(declarationIn('.note-menu', 'overflow-y')).toBe('auto');
    expect(declarationIn('.note-menu', 'overflow-x')).toBe('hidden');

    click(trigger);
    const wheel = new WheelEvent('wheel', { deltaY: 80, bubbles: true, cancelable: true });
    menu.element.dispatchEvent(wheel);
    expect(wheel.defaultPrevented).toBe(false);
    expect(menu.isOpen()).toBe(true);
  });

  it('opens both quick panels through the same handlers without a duplicate popover', () => {
    const { menu, colorTrigger, textSizeTrigger, handlers } = mountMenu();
    active = menu;

    click(colorTrigger);
    expect(menu.activePanel()).toBe('paper');
    expect(colorTrigger.getAttribute('aria-expanded')).toBe('true');
    expect(handlers.onSelectColor).not.toHaveBeenCalled();

    click(textSizeTrigger);
    expect(menu.activePanel()).toBe('textSize');
    expect(colorTrigger.getAttribute('aria-expanded')).toBe('false');
    expect(textSizeTrigger.getAttribute('aria-expanded')).toBe('true');
    expect(document.querySelectorAll('.note-menu')).toHaveLength(1);
    expect(handlers.onSelectTextSize).not.toHaveBeenCalled();

    const size = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-sizes .note-menu-option',
    )[4];
    click(size);
    expect(handlers.onSelectTextSize).toHaveBeenCalledWith(18);
  });
});
