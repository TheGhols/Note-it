import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TrashEntry } from '../src/bridge/types.ts';
import { formatNoteTimestamp } from '../src/format/datetime.ts';
import { NoteMenu } from '../src/ui/menu.ts';
import { NoteStatus, STATUS_TIMEOUT_MS } from '../src/ui/status.ts';
import { TrashPanel } from '../src/ui/trashPanel.ts';
import { PaperColor } from '../src/bridge/types.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];

function menuHandlers() {
  return {
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
}

function mountMenu() {
  const left = document.createElement('div');
  left.id = 'note-controls-left';
  const trigger = document.createElement('button');
  trigger.id = 'btn-menu';
  left.append(trigger);
  document.body.append(left);

  const handlers = menuHandlers();
  const menu = new NoteMenu({ trigger, mount: left, colors: COLORS, handlers });
  return { menu, trigger, handlers };
}

function click(element: Element | null | undefined): void {
  element?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

/** The row of the open panel whose label reads exactly `label`. */
function row(menu: NoteMenu, label: string): HTMLButtonElement {
  const found = Array.from(menu.element.querySelectorAll<HTMLButtonElement>('button')).find(
    (button) => button.firstChild?.textContent === label,
  );
  if (!found) throw new Error(`no menu row labelled "${label}"`);
  return found;
}

let menu: NoteMenu | null = null;
let panel: TrashPanel | null = null;
let status: NoteStatus | null = null;

afterEach(() => {
  menu?.destroy();
  panel?.destroy();
  status?.destroy();
  menu = null;
  panel = null;
  status = null;
  document.body.innerHTML = '';
  vi.useRealTimers();
});

describe('the Dados section of the note menu', () => {
  it('offers the trash, the trash listing and a backup, and nothing else', () => {
    const mounted = mountMenu();
    menu = mounted.menu;
    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));

    expect(menu.activePanel()).toBe('data');
    const labels = Array.from(
      menu.element.querySelectorAll<HTMLElement>('.note-menu-data button'),
    ).map((button) => button.firstChild?.textContent);
    expect(labels).toEqual([
      'Mover esta nota para a lixeira',
      'Lixeira',
      'Fazer backup agora',
    ]);
  });

  it('never deletes on the first click: it asks, and says the deletion is recoverable', () => {
    const mounted = mountMenu();
    menu = mounted.menu;
    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));
    click(row(menu, 'Mover esta nota para a lixeira'));

    expect(menu.activePanel()).toBe('trashConfirm');
    expect(mounted.handlers.onTrashNote).not.toHaveBeenCalled();
    expect(menu.isOpen()).toBe(true);

    const question = menu.element.querySelector('.note-menu-confirm .note-menu-hint');
    // The reader is told it can be undone. "Excluir?" would describe software
    // this is not.
    expect(question?.textContent).toContain('restaurá-la');
    expect(question?.textContent).toContain('Lixeira');
  });

  it('gives the toolbar shortcut the same confirmation and no deletion authority', () => {
    const mounted = mountMenu();
    menu = mounted.menu;
    const toolbarTrash = document.createElement('button');
    document.body.append(toolbarTrash);

    menu.openTrashConfirmation(toolbarTrash);
    expect(menu.activePanel()).toBe('trashConfirm');
    expect(mounted.handlers.onTrashNote).not.toHaveBeenCalled();
    expect(document.activeElement?.textContent).toBe('Cancelar');

    click(row(menu, 'Mover'));
    expect(mounted.handlers.onTrashNote).toHaveBeenCalledTimes(1);
  });

  it('focuses Cancelar, so the key already under the finger is the safe one', () => {
    const mounted = mountMenu();
    menu = mounted.menu;
    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));
    click(row(menu, 'Mover esta nota para a lixeira'));

    const buttons = Array.from(
      menu.element.querySelectorAll<HTMLElement>('.note-menu-confirm button'),
    ).map((button) => button.firstChild?.textContent);
    expect(buttons).toEqual(['Cancelar', 'Mover']);
    expect(document.activeElement?.firstChild?.textContent).toBe('Cancelar');
  });

  it('cancels back to Dados without deleting anything', () => {
    const mounted = mountMenu();
    menu = mounted.menu;
    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));
    click(row(menu, 'Mover esta nota para a lixeira'));
    click(row(menu, 'Cancelar'));

    expect(menu.activePanel()).toBe('data');
    expect(mounted.handlers.onTrashNote).not.toHaveBeenCalled();
  });

  it('treats Escape and a click outside as no', () => {
    for (const dismiss of ['escape', 'outside'] as const) {
      const mounted = mountMenu();
      menu = mounted.menu;
      click(mounted.trigger);
      click(menu.element.querySelector('[data-panel="data"]'));
      click(row(menu, 'Mover esta nota para a lixeira'));

      if (dismiss === 'escape') {
        document.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
        );
      } else {
        document.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      }

      expect(menu.isOpen(), dismiss).toBe(false);
      expect(mounted.handlers.onTrashNote, dismiss).not.toHaveBeenCalled();
      menu.destroy();
      menu = null;
      document.body.innerHTML = '';
    }
  });

  it('goes back to Dados rather than to the root when the arrow says back', () => {
    const mounted = mountMenu();
    menu = mounted.menu;
    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));
    click(row(menu, 'Mover esta nota para a lixeira'));

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }),
    );
    expect(menu.activePanel()).toBe('data');
    expect(mounted.handlers.onTrashNote).not.toHaveBeenCalled();
  });

  it('deletes only once Mover has been chosen, and closes the menu first', () => {
    const mounted = mountMenu();
    menu = mounted.menu;
    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));
    click(row(menu, 'Mover esta nota para a lixeira'));
    click(row(menu, 'Mover'));

    expect(mounted.handlers.onTrashNote).toHaveBeenCalledTimes(1);
    expect(menu.isOpen()).toBe(false);
  });

  it('opens the trash and takes a backup from their own rows', () => {
    const mounted = mountMenu();
    menu = mounted.menu;

    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));
    click(row(menu, 'Lixeira'));
    expect(mounted.handlers.onOpenTrash).toHaveBeenCalledTimes(1);
    expect(menu.isOpen()).toBe(false);

    click(mounted.trigger);
    click(menu.element.querySelector('[data-panel="data"]'));
    click(row(menu, 'Fazer backup agora'));
    expect(mounted.handlers.onCreateBackup).toHaveBeenCalledTimes(1);
    expect(menu.isOpen()).toBe(false);
  });
});

