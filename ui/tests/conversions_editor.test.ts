import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { scanMathLines } from '../src/editor/math.ts';

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

function results(element: HTMLElement): string[] {
  return Array.from(element.querySelectorAll('.note-math-result')).map(
    (node) => node.textContent ?? '',
  );
}

describe('a note converts as it is written', () => {
  it('shows the converted quantity beside the line', () => {
    const { element } = mount(['= 10 km em m', '', '= 1500 m em km'].join('\n'));
    expect(results(element)).toEqual(['10000 m', '1,5 km']);
  });

  it('reuses the result decoration rather than a second visual mechanism', () => {
    const { element } = mount('= 0 C em F');
    const chips = element.querySelectorAll('.note-math-result');
    expect(chips).toHaveLength(1);
    expect(chips[0].getAttribute('data-note-it-math')).toBe('value');
    expect(chips[0].getAttribute('contenteditable')).toBe('false');
    expect(chips[0].textContent).toBe('32 °F');
  });

  it('shows a failed conversion in the same quiet words', () => {
    const { element } = mount(
      ['= 10 kg em km', '', '= 10 banana em m', '', '= -300 C em K'].join('\n'),
    );
    expect(results(element)).toEqual([
      'unidades incompatíveis',
      'unidade desconhecida',
      'conversão inválida',
    ]);
    expect(element.querySelectorAll('[data-note-it-math="error"]')).toHaveLength(3);
  });

  it('converts and calculates in the same note', () => {
    const { element } = mount(
      [
        '= 2 + 2',
        '',
        '= 10% de 200',
        '',
        '= 10 km em m',
        '',
        'preco := 120',
        '',
        '= preco * 3',
        '',
        '= 200 + 10%',
      ].join('\n'),
    );
    expect(results(element)).toEqual(['4', '20', '10000 m', '360', '220']);
  });
});

describe('a conversion follows the note as it is edited', () => {
  it('updates the moment its variable changes', () => {
    const { note, element } = mount(['distancia := 5', '', '= distancia km em m'].join('\n'));
    expect(results(element)).toEqual(['5000 m']);

    const editor = note.getRawEditor();
    const text = editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n');
    const at = text.indexOf('5');
    editor.commands.insertContentAt({ from: at + 1, to: at + 2 }, '10');

    expect(editor.getText()).toContain('distancia := 10');
    expect(results(element)).toEqual(['10000 m']);
  });

  it('updates when the target unit is edited', () => {
    const { note, element } = mount('= 1 km em m');
    expect(results(element)).toEqual(['1000 m']);

    const size = note.getRawEditor().state.doc.content.size;
    note.getRawEditor().commands.insertContentAt({ from: size - 1, to: size - 1 }, 'm');
    expect(note.getRawEditor().getText()).toBe('= 1 km em mm');
    expect(results(element)).toEqual(['1000000 mm']);
  });

  it('needs nothing pressed, saved or reopened for a conversion to appear', () => {
    const { note, element } = mount('');
    expect(results(element)).toEqual([]);
    note.getRawEditor().commands.insertContent('= 1000 g em kg');
    expect(results(element)).toEqual(['1 kg']);
  });
});

describe('undo and redo carry a conversion with them', () => {
  it('restores the earlier value and the result derived from it', () => {
    const { note, element } = mount('distancia := 10\n\n= distancia km em m');
    const editor = note.getRawEditor();
    expect(results(element)).toEqual(['10000 m']);

    const text = editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n');
    const at = text.indexOf('10');
    editor.commands.insertContentAt({ from: at + 1, to: at + 3 }, '20');
    expect(results(element)).toEqual(['20000 m']);

    editor.commands.undo();
    expect(editor.getText()).toContain('distancia := 10');
    expect(results(element)).toEqual(['10000 m']);

    editor.commands.redo();
    expect(editor.getText()).toContain('distancia := 20');
    expect(results(element)).toEqual(['20000 m']);
  });

  it('adds no undo step of its own', () => {
    const { note } = mount('= 1 km em m');
    const editor = note.getRawEditor();
    editor.commands.focus('end');
    editor.commands.insertContent('m');
    expect(editor.getText()).toBe('= 1 km em mm');

    editor.commands.undo();
    expect(editor.getText()).toBe('= 1 km em m');
  });
});

