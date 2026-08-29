import { Fragment, Slice } from '@tiptap/pm/model';
import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { findMatches } from '../src/editor/find.ts';
import { safeLinkUrl } from '../src/markdown/sanitizer.ts';

const open: NoteEditor[] = [];

function mount(initial = ''): { note: NoteEditor; element: HTMLElement } {
  const element = document.createElement('div');
  document.body.append(element);
  const note = new NoteEditor({ element, initialContent: initial });
  open.push(note);
  return { note, element };
}

afterEach(() => {
  while (open.length) open.pop()!.destroy();
  document.body.innerHTML = '';
});

/** A clipboard carrying plain text, as a paste event ProseMirror will see. */
function pasteEvent(text: string): ClipboardEvent {
  const event = new Event('paste', { bubbles: true, cancelable: true }) as ClipboardEvent;
  Object.defineProperty(event, 'clipboardData', {
    value: {
      getData: (type: string) => (type === 'text/plain' ? text : ''),
      types: ['text/plain'],
    },
  });
  return event;
}

/** The clipboard as ProseMirror hands it to a paste handler. */
function pasteSlice(note: NoteEditor, text: string): Slice {
  return new Slice(Fragment.from(note.getRawEditor().schema.text(text)), 0, 0);
}

/** Runs the paste handlers over the current selection. */
function paste(note: NoteEditor, clipboard: string): boolean {
  const view = note.getView();
  return (
    view.someProp('handlePaste', (handler) =>
      handler(view, pasteEvent(clipboard), pasteSlice(note, clipboard)),
    ) === true
  );
}

/** Selects `text` wherever it is in the document, then pastes over it. */
function pasteOver(note: NoteEditor, text: string, clipboard: string): boolean {
  // Real document positions, found the same way the editor's own search finds
  // them, so this works inside a list item or a heading and not only in a
  // top-level paragraph.
  const [match] = findMatches(note.getRawEditor().state.doc, {
    text,
    caseSensitive: true,
  });
  if (!match) throw new Error(`"${text}" is not in the document`);

  note.getRawEditor().commands.setTextSelection({ from: match.from, to: match.to });
  return paste(note, clipboard);
}

describe('what may become a link', () => {
  it('accepts the schemes the application already allows', () => {
    expect(safeLinkUrl('https://example.com')).toBe('https://example.com');
    expect(safeLinkUrl('http://example.com/path?a=1')).toBe('http://example.com/path?a=1');
    expect(safeLinkUrl('mailto:alguem@example.com')).toBe('mailto:alguem@example.com');
    expect(safeLinkUrl('  https://example.com  ')).toBe('https://example.com');
  });

  it('refuses everything else, and says nothing about why', () => {
    for (const candidate of [
      'javascript:alert(1)',
      'JavaScript:alert(1)',
      'data:text/html,<script>alert(1)</script>',
      'file:///etc/passwd',
      'vbscript:msgbox(1)',
      'ftp://example.com',
      'obsidian://open?vault=x',
      'https://',
      'mailto:',
      'example.com',
      'só um texto',
      'https://exa mple.com',
      '',
      'https://example.com\nhttps://outro.com',
    ]) {
      expect(safeLinkUrl(candidate), candidate).toBeNull();
    }
  });
});

