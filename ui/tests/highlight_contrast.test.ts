import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { HIGHLIGHT_COLORS, HIGHLIGHT_TEXT_COLOR, TEXT_COLORS } from '../src/ui/palettes.ts';

const PAPER_COLORS = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'] as const;

/** Relative luminance per WCAG 2.1. */
function luminance(hex: string): number {
  const value = hex.replace('#', '');
  const channels = [0, 2, 4].map((offset) => {
    const raw = parseInt(value.slice(offset, offset + 2), 16) / 255;
    return raw <= 0.03928 ? raw / 12.92 : ((raw + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrastRatio(foreground: string, background: string): number {
  const a = luminance(foreground);
  const b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** Foreground highlighted text is actually rendered with. */
const HIGHLIGHT_TEXT = HIGHLIGHT_TEXT_COLOR;
/** Default text colour of the dark paper, which used to stay on the highlight. */
const DARK_PAPER_TEXT = '#F4F4F5';

function mount(initial = ''): { note: NoteEditor; editor: any } {
  const el = document.createElement('div');
  document.body.append(el);
  const note = new NoteEditor({ element: el, initialContent: initial });
  return { note, editor: note.getRawEditor() };
}

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

describe('highlight readability', () => {
  it('every highlight in the palette is readable with the highlight foreground', () => {
    for (const entry of HIGHLIGHT_COLORS) {
      if (entry.value === null) continue;
      const ratio = contrastRatio(HIGHLIGHT_TEXT, entry.value);
      expect(ratio, `${entry.label} (${entry.value})`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('the dark paper default text would have been unreadable on the highlights', () => {
    // This is the reported defect: light text kept on a pale highlight.
    for (const entry of HIGHLIGHT_COLORS) {
      if (entry.value === null) continue;
      const ratio = contrastRatio(DARK_PAPER_TEXT, entry.value);
      expect(ratio, `${entry.label} (${entry.value})`).toBeLessThan(4.5);
    }
  });

  it('an explicit text colour stays readable on every highlight', () => {
    // The user's colour wins over the highlight foreground, so each palette
    // entry has to work on the pale highlights too.
    for (const text of TEXT_COLORS) {
      if (text.value === null) continue;
      for (const highlight of HIGHLIGHT_COLORS) {
        if (highlight.value === null) continue;
        const ratio = contrastRatio(text.value, highlight.value);
        expect(ratio, `${text.label} on ${highlight.label}`).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

describe('highlight rendering', () => {
  let open: NoteEditor[] = [];

  afterEach(() => {
    for (const note of open) note.destroy();
    open = [];
    document.body.innerHTML = '';
    document.body.removeAttribute('data-color');
  });

  function track(mounted: { note: NoteEditor; editor: any }) {
    open.push(mounted.note);
    return mounted;
  }

  it('renders a mark the stylesheet can colour, on every paper colour', () => {
    for (const paper of PAPER_COLORS) {
      document.body.setAttribute('data-color', paper);
      const { note, editor } = track(mount());
      note.setMarkdown('texto marcado');
      selectWord(editor, 'marcado');
      note.setHighlight('#FDE68A');

      // The stylesheet colours `.ProseMirror mark`, so the fix does not depend
      // on which attribute the extension renders.
      const html = editor.getHTML();
      expect(html, paper).toContain('<mark');
      expect(note.getMarkdown(), paper).toContain('data-note-it-highlight="#FDE68A"');
      // No colour is baked into the document: the paper colour never rewrites
      // the note's Markdown.
      expect(note.getMarkdown(), paper).not.toContain('data-note-it-color');
    }
  });

  it('keeps an explicit text colour inside a highlight, in the document', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('palavra colorida');
    selectWord(editor, 'colorida');
    note.setTextColor('#DC2626');
    selectWord(editor, 'colorida');
    note.setHighlight('#BFDBFE');

    const markdown = note.getMarkdown();
    expect(markdown).toContain('data-note-it-color="#DC2626"');
    expect(markdown).toContain('data-note-it-highlight="#BFDBFE"');

    const reopened = track(mount());
    reopened.note.setMarkdown(markdown);
    expect(reopened.note.getMarkdown()).toBe(markdown);
  });

  it('combines a highlight with bold, italic, strike, size and a task item', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('- [ ] Comprar material importante');
    selectWord(editor, 'material');
    note.setHighlight('#BBF7D0');
    selectWord(editor, 'material');
    note.setTextSize(22);
    selectWord(editor, 'material');
    editor.chain().toggleBold().toggleItalic().toggleStrike().run();

    const markdown = note.getMarkdown();
    expect(markdown).toContain('- [ ] Comprar');
    expect(markdown).toContain('data-note-it-highlight="#BBF7D0"');
    expect(markdown).toContain('data-note-it-font-size="22"');

    const reopened = track(mount());
    reopened.note.setMarkdown(markdown);
    const html = reopened.editor.getHTML();
    expect(html).toContain('data-type="taskItem"');
    expect(html).toContain('<mark');
    expect(reopened.note.getMarkdown()).toBe(markdown);
  });

  it('removing the highlight leaves the text and its colour behind', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('João não está em Goiânia');
    selectWord(editor, 'Goiânia');
    note.setTextColor('#16A34A');
    selectWord(editor, 'Goiânia');
    note.setHighlight('#DDD6FE');
    selectWord(editor, 'Goiânia');
    note.setHighlight(null);

    const markdown = note.getMarkdown();
    expect(markdown).not.toContain('data-note-it-highlight');
    expect(markdown).toContain('data-note-it-color="#16A34A"');
    expect(markdown).toContain('João não está em');
  });
});

describe('text palette readability', () => {
  const PAPER_BACKGROUNDS = [
    '#FEF9C3', '#E0F2FE', '#DCFCE7', '#FCE7F3', '#F3E8FF', '#F1F5F9', '#18181B',
  ];

  it('every text colour stays readable on every paper colour', () => {
    for (const entry of TEXT_COLORS) {
      if (entry.value === null) continue;
      for (const paper of PAPER_BACKGROUNDS) {
        const ratio = contrastRatio(entry.value, paper);
        expect(ratio, `${entry.label} on paper ${paper}`).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

/**
 * The defect these cover is a cascade problem, not an arithmetic one: the
 * palette contrast was already correct while the DOM was still painting white
 * text, because the highlight extension emitted an inline `color: inherit`
 * that beat the stylesheet. These assert the colour the element really ends up
 * with.
 */
describe('highlighted text is really rendered dark', () => {
  let open: NoteEditor[] = [];

  afterEach(() => {
    for (const note of open) note.destroy();
    open = [];
    document.body.innerHTML = '';
    document.body.removeAttribute('data-color');
  });

  function track(mounted: { note: NoteEditor; editor: any }) {
    open.push(mounted.note);
    return mounted;
  }

  function markIn(container: HTMLElement): HTMLElement {
    const mark = container.querySelector('mark');
    if (!mark) throw new Error('no highlight rendered');
    return mark as HTMLElement;
  }

  function highlighted(markdown: string): { mark: HTMLElement; note: NoteEditor; editor: any } {
    const el = document.createElement('div');
    document.body.append(el);
    const note = new NoteEditor({ element: el, initialContent: '' });
    note.setMarkdown(markdown);
    track({ note, editor: note.getRawEditor() });
    return { mark: markIn(el), note, editor: note.getRawEditor() };
  }

  it('paints a dark foreground for every highlight on the dark paper', () => {
    document.body.setAttribute('data-color', 'black');

    for (const entry of HIGHLIGHT_COLORS) {
      if (entry.value === null) continue;
      const { mark } = highlighted(
        `<mark data-note-it-highlight="${entry.value}">marcado</mark>`,
      );

      // The colour actually applied to the element, not a palette calculation.
      expect(getComputedStyle(mark).color, entry.label).toBe(HIGHLIGHT_TEXT);
      expect(mark.style.backgroundColor, entry.label).toBeTruthy();
    }
  });

  it('never leaves the inherit that used to keep the text white', () => {
    document.body.setAttribute('data-color', 'black');
    const { mark } = highlighted('<mark data-note-it-highlight="#FDE68A">marcado</mark>');

    const style = mark.getAttribute('style') ?? '';
    expect(style).toContain('color');
    expect(style).not.toContain('inherit');
    expect(getComputedStyle(mark).color).not.toBe('inherit');
  });

  it('renders the same dark foreground whatever the paper colour is', () => {
    for (const paper of PAPER_COLORS) {
      document.body.setAttribute('data-color', paper);
      const { mark } = highlighted('<mark data-note-it-highlight="#BBF7D0">marcado</mark>');
      expect(getComputedStyle(mark).color, paper).toBe(HIGHLIGHT_TEXT);
    }
  });

  it('leaves unhighlighted text to inherit the paper colour', () => {
    document.body.setAttribute('data-color', 'black');
    const el = document.createElement('div');
    document.body.append(el);
    const note = new NoteEditor({ element: el, initialContent: '' });
    note.setMarkdown('texto simples');
    track({ note, editor: note.getRawEditor() });

    // No inline colour anywhere, so the paper's own text colour applies.
    expect(el.querySelector('mark')).toBeNull();
    expect(el.innerHTML).not.toContain('color:');
  });

  it('returns to the normal colour when the highlight is removed', () => {
    document.body.setAttribute('data-color', 'black');
    const el = document.createElement('div');
    document.body.append(el);
    const note = new NoteEditor({ element: el, initialContent: '' });
    note.setMarkdown('palavra destacada');
    const editor = note.getRawEditor();
    track({ note, editor });

    selectWord(editor, 'destacada');
    note.setHighlight('#FDE68A');
    expect(getComputedStyle(markIn(el)).color).toBe(HIGHLIGHT_TEXT);

    selectWord(editor, 'destacada');
    note.setHighlight(null);

    expect(el.querySelector('mark')).toBeNull();
    // Nothing dark is left behind pinning the text.
    expect(el.innerHTML).not.toContain(HIGHLIGHT_TEXT);
    expect(note.getMarkdown()).not.toContain('data-note-it-highlight');
  });

  it('keeps the highlighted run dark even under an explicit text colour', () => {
    document.body.setAttribute('data-color', 'black');
    const { mark, note } = highlighted(
      '<mark data-note-it-highlight="#FDE68A"><span data-note-it-color="#DC2626">x</span></mark>',
    );

    // Legibility wins while the highlight is there...
    expect(getComputedStyle(mark).color).toBe(HIGHLIGHT_TEXT);
    // ...and the user's own colour is still recorded in the note.
    expect(note.getMarkdown()).toContain('data-note-it-color="#DC2626"');
  });

  it('brings an explicit colour back once the highlight is removed', () => {
    const el = document.createElement('div');
    document.body.append(el);
    const note = new NoteEditor({ element: el, initialContent: '' });
    note.setMarkdown('palavra colorida');
    const editor = note.getRawEditor();
    track({ note, editor });

    selectWord(editor, 'colorida');
    note.setTextColor('#DC2626');
    selectWord(editor, 'colorida');
    note.setHighlight('#BFDBFE');
    selectWord(editor, 'colorida');
    note.setHighlight(null);

    const span = el.querySelector('span[style*="color"]') as HTMLElement | null;
    expect(span).not.toBeNull();
    expect(getComputedStyle(span!).color).toBe('#DC2626');
    expect(note.getMarkdown()).toContain('data-note-it-color="#DC2626"');
  });

  it('stays dark combined with bold, italic, strike, a size and a task item', () => {
    document.body.setAttribute('data-color', 'black');
    const el = document.createElement('div');
    document.body.append(el);
    const note = new NoteEditor({ element: el, initialContent: '' });
    note.setMarkdown('- [ ] Comprar material');
    const editor = note.getRawEditor();
    track({ note, editor });

    selectWord(editor, 'material');
    note.setHighlight('#DDD6FE');
    selectWord(editor, 'material');
    note.setTextSize(22);
    selectWord(editor, 'material');
    editor.chain().toggleBold().toggleItalic().toggleStrike().run();

    expect(getComputedStyle(markIn(el)).color).toBe(HIGHLIGHT_TEXT);

    const markdown = note.getMarkdown();
    const reopened = document.createElement('div');
    document.body.append(reopened);
    const again = new NoteEditor({ element: reopened, initialContent: '' });
    again.setMarkdown(markdown);
    track({ note: again, editor: again.getRawEditor() });

    // Survives the round trip, still dark, still a task.
    expect(again.getMarkdown()).toBe(markdown);
    expect(getComputedStyle(markIn(reopened)).color).toBe(HIGHLIGHT_TEXT);
    expect(again.getRawEditor().getHTML()).toContain('data-type="taskItem"');
  });

  it('never writes the highlight foreground into the Markdown', () => {
    document.body.setAttribute('data-color', 'black');
    const { note } = highlighted('<mark data-note-it-highlight="#FBCFE8">marcado</mark>');

    const markdown = note.getMarkdown();
    expect(markdown).toContain('data-note-it-highlight="#FBCFE8"');
    // The paper colour must not leave a colour mark behind in the document.
    expect(markdown).not.toContain(HIGHLIGHT_TEXT);
    expect(markdown).not.toContain('data-note-it-color');
  });

});
