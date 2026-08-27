import { afterEach, describe, expect, it } from 'vitest';
import { closeHistory } from '@tiptap/pm/history';
import { NoteEditor } from '../src/editor/editor.ts';
import { ARROW_CHARACTER } from '../src/editor/typography.ts';

/** Drives text through the input-rule plugin exactly as typing does. */
function typeText(editor: any, text: string): void {
  for (const ch of text) {
    const { from, to } = editor.state.selection;
    const handled = editor.view.someProp('handleTextInput', (f: any) =>
      f(editor.view, from, to, ch),
    );
    if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
  }
}

function pressEnter(editor: any): void {
  editor.view.someProp('handleKeyDown', (f: any) =>
    f(editor.view, new KeyboardEvent('keydown', { key: 'Enter' })),
  );
}

function mount(initial = ''): { note: NoteEditor; editor: any } {
  const el = document.createElement('div');
  document.body.append(el);
  const note = new NoteEditor({ element: el, initialContent: initial });
  return { note, editor: note.getRawEditor() };
}

describe('arrow substitution', () => {
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

  it('replaces a typed "->" at the start of a paragraph', () => {
    const { editor } = track(mount());
    typeText(editor, '-> continua');
    expect(editor.state.doc.textContent).toBe(`${ARROW_CHARACTER} continua`);
  });

  it('replaces "->" in the middle of a sentence', () => {
    const { editor } = track(mount());
    typeText(editor, 'antes -> depois');
    expect(editor.state.doc.textContent).toBe(`antes ${ARROW_CHARACTER} depois`);
  });

  it('replaces "->" directly after a word, with no space', () => {
    const { editor } = track(mount());
    typeText(editor, 'a->b');
    expect(editor.state.doc.textContent).toBe(`a${ARROW_CHARACTER}b`);
  });

  it('stores the real arrow character in the Markdown', () => {
    const { note, editor } = track(mount());
    typeText(editor, 'entrada -> saida');
    const markdown = note.getMarkdown();
    expect(markdown).toContain(ARROW_CHARACTER);
    expect(markdown).not.toContain('->');
    // The note does not depend on a font with ligatures to show an arrow.
    expect(markdown.codePointAt(markdown.indexOf(ARROW_CHARACTER))).toBe(0x2192);
  });

  it('works in a heading', () => {
    const { editor } = track(mount());
    typeText(editor, '# Fluxo -> final');
    expect(editor.state.doc.textContent).toBe(`Fluxo ${ARROW_CHARACTER} final`);
    expect(editor.state.doc.firstChild?.type.name).toBe('heading');
  });

  it('works inside a task item', () => {
    const { note, editor } = track(mount());
    typeText(editor, '- [ ] etapa -> proxima');
    expect(editor.state.doc.textContent).toBe(`etapa ${ARROW_CHARACTER} proxima`);
    expect(note.getMarkdown()).toContain(`- [ ] etapa ${ARROW_CHARACTER} proxima`);
  });

  it('works inside an ordinary bullet list', () => {
    const { editor } = track(mount());
    typeText(editor, '- item -> destino');
    expect(editor.state.doc.textContent).toBe(`item ${ARROW_CHARACTER} destino`);
  });

  it('works far into a long note and with pt-BR text', () => {
    const { editor } = track(mount());
    typeText(editor, '# Reunião');
    pressEnter(editor);
    typeText(editor, 'João não está em Goiânia amanhã.');
    pressEnter(editor);
    typeText(editor, 'Ação coração ímpar português.');
    pressEnter(editor);
    typeText(editor, 'São Paulo -> Goiânia');

    const text = editor.state.doc.textContent;
    expect(text).toContain(`São Paulo ${ARROW_CHARACTER} Goiânia`);
    // The accented text before it is untouched.
    expect(text).toContain('João não está em Goiânia amanhã.');
    expect(text).toContain('Ação coração ímpar português.');
  });

  it('leaves "->" alone inside an inline code span', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('use `a -> b` assim');
    expect(editor.state.doc.textContent).toContain('a -> b');
    expect(note.getMarkdown()).toContain('`a -> b`');
    expect(note.getMarkdown()).not.toContain(ARROW_CHARACTER);
  });

  it('leaves "->" alone while typing inside an inline code span', () => {
    const { editor } = track(mount());
    typeText(editor, 'antes ');
    editor.chain().toggleCode().run();
    typeText(editor, 'a->b');

    expect(editor.state.doc.textContent).toBe('antes a->b');
    expect(editor.state.doc.textContent).not.toContain(ARROW_CHARACTER);
  });

  it('leaves "->" alone inside a code block', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('```\nif a -> b\n```');
    expect(editor.state.doc.textContent).toContain('a -> b');
    expect(note.getMarkdown()).not.toContain(ARROW_CHARACTER);

    // And while typing into one.
    const fresh = track(mount());
    fresh.editor.chain().setCodeBlock().run();
    typeText(fresh.editor, 'x -> y');
    expect(fresh.editor.state.doc.textContent).toBe('x -> y');
  });

  it('does not mangle a "-->" sequence', () => {
    const { editor } = track(mount());
    typeText(editor, 'a --> b');
    expect(editor.state.doc.textContent).toBe('a --> b');
    expect(editor.state.doc.textContent).not.toContain(ARROW_CHARACTER);
  });

  it('undo reverts the substitution without touching the text before it', () => {
    const { editor } = track(mount());
    typeText(editor, 'antes ');

    // A real user pauses before typing the arrow; force the same history
    // boundary rather than depending on wall-clock timing in the test.
    editor.view.dispatch(closeHistory(editor.state.tr));
    typeText(editor, '->');
    expect(editor.state.doc.textContent).toBe(`antes ${ARROW_CHARACTER}`);

    editor.commands.undo();

    // The arrow is gone and the surrounding text survived intact.
    expect(editor.state.doc.textContent).toBe('antes ');
  });

  it('the substitution is one undo step, not a partial edit', () => {
    const { editor } = track(mount());
    typeText(editor, 'a');
    editor.view.dispatch(closeHistory(editor.state.tr));
    typeText(editor, '->');
    expect(editor.state.doc.textContent).toBe(`a${ARROW_CHARACTER}`);

    editor.commands.undo();
    // Never leaves a half-replaced "a-" or "a>" behind.
    expect(editor.state.doc.textContent).toBe('a');
  });

  it('round-trips the arrow through save and reload', () => {
    const { note, editor } = track(mount());
    typeText(editor, 'entrada -> saida');
    const saved = note.getMarkdown();

    const reopened = track(mount());
    reopened.note.setMarkdown(saved);
    expect(reopened.note.getMarkdown()).toBe(saved);
    expect(reopened.editor.state.doc.textContent).toContain(ARROW_CHARACTER);
  });
});
