import { afterEach, describe, expect, it, vi } from 'vitest';
import { NoteMenu } from '../src/ui/menu.ts';
import { PaperColor } from '../src/bridge/types.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];

function buildHeader() {
  const header = document.createElement('div');
  header.className = 'note-header';

  const left = document.createElement('div');
  left.id = 'note-controls-left';
  const trigger = document.createElement('button');
  trigger.id = 'btn-menu';
  left.append(trigger);

  const dragRegion = document.createElement('div');
  dragRegion.className = 'drag-region';

  header.append(left, dragRegion);
  document.body.append(header);
  return { header, left, trigger, dragRegion };
}

function mountMenu() {
  const { left, trigger, dragRegion } = buildHeader();
  const handlers = {
    onSelectColor: vi.fn(),
    onToggleCollapsed: vi.fn(),
    onSelectTextSize: vi.fn(),
    onSelectTextColor: vi.fn(),
    onSelectHighlight: vi.fn(),
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn(),
    onResetZoom: vi.fn(),
    onSelectLayerMode: vi.fn(),
    onOpen: vi.fn(),
    onClose: vi.fn(),
  };
  const menu = new NoteMenu({ trigger, mount: left, colors: COLORS, handlers });
  return { menu, trigger, dragRegion, handlers };
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

  it('the colour palette is reachable through the menu and applies a colour', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    click(trigger);
    expect(menu.activePanel()).toBe('root');

    click(menu.element.querySelector('[data-panel="paper"]')!);
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
    const { menu, trigger } = mountMenu();
    active = menu;

    menu.setSelectedColor('green');
    click(trigger);
    click(menu.element.querySelector('[data-panel="paper"]')!);

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
    expect(menu.activePanel()).toBe('paper');

    key('ArrowLeft');
    expect(menu.activePanel()).toBe('root');
  });

  it('gives both header buttons an accessible name', () => {
    const { menu, trigger } = mountMenu();
    active = menu;

    trigger.setAttribute('aria-label', 'Configurações da nota');
    expect(trigger.getAttribute('aria-label')).toBeTruthy();
    expect(trigger.getAttribute('aria-haspopup')).toBe('true');
    expect(trigger.getAttribute('aria-controls')).toBe(menu.element.id);

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

  it('offers zoom, layer, text size, text colour and highlight panels', () => {
    const { menu, trigger } = mountMenu();
    active = menu;
    click(trigger);

    const panels = Array.from(
      menu.element.querySelectorAll<HTMLElement>('.note-menu-submenu'),
    ).map((item) => item.dataset.panel);
    expect(panels).toEqual(['paper', 'textSize', 'textColor', 'highlight', 'zoom', 'layer']);
  });

  it('shows the current zoom and steps it from the submenu', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setZoomPercent(130);
    click(trigger);
    expect(menu.element.querySelector('.note-menu-value')!.textContent).toBe('130%');

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

  it('shows the active layer and offers the other one', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setLayerMode('overlay');
    click(trigger);
    expect(menu.element.querySelectorAll('.note-menu-value')[1].textContent).toBe(
      'Sempre no topo',
    );

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
    expect(menu.element.querySelectorAll('.note-menu-value')[1].textContent).toBe(
      'Área de trabalho',
    );
  });

  it('marks the current text size and reports a mixed selection', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    menu.setInlineFormatting({
      textSize: 22,
      textSizeMixed: false,
      textColor: null,
      highlight: null,
    });
    click(trigger);
    click(menu.element.querySelector('[data-panel="textSize"]')!);

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
    click(trigger);
    click(menu.element.querySelector('[data-panel="textSize"]')!);
    expect(menu.element.querySelector<HTMLElement>('.note-menu-hint')!.hidden).toBe(false);
    expect(
      menu.element.querySelectorAll('.note-menu-sizes .note-menu-option[aria-checked="true"]'),
    ).toHaveLength(0);
  });

  it('applies a text colour and a highlight from their palettes', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    click(trigger);
    click(menu.element.querySelector('[data-panel="textColor"]')!);
    const colors = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-colors .note-menu-swatch',
    );
    click(colors[2]);
    expect(handlers.onSelectTextColor).toHaveBeenCalledWith('#DC2626');

    click(trigger);
    click(menu.element.querySelector('[data-panel="highlight"]')!);
    const highlights = menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-highlights .note-menu-swatch',
    );
    click(highlights[1]);
    expect(handlers.onSelectHighlight).toHaveBeenCalledWith('#FDE68A');

    // The first entry of each palette clears the mark.
    click(trigger);
    click(menu.element.querySelector('[data-panel="textColor"]')!);
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
});
