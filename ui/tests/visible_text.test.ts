import { inject } from 'vitest';
import { describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { noteTitle } from '../src/ui/noteTitle.ts';
import { visibleText } from '../src/markdown/visibleText.ts';

declare module 'vitest' {
  export interface ProvidedContext {
    /** `tests/visible_text_cases.json`, supplied by the Vitest config. */
    visibleTextCases: string;
  }
}

interface Case {
  readonly name: string;
  readonly stored: string;
  readonly visible: string;
  /**
   * Set on the cases where Note-it's own spelling *is* the text: a reader who
   * typed `<span` had it stored escaped, and a reader who quoted an attribute
   * in code meant to quote it. Those must survive the projection, so they are
   * exempt from the leak sweep.
   */
  readonly readerTyped?: boolean;
}

/** Every spelling of Note-it's storage that could reach a reader's eye. */
const STORAGE_SPELLINGS = [
  'data-note-it-color',
  'data-note-it-highlight',
  'data-note-it-font-size',
  'note-it:completed_at',
  '<span',
  '</span>',
  '<mark',
  '</mark>',
  '<u>',
  '</u>',
  'background-color',
  '-->',
];

/**
 * The corpus both projections are held to.
 *
 * The same file is read by `src/visible_text.rs`, because the host and the
 * WebView each carry an implementation and two implementations that are only
 * *described* as equivalent drift. Every case is a stored note and the text a
 * reader sees in it.
 */
const CASES: Case[] = JSON.parse(inject('visibleTextCases'));

describe('the stored-note to visible-text projection', () => {
  it('reads the shared corpus the host is also held to', () => {
    expect(CASES.length).toBeGreaterThan(30);
  });

  for (const { name, stored, visible } of CASES) {
    it(name, () => {
      expect(visibleText(stored)).toBe(visible);
    });
  }

  it('never changes the Markdown it was given', () => {
    const markdown =
      '# <span data-note-it-color="#64748B" style="color:#64748B">Título</span>\n\n**corpo**';
    const before = markdown;

    expect(visibleText(markdown)).toBe('Título\n\ncorpo');
    expect(markdown).toBe(before);
  });

  it('leaks no storage syntax anywhere in the corpus', () => {
    for (const { name, visible, readerTyped } of CASES) {
      if (readerTyped) continue;
      for (const spelling of STORAGE_SPELLINGS) {
        expect(visible, `case ${name}`).not.toContain(spelling);
      }
    }
  });

  it('is held to the Markdown the real editor actually writes', () => {
    // The projection is built from the forms Note-it's own serializer
    // produces, so the serializer is what it is checked against. If a mark is
    // ever spelled differently, this fails here rather than in a note bar.
    const container = document.createElement('div');
    document.body.appendChild(container);
    const heading =
      '# <mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">' +
      '<span data-note-it-color="#64748B" style="color:#64748B">teste de verdade</span>' +
      '</mark>';
    const authored = [
      heading,
      '',
      '**OBSERVAÇÃO:** um *itálico*, um ~~riscado~~ e um <u>sublinhado</u>.',
      '',
      '<span data-note-it-font-size="22" style="font-size:22px">texto grande</span>',
      '',
      '<!-- esse é um comentário de teste -->',
      '',
      '> [!WARNING]',
      '> Cuidado com a agulha',
      '',
      '- [x] comprar pão <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->',
    ].join('\n');
    const editor = new NoteEditor({ element: container, initialContent: authored });
    const stored = editor.getMarkdown();
    editor.destroy();
    container.remove();

    // The stored file really does carry the storage — that is the point.
    expect(stored).toContain('data-note-it-color');
    expect(stored).toContain('data-note-it-highlight');
    expect(stored).toContain('note-it:completed_at');

    const visible = visibleText(stored);
    for (const spelling of STORAGE_SPELLINGS) {
      expect(visible).not.toContain(spelling);
    }
    for (const words of [
      'teste de verdade',
      'OBSERVAÇÃO:',
      'itálico',
      'riscado',
      'sublinhado',
      'texto grande',
      'esse é um comentário de teste',
      'Cuidado com a agulha',
      'comprar pão',
    ]) {
      expect(visible).toContain(words);
    }
    expect(noteTitle(stored)).toBe('teste de verdade');
  });
});
