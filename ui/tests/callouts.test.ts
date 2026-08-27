import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { CALLOUT_TYPES, calloutType } from '../src/editor/callout.ts';

const open: NoteEditor[] = [];

function mount(initial = ''): { note: NoteEditor; element: HTMLElement } {
  const element = document.createElement('div');
  document.body.append(element);
  const note = new NoteEditor({ element, initialContent: initial });
  open.push(note);
  return { note, element };
}

function roundTrip(markdown: string): string {
  return mount(markdown).note.getMarkdown().trim();
}

function reopen(markdown: string): string {
  return roundTrip(roundTrip(markdown));
}

afterEach(() => {
  while (open.length) open.pop()!.destroy();
  document.body.innerHTML = '';
});

describe('callouts', () => {
  it.each(CALLOUT_TYPES.map((entry) => entry.id))('round-trips a %s callout', (type) => {
    const source = `> [!${type}]\n> Corpo do aviso.`;
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);

    const { element } = mount(source);
    const quote = element.querySelector('blockquote')!;
    expect(quote.getAttribute('data-callout')).toBe(type);
    // The marker is decoration, not text: it never stays in the body.
    expect(quote.textContent).toBe('Corpo do aviso.');
  });

  it('labels each kind from the whitelist rather than from the note', () => {
    for (const entry of CALLOUT_TYPES) {
      const { element } = mount(`> [!${entry.id}]\n> corpo`);
      expect(element.querySelector('blockquote')?.getAttribute('data-callout-label')).toBe(
        entry.label,
      );
    }
  });

  it('keeps several paragraphs inside one callout', () => {
    const source = '> [!WARNING]\n> Primeiro parágrafo.\n>\n> Segundo parágrafo.';
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);

    const { element } = mount(source);
    expect(element.querySelectorAll('blockquote[data-callout] p')).toHaveLength(2);
  });

  it('keeps a list inside a callout', () => {
    const source = '> [!TIP]\n> Considere:\n>\n> - primeiro\n> - segundo';
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);

    const { element } = mount(source);
    expect(element.querySelectorAll('blockquote[data-callout] li')).toHaveLength(2);
  });

  it('keeps a soft-wrapped body on one paragraph', () => {
    const source = '> [!NOTE]\n> linha um\n> linha dois';
    expect(roundTrip(source)).toBe(source);
  });

  it('does not swallow the text that follows it', () => {
    const source = '> [!IMPORTANT]\n> Dentro do callout.\n\nTexto normal depois.';
    expect(roundTrip(source)).toBe(source);

    const { element } = mount(source);
    expect(element.querySelectorAll('blockquote')).toHaveLength(1);
    expect(element.querySelector('blockquote + p')?.textContent).toBe('Texto normal depois.');
  });

  it('reads a lowercase marker and stores the canonical kind', () => {
    const { note, element } = mount('> [!warning]\n> corpo');
    expect(element.querySelector('blockquote')?.getAttribute('data-callout')).toBe('WARNING');
    expect(note.getMarkdown().trim()).toBe('> [!WARNING]\n> corpo');
  });

  it('degrades an unknown kind to a plain quote without losing the text', () => {
    const { note, element } = mount('> [!FOO]\n> corpo preservado');
    const quote = element.querySelector('blockquote')!;

    expect(quote.hasAttribute('data-callout')).toBe(false);
    expect(quote.textContent).toContain('[!FOO]');
    expect(quote.textContent).toContain('corpo preservado');

    // The text survives; only the bracket is escaped, which is how Markdown
    // writes a literal `[`. The result is stable from then on, and reading it
    // back gives the words that were written.
    const once = note.getMarkdown().trim();
    expect(once).toBe('> \\[!FOO\\]\n> corpo preservado');
    expect(roundTrip(once)).toBe(once);
    expect(mount(once).element.querySelector('blockquote')?.textContent).toContain('[!FOO]');
  });

  it.each([
    ['> [!NOTE] com texto na mesma linha', 'texto na mesma linha'],
    ['> [!NOTE', 'marcador sem fechamento'],
    ['> !NOTE]\n> corpo', 'marcador sem abertura'],
    ['> [!]\n> corpo', 'marcador vazio'],
    ['> texto\n>\n> [!NOTE]', 'marcador fora da primeira linha'],
  ])('leaves %s as a plain quote (%s)', (source) => {
    const { element } = mount(source);
    const quote = element.querySelector('blockquote');
    expect(quote).not.toBeNull();
    expect(quote!.hasAttribute('data-callout')).toBe(false);
    // Whatever it was, the words are still there.
    expect(quote!.textContent!.length).toBeGreaterThan(0);
  });

  it('resolves only the five supported kinds', () => {
    for (const entry of CALLOUT_TYPES) {
      expect(calloutType(entry.id)).toBe(entry.id);
      expect(calloutType(entry.id.toLowerCase())).toBe(entry.id);
    }
    for (const rejected of ['', 'FOO', 'note ', null, undefined, 42, {}]) {
      expect(calloutType(rejected)).toBeNull();
    }
  });

  it('sets, changes and clears the kind from the menu', () => {
    const { note } = mount('parágrafo comum');

    note.setCallout('NOTE');
    expect(note.getMarkdown().trim()).toBe('> [!NOTE]\n> parágrafo comum');
    expect(note.currentBlock().callout).toBe('NOTE');

    note.setCallout('CAUTION');
    expect(note.getMarkdown().trim()).toBe('> [!CAUTION]\n> parágrafo comum');

    // Clearing the kind leaves the quote, never deletes what it holds.
    note.setCallout(null);
    expect(note.getMarkdown().trim()).toBe('> parágrafo comum');
    expect(note.currentBlock()).toMatchObject({ blockquote: true, callout: null });
  });

  it('survives a save, close and reopen', () => {
    const source = '> [!CAUTION]\n> Não faça isso.';
    const saved = mount(source).note.getMarkdown();
    const { note, element } = mount(saved);

    expect(element.querySelector('blockquote')?.getAttribute('data-callout')).toBe('CAUTION');
    expect(note.getMarkdown().trim()).toBe(source);
  });
});

