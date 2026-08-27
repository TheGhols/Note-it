import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import {
  largerTextSize,
  normalizeTextSize,
  smallerTextSize,
  TEXT_SIZES,
} from '../src/editor/textSize.ts';
import { HIGHLIGHT_COLORS, TEXT_COLORS } from '../src/ui/palettes.ts';

function mount(initial = ''): { note: NoteEditor; editor: any } {
  const el = document.createElement('div');
  document.body.append(el);
  const note = new NoteEditor({ element: el, initialContent: initial });
  return { note, editor: note.getRawEditor() };
}

/** Selects the first occurrence of `word` in the document. */
function selectWord(editor: any, word: string): void {
  let found = -1;
  editor.state.doc.descendants((node: any, pos: number) => {
    if (found !== -1 || !node.isText) return;
    const index = node.text.indexOf(word);
    if (index !== -1) found = pos + index;
  });
  if (found === -1) throw new Error(`word not found: ${word}`);
  editor.commands.setTextSelection({ from: found, to: found + word.length });
}

describe('text size scale', () => {
  it('accepts only the whitelisted sizes', () => {
    for (const size of TEXT_SIZES) expect(normalizeTextSize(size)).toBe(size);
    for (const bad of [0, -12, 13, 999999, 'calc(100px)', 'url(x)', 'var(--y)', null, {}]) {
      expect(normalizeTextSize(bad)).toBeNull();
    }
    // Digits arriving as a string from stored content are still checked.
    expect(normalizeTextSize('22')).toBe(22);
    expect(normalizeTextSize('22px')).toBeNull();
  });

  it('steps up and down and clamps at both ends', () => {
    expect(largerTextSize(null)).toBe(12);
    expect(largerTextSize(12)).toBe(14);
    expect(largerTextSize(32)).toBe(32);

    expect(smallerTextSize(32)).toBe(26);
    expect(smallerTextSize(12)).toBeNull();
    expect(smallerTextSize(null)).toBeNull();
  });
});

describe('inline formatting', () => {
  let open: NoteEditor[] = [];

  afterEach(() => {
    for (const note of open) note.destroy();
    open = [];
    document.body.innerHTML = '';
  });

  function track(mounted: { note: NoteEditor; editor: any }) {
    open.push(mounted.note);
    return mounted;
  }

  it('applies a size to the selection and leaves the rest untouched', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('Comprar material para a obra amanhã.');
    selectWord(editor, 'material');
    note.setTextSize(22);

    const markdown = note.getMarkdown();
    expect(markdown).toContain('data-note-it-font-size="22"');
    expect(markdown).toContain('>material<');
    // The surrounding words keep the default size.
    expect(markdown).toContain('Comprar ');
    expect(markdown).toContain(' para a obra amanhã.');
    expect(markdown.match(/data-note-it-font-size/g)).toHaveLength(1);
  });

  it('Padrão removes the custom size', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('Atenção especial');
    selectWord(editor, 'Atenção');
    note.setTextSize(26);
    expect(note.getMarkdown()).toContain('data-note-it-font-size="26"');

    selectWord(editor, 'Atenção');
    note.setTextSize(null);
    expect(note.getMarkdown()).not.toContain('data-note-it-font-size');
    expect(note.getMarkdown()).toContain('Atenção especial');
  });

  it('sets a stored mark when there is no selection', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('');
    note.setTextSize(18);

    editor.commands.insertContent('grande');
    expect(note.getMarkdown()).toContain('data-note-it-font-size="18"');
  });

  it('reports the current size and a mixed selection', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('um dois');

    selectWord(editor, 'um');
    note.setTextSize(22);
    selectWord(editor, 'um');
    expect(note.currentTextSize()).toBe(22);
    expect(note.hasMixedTextSize()).toBe(false);

    editor.commands.setTextSelection({ from: 1, to: editor.state.doc.content.size - 1 });
    expect(note.hasMixedTextSize()).toBe(true);
  });

  it('steps the size through the shortcuts and stops at the top', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('IMPORTANTE');
    selectWord(editor, 'IMPORTANTE');

    for (let i = 0; i < TEXT_SIZES.length + 3; i += 1) {
      note.increaseTextSize();
      selectWord(editor, 'IMPORTANTE');
    }
    expect(note.currentTextSize()).toBe(32);

    for (let i = 0; i < TEXT_SIZES.length + 3; i += 1) {
      note.decreaseTextSize();
      selectWord(editor, 'IMPORTANTE');
    }
    // Stepping below the smallest size returns to the theme default.
    expect(note.currentTextSize()).toBeNull();
  });

  it('round-trips a size through save and reload', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('João não está em Goiânia amanhã.');
    selectWord(editor, 'Goiânia');
    note.setTextSize(22);
    const saved = note.getMarkdown();

    const reopened = track(mount());
    reopened.note.setMarkdown(saved);

    expect(reopened.note.getMarkdown()).toBe(saved);
    expect(reopened.editor.state.doc.textContent).toContain('João não está em Goiânia amanhã.');
  });

  it('combines a size with bold, italic, strike, colour and highlight', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('destaque');
    selectWord(editor, 'destaque');
    note.setTextSize(26);
    editor.chain().setTextSelection({ from: 1, to: 9 }).toggleBold().toggleItalic().toggleStrike().run();
    editor.commands.setTextSelection({ from: 1, to: 9 });
    note.setTextColor('#2563EB');
    editor.commands.setTextSelection({ from: 1, to: 9 });
    note.setHighlight('#FDE68A');

    const markdown = note.getMarkdown();
    expect(markdown).toContain('data-note-it-font-size="26"');
    expect(markdown).toContain('data-note-it-color="#2563EB"');
    expect(markdown).toContain('data-note-it-highlight="#FDE68A"');

    const reopened = track(mount());
    reopened.note.setMarkdown(markdown);
    const html = reopened.editor.getHTML();
    expect(html).toContain('data-note-it-font-size="26"');
    expect(html).toContain('#2563EB');
    expect(html).toContain('#FDE68A');
    expect(reopened.note.getMarkdown()).toBe(markdown);
  });

  it('applies a size inside a task item', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('- [ ] Comprar material');
    selectWord(editor, 'material');
    note.setTextSize(22);

    const markdown = note.getMarkdown();
    expect(markdown).toContain('- [ ] Comprar <span data-note-it-font-size="22"');

    const reopened = track(mount());
    reopened.note.setMarkdown(markdown);
    expect(reopened.editor.getHTML()).toContain('data-type="taskItem"');
    expect(reopened.editor.getHTML()).toContain('data-note-it-font-size="22"');
    expect(reopened.note.getMarkdown()).toBe(markdown);
  });

  it('drops a font size that is not on the whitelist', () => {
    const { note } = track(mount());
    note.setMarkdown('<span data-note-it-font-size="999999" style="font-size:999999px">enorme</span>');

    const markdown = note.getMarkdown();
    expect(markdown).not.toContain('999999');
    expect(markdown).toContain('enorme');
  });
});

