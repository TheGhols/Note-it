import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SearchResult } from '../src/bridge/types.ts';
import { SEARCH_DEBOUNCE_MS, SearchPalette } from '../src/ui/searchPalette.ts';

let palette: SearchPalette | null = null;
let mount: HTMLElement;

const handlers = {
  onQuery: vi.fn(),
  onOpen: vi.fn(),
  onClose: vi.fn(),
};

function result(
  noteId: string,
  label: string,
  snippet = '',
  matchCount = 1,
  matchedText = '',
): SearchResult {
  return { noteId, label, snippet, matchCount, matchedText };
}

function mountPalette(): SearchPalette {
  mount = document.createElement('div');
  document.body.append(mount);
  palette = new SearchPalette({ mount, handlers });
  return palette;
}

function input(): HTMLInputElement {
  return mount.querySelector<HTMLInputElement>('.note-search-input')!;
}

function rows(): HTMLElement[] {
  return Array.from(mount.querySelectorAll<HTMLElement>('.note-search-row'));
}

function selectedRow(): HTMLElement | null {
  return mount.querySelector<HTMLElement>('.note-search-row.selected');
}

/** A key the palette may or may not claim; reports whether it escaped. */
function press(key: string, options: KeyboardEventInit = {}): boolean {
  const event = new KeyboardEvent('keydown', {
    key,
    bubbles: true,
    cancelable: true,
    ...options,
  });
  input().dispatchEvent(event);
  return !event.defaultPrevented;
}

function type(text: string): void {
  input().value = text;
  input().dispatchEvent(new Event('input', { bubbles: true }));
}

beforeEach(() => {
  vi.useFakeTimers();
  handlers.onQuery.mockClear();
  handlers.onOpen.mockClear();
  handlers.onClose.mockClear();
});

afterEach(() => {
  palette?.destroy();
  palette = null;
  vi.useRealTimers();
  document.body.innerHTML = '';
});

describe('opening and closing', () => {
  it('opens hidden and shows itself when asked', () => {
    const p = mountPalette();
    expect(p.element().hidden).toBe(true);
    expect(p.isOpen()).toBe(false);

    p.openPalette();
    expect(p.element().hidden).toBe(false);
    expect(p.isOpen()).toBe(true);
    expect(document.activeElement).toBe(input());
  });

  it('asks for the recent notes the moment it opens, with no query', () => {
    mountPalette().openPalette();
    expect(handlers.onQuery).toHaveBeenCalledTimes(1);
    expect(handlers.onQuery.mock.calls[0][1]).toBe('');
  });

  it('closes on Escape and hands the keyboard back', () => {
    const p = mountPalette();
    p.openPalette();
    expect(press('Escape')).toBe(false);
    expect(p.isOpen()).toBe(false);
    expect(p.element().hidden).toBe(true);
    expect(handlers.onClose).toHaveBeenCalledTimes(1);
  });

  it('closing twice reports closing once', () => {
    const p = mountPalette();
    p.openPalette();
    p.close();
    p.close();
    expect(handlers.onClose).toHaveBeenCalledTimes(1);
  });
});

describe('typing', () => {
  it('waits before asking, and asks once for a word typed quickly', () => {
    const p = mountPalette();
    p.openPalette();
    handlers.onQuery.mockClear();

    type('b');
    type('bi');
    type('bio');
    expect(handlers.onQuery).not.toHaveBeenCalled();

    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS);
    expect(handlers.onQuery).toHaveBeenCalledTimes(1);
    expect(handlers.onQuery.mock.calls[0][1]).toBe('bio');
  });

  it('gives every request a number of its own', () => {
    const p = mountPalette();
    p.openPalette();
    type('a');
    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS);
    type('ab');
    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS);

    const ids = handlers.onQuery.mock.calls.map((call) => call[0] as number);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toEqual([...ids].sort((left, right) => left - right));
  });
});

describe('an answer to an older question is dropped', () => {
  it('never lets a slow reply overwrite a newer one', () => {
    const p = mountPalette();
    p.openPalette();
    type('bio');
    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS);
    type('biopsia');
    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS);

    const [first, second] = handlers.onQuery.mock.calls.slice(-2).map((call) => call[0] as number);

    // The newer answer arrives first...
    p.showResults(second, [result('n-2', 'resposta nova')]);
    expect(rows()).toHaveLength(1);
    expect(rows()[0].textContent).toContain('resposta nova');

    // ...and the slow one for `bio` must not replace it.
    p.showResults(first, [result('n-1', 'resposta antiga')]);
    expect(rows()[0].textContent).toContain('resposta nova');
  });

  it('accepts results while it is open and ignores them once closed', () => {
    const p = mountPalette();
    p.openPalette();
    p.showResults(1, [result('n-1', 'alguma nota')]);
    expect(rows()).toHaveLength(1);

    p.close();
    p.showResults(2, [result('n-2', 'tarde demais')]);
    expect(rows()).toHaveLength(0);
  });
});