describe('blockquotes', () => {
  it('round-trips a simple quote', () => {
    const source = '> uma citação';
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);
  });

  it('round-trips a multiline quote', () => {
    const source = '> primeira linha\n> segunda linha';
    expect(roundTrip(source)).toBe(source);
  });

  it('round-trips several paragraphs in one quote', () => {
    const source = '> um\n>\n> dois';
    expect(roundTrip(source)).toBe(source);
    expect(mount(source).element.querySelectorAll('blockquote p')).toHaveLength(2);
  });

  it('round-trips a quote holding a list', () => {
    const source = '> antes\n>\n> - a\n> - b';
    expect(roundTrip(source)).toBe(source);
    expect(mount(source).element.querySelectorAll('blockquote li')).toHaveLength(2);
  });

  it('keeps consecutive quotes separate', () => {
    const source = '> primeira\n\n> segunda';
    expect(roundTrip(source)).toBe(source);
    expect(mount(source).element.querySelectorAll('blockquote')).toHaveLength(2);
  });

  it('sits beside paragraphs and lists without absorbing them', () => {
    const source = 'antes\n\n> citada\n\n- item\n\ndepois';
    expect(roundTrip(source)).toBe(source);

    const { element } = mount(source);
    expect(element.querySelectorAll('blockquote')).toHaveLength(1);
    expect(element.querySelector('blockquote')?.textContent).toBe('citada');
    expect(element.querySelectorAll('li')).toHaveLength(1);
  });

  it('is never turned into a callout on its own', () => {
    const { element } = mount('> citação comum');
    expect(element.querySelector('blockquote')?.hasAttribute('data-callout')).toBe(false);
    expect(element.querySelector('blockquote')?.hasAttribute('data-callout-label')).toBe(false);
  });

  it('quotes and unquotes from the menu', () => {
    const { note } = mount('texto');
    note.toggleBlockquote();
    expect(note.getMarkdown().trim()).toBe('> texto');
    note.toggleBlockquote();
    expect(note.getMarkdown().trim()).toBe('texto');
  });

  it('writes no proprietary decoration into the Markdown', () => {
    const output = roundTrip('> uma citação\n\n> [!NOTE]\n> um callout');
    expect(output).not.toContain('data-');
    expect(output).not.toContain('<blockquote');
    expect(output).not.toContain('class=');
  });
});
