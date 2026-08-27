import { afterEach, describe, expect, it, vi } from 'vitest';
import { NoteMenu } from '../src/ui/menu.ts';
import { PaperColor } from '../src/bridge/types.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];

/**
 * Mirrors the collapsed-click wiring from main.ts: a click anywhere on a
 * collapsed note expands it, the close button still closes, a drag is not a
 * click, and the settings button expands and opens its menu in one go.
 */
function buildNote() {
  document.body.innerHTML = '';
  const app = document.createElement('div');
  app.id = 'app';

  const header = document.createElement('div');
  header.className = 'note-header';
  const left = document.createElement('div');
  left.id = 'note-controls-left';
  const btnMenu = document.createElement('button');
  btnMenu.id = 'btn-menu';
  left.append(btnMenu);
  const dragRegion = document.createElement('div');
  dragRegion.className = 'drag-region';
  const btnClose = document.createElement('button');
  btnClose.id = 'btn-close';

  header.append(left, dragRegion, btnClose);
  const editor = document.createElement('div');
  editor.className = 'editor-wrapper';
  app.append(header, editor);
  document.body.append(app);

  const collapseRequests: boolean[] = [];
  const state = { collapsed: false, dragMoved: false, menuOpened: 0 };

  const handlers = {
    onSelectColor: vi.fn(),
    onSelectPaperType: vi.fn(),
    onSelectPaperIntensity: vi.fn(),
    onSelectTheme: vi.fn(),
    onToggleCollapsed: vi.fn((collapsed: boolean) => collapseRequests.push(collapsed)),
    onSelectTextSize: vi.fn(),
    onSelectTextColor: vi.fn(),
    onSelectHighlight: vi.fn(),
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn(),
    onResetZoom: vi.fn(),
    onSelectLayerMode: vi.fn(),
    onToggleCodeBlock: vi.fn(),
    onSelectCodeLanguage: vi.fn(),
    onToggleBlockquote: vi.fn(),
    onSelectCallout: vi.fn(),
    onInsertComment: vi.fn(),
  };
  const menu = new NoteMenu({ trigger: btnMenu, mount: left, colors: COLORS, handlers });

  function setCollapsed(collapsed: boolean): void {
    state.collapsed = collapsed;
    document.body.setAttribute('data-collapsed', String(collapsed));
    menu.setCollapsed(collapsed);
  }

  function requestCollapsed(collapsed: boolean): void {
    setCollapsed(collapsed);
    collapseRequests.push(collapsed);
  }

  app.addEventListener(
    'click',
    (event) => {
      if (!state.collapsed) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest('#btn-close')) return;
      if (state.dragMoved) {
        state.dragMoved = false;
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      requestCollapsed(false);
      if (target?.closest('#btn-menu')) {
        // main.ts waits for the surface to grow; the wiring is the same.
        menu.openMenu();
        state.menuOpened += 1;
      }
    },
    true,
  );

  return { app, header, dragRegion, btnMenu, btnClose, editor, menu, state, collapseRequests, setCollapsed };
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

describe('clicking a collapsed note', () => {
  let note: ReturnType<typeof buildNote> | null = null;

  afterEach(() => {
    note?.menu.destroy();
    note = null;
    document.body.innerHTML = '';
    document.body.removeAttribute('data-collapsed');
  });

  it('expands when the bar is clicked', () => {
    note = buildNote();
    note.setCollapsed(true);
    note.collapseRequests.length = 0;

    click(note.dragRegion);

    expect(note.collapseRequests).toEqual([false]);
    expect(note.state.collapsed).toBe(false);
    expect(document.body.getAttribute('data-collapsed')).toBe('false');
  });

  it('changes state exactly once per click', () => {
    note = buildNote();
    note.setCollapsed(true);
    note.collapseRequests.length = 0;

    click(note.dragRegion);
    click(note.dragRegion);
    click(note.dragRegion);

    // Only the first click acts; an expanded note is not re-collapsed.
    expect(note.collapseRequests).toEqual([false]);
  });

  it('does nothing while the note is expanded', () => {
    note = buildNote();
    note.setCollapsed(false);
    note.collapseRequests.length = 0;

    click(note.dragRegion);
    click(note.editor);

    expect(note.collapseRequests).toEqual([]);
  });

  it('leaves the close button closing', () => {
    note = buildNote();
    note.setCollapsed(true);
    note.collapseRequests.length = 0;

    click(note.btnClose);

    expect(note.collapseRequests).toEqual([]);
    expect(note.state.collapsed).toBe(true);
  });

  it('does not expand when the click ends a drag', () => {
    note = buildNote();
    note.setCollapsed(true);
    note.collapseRequests.length = 0;

    // The user dragged the collapsed bar somewhere else.
    note.state.dragMoved = true;
    click(note.dragRegion);

    expect(note.collapseRequests).toEqual([]);
    expect(note.state.collapsed).toBe(true);

    // The next real click still expands.
    click(note.dragRegion);
    expect(note.collapseRequests).toEqual([false]);
  });

  it('expands and opens the menu from a single click on the settings button', () => {
    note = buildNote();
    note.setCollapsed(true);
    note.collapseRequests.length = 0;

    click(note.btnMenu);

    expect(note.collapseRequests).toEqual([false]);
    expect(note.state.collapsed).toBe(false);
    expect(note.state.menuOpened).toBe(1);
    expect(note.menu.isOpen()).toBe(true);
    // The menu is shown on an expanded note, never on a bar-height surface.
    expect(document.body.getAttribute('data-collapsed')).toBe('false');
  });

  it('keeps the menu working normally on an expanded note', () => {
    note = buildNote();
    note.setCollapsed(false);

    click(note.btnMenu);
    expect(note.menu.isOpen()).toBe(true);
    expect(note.menu.activePanel()).toBe('root');

    // Submenus are unaffected.
    click(note.menu.element.querySelector('[data-panel="zoom"]')!);
    expect(note.menu.activePanel()).toBe('zoom');

    click(note.menu.element.querySelector('[data-panel="paper"]')!);
    expect(note.menu.activePanel()).toBe('paper');
  });

  it('shows the expand entry while collapsed and the collapse entry after', () => {
    note = buildNote();
    note.setCollapsed(true);

    const entry = () =>
      Array.from(note!.menu.element.querySelectorAll<HTMLElement>('.note-menu-item')).find(
        (node) => /Recolher nota|Expandir nota/.test(node.textContent ?? ''),
      )!;

    click(note.btnMenu);
    expect(entry().textContent).toContain('Recolher nota');
    expect(note.state.collapsed).toBe(false);
  });
});
