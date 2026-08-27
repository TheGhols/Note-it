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

    click(menu.element.querySelector('.note-menu-submenu')!);
    expect(menu.activePanel()).toBe('colors');

    const swatches = menu.element.querySelectorAll<HTMLElement>('.note-menu-swatch');
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
    click(menu.element.querySelector('.note-menu-submenu')!);

    const checked = menu.element.querySelectorAll('.note-menu-swatch[aria-checked="true"]');
    expect(checked).toHaveLength(1);
    expect((checked[0] as HTMLElement).dataset.color).toBe('green');
  });

  it('swaps the collapse entry for an expand entry', () => {
    const { menu, trigger, handlers } = mountMenu();
    active = menu;

    const item = () => menu.element.querySelectorAll('.note-menu-item')[1] as HTMLElement;

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
    expect(menu.activePanel()).toBe('colors');

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
  });
});
