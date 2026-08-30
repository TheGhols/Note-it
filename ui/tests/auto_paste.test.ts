import { describe, expect, it } from 'vitest';
import { TextSelection } from '@tiptap/pm/state';
import { redo, undo } from '@tiptap/pm/history';
import type { Transaction } from '@tiptap/pm/state';
import { NoteEditor } from '../src/editor/editor.ts';
import {
  appendCapture,
  CAPTURE_DELIMITERS,
  CaptureDelimiter,
  DEFAULT_CAPTURE_DELIMITER,
  delimiterLabel,
  isCapturable,
  isDocumentEmpty,
  normalizeDelimiter,
  splitCapture,
} from '../src/capture/autoPaste.ts';

function note(markdown = ''): NoteEditor {
  return new NoteEditor({
    element: document.createElement('div'),
    initialContent: markdown,
  });
}

/** One capture, the way the host delivers one. */
function capture(
  editor: NoteEditor,
  text: string,
  delimiter: CaptureDelimiter = DEFAULT_CAPTURE_DELIMITER,
): boolean {
  return appendCapture(editor.getView(), text, delimiter);
}

/** The transaction a capture would dispatch, without dispatching it. */
function transactionFor(editor: NoteEditor, text: string, delimiter: CaptureDelimiter) {
  const view = editor.getView();
  const original = view.dispatch.bind(view);
  let seen: Transaction | null = null;
  view.dispatch = (tr) => {
    seen = tr;
    original(tr);
  };
  appendCapture(view, text, delimiter);
  view.dispatch = original;
  return seen as Transaction | null;
}

describe('what counts as a capture at all', () => {
  it('files anything with words in it', () => {
    for (const text of ['café', 'a', '日本語', '🧪', '  espaços internos  ']) {
      expect(isCapturable(text)).toBe(true);
    }
  });

  it('files nothing for an empty or blank clipboard', () => {
    for (const text of ['', ' ', '\n', '\r\n', '\t  \n ']) {
      expect(isCapturable(text)).toBe(false);
    }
  });

  it('leaves the note untouched when there is nothing to file', () => {
    const editor = note('conteúdo existente');
    const before = editor.getMarkdown();
    for (const empty of ['', '   ', '\n\n']) {
      expect(capture(editor, empty)).toBe(false);
    }
    expect(editor.getMarkdown()).toBe(before);
  });
});

describe('how captured text is split', () => {
  it('splits on runs of newlines, exactly as a plain-text paste does', () => {
    expect(splitCapture('uma linha')).toEqual(['uma linha']);
    expect(splitCapture('a\nb')).toEqual(['a', 'b']);
    expect(splitCapture('a\n\n\nb')).toEqual(['a', 'b']);
  });

  it('treats every line ending the same way, so no CRLF survives', () => {
    expect(splitCapture('a\r\nb')).toEqual(['a', 'b']);
    expect(splitCapture('a\rb')).toEqual(['a', 'b']);
    for (const block of splitCapture('a\r\nb\r\nc')) {
      expect(block).not.toContain('\r');
    }
  });

  it('drops the empties a leading or trailing newline would make', () => {
    // A copy that happens to end in a newline must not file a blank line.
    expect(splitCapture('\ntexto\n')).toEqual(['texto']);
    expect(splitCapture('\n\n')).toEqual([]);
  });

  it('never trims the reader\'s own spacing inside a line', () => {
    expect(splitCapture('   indentado')).toEqual(['   indentado']);
    expect(splitCapture('fim   ')).toEqual(['fim   ']);
  });
});

describe('a first capture into an empty note', () => {
  it('becomes the note, with no delimiter in front of it', () => {
    for (const delimiter of CAPTURE_DELIMITERS) {
      const editor = note('');
      expect(isDocumentEmpty(editor.getView().state.doc)).toBe(true);
      expect(capture(editor, 'Guia de anticoagulação', delimiter.id)).toBe(true);
      // No blank line, no rule, no leading break, whichever delimiter is set.
      expect(editor.getMarkdown()).toBe('Guia de anticoagulação');
    }
  });

  it('lets the note take that text as its own title afterwards', () => {
    // Not a special case: the capture is content now, so the ordinary
    // projection is what names the collapsed note.
    const editor = note('');
    capture(editor, 'Guia de anticoagulação');
    expect(editor.getMarkdown()).toContain('Guia de anticoagulação');
  });
});

