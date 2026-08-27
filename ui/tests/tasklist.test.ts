import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { sanitizeMarkdown } from '../src/markdown/sanitizer.ts';
import {
  extractCompletedAt,
  isValidCompletedAt,
  renderCompletedAtComment,
} from '../src/markdown/taskMeta.ts';

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

function taskItems(editor: any): Array<{ checked: boolean; completedAt: string | null; text: string }> {
  const found: Array<{ checked: boolean; completedAt: string | null; text: string }> = [];
  editor.state.doc.descendants((node: any) => {
    if (node.type.name !== 'taskItem') return;
    found.push({
      checked: node.attrs.checked,
      completedAt: node.attrs.completedAt,
      text: node.textContent,
    });
  });
  return found;
}

function setChecked(editor: any, index: number, checked: boolean): void {
  const positions: number[] = [];
  editor.state.doc.descendants((node: any, pos: number) => {
    if (node.type.name === 'taskItem') positions.push(pos);
  });
  const pos = positions[index];
  const node = editor.state.doc.nodeAt(pos);
  editor.view.dispatch(
    editor.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, checked }),
  );
}

describe('task list input rule', () => {
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

  it('turns "- [ ] " into an unchecked task instead of a bullet', () => {
    const { note, editor } = track(mount());
    typeText(editor, '- [ ] Comprar leite');

    const tasks = taskItems(editor);
    expect(tasks).toHaveLength(1);
    expect(tasks[0].checked).toBe(false);
    expect(tasks[0].text).toBe('Comprar leite');
    // The literal marker must not survive as bullet text.
    expect(note.getMarkdown()).toContain('- [ ] Comprar leite');
    expect(note.getMarkdown()).not.toContain('\\[');
  });

  it('turns "- [x] " and "- [X] " into a completed task', () => {
    for (const marker of ['- [x] ', '- [X] ']) {
      const { editor } = track(mount());
      typeText(editor, `${marker}Feita`);
      const tasks = taskItems(editor);
      expect(tasks).toHaveLength(1);
      expect(tasks[0].checked).toBe(true);
      expect(tasks[0].text).toBe('Feita');
    }
  });

  it('works after a heading, after paragraphs and inside a long note', () => {
    const { editor } = track(mount());
    typeText(editor, '# Titulo');
    pressEnter(editor);
    typeText(editor, 'Primeiro paragrafo');
    pressEnter(editor);
    typeText(editor, 'Segundo paragrafo com bastante texto para alongar a nota');
    pressEnter(editor);
    typeText(editor, '- [ ] Tarefa no meio');

    const tasks = taskItems(editor);
    expect(tasks).toHaveLength(1);
    expect(tasks[0].checked).toBe(false);
    expect(tasks[0].text).toBe('Tarefa no meio');
    expect(editor.state.doc.textContent).toContain('Titulo');
  });

  it('works right after an ordinary bullet list', () => {
    const { editor } = track(mount());
    typeText(editor, '- item comum');
    pressEnter(editor);
    pressEnter(editor);
    typeText(editor, '- [ ] tarefa');

    const tasks = taskItems(editor);
    expect(tasks).toHaveLength(1);
    expect(tasks[0].text).toBe('tarefa');
  });

  it('Enter after a task creates the next task', () => {
    const { editor } = track(mount());
    typeText(editor, '- [ ] Primeira');
    pressEnter(editor);
    typeText(editor, 'Segunda');

    const tasks = taskItems(editor);
    expect(tasks).toHaveLength(2);
    expect(tasks.map((t) => t.text)).toEqual(['Primeira', 'Segunda']);
    expect(tasks[1].checked).toBe(false);
  });

  it('round-trips three nested levels', () => {
    const source = '- [ ] Nivel 1\n  - [ ] Nivel 2\n    - [x] Nivel 3';
    const { note } = track(mount());
    note.setMarkdown(source);

    const output = note.getMarkdown();
    expect(output).toContain('- [ ] Nivel 1');
    expect(output).toContain('  - [ ] Nivel 2');
    expect(output).toContain('    - [x] Nivel 3');
  });
});

