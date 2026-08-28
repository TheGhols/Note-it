import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';

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

/**
 * A note holding every kind of calculation beside every kind of block the
 * editor already knew about.
 *
 * Written in the canonical spelling the serializer produces, so a save is a
 * save and not a normalisation.
 */
const MATH_NOTE = [
  '# Orçamento',
  '',
  'preco := 120',
  '',
  'quantidade := 3',
  '',
  'subtotal := preco \\* quantidade',
  '',
  '= subtotal + 10%',
  '',
  '= 10% de 200',
  '',
  'Gastos do mês:',
  '',
  '= 10',
  '',
  '= 20,5',
  '',
  '= sum',
  '',
  '= avg',
  '',
  '= count',
  '',
  '> [!NOTE]',
  '> Um lembrete com `= 2 + 2` dentro.',
  '',
  '```text',
  '= 2 + 2',
  '```',
  '',
  '<!-- = 2 + 2 -->',
  '',
  '- [ ] conferir = 2 + 2',
  '',
  'Fim.',
].join('\n');

describe('a note with calculations is still an ordinary Markdown file', () => {
  it('is written back byte for byte when nothing was edited', () => {
    const { note } = mount(MATH_NOTE);
    expect(note.getMarkdown().trim()).toBe(MATH_NOTE);
  });

  it('is stable across a save, a close and a reopen', () => {
    const first = mount(MATH_NOTE).note.getMarkdown();
    const second = mount(first).note.getMarkdown();
    const third = mount(second).note.getMarkdown();
    expect(second).toBe(first);
    expect(third).toBe(first);
  });

  it('recomputes on reopening rather than carrying a stored result', () => {
    const { element } = mount(MATH_NOTE);
    const shown = results(element);
    expect(shown).toEqual(['360', '396', '20', '10', '20,5', '30,5', '15,25', '2']);

    const reopened = mount(mount(MATH_NOTE).note.getMarkdown());
    expect(results(reopened.element)).toEqual(shown);
  });

  it('puts no result, no marker and no attribute of its own into the file', () => {
    const { note } = mount(MATH_NOTE);
    const saved = note.getMarkdown();
    for (const trace of [
      'note-math-result',
      'data-note-it-math',
      '396',
      '30,5',
      '15,25',
      'contenteditable',
    ]) {
      expect(saved, trace).not.toContain(trace);
    }
  });

  it('leaves every other block exactly as it found it', () => {
    const { element } = mount(MATH_NOTE);
    expect(element.querySelectorAll('h1')).toHaveLength(1);
    expect(element.querySelectorAll('blockquote[data-callout="NOTE"]')).toHaveLength(1);
    expect(element.querySelectorAll('pre code.language-text')).toHaveLength(1);
    expect(element.querySelectorAll('[data-note-it-comment]')).toHaveLength(1);
    expect(element.querySelectorAll('ul[data-type="taskList"] li')).toHaveLength(1);
  });
});

describe('the phases before this one are untouched', () => {
  /** The mixed note from the smart-blocks phase, still round-tripping. */
  const MIXED_NOTE = [
    '# Título da nota',
    '',
    'Texto com **negrito**, *itálico*, <u>sublinhado</u> e um <span data-note-it-color="#1D4ED8" style="color:#1D4ED8">trecho colorido</span>.',
    '',
    'E um <mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">trecho marcado</mark> com <span data-note-it-font-size="22" style="font-size:22px">tamanho próprio</span>.',
    '',
    '- [ ] tarefa aberta',
    '- [x] tarefa concluída <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->',
    '',
    '```python',
    'def soma(a, b):',
    '    return a + b',
    '```',
    '',
    '> uma citação comum',
    '',
    '> [!WARNING]',
    '> Um aviso com **ênfase**.',
    '>',
    '> - detalhe',
    '',
    '<!-- lembrete que não é conteúdo -->',
    '',
    'Parágrafo final.',
  ].join('\n');

  it('round-trips a note with no arithmetic in it at all', () => {
    const { note, element } = mount(MIXED_NOTE);
    expect(note.getMarkdown().trim()).toBe(MIXED_NOTE);
    expect(results(element)).toEqual([]);
  });

  it('leaves an autolink, an arrow and the inline marks alone', () => {
    // The link is written back in the explicit form the serializer has always
    // used for one; everything else on the line is unchanged, and the note is
    // stable from the second save on.
    const source = [
      'Veja [https://example.com](https://example.com) e escreva a → b.',
      '',
      '= 2 + 2',
      '',
      'Texto com **negrito** e `código`.',
    ].join('\n');
    const { note, element } = mount(source);
    expect(note.getMarkdown().trim()).toBe(source);
    expect(results(element)).toEqual(['4']);
    expect(element.querySelectorAll('a[href="https://example.com"]')).toHaveLength(1);
  });

  it('does not turn a sentence that mentions arithmetic into a calculation', () => {
    const source = [
      'A conta 2 + 2 = 4 está certa.',
      '',
      'Total: 360 reais.',
      '',
      'nota: um valor := aqui não',
      '',
      '10',
      '',
      '20',
    ].join('\n');
    const { note, element } = mount(source);
    expect(note.getMarkdown().trim()).toBe(source);
    expect(results(element)).toEqual([]);
  });
});