describe('the trash panel', () => {
  const handlers = {
    onList: vi.fn(),
    onRestore: vi.fn(),
    onClose: vi.fn(),
  };
  let mount: HTMLElement;

  function entry(noteId: string, label: string, snippet = '', deletedAt: string | null = null): TrashEntry {
    return { noteId, label, snippet, deletedAt };
  }

  function mountPanel(): TrashPanel {
    mount = document.createElement('div');
    document.body.append(mount);
    panel = new TrashPanel({ mount, handlers });
    return panel;
  }

  function pendingRequestId(): number {
    const calls = handlers.onList.mock.calls;
    return calls[calls.length - 1][0] as number;
  }

  function rows(): HTMLElement[] {
    return Array.from(mount.querySelectorAll<HTMLElement>('.note-trash-row'));
  }

  beforeEach(() => {
    handlers.onList.mockClear();
    handlers.onRestore.mockClear();
    handlers.onClose.mockClear();
  });

  it('asks the host for the list when it opens, and reads nothing itself', () => {
    const trash = mountPanel();
    trash.openPanel();
    expect(handlers.onList).toHaveBeenCalledTimes(1);
    expect(trash.isOpen()).toBe(true);
  });

  it('says so when there is nothing to recover', () => {
    const trash = mountPanel();
    trash.openPanel();
    trash.showEntries(pendingRequestId(), []);
    expect(mount.querySelector('.note-trash-status')?.textContent).toBe('a lixeira está vazia');
    expect(rows()).toHaveLength(0);
  });

  it('shows the label, the opening and the date it was deleted', () => {
    const trash = mountPanel();
    trash.openPanel();
    trash.showEntries(pendingRequestId(), [
      entry('11111111-1111-4111-8111-111111111111', 'Uma nota', 'MARCADOR-8391', '2026-08-29T09:30:00Z'),
    ]);

    const [only] = rows();
    expect(only.querySelector('.note-trash-label')?.textContent).toBe('Uma nota');
    expect(only.querySelector('.note-trash-snippet')?.textContent).toBe('MARCADOR-8391');
    expect(only.querySelector('.note-trash-date')?.textContent).toBe(
      formatNoteTimestamp('2026-08-29T09:30:00Z'),
    );
  });

  it('reports an unknown deletion date as unknown rather than inventing one', () => {
    const trash = mountPanel();
    trash.openPanel();
    trash.showEntries(pendingRequestId(), [
      entry('11111111-1111-4111-8111-111111111111', 'Sem data', '', null),
    ]);
    expect(rows()[0].querySelector('.note-trash-date')?.textContent).toBe('—');
  });

  it('renders a note as text, never as markup', () => {
    const trash = mountPanel();
    trash.openPanel();
    trash.showEntries(pendingRequestId(), [
      entry(
        '11111111-1111-4111-8111-111111111111',
        '<img src=x onerror="alert(1)">',
        '<script>alert(1)</script>',
      ),
    ]);

    const [only] = rows();
    expect(only.querySelector('.note-trash-label')?.textContent).toBe(
      '<img src=x onerror="alert(1)">',
    );
    expect(only.querySelector('img')).toBeNull();
    expect(only.querySelector('script')).toBeNull();
  });

  it('restores by identifier, never by label', () => {
    const trash = mountPanel();
    trash.openPanel();
    trash.showEntries(pendingRequestId(), [
      entry('11111111-1111-4111-8111-111111111111', 'Nome repetido'),
      entry('22222222-2222-4222-8222-222222222222', 'Nome repetido'),
    ]);

    click(rows()[1].querySelector('.note-trash-restore'));
    expect(handlers.onRestore).toHaveBeenCalledWith('22222222-2222-4222-8222-222222222222');
  });

  it('drops an answer to a question it is no longer asking', () => {
    const trash = mountPanel();
    trash.openPanel();
    const stale = pendingRequestId();
    trash.refresh();

    trash.showEntries(stale, [entry('11111111-1111-4111-8111-111111111111', 'Antiga')]);
    expect(rows()).toHaveLength(0);

    trash.showEntries(pendingRequestId(), [
      entry('22222222-2222-4222-8222-222222222222', 'Atual'),
    ]);
    expect(rows()[0].querySelector('.note-trash-label')?.textContent).toBe('Atual');
  });

  it('walks the list with the arrows and restores the selected one with Enter', () => {
    const trash = mountPanel();
    trash.openPanel();
    trash.showEntries(pendingRequestId(), [
      entry('11111111-1111-4111-8111-111111111111', 'Primeira'),
      entry('22222222-2222-4222-8222-222222222222', 'Segunda'),
    ]);

    const press = (key: string): boolean => {
      const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
      trash.element().dispatchEvent(event);
      return !event.defaultPrevented;
    };

    expect(press('ArrowDown')).toBe(false);
    expect(rows()[1].className).toContain('selected');
    expect(press('Enter')).toBe(false);
    expect(handlers.onRestore).toHaveBeenCalledWith('22222222-2222-4222-8222-222222222222');
  });

  it('closes on Escape and hands the keyboard back, without claiming the layer chord', () => {
    const trash = mountPanel();
    trash.openPanel();

    const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    trash.element().dispatchEvent(escape);
    expect(escape.defaultPrevented).toBe(true);
    expect(trash.isOpen()).toBe(false);
    expect(handlers.onClose).toHaveBeenCalledTimes(1);

    trash.openPanel();
    const layer = new KeyboardEvent('keydown', {
      key: ' ',
      ctrlKey: true,
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    });
    trash.element().dispatchEvent(layer);
    expect(layer.defaultPrevented).toBe(false);
  });

  it('asks again after a restore, so what came back stops being listed', () => {
    const trash = mountPanel();
    trash.openPanel();
    expect(handlers.onList).toHaveBeenCalledTimes(1);
    trash.refresh();
    expect(handlers.onList).toHaveBeenCalledTimes(2);
  });

  it('ignores an answer that arrives after it was closed', () => {
    const trash = mountPanel();
    trash.openPanel();
    const requestId = pendingRequestId();
    trash.close();
    trash.showEntries(requestId, [entry('11111111-1111-4111-8111-111111111111', 'Tarde demais')]);
    expect(rows()).toHaveLength(0);
  });
});

