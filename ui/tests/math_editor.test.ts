import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { scanMathLines } from '../src/editor/math.ts';
import { declarationIn, ruleFor } from './support/stylesheet.ts';
import { composite, contrastRatio, hexToRgb } from './support/color.ts';

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

/** Every result the note is showing, in document order. */
function results(element: HTMLElement): string[] {
  return Array.from(element.querySelectorAll('.note-math-result')).map(
    (node) => node.textContent ?? '',
  );
}

/** The lines the engine reads out of the current document. */
function lines(note: NoteEditor): (string | null)[] {
  return scanMathLines(note.getRawEditor().state.doc).map((line) => line.text);
}

describe('a note calculates as it is written', () => {
  it('shows the result of an expression beside its line', () => {
    const { element } = mount('= 2 + 2');
    expect(results(element)).toEqual(['4']);
  });

  it('resolves variables declared above the expression', () => {
    const { element } = mount(
      ['preco := 120', '', 'quantidade := 3', '', '= preco * quantidade'].join('\n'),
    );
    expect(results(element)).toEqual(['360']);
  });

  it('reads a note whose lines share one paragraph, as another editor writes it', () => {
    // A `.md` written elsewhere with no blank line between its lines arrives
    // as a single paragraph carrying newlines. Every one of them is a line.
    const { note, element } = mount(
      ['preco := 120', 'quantidade := 3', '= preco * quantidade'].join('\n'),
    );
    expect(note.getRawEditor().state.doc.childCount).toBe(1);
    expect(lines(note)).toEqual(['preco := 120', 'quantidade := 3', '= preco * quantidade']);
    expect(results(element)).toEqual(['360']);
  });

  it('reads a line ended with a hard break', () => {
    const { note, element } = mount('= 2 + 2  \n= 3 + 3');
    expect(lines(note)).toEqual(['= 2 + 2', '= 3 + 3']);
    expect(results(element)).toEqual(['4', '6']);
  });

  it('aggregates the block of calculations above the aggregator', () => {
    const { element } = mount(['= 10', '', '= 20', '', '= 30', '', '= sum'].join('\n'));
    expect(results(element)).toEqual(['10', '20', '30', '60']);
  });

  it('shows an error beside the line that failed and nowhere else', () => {
    const { element } = mount(['= 2 + 2', '', '= valor_que_nao_existe * 2'].join('\n'));
    expect(results(element)).toEqual(['4', 'variável desconhecida']);
    expect(element.querySelectorAll('[data-note-it-math="error"]')).toHaveLength(1);
  });

  it('names each kind of failure in words, and never with a stack trace', () => {
    const { element } = mount(['= 1 / 0', '', '= (2 + 3', '', '12preco := 1'].join('\n'));
    expect(results(element)).toEqual([
      'divisão por zero',
      'expressão inválida',
      'nome inválido',
    ]);
  });
});

describe('results follow the note as it is edited', () => {
  it('updates every dependent line the moment a variable changes', () => {
    const { note, element } = mount(['preco := 100', '', '= preco * 3'].join('\n'));
    expect(results(element)).toEqual(['300']);

    // Exactly what typing over the `100` does: replace it in place.
    const editor = note.getRawEditor();
    const at = editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n').indexOf('100');
    editor.commands.insertContentAt({ from: at + 1, to: at + 4 }, '150');

    expect(results(element)).toEqual(['450']);
  });

  it('updates when the expression itself is edited', () => {
    const { note, element } = mount('= 2 + 2');
    expect(results(element)).toEqual(['4']);

    note.getRawEditor().commands.insertContentAt(
      { from: 1, to: note.getRawEditor().state.doc.content.size - 1 },
      '= 10 * 8',
    );
    expect(results(element)).toEqual(['80']);
  });

  it('needs nothing to be pressed, saved or reopened for a result to appear', () => {
    const { note, element } = mount('');
    expect(results(element)).toEqual([]);

    note.getRawEditor().commands.insertContent('= 7 * 6');
    expect(results(element)).toEqual(['42']);
  });

  it('invalidates the dependants when a declaration is deleted, and recovers them', () => {
    const { note, element } = mount(['preco := 100', '', '= preco * 2'].join('\n'));
    expect(results(element)).toEqual(['200']);

    const editor = note.getRawEditor();
    // Remove the whole declaration paragraph.
    const declaration = editor.state.doc.child(0);
    editor.commands.deleteRange({ from: 0, to: declaration.nodeSize });
    expect(results(element)).toEqual(['variável desconhecida']);

    editor.commands.undo();
    expect(results(element)).toEqual(['200']);
  });
});