describe('pasting a URL over selected text', () => {
  it('turns the selection into a link and keeps the words', () => {
    const { note } = mount('visite o site oficial hoje');
    expect(pasteOver(note, 'site oficial', 'https://example.com')).toBe(true);
    expect(note.getMarkdown()).toBe('visite o [site oficial](https://example.com) hoje');
  });

  it('round-trips through Markdown and back', () => {
    const { note } = mount('OpenAI');
    pasteOver(note, 'OpenAI', 'https://openai.com');
    const saved = note.getMarkdown();
    expect(saved).toBe('[OpenAI](https://openai.com)');

    const reopened = mount(saved);
    expect(reopened.note.getMarkdown()).toBe(saved);
    expect(reopened.element.querySelector('a[href="https://openai.com"]')?.textContent).toBe(
      'OpenAI',
    );
  });

  it('comes undone in a single step, and goes back with redo', () => {
    const { note } = mount('texto simples');
    pasteOver(note, 'texto', 'https://example.com');
    expect(note.getMarkdown()).toBe('[texto](https://example.com) simples');

    note.getRawEditor().commands.undo();
    expect(note.getMarkdown()).toBe('texto simples');

    note.getRawEditor().commands.redo();
    expect(note.getMarkdown()).toBe('[texto](https://example.com) simples');
  });

  it('accepts mailto as readily as https', () => {
    const { note } = mount('escreva para mim');
    pasteOver(note, 'mim', 'mailto:alguem@example.com');
    expect(note.getMarkdown()).toBe('escreva para [mim](mailto:alguem@example.com)');
  });

  it('replaces a link the selection already had', () => {
    const { note } = mount('[texto](https://antigo.example) fim');
    pasteOver(note, 'texto', 'https://novo.example');
    expect(note.getMarkdown()).toBe('[texto](https://novo.example) fim');
  });
});

describe('when pasting stays a paste', () => {
  it('does nothing without a selection', () => {
    const { note } = mount('sem seleção');
    note.getRawEditor().commands.setTextSelection(3);
    expect(paste(note, 'https://example.com')).toBe(false);
    expect(note.getMarkdown()).toBe('sem seleção');
  });

  it('does nothing when the clipboard is not a URL Note-it allows', () => {
    for (const clipboard of ['javascript:alert(1)', 'apenas texto', 'ftp://example.com']) {
      const { note } = mount('alvo aqui');
      expect(pasteOver(note, 'alvo', clipboard), clipboard).toBe(false);
      expect(note.getMarkdown(), clipboard).toBe('alvo aqui');
    }
  });

  it('leaves code alone: a URL in source is characters', () => {
    const block = mount('```text\nalvo no código\n```').note;
    expect(pasteOver(block, 'alvo', 'https://example.com')).toBe(false);
    expect(block.getMarkdown().trim()).toBe('```text\nalvo no código\n```');

    const inline = mount('texto com `alvo` embutido').note;
    expect(pasteOver(inline, 'alvo', 'https://example.com')).toBe(false);
    expect(inline.getMarkdown()).toBe('texto com `alvo` embutido');
  });

  it('is the only path from a paste to a link', () => {
    // Upstream's own paste handler uses `linkifyjs`, which recognises schemes
    // Note-it does not allow and does not care how many blocks the selection
    // covers. It is switched off, so the allowlist is not merely consulted —
    // it is the only thing consulted.
    for (const clipboard of ['ftp://example.com', 'ssh://example.com', 'www.example.com']) {
      const { note } = mount('alvo aqui');
      pasteOver(note, 'alvo', clipboard);
      expect(note.getMarkdown(), clipboard).toBe('alvo aqui');
      expect(note.getRawEditor().getHTML(), clipboard).not.toContain('<a ');
    }
  });

  it('leaves a selection spanning two blocks alone', () => {
    const { note } = mount('primeiro parágrafo\n\nsegundo parágrafo');
    const size = note.getRawEditor().state.doc.content.size;
    note.getRawEditor().commands.setTextSelection({ from: 1, to: size - 1 });
    expect(paste(note, 'https://example.com')).toBe(false);
    expect(note.getMarkdown()).toBe('primeiro parágrafo\n\nsegundo parágrafo');
    expect(note.getRawEditor().getHTML()).not.toContain('<a ');
  });

  it('keeps working inside a heading, a list and a quote', () => {
    for (const source of ['# alvo no título', '- alvo na lista', '> alvo na citação']) {
      const { note } = mount(source);
      expect(pasteOver(note, 'alvo', 'https://example.com'), source).toBe(true);
      expect(note.getMarkdown().trim()).toBe(
        source.replace('alvo', '[alvo](https://example.com)'),
      );
    }
  });
});