describe('the three delimiters', () => {
  it('names all three and defaults to a blank line', () => {
    expect(CAPTURE_DELIMITERS.map((entry) => entry.id)).toEqual([
      'line',
      'blankLine',
      'separator',
    ]);
    expect(DEFAULT_CAPTURE_DELIMITER).toBe('blankLine');
    expect(delimiterLabel('line')).toBe('Linha');
    expect(delimiterLabel('blankLine')).toBe('Linha em branco');
    expect(delimiterLabel('separator')).toBe('Separador');
  });

  it('falls back to a blank line for anything it does not know', () => {
    for (const unknown of ['', 'linha', 'BLANKLINE', 'regex', null, undefined, 7, {}]) {
      expect(normalizeDelimiter(unknown)).toBe('blankLine');
    }
    for (const entry of CAPTURE_DELIMITERS) {
      expect(normalizeDelimiter(entry.id)).toBe(entry.id);
    }
  });

  it('puts each capture on the next line for Linha', () => {
    const editor = note('captura A');
    capture(editor, 'captura B', 'line');
    expect(editor.getRawEditor().getHTML()).toBe('<p>captura A<br>captura B</p>');
    expect(editor.getMarkdown()).toBe('captura A  \ncaptura B');
  });

  it('gives each capture its own paragraph for Linha em branco', () => {
    const editor = note('captura A');
    capture(editor, 'captura B', 'blankLine');
    expect(editor.getMarkdown()).toBe('captura A\n\ncaptura B');
  });

  it('stands a rule between captures for Separador', () => {
    const editor = note('captura A');
    capture(editor, 'captura B', 'separator');
    expect(editor.getMarkdown()).toBe('captura A\n\n---\n\ncaptura B');
  });

  it('applies exactly one delimiter between each pair, never accumulating', () => {
    const editor = note('início');
    capture(editor, 'A', 'blankLine');
    capture(editor, 'B', 'blankLine');
    capture(editor, 'C', 'blankLine');
    expect(editor.getMarkdown()).toBe('início\n\nA\n\nB\n\nC');
    expect(editor.getMarkdown()).not.toContain('\n\n\n');
  });

  it('applies one rule between each pair for Separador', () => {
    const editor = note('início');
    capture(editor, 'A', 'separator');
    capture(editor, 'B', 'separator');
    expect(editor.getMarkdown()).toBe('início\n\n---\n\nA\n\n---\n\nB');
  });

  it('falls back to a paragraph when the last block cannot take a line break', () => {
    // A note ending in a rule or a fence has nothing for `Linha` to continue.
    // Refusing the capture would be worse than laying it out as a paragraph.
    const editor = note('texto\n\n---');
    expect(capture(editor, 'depois da régua', 'line')).toBe(true);
    expect(editor.getMarkdown()).toContain('depois da régua');

    const fenced = note('```\ncódigo\n```');
    expect(capture(fenced, 'fora do bloco', 'line')).toBe(true);
    const markdown = fenced.getMarkdown();
    expect(markdown).toContain('fora do bloco');
    // The capture is not swallowed into the code block.
    expect(markdown.indexOf('fora do bloco')).toBeGreaterThan(markdown.lastIndexOf('```'));
  });

  it('changes nothing already written when the delimiter changes', () => {
    const editor = note('início');
    capture(editor, 'A', 'blankLine');
    const afterFirst = editor.getMarkdown();

    capture(editor, 'B', 'separator');
    // The preference applies to the join it was chosen for and to no other.
    expect(editor.getMarkdown().startsWith(afterFirst)).toBe(true);
    expect(editor.getMarkdown()).toBe('início\n\nA\n\n---\n\nB');
  });
});