describe('text colour and highlight', () => {
  let open: NoteEditor[] = [];

  afterEach(() => {
    for (const note of open) note.destroy();
    open = [];
    document.body.innerHTML = '';
  });

  function track(mounted: { note: NoteEditor; editor: any }) {
    open.push(mounted.note);
    return mounted;
  }

  it('offers a small, fixed palette with an explicit clear option', () => {
    expect(TEXT_COLORS[0]).toEqual({ label: 'Padrão', value: null });
    expect(HIGHLIGHT_COLORS[0]).toEqual({ label: 'Sem marca-texto', value: null });
    expect(TEXT_COLORS.length).toBeLessThanOrEqual(10);
    expect(HIGHLIGHT_COLORS.length).toBeLessThanOrEqual(8);
    for (const entry of [...TEXT_COLORS, ...HIGHLIGHT_COLORS]) {
      if (entry.value !== null) expect(entry.value).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });

  it('colours only the selection and clears it again', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('João não está em Goiânia amanhã.');
    selectWord(editor, 'Goiânia');
    note.setTextColor('#2563EB');

    let markdown = note.getMarkdown();
    expect(markdown).toContain('data-note-it-color="#2563EB"');
    expect(markdown).toContain('>Goiânia<');
    expect(markdown).toContain('João não está em ');

    selectWord(editor, 'Goiânia');
    note.setTextColor(null);
    markdown = note.getMarkdown();
    expect(markdown).not.toContain('data-note-it-color');
    expect(markdown).toContain('João não está em Goiânia amanhã.');
  });

  it('highlights the selection and removes the highlight', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('texto marcado aqui');
    selectWord(editor, 'marcado');
    note.setHighlight('#BBF7D0');
    expect(note.getMarkdown()).toContain('data-note-it-highlight="#BBF7D0"');

    selectWord(editor, 'marcado');
    note.setHighlight(null);
    expect(note.getMarkdown()).not.toContain('data-note-it-highlight');
  });

  it('supports several highlight colours in one note', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('alfa beta');
    selectWord(editor, 'alfa');
    note.setHighlight('#FDE68A');
    selectWord(editor, 'beta');
    note.setHighlight('#BFDBFE');

    const markdown = note.getMarkdown();
    expect(markdown).toContain('#FDE68A');
    expect(markdown).toContain('#BFDBFE');
  });

  it('sets a stored colour mark when nothing is selected', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('');
    note.setTextColor('#DC2626');
    editor.commands.insertContent('vermelho');
    expect(note.getMarkdown()).toContain('data-note-it-color="#DC2626"');
  });

  it('rejects a colour that is not a plain hex value', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('texto');
    selectWord(editor, 'texto');

    for (const bad of ['red', 'javascript:alert(1)', 'url(x)', 'var(--y)', '#12', '']) {
      note.setTextColor(bad);
      note.setHighlight(bad);
    }
    const markdown = note.getMarkdown();
    expect(markdown).not.toContain('data-note-it-color');
    expect(markdown).not.toContain('data-note-it-highlight');
    expect(markdown).toContain('texto');
  });

  it('round-trips colour and highlight together with pt-BR text', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('Ação coração ímpar');
    selectWord(editor, 'Ação');
    note.setTextColor('#16A34A');
    selectWord(editor, 'coração');
    note.setHighlight('#DDD6FE');
    const saved = note.getMarkdown();

    const reopened = track(mount());
    reopened.note.setMarkdown(saved);
    expect(reopened.note.getMarkdown()).toBe(saved);
    expect(reopened.editor.state.doc.textContent).toContain('Ação coração ímpar');
  });

  it('scales a sized run with the view zoom while keeping plain pixels in Markdown', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('normal grande');
    selectWord(editor, 'grande');
    note.setTextSize(22);

    // The editor carries only the data attribute the stylesheet keys off, so
    // no inline style ever reaches the document.
    expect(editor.getHTML()).toContain('data-note-it-font-size="22"');
    expect(editor.getHTML()).not.toContain('style="font-size');

    // The stored Markdown keeps a plain pixel value for other tools.
    expect(note.getMarkdown()).toContain('style="font-size:22px"');
    expect(note.getMarkdown()).not.toContain('var(--note-zoom');
  });
});