describe('the status line', () => {
  it('shows the sentence the host sent, and takes nothing from the reader', () => {
    vi.useFakeTimers();
    const mount = document.createElement('div');
    document.body.append(mount);
    status = new NoteStatus({ mount });

    status.show('Backup concluído.');
    expect(status.isVisible()).toBe(true);
    expect(status.element().textContent).toBe('Backup concluído.');
    expect(status.element().getAttribute('role')).toBe('status');
    // Nothing to dismiss, and nothing focusable to trap the keyboard in.
    expect(status.element().querySelector('button')).toBeNull();

    vi.advanceTimersByTime(STATUS_TIMEOUT_MS);
    expect(status.isVisible()).toBe(false);
  });

  it('marks a failure without composing its own wording', () => {
    vi.useFakeTimers();
    const mount = document.createElement('div');
    document.body.append(mount);
    status = new NoteStatus({ mount });

    status.show('Não foi possível criar o backup. Nada foi alterado.', false);
    expect(status.element().dataset.ok).toBe('false');
    expect(status.element().textContent).toBe(
      'Não foi possível criar o backup. Nada foi alterado.',
    );
  });

  it('replaces a message rather than stacking them', () => {
    vi.useFakeTimers();
    const mount = document.createElement('div');
    document.body.append(mount);
    status = new NoteStatus({ mount });

    status.show('Backup concluído.');
    status.show('Nota restaurada.');
    expect(mount.querySelectorAll('.note-status')).toHaveLength(1);
    expect(status.element().textContent).toBe('Nota restaurada.');

    vi.advanceTimersByTime(STATUS_TIMEOUT_MS);
    expect(status.isVisible()).toBe(false);
  });
});