describe('where a capture lands', () => {
  it('always goes to the end, whatever the cursor is doing', () => {
    const editor = note('primeira linha\n\núltima linha');
    const view = editor.getView();
    // The reader is mid-document — in another application, in fact, and this
    // caret is wherever they left it.
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 3)));

    capture(editor, 'capturado');
    expect(editor.getMarkdown()).toBe('primeira linha\n\núltima linha\n\ncapturado');
    expect(editor.getMarkdown().endsWith('capturado')).toBe(true);
  });

  it('never moves the selection the reader left behind', () => {
    const editor = note('texto original com bastante espaço');
    const view = editor.getView();
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 3, 8)));
    const before = { from: view.state.selection.from, to: view.state.selection.to };

    capture(editor, 'capturado');

    expect({
      from: view.state.selection.from,
      to: view.state.selection.to,
    }).toEqual(before);
    expect(view.state.doc.textBetween(before.from, before.to)).toBe('xto o');
  });

  it('never takes the focus', () => {
    const editor = note('conteúdo');
    const elsewhere = document.createElement('input');
    document.body.append(elsewhere);
    elsewhere.focus();

    capture(editor, 'capturado');

    expect(document.activeElement).toBe(elsewhere);
    elsewhere.remove();
  });

  it('never asks the note to scroll to what arrived', () => {
    // The reader is looking at another window. Scrolling a note they are not
    // watching to show text they did not ask to see is the wrong kind of help.
    const editor = note('conteúdo');
    const tr = transactionFor(editor, 'capturado', 'blankLine');
    expect(tr).not.toBeNull();
    expect(tr!.scrolledIntoView).toBe(false);
  });
});

describe('undo', () => {
  it('takes back one capture per undo', () => {
    const editor = note('início');
    const view = editor.getView();
    capture(editor, 'A');
    capture(editor, 'B');
    expect(editor.getMarkdown()).toBe('início\n\nA\n\nB');

    undo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe('início\n\nA');

    undo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe('início');
  });

  it('is one step even for a capture of several blocks', () => {
    const editor = note('início');
    const view = editor.getView();
    capture(editor, 'linha um\nlinha dois\nlinha três');
    expect(editor.getMarkdown()).toBe('início\n\nlinha um\n\nlinha dois\n\nlinha três');

    undo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe('início');
  });

  it('is one step for the separator and the rule that comes with it', () => {
    const editor = note('início');
    const view = editor.getView();
    capture(editor, 'A', 'separator');
    undo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe('início');
  });

  it('is one step for a capture that continued the previous line', () => {
    const editor = note('início');
    const view = editor.getView();
    capture(editor, 'A', 'line');
    capture(editor, 'B', 'line');
    expect(editor.getMarkdown()).toBe('início  \nA  \nB');

    undo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe('início  \nA');
    undo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe('início');
  });

  it('puts a capture back on redo', () => {
    const editor = note('início');
    const view = editor.getView();
    capture(editor, 'A');
    undo(view.state, view.dispatch);
    redo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe('início\n\nA');
  });
});