describe('a conversion is read from the same places a calculation is', () => {
  it('is ignored inside a code block, an inline span and a comment', () => {
    const { note, element } = mount(
      ['```text', '= 100 km em m', '```', '', '`= 10 km em m`', '', '<!-- = 10 km em m -->'].join(
        '\n',
      ),
    );
    expect(scanMathLines(note.getRawEditor().state.doc).map((line) => line.text)).toEqual([
      null,
      null,
      null,
    ]);
    expect(results(element)).toEqual([]);
  });

  it('is ignored inside a heading, a list, a task, a quote and a callout', () => {
    const { element } = mount(
      [
        '# = 10 km em m',
        '',
        '- = 10 km em m',
        '',
        '- [ ] = 10 km em m',
        '',
        '> = 10 km em m',
        '',
        '> [!NOTE]',
        '> = 10 km em m',
      ].join('\n'),
    );
    expect(results(element)).toEqual([]);
  });

  it('is read from a plain paragraph beside all of them', () => {
    const { element } = mount('```js\nconst a = 1;\n```\n\n= 10 km em m');
    expect(results(element)).toEqual(['10000 m']);
  });
});

describe('a converted result is decoration, never content', () => {
  const NOTE = [
    '= 10 km em m',
    '',
    '= 2 + 2',
    '',
    'preco := 100',
    '',
    '= preco \\* 2',
    '',
    '> quote',
    '',
    '- [ ] tarefa',
    '',
    '```text',
    '= 100 km em m',
    '```',
    '',
    '<!-- = 10 km em m -->',
  ].join('\n');

  it('writes nothing into the Markdown the note saves', () => {
    const { note, element } = mount(NOTE);
    expect(results(element)).toEqual(['10000 m', '4', '200']);
    expect(note.getMarkdown().trim()).toBe(NOTE);
  });

  it('is stable across a save, a close and a reopen', () => {
    const first = mount(NOTE).note.getMarkdown();
    const second = mount(first).note.getMarkdown();
    expect(second).toBe(first);

    const reopened = mount(second);
    expect(results(reopened.element)).toEqual(['10000 m', '4', '200']);
  });

  it('leaves no trace of a unit or a result in the file', () => {
    const { note } = mount(NOTE);
    const saved = note.getMarkdown();
    for (const trace of ['10000', '10 000', 'note-math-result', 'data-note-it-math']) {
      expect(saved, trace).not.toContain(trace);
    }
  });

  it('is not in the document, so it cannot be selected or serialized', () => {
    const { note, element } = mount('= 10 km em m');
    const editor = note.getRawEditor();
    expect(editor.state.doc.textContent).toBe('= 10 km em m');
    expect(editor.getText()).toBe('= 10 km em m');
    expect(editor.getHTML()).not.toContain('10000');

    editor.commands.selectAll();
    const selected = editor.state.doc.textBetween(
      editor.state.selection.from,
      editor.state.selection.to,
      '\n',
    );
    expect(selected).toBe('= 10 km em m');
    expect(element.querySelector('.note-math-result')!.textContent).toBe('10000 m');
  });

  it('never reports an edit, so nothing about it can move updated_at', () => {
    const updates: string[] = [];
    const element = document.createElement('div');
    document.body.append(element);
    const note = new NoteEditor({
      element,
      initialContent: 'distancia := 5\n\n= distancia km em m',
      onUpdate: (markdown) => updates.push(markdown),
    });
    open.push(note);

    expect(results(element)).toEqual(['5000 m']);
    expect(updates).toEqual([]);
    expect(note.hasPendingSave()).toBe(false);

    note.setMarkdown('distancia := 10\n\n= distancia km em m');
    expect(results(element)).toEqual(['10000 m']);
    expect(updates).toEqual([]);
    expect(note.hasPendingSave()).toBe(false);
  });
});

describe('conversions stay cheap', () => {
  /** A note with prose, variables, calculations, conversions and aggregators. */
  function bigNote(): string {
    const parts: string[] = ['# Medidas', ''];
    for (let index = 0; index < 20; index += 1) parts.push(`v${index} := ${index + 1}`, '');
    for (let index = 0; index < 100; index += 1) {
      parts.push(`Linha ${index} de texto comum, com contexto e alguma prosa.`, '');
    }
    for (let index = 0; index < 50; index += 1) {
      parts.push(`= v${index % 20} * ${index + 1} + 10%`, '');
    }
    const pairs = [
      ['km', 'm'],
      ['kg', 'g'],
      ['L', 'mL'],
      ['C', 'F'],
      ['h', 'min'],
      ['m2', 'cm2'],
      ['GB', 'MB'],
      ['km/h', 'm/s'],
    ];
    for (let index = 0; index < 50; index += 1) {
      const [from, to] = pairs[index % pairs.length];
      parts.push(`= v${index % 20} ${from} em ${to}`, '');
    }
    parts.push('= sum', '', '= avg', '', '= count', '', 'Parágrafo final onde vou digitar.');
    return parts.join('\n');
  }

  it('does not make typing expensive in a note full of conversions', () => {
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
    expect((Date.now() - typedAt) / 40).toBeLessThan(25);

    // 50 calculations, 50 conversions and the three aggregators.
    expect(results(element)).toHaveLength(103);
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
