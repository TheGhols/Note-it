import { describe, expect, it } from 'vitest';
import { noteTitle } from '../src/ui/noteTitle.ts';

describe('the collapsed note title', () => {
  it('uses the first non-empty textual line', () => {
    expect(noteTitle('\n\nComprar madeira amanhã\nsegunda linha')).toBe(
      'Comprar madeira amanhã',
    );
  });

  it('removes a superficial heading marker for presentation', () => {
    expect(noteTitle('# Estudar biópsia hepática\n')).toBe('Estudar biópsia hepática');
  });

  it('unwraps common list, task and quote prefixes without changing their text', () => {
    expect(noteTitle('- [ ] ligar para Ana')).toBe('ligar para Ana');
    expect(noteTitle('> frase importante')).toBe('frase importante');
    expect(noteTitle('2. revisar proposta')).toBe('revisar proposta');
  });

  it('uses the explicit fallback for an empty note', () => {
    expect(noteTitle(' \n\t\n')).toBe('Nota sem título');
  });

  it('skips non-textual Markdown separators and fences', () => {
    expect(noteTitle('---\n```\nTítulo útil')).toBe('Título útil');
  });

  it('caps a long label with the requested ellipsis glyph', () => {
    const title = noteTitle('A'.repeat(160));
    expect(title).toHaveLength(80);
    expect(title.endsWith('…')).toBe(true);
  });

  it('shows a coloured phrase as the phrase, not as the span around it', () => {
    // 3.9UX.R.1, exactly as reported.
    expect(
      noteTitle(
        '<span data-note-it-color="#64748B" style="color:#64748B">teste de verdade</span>',
      ),
    ).toBe('teste de verdade');
  });

  it('shows a highlighted phrase, and a highlight nested with a colour', () => {
    expect(
      noteTitle(
        '<mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">teste de verdade</mark>',
      ),
    ).toBe('teste de verdade');
    expect(
      noteTitle(
        '<span data-note-it-color="#64748B" style="color:#64748B">' +
          '<mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">teste de verdade</mark>' +
          '</span>',
      ),
    ).toBe('teste de verdade');
  });

  it('unwraps every heading level', () => {
    for (let level = 1; level <= 6; level += 1) {
      expect(noteTitle(`${'#'.repeat(level)} Biópsia hepática\n\ncorpo`)).toBe('Biópsia hepática');
    }
  });

  it('unwraps bold, italic, strike, underline and code', () => {
    expect(noteTitle('**OBSERVAÇÃO:** algo importante')).toBe('OBSERVAÇÃO: algo importante');
    expect(noteTitle('*itálico*')).toBe('itálico');
    expect(noteTitle('~~riscado~~')).toBe('riscado');
    expect(noteTitle('<u>sublinhado</u>')).toBe('sublinhado');
    expect(noteTitle('`código`')).toBe('código');
  });

  it('unwraps an explicit text size', () => {
    expect(
      noteTitle('<span data-note-it-font-size="22" style="font-size:22px">texto grande</span>'),
    ).toBe('texto grande');
  });

  it('shows a comment as its text and never as its delimiters', () => {
    expect(noteTitle('<!-- esse é um comentário de teste -->')).toBe(
      'esse é um comentário de teste',
    );
  });

  it('never shows a task completion timestamp, which is machine bookkeeping', () => {
    expect(
      noteTitle('- [x] comprar pão <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->'),
    ).toBe('comprar pão');
  });

  it('shows a callout by its body, never by its marker', () => {
    expect(noteTitle('> [!WARNING]\n> Cuidado com isso')).toBe('Cuidado com isso');
  });

  it('unwraps a task inside a list that also carries a colour', () => {
    expect(
      noteTitle('- [ ] <span data-note-it-color="#64748B" style="color:#64748B">ligar para Ana</span>'),
    ).toBe('ligar para Ana');
  });

  it('leaks nothing when every mark is nested at once', () => {
    const markdown = [
      '# <mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">',
      '<span data-note-it-color="#64748B" style="color:#64748B">',
      '<span data-note-it-font-size="22" style="font-size:22px">**teste de verdade**</span>',
      '</span></mark>',
    ].join('');

    const title = noteTitle(markdown);
    expect(title).toBe('teste de verdade');
    for (const spelling of ['<span', '<mark', 'data-note-it-', 'style=', '**', '<!--']) {
      expect(title).not.toContain(spelling);
    }
  });

  it('keeps accents, emoji and other scripts exactly as written', () => {
    expect(
      noteTitle('# 🎉 <span data-note-it-color="#64748B" style="color:#64748B">ação</span> 日本語'),
    ).toBe('🎉 ação 日本語');
  });

  it('still calls a note without any text at all untitled', () => {
    expect(noteTitle('<!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->')).toBe(
      'Nota sem título',
    );
    expect(noteTitle('---\n\n***')).toBe('Nota sem título');
  });

  it('still caps a long label after unwrapping, not before', () => {
    const markdown = `<span data-note-it-color="#64748B" style="color:#64748B">${'A'.repeat(160)}</span>`;
    const title = noteTitle(markdown);
    expect(title).toHaveLength(80);
    expect(title.endsWith('…')).toBe(true);
    expect(title).not.toContain('span');
  });

  it('is presentation only and never mutates the Markdown source', () => {
    const markdown = '# Título real\n\nTexto **intacto**';
    const before = markdown;

    expect(noteTitle(markdown)).toBe('Título real');
    expect(markdown).toBe(before);
  });
});