describe('what a capture may not become', () => {
  it('keeps Markdown-looking text literal', () => {
    // A capture is a paste of text, and pasting `**x**` here has never made
    // anything bold. Nothing about AutoPaste changes that.
    const editor = note('');
    capture(editor, '**isso é literal** _e isto_ # não é um título');

    expect(editor.getRawEditor().getHTML()).toBe(
      '<p>**isso é literal** _e isto_ # não é um título</p>',
    );
    expect(editor.getRawEditor().getHTML()).not.toContain('<strong>');
    expect(editor.getRawEditor().getHTML()).not.toContain('<em>');
    expect(editor.getRawEditor().getHTML()).not.toContain('<h1>');

    // ...and it survives the round trip through the stored file as the same
    // characters, because the serializer escapes them.
    const stored = editor.getMarkdown();
    const reopened = note(stored);
    expect(reopened.getRawEditor().getText()).toBe(
      '**isso é literal** _e isto_ # não é um título',
    );
  });

  it('never turns a copied script tag into a script', () => {
    const editor = note('');
    capture(editor, '<script>alert("x")</script>');

    const html = editor.getRawEditor().getHTML();
    expect(html).toBe('<p>&lt;script&gt;alert("x")&lt;/script&gt;</p>');
    // Not a tag anywhere in the real document either.
    expect(editor.getView().dom.querySelector('script')).toBeNull();
    expect(editor.getView().dom.textContent).toContain('<script>alert("x")</script>');
  });

  it('never turns a copied image tag into a load', () => {
    const editor = note('');
    capture(editor, '<img src=x onerror=alert(1)>');

    expect(editor.getView().dom.querySelector('img')).toBeNull();
    expect(editor.getView().dom.querySelector('[onerror]')).toBeNull();
    expect(editor.getRawEditor().getHTML()).toContain('&lt;img');
  });

  it('never turns copied markup into an attribute of the note', () => {
    const editor = note('');
    capture(editor, '<div onclick="roubar()">teste</div><span style="color:red">x</span>');

    const dom = editor.getView().dom;
    expect(dom.querySelector('[onclick]')).toBeNull();
    expect(dom.querySelector('div')).toBeNull();
    expect(dom.querySelector('span[style]')).toBeNull();
    expect(dom.textContent).toContain('<div onclick="roubar()">teste</div>');
  });

  it('leaves a copied URL as the text it was', () => {
    // No fetch, no title lookup, no preview: AutoPaste works on a train.
    const editor = note('');
    capture(editor, 'https://example.com/artigo?x=1');

    expect(editor.getRawEditor().getHTML()).toBe('<p>https://example.com/artigo?x=1</p>');
    expect(editor.getRawEditor().getHTML()).not.toContain('<a ');
  });

  it('leaves a copied javascript: URL as inert text', () => {
    const editor = note('');
    capture(editor, 'javascript:alert(1)');
    expect(editor.getRawEditor().getHTML()).not.toContain('<a ');
    expect(editor.getRawEditor().getText()).toBe('javascript:alert(1)');
  });
});

describe('what a capture must preserve', () => {
  it('keeps accents, emoji, scripts and combining marks intact', () => {
    const editor = note('');
    const text = 'Biópsia hepática 🧪\n日本語\ncafé\ncafé\n🇧🇷 família 👨‍👩‍👧';
    capture(editor, text);

    const rendered = editor.getRawEditor().getText();
    for (const piece of [
      'Biópsia hepática 🧪',
      '日本語',
      'café',
      'café',
      '🇧🇷 família 👨‍👩‍👧',
    ]) {
      expect(rendered).toContain(piece);
    }
    // The decomposed form stays decomposed: nothing normalises the reader's
    // text on the way in.
    expect(rendered).toContain('café');
  });

  it('keeps a multi-line capture as several lines', () => {
    const editor = note('');
    capture(editor, 'linha um\nlinha dois\nlinha três');
    expect(editor.getMarkdown()).toBe('linha um\n\nlinha dois\n\nlinha três');
    expect(editor.getRawEditor().getHTML()).toBe(
      '<p>linha um</p><p>linha dois</p><p>linha três</p>',
    );
  });

  it('keeps the note it was appended to exactly as it was', () => {
    const editor = note('# Biópsia hepática\n\nprimeiro parágrafo');
    capture(editor, 'anexado');
    const markdown = editor.getMarkdown();
    expect(markdown.startsWith('# Biópsia hepática\n\nprimeiro parágrafo')).toBe(true);
    // The heading is still a heading: nothing about the append rewrote it.
    expect(editor.getRawEditor().getHTML()).toContain('<h1>Biópsia hepática</h1>');
  });
});

describe('two deliberate copies of the same words', () => {
  it('are two captures, because they were two actions', () => {
    // No content comparison anywhere on this path. Copying `ABC` twice files
    // it twice, which is what the reader asked for both times.
    const editor = note('');
    capture(editor, 'ABC');
    capture(editor, 'ABC');
    expect(editor.getMarkdown()).toBe('ABC\n\nABC');
  });
});
