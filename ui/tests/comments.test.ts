import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { decodeCommentText, encodeCommentText, parseCommentBody } from '../src/editor/comment.ts';
import { sanitizeMarkdown } from '../src/markdown/sanitizer.ts';

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

afterEach(() => {
  while (open.length) open.pop()!.destroy();
  document.body.innerHTML = '';
});

describe('comments', () => {
  it('round-trips a simple comment', () => {
    const source = '<!-- lembrete para mim -->';
    expect(roundTrip(source)).toBe(source);
    expect(roundTrip(roundTrip(source))).toBe(source);
  });

  it('round-trips a comment spanning several lines', () => {
    const source = '<!-- primeira linha\nsegunda linha -->';
    expect(roundTrip(source)).toBe(source);
    expect(roundTrip(roundTrip(source))).toBe(source);
  });

  it('survives sanitization instead of being deleted on every save', () => {
    // Before Phase 3.5 the sanitizer dropped every comment, so a note holding
    // one lost it the first time it was written back.
    const source = '<!-- não me apague -->';
    expect(sanitizeMarkdown(source)).toBe(source);
  });

  it('is not part of the note text', () => {
    const { element } = mount('<!-- interno -->\n\nTexto visível.');

    // It is its own block, labelled, and never a paragraph of the note.
    const comment = element.querySelector('[data-note-it-comment]')!;
    expect(comment).not.toBeNull();
    expect(comment.getAttribute('data-note-it-comment-label')).toBe('Comentário');
    expect(comment.tagName.toLowerCase()).toBe('div');

    const paragraphs = Array.from(element.querySelectorAll('p')).map((p) => p.textContent);
    expect(paragraphs).toEqual(['Texto visível.']);
  });

  it('stays editable rather than hidden', () => {
    const { note, element } = mount('<!-- rascunho -->');
    expect(element.querySelector('[data-note-it-comment]')?.textContent).toBe('rascunho');
    expect(note.currentBlock().comment).toBe(true);
  });

  it('never executes what it holds', () => {
    const source = '<!-- <script>alert(1)</script> -->';
    const { element } = mount(source);

    expect(element.querySelector('script')).toBeNull();
    // The tags are characters inside a text node, not markup.
    expect(element.querySelector('[data-note-it-comment]')?.textContent).toBe(
      '<script>alert(1)</script>',
    );
    expect(roundTrip(source)).toBe(source);
  });

  it.each([
    '<!-- <img src=x onerror=alert(1)> -->',
    '<!-- </div><iframe src="javascript:alert(1)"></iframe> -->',
    '<!-- <span data-note-it-color="#fff">x</span> -->',
    '<!-- & < > " \' -->',
  ])('keeps %s as inert text through a round trip', (source) => {
    expect(roundTrip(source)).toBe(source);
    const { element } = mount(source);
    expect(element.querySelectorAll('script, iframe, img, span')).toHaveLength(0);
  });

  it('cannot be closed early by its own content', () => {
    // A `-->` inside would end the comment and spill the rest of the note out.
    const { note } = mount('');
    note.insertComment();
    note.getRawEditor().commands.insertContent('fuga --> aqui');

    const markdown = note.getMarkdown().trim();
    expect(markdown).toBe('<!-- fuga --&gt; aqui -->');
    // One comment, and the text is intact when it is read back.
    expect(roundTrip(markdown)).toBe(markdown);
    expect(mount(markdown).element.querySelector('[data-note-it-comment]')?.textContent).toBe(
      'fuga --> aqui',
    );
  });

  it('escapes and unescapes the terminator symmetrically', () => {
    expect(encodeCommentText('a --> b')).toBe('a --&gt; b');
    expect(decodeCommentText('a --&gt; b')).toBe('a --> b');
    expect(decodeCommentText(encodeCommentText('-->-->'))).toBe('-->-->');
  });

  it('recognises only a whole-block comment', () => {
    expect(parseCommentBody('<!-- oi -->')).toBe('oi');
    expect(parseCommentBody('<!--\nmulti\n-->')).toBe('multi');
    expect(parseCommentBody('<div>oi</div>')).toBeNull();
    expect(parseCommentBody('texto <!-- oi -->')).toBeNull();
    expect(parseCommentBody(null)).toBeNull();
  });

  it('leaves an unterminated opening as text instead of eating the note', () => {
    // There is no comment here, only an opening that never closes.
    expect(sanitizeMarkdown('<!-- sem fim\n\ntexto importante')).toBe(
      '&lt;!-- sem fim\n\ntexto importante',
    );
    expect(roundTrip('<!-- sem fim\n\ntexto importante')).toContain('texto importante');
  });

  it('survives a save, close and reopen', () => {
    const source = '# Nota\n\n<!-- revisar depois -->\n\nCorpo.';
    const saved = mount(source).note.getMarkdown();
    const { note, element } = mount(saved);

    expect(element.querySelector('[data-note-it-comment]')?.textContent).toBe('revisar depois');
    expect(note.getMarkdown().trim()).toBe(source);
  });

  it('still lets a task keep its own completion comment', () => {
    // The task metadata is an inline comment inside the task line, and it is
    // still absorbed by the task rather than becoming a comment block.
    const source = '- [x] feito <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->';
    const { element } = mount(source);

    expect(element.querySelector('[data-note-it-comment]')).toBeNull();
    expect(element.querySelector('[data-completed-at]')).not.toBeNull();
    expect(roundTrip(source)).toBe(source);
  });
});