describe('undo and redo carry the results with them', () => {
  it('restores the earlier value and everything derived from it', () => {
    const { note, element } = mount('preco := 100\n\n= preco * 2');
    const editor = note.getRawEditor();
    expect(results(element)).toEqual(['200']);

    const at = editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n').indexOf('100');
    editor.commands.insertContentAt({ from: at + 1, to: at + 4 }, '200');
    expect(results(element)).toEqual(['400']);

    editor.commands.undo();
    expect(editor.getText()).toContain('preco := 100');
    expect(results(element)).toEqual(['200']);

    editor.commands.redo();
    expect(editor.getText()).toContain('preco := 200');
    expect(results(element)).toEqual(['400']);
  });

  it('adds no undo step of its own', () => {
    // A result is not an edit, so one undo puts back exactly one edit.
    const { note } = mount('= 1 + 1');
    const editor = note.getRawEditor();
    editor.commands.focus('end');
    editor.commands.insertContent('0');
    expect(editor.getText()).toBe('= 1 + 10');

    editor.commands.undo();
    expect(editor.getText()).toBe('= 1 + 1');
  });
});

describe('the ordinary editing keys are unaffected', () => {
  it('keeps typing, Enter and Backspace working around a calculated line', () => {
    const { note, element } = mount('= 2 + 2');
    const editor = note.getRawEditor();

    editor.commands.focus('end');
    editor.commands.splitBlock();
    editor.commands.insertContent('= 3 * 3');
    expect(results(element)).toEqual(['4', '9']);

    // Backspace over the last character.
    const end = editor.state.doc.content.size - 1;
    editor.commands.deleteRange({ from: end - 1, to: end });
    expect(editor.getText()).toMatch(/= 3 \* $/);
    expect(results(element)).toEqual(['4', 'expressão inválida']);
  });

  it('leaves the cursor where the writer put it', () => {
    const { note } = mount('= 2 + 2');
    const editor = note.getRawEditor();
    editor.commands.setTextSelection(4);
    expect(editor.state.selection.from).toBe(4);
    expect(editor.state.selection.empty).toBe(true);

    // Selecting the whole line covers the text and nothing else: a result is a
    // decoration, so it is not in the document to be selected.
    editor.commands.selectAll();
    const selected = editor.state.doc.textBetween(
      editor.state.selection.from,
      editor.state.selection.to,
      '\n',
    );
    expect(selected).toBe('= 2 + 2');
  });

  it('survives a paste of several calculating lines', () => {
    const { note, element } = mount('');
    note.getRawEditor().commands.insertContent('<p>preco := 5</p><p>= preco * 4</p>');
    expect(results(element)).toEqual(['20']);
  });

  it('leaves pt-BR composition and dead keys to the editor', () => {
    // The plugin adds no key handler and dispatches no transaction, so an
    // accented character arrives as the character it is.
    const { note, element } = mount('= 2 + 2');
    const editor = note.getRawEditor();
    editor.commands.focus('end');
    editor.commands.splitBlock();
    editor.commands.insertContent('ação e não');
    expect(editor.getText()).toContain('ação e não');
    expect(results(element)).toEqual(['4']);
  });
});

describe('where a calculation is not read', () => {
  it('ignores a fenced code block', () => {
    const { note, element } = mount('```text\n= 2 + 2\n```');
    expect(lines(note)).toEqual([null]);
    expect(results(element)).toEqual([]);
  });

  it('ignores an inline code span', () => {
    const { note, element } = mount('`= 2 + 2`');
    expect(lines(note)).toEqual([null]);
    expect(results(element)).toEqual([]);
  });

  it('ignores a comment', () => {
    const { note, element } = mount('<!-- = 2 + 2 -->');
    expect(lines(note)).toEqual([null]);
    expect(results(element)).toEqual([]);
  });

  it('ignores a heading, a list, a task and a quote', () => {
    const { note, element } = mount(
      ['# = 2 + 2', '', '- = 2 + 2', '', '- [ ] = 2 + 2', '', '> = 2 + 2'].join('\n'),
    );
    expect(lines(note)).toEqual([null, null, null, null]);
    expect(results(element)).toEqual([]);
  });

  it('ignores a callout, so the smart blocks are untouched', () => {
    const { note, element } = mount('> [!WARNING]\n> = 2 + 2');
    expect(lines(note)).toEqual([null]);
    expect(results(element)).toEqual([]);
    expect(element.querySelectorAll('blockquote[data-callout="WARNING"]')).toHaveLength(1);
  });

  it('still counts the block it skipped, so it breaks a run of values', () => {
    const { element } = mount(['= 10', '', '```text', 'nada', '```', '', '= 20', '', '= sum'].join('\n'));
    expect(results(element)).toEqual(['10', '20', '20']);
  });

  it('reads a calculation standing beside a code block in the same note', () => {
    const { element } = mount('```js\nconst a = 1;\n```\n\n= 2 + 2');
    expect(results(element)).toEqual(['4']);
  });
});

