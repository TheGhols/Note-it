import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { HIGHLIGHT_COLORS, TEXT_COLORS } from '../src/ui/palettes.ts';

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

/** Foreground the stylesheet gives highlighted text, on every paper colour. */
const HIGHLIGHT_TEXT = '#1E293B';
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