describe('task completion timestamps', () => {
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

  it('records completed_at when a task is checked', () => {
    const { editor } = track(mount());
    typeText(editor, '- [ ] Comprar material');
    expect(taskItems(editor)[0].completedAt).toBeNull();

    setChecked(editor, 0, true);

    const stamped = taskItems(editor)[0];
    expect(stamped.checked).toBe(true);
    expect(isValidCompletedAt(stamped.completedAt)).toBe(true);
  });

  it('clears completed_at when a task is reopened', () => {
    const { editor } = track(mount());
    typeText(editor, '- [ ] Tarefa');
    setChecked(editor, 0, true);
    expect(taskItems(editor)[0].completedAt).not.toBeNull();

    setChecked(editor, 0, false);

    expect(taskItems(editor)[0].checked).toBe(false);
    // No silently kept old date after reopening.
    expect(taskItems(editor)[0].completedAt).toBeNull();
  });

  it('mints a fresh timestamp when a task is completed again', () => {
    const { editor } = track(mount());
    typeText(editor, '- [ ] Tarefa');
    setChecked(editor, 0, true);
    const first = taskItems(editor)[0].completedAt!;

    setChecked(editor, 0, false);
    setChecked(editor, 0, true);
    const second = taskItems(editor)[0].completedAt!;

    expect(isValidCompletedAt(second)).toBe(true);
    expect(new Date(second).getTime()).toBeGreaterThanOrEqual(new Date(first).getTime());
  });

  it('never invents a date for a task completed outside Note-it', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('- [x] tarefa antiga\n- [ ] pendente');

    const tasks = taskItems(editor);
    expect(tasks[0].checked).toBe(true);
    // Loaded as done, with no fabricated completion date.
    expect(tasks[0].completedAt).toBeNull();
    expect(note.getMarkdown()).toContain('- [x] tarefa antiga');
    expect(note.getMarkdown()).not.toContain('completed_at');
  });

  it('round-trips a completion date through Markdown', () => {
    const stamp = '2026-08-27T11:32:00-03:00';
    const source = `- [x] Comprar material <!-- note-it:completed_at=${stamp} -->\n- [ ] Pendente`;
    const { note, editor } = track(mount());
    note.setMarkdown(source);

    const tasks = taskItems(editor);
    expect(tasks[0].completedAt).toBe(stamp);
    // The metadata never becomes visible text in the editor.
    expect(tasks[0].text).toBe('Comprar material');

    const output = note.getMarkdown();
    expect(output).toContain(`- [x] Comprar material <!-- note-it:completed_at=${stamp} -->`);
    expect(output).toContain('- [ ] Pendente');
  });

  it('survives a save and reload cycle', () => {
    const { note, editor } = track(mount());
    typeText(editor, '- [ ] Comprar pão');
    setChecked(editor, 0, true);
    const saved = note.getMarkdown();
    const stamp = taskItems(editor)[0].completedAt;

    const reopened = track(mount());
    reopened.note.setMarkdown(saved);

    const tasks = taskItems(reopened.editor);
    expect(tasks[0].checked).toBe(true);
    expect(tasks[0].completedAt).toBe(stamp);
    expect(tasks[0].text).toBe('Comprar pão');
  });

  it('rejects a malformed or hostile completion timestamp', () => {
    for (const value of [
      'not-a-date',
      '2026-08-27',
      '2026-08-27T11:32:00',
      '"><script>alert(1)</script>',
      '9999999999999999999',
    ]) {
      expect(isValidCompletedAt(value)).toBe(false);
      expect(renderCompletedAtComment(value)).toBe('');
      expect(extractCompletedAt(`x <!-- note-it:completed_at=${value} -->`).completedAt).toBeNull();
    }
  });

  it('carries the task comment through sanitization untouched', () => {
    const kept = '- [x] ok <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->';
    expect(sanitizeMarkdown(kept)).toBe(kept);

    // Since Phase 3.5 every comment survives sanitization, not only this one.
    // The task metadata is still the only comment the task itself absorbs:
    // any other stays in the note as the comment it is.
    expect(sanitizeMarkdown('texto <!-- rastreador -->')).toBe('texto <!-- rastreador -->');
    const malformed = 'a <!-- note-it:completed_at=hoje --> b';
    expect(sanitizeMarkdown(malformed)).toBe(malformed);
  });

  it('never lets a malformed completion date reach a task', () => {
    const { note } = track(mount());
    note.setMarkdown('- [x] ok <!-- note-it:completed_at=hoje -->');
    // The date is rejected, and the comment is not silently adopted as one.
    expect(note.getRawEditor().getHTML()).not.toContain('data-completed-at');
  });

  it('shows a completed task struck through with its date', () => {
    const { note, editor } = track(mount());
    note.setMarkdown('- [x] Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->');

    const html = editor.getHTML();
    // The strike is applied by the stylesheet through data-checked.
    expect(html).toContain('data-checked="true"');
    expect(html).toContain('data-completed-at="2026-08-27T11:32:00-03:00"');
    expect(html).toContain('Concluído 27/08/2026');
  });
});