describe('a result is decoration, never content', () => {
  it('writes nothing into the Markdown the note saves', () => {
    const source = [
      'preco := 120',
      '',
      'quantidade := 3',
      '',
      '= preco * quantidade',
      '',
      '= 10% de 200',
    ].join('\n');
    const { note, element } = mount(source);

    expect(results(element)).toEqual(['360', '20']);
    // `*` is escaped by the Markdown serializer, as it is in any prose.
    // Byte for byte the note that was written, with only the `*` escape the
    // Markdown serializer applies to any prose. Not one result reached it.
    expect(note.getMarkdown().trim()).toBe(
      source.replace('preco * quantidade', 'preco \\* quantidade'),
    );
    expect(note.getMarkdown()).not.toContain('360');
  });

  it('survives a save and a reopen by being recomputed, not stored', () => {
    const source = [
      'preco := 120',
      '',
      '= preco + 30',
      '',
      'Compras do mês:',
      '',
      '= 10',
      '',
      '= 20',
      '',
      '= sum',
    ].join('\n');
    const { note, element } = mount(source);
    expect(results(element)).toEqual(['150', '10', '20', '30']);

    const saved = note.getMarkdown();
    const reopened = mount(saved);
    expect(results(reopened.element)).toEqual(['150', '10', '20', '30']);
    // The file did not grow a single character between the two saves.
    expect(reopened.note.getMarkdown()).toBe(saved);
  });

  it('is not part of the document, so it cannot be selected or serialized', () => {
    const { note, element } = mount('= 2 + 2');
    expect(results(element)).toEqual(['4']);

    const editor = note.getRawEditor();
    expect(editor.state.doc.textContent).toBe('= 2 + 2');
    expect(editor.getText()).toBe('= 2 + 2');
    expect(editor.getHTML()).not.toContain('note-math-result');

    const widget = element.querySelector('.note-math-result')!;
    expect(widget.getAttribute('contenteditable')).toBe('false');
  });

  it('never reports an edit, so nothing about it can move updated_at', () => {
    const updates: string[] = [];
    const element = document.createElement('div');
    document.body.append(element);
    const note = new NoteEditor({
      element,
      initialContent: 'preco := 100\n\n= preco * 2',
      onUpdate: (markdown) => updates.push(markdown),
    });
    open.push(note);

    expect(results(element)).toEqual(['200']);
    // Loading a note evaluates it. Evaluating it is not editing it.
    expect(updates).toEqual([]);
    expect(note.hasPendingSave()).toBe(false);

    // Reloading the same note over the top is the same story.
    note.setMarkdown('preco := 150\n\n= preco * 2');
    expect(results(element)).toEqual(['300']);
    expect(updates).toEqual([]);
    expect(note.hasPendingSave()).toBe(false);
  });
});