describe('moving through the results', () => {
  beforeEach(() => {
    const p = mountPalette();
    p.openPalette();
    p.showResults(1, [
      result('n-1', 'primeira'),
      result('n-2', 'segunda'),
      result('n-3', 'terceira'),
    ]);
  });

  it('starts on the first', () => {
    expect(selectedRow()?.dataset.noteId).toBe('n-1');
  });

  it('steps down and up, wrapping at both ends', () => {
    press('ArrowDown');
    expect(selectedRow()?.dataset.noteId).toBe('n-2');
    press('ArrowDown');
    press('ArrowDown');
    expect(selectedRow()?.dataset.noteId).toBe('n-1');
    press('ArrowUp');
    expect(selectedRow()?.dataset.noteId).toBe('n-3');
  });

  it('opens the selected note by identifier, never by label', () => {
    input().value = 'alvo';
    press('ArrowDown');
    press('Enter');
    expect(handlers.onOpen).toHaveBeenCalledWith('n-2', 'alvo');
  });

  it('closes itself when a result is chosen', () => {
    press('Enter');
    expect(palette!.isOpen()).toBe(false);
    expect(handlers.onClose).toHaveBeenCalled();
  });

  it('asks the note to look for the spelling that matched, not what was typed', () => {
    // `biopsia` found `Biópsia`; the editor's own find does not fold accents,
    // so the note is told the spelling it actually contains.
    palette!.showResults(2, [result('n-9', 'Biópsia hepática', '', 2, 'Biópsia')]);
    input().value = 'biopsia';
    press('Enter');
    expect(handlers.onOpen).toHaveBeenCalledWith('n-9', 'Biópsia');
  });

  it('falls back to what was typed when nothing was matched', () => {
    palette!.showResults(2, [result('n-9', 'recente', '', 0, '')]);
    input().value = 'algo';
    press('Enter');
    expect(handlers.onOpen).toHaveBeenCalledWith('n-9', 'algo');
  });

  it('tells two notes with the same first line apart', () => {
    palette!.showResults(2, [result('n-a', 'Compras'), result('n-b', 'Compras')]);
    press('ArrowDown');
    press('Enter');
    expect(handlers.onOpen).toHaveBeenCalledWith('n-b', '');
  });
});

describe('keys never reach the note behind it', () => {
  it('claims the keys it handles', () => {
    const p = mountPalette();
    p.openPalette();
    p.showResults(1, [result('n-1', 'uma nota')]);

    for (const key of ['ArrowDown', 'ArrowUp', 'Enter', 'Escape']) {
      p.openPalette();
      expect(press(key), `${key} escaped to the editor`).toBe(false);
    }
  });

  it('leaves the layer shortcut alone, so it still belongs to everything', () => {
    const p = mountPalette();
    p.openPalette();
    // `Ctrl+Shift+Space` is the application's, not the palette's: it passes
    // straight through and never becomes a space in the search field.
    const escaped = press(' ', { ctrlKey: true, shiftKey: true });
    expect(escaped).toBe(true);
    expect(input().value).toBe('');
  });

  it('ignores keys arriving mid-composition, so pt-BR dead keys survive', () => {
    const p = mountPalette();
    p.openPalette();
    p.showResults(1, [result('n-1', 'uma nota'), result('n-2', 'outra')]);
    expect(press('ArrowDown', { isComposing: true } as KeyboardEventInit)).toBe(true);
    expect(selectedRow()?.dataset.noteId).toBe('n-1');
  });
});

describe('what a result looks like', () => {
  it('shows the label, the snippet and a count only when there are several', () => {
    const p = mountPalette();
    p.openPalette();
    p.showResults(1, [
      result('n-1', 'Biópsia hepática', '…a biópsia transjugular…', 4),
      result('n-2', 'Uma vez só', 'trecho', 1),
    ]);

    expect(rows()[0].querySelector('.note-search-label')?.textContent).toBe('Biópsia hepática');
    expect(rows()[0].querySelector('.note-search-snippet')?.textContent).toBe(
      '…a biópsia transjugular…',
    );
    expect(rows()[0].querySelector('.note-search-count')?.textContent).toBe('4');
    expect(rows()[1].querySelector('.note-search-count')).toBeNull();
  });

  it('renders a note as text, never as markup', () => {
    const p = mountPalette();
    p.openPalette();
    p.showResults(1, [
      result('n-1', '<script>alert(1)</script>', '<img src=x onerror=alert(1)> e <b>negrito</b>'),
    ]);

    const row = rows()[0];
    // The characters are there, as characters.
    expect(row.textContent).toContain('<script>alert(1)</script>');
    expect(row.textContent).toContain('<img src=x onerror=alert(1)>');
    // ...and no element was created from them.
    expect(row.querySelector('script')).toBeNull();
    expect(row.querySelector('img')).toBeNull();
    expect(row.querySelector('b')).toBeNull();
  });

  it('says what it is showing', () => {
    const p = mountPalette();
    p.openPalette();
    const status = () => mount.querySelector('.note-search-status')?.textContent;

    p.showResults(1, [result('n-1', 'recente')]);
    expect(status()).toBe('notas recentes');

    input().value = 'alvo';
    p.showResults(2, [result('n-1', 'uma'), result('n-2', 'duas')]);
    expect(status()).toBe('2 nota(s)');

    p.showResults(3, []);
    expect(status()).toBe('nenhum resultado');
  });
});

describe('a note that disappeared between the search and the choice', () => {
  it('says so, drops the row and asks again instead of crashing', () => {
    const p = mountPalette();
    p.openPalette();
    p.showResults(1, [result('n-1', 'ainda existe'), result('n-2', 'apagada')]);
    handlers.onQuery.mockClear();

    p.reportMissing('n-2');

    expect(rows()).toHaveLength(1);
    expect(rows()[0].dataset.noteId).toBe('n-1');
    expect(mount.querySelector('.note-search-status')?.textContent).toBe('nota não encontrada');
    expect(handlers.onQuery).toHaveBeenCalledTimes(1);
  });

  it('does nothing when it is not open', () => {
    const p = mountPalette();
    expect(() => p.reportMissing('n-1')).not.toThrow();
  });
});