describe('a result is legible on every paper, in either theme', () => {
  const PAPERS: Record<string, { bg: string; text: string; muted: string }> = {
    yellow: { bg: '#FEF9C3', text: '#1E293B', muted: '#64748B' },
    blue: { bg: '#E0F2FE', text: '#0F172A', muted: '#475569' },
    green: { bg: '#DCFCE7', text: '#064E3B', muted: '#335C4D' },
    pink: { bg: '#FCE7F3', text: '#831843', muted: '#701A75' },
    purple: { bg: '#F3E8FF', text: '#3B0764', muted: '#581C87' },
    gray: { bg: '#F1F5F9', text: '#0F172A', muted: '#64748B' },
    black: { bg: '#18181B', text: '#F4F4F5', muted: '#A1A1AA' },
  };

  it('is painted from the paper tokens rather than from colours of its own', () => {
    const rule = ruleFor('.ProseMirror .note-math-result');
    expect(declarationIn('.ProseMirror .note-math-result', 'background-color')).toBe(
      'var(--block-bg)',
    );
    // The ink is mixed from the two the paper already defines, with the plain
    // muted one standing in wherever `color-mix` is unavailable.
    expect(rule.body).toContain('color: var(--paper-muted)');
    expect(rule.body).toContain(
      'color-mix(in srgb, var(--paper-text) 30%, var(--paper-muted))',
    );
    // No literal colour anywhere in the rule: a result belongs to the paper it
    // lands on, so it needs no palette of its own and no theme override.
    expect(rule.body).not.toMatch(/#[0-9A-Fa-f]{3,8}/);
    expect(rule.body).not.toMatch(/\brgba?\(/);
  });

  it('reads on all seven papers', () => {
    // `--block-bg` is the paper's own ink at low alpha, exactly as the comment
    // and code grounds are, so the result sits on the same surface they do.
    // `color-mix(in srgb, …)` is a plain per-channel mix of the two inks.
    for (const [name, paper] of Object.entries(PAPERS)) {
      const ink: [number, number, number] =
        name === 'black' ? [255, 255, 255] : [15, 23, 42];
      const ground = composite(ink, name === 'black' ? 0.06 : 0.055, hexToRgb(paper.bg));
      const result = composite(hexToRgb(paper.text), 0.3, hexToRgb(paper.muted));
      expect(contrastRatio(result, ground), name).toBeGreaterThanOrEqual(4.5);

      // ...and quieter than the note's own text wherever the paper defines a
      // quieter ink at all. The pink palette does not: its `--paper-muted` is
      // darker than its `--paper-text`, so on that one paper there is nothing
      // to stand back from, and the smaller size and the chip do the work.
      const mutedIsQuieter =
        contrastRatio(hexToRgb(paper.muted), ground) < contrastRatio(hexToRgb(paper.text), ground);
      if (mutedIsQuieter) {
        expect(contrastRatio(result, ground), name).toBeLessThan(
          contrastRatio(hexToRgb(paper.text), ground),
        );
      }
    }
  });

  it('does not take part in the selection or in pointer interaction', () => {
    expect(declarationIn('.ProseMirror .note-math-result', 'user-select')).toBe('none');
    expect(declarationIn('.ProseMirror .note-math-result', 'pointer-events')).toBe('none');
  });
});

describe('the math engine stays cheap', () => {
  /** A note far past anything a post-it holds: prose, variables, expressions
   *  and the three aggregators, all reactive. */
  function bigNote(): string {
    const parts: string[] = ['# Orçamento', ''];
    for (let index = 0; index < 20; index += 1) parts.push(`v${index} := ${index + 1}`, '');
    for (let index = 0; index < 100; index += 1) {
      parts.push(`Linha ${index} de texto comum, com contexto e alguma prosa.`, '');
    }
    for (let index = 0; index < 50; index += 1) {
      parts.push(`= v${index % 20} * ${index + 1} + 10%`, '');
    }
    parts.push('= sum', '', '= avg', '', '= count', '', 'Parágrafo final onde vou digitar.');
    return parts.join('\n');
  }

  it('does not make typing expensive in a note full of expressions', () => {
    // The budgets sit an order of magnitude above what this measures, so they
    // catch an engine that started doing real work per keystroke without
    // failing on the ordinary variance of a shared machine.
    const element = document.createElement('div');
    document.body.append(element);

    const openedAt = Date.now();
    const note = new NoteEditor({ element, initialContent: bigNote() });
    open.push(note);
    expect(Date.now() - openedAt).toBeLessThan(2000);

    const editor = note.getRawEditor();
    editor.commands.focus('end');
    const typedAt = Date.now();
    for (let index = 0; index < 40; index += 1) editor.commands.insertContent('a');
    const perKeystroke = (Date.now() - typedAt) / 40;
    expect(perKeystroke).toBeLessThan(25);

    // ...and it really was calculating all of it.
    expect(results(element)).toHaveLength(53);
    expect(results(element).at(-1)).toBe('50');
  });

  it('evaluates a whole large note in well under a frame', () => {
    const element = document.createElement('div');
    document.body.append(element);
    const note = new NoteEditor({ element, initialContent: bigNote() });
    open.push(note);

    const doc = note.getRawEditor().state.doc;
    const startedAt = performance.now();
    for (let index = 0; index < 50; index += 1) scanMathLines(doc);
    expect((performance.now() - startedAt) / 50).toBeLessThan(5);
  });
});
