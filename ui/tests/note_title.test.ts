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

  describe('cutting a long title', () => {
    /**
     * A high surrogate with no low one after it, or a low one with no high one
     * before it. Either is a broken character: the bar renders a replacement
     * glyph where a letter or an emoji should be.
     */
    function hasLoneSurrogate(text: string): boolean {
      return /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/.test(text);
    }

    /** What the reader counts: characters, not UTF-16 code units. */
    function characters(text: string): string[] {
      return Array.from(new Intl.Segmenter('pt-BR', { granularity: 'grapheme' }).segment(text))
        .map((piece) => piece.segment);
    }

    function isWhole(title: string): void {
      expect(hasLoneSurrogate(title)).toBe(false);
      expect(title).not.toContain('�');
      expect(characters(title).length).toBeLessThanOrEqual(80);
    }

    it('leaves a short ASCII title exactly as it is', () => {
      expect(noteTitle('Comprar madeira amanhã')).toBe('Comprar madeira amanhã');
    });

    it('caps a long ASCII title with the ellipsis glyph', () => {
      const title = noteTitle('A'.repeat(160));
      expect(title).toHaveLength(80);
      expect(title.endsWith('…')).toBe(true);
      isWhole(title);
    });

    it('keeps an emoji that sits before the limit', () => {
      const title = noteTitle(`🎉 festa ${'A'.repeat(200)}`);
      expect(title.startsWith('🎉 festa ')).toBe(true);
      isWhole(title);
    });

    it('never cuts an emoji that lands exactly on the boundary', () => {
      // 3.9UX.R.2. The emoji begins at UTF-16 index 78, so the old
      // `slice(0, 79)` landed between its two halves and the bar was handed a
      // lone surrogate. Counted in characters, it is either kept whole or
      // dropped whole.
      const markdown = `${'A'.repeat(78)}🎉 e muito mais texto depois do limite`;
      expect(markdown.indexOf('🎉')).toBe(78);

      const title = noteTitle(markdown);
      expect(title).toBe(`${'A'.repeat(78)}🎉…`);
      isWhole(title);
    });

    it('never cuts an emoji wherever the boundary happens to fall', () => {
      // Every offset around the limit, so the case cannot be fixed by luck.
      for (let lead = 70; lead <= 85; lead += 1) {
        const title = noteTitle(`${'A'.repeat(lead)}🎉${'B'.repeat(60)}`);
        isWhole(title);
      }
    });

    it('keeps a run of consecutive emoji whole', () => {
      const title = noteTitle('🎉🎊🥳🎈🍰'.repeat(40));
      isWhole(title);
      expect(characters(title).length).toBe(80);
      expect(title.endsWith('…')).toBe(true);
    });

    it('never separates a combining accent from the letter it belongs to', () => {
      // Text reaches Note-it decomposed as well as precomposed; search folds
      // `o` + U+0301 exactly like `\u00F3` for that reason. Counted in UTF-16 the
      // letter sits inside the limit and its accent just outside it, so the cut
      // fell between them and a note about a `beb\u00E9` was named after a `bebe`.
      const accented = `${'a'.repeat(78)}e\u0301${'b'.repeat(60)}`;
      expect(accented.indexOf('\u0301')).toBe(79);

      const title = noteTitle(accented);
      expect(title).toBe(`${'a'.repeat(78)}e\u0301\u2026`);
      isWhole(title);
    });

    it('counts accented and CJK characters as the characters they are', () => {
      expect(noteTitle('Biópsia hepática — ação e coração')).toBe(
        'Biópsia hepática — ação e coração',
      );
      const japanese = noteTitle('日本語'.repeat(40));
      isWhole(japanese);
      expect(characters(japanese).length).toBe(80);
    });

    it('cuts the projected text and never the stored Markdown', () => {
      const markdown = `**${'A'.repeat(200)}🎉**`;
      const before = markdown;

      const title = noteTitle(markdown);
      isWhole(title);
      expect(title).not.toContain('*');
      expect(markdown).toBe(before);
    });

    it('leaks no markup from a long coloured or highlighted title', () => {
      const long = `${'A'.repeat(78)}🎉 continua bem depois do limite de oitenta`;
      for (const markdown of [
        `<span data-note-it-color="#64748B" style="color:#64748B">${long}</span>`,
        `<mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">${long}</mark>`,
        `# <mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">` +
          `<span data-note-it-color="#64748B" style="color:#64748B">${long}</span></mark>`,
      ]) {
        const title = noteTitle(markdown);
        expect(title).toBe(`${'A'.repeat(78)}🎉…`);
        isWhole(title);
        for (const spelling of ['<span', '<mark', 'data-note-it-', 'style=', '#']) {
          expect(title).not.toContain(spelling);
        }
      }
    });
  });

  it('is presentation only and never mutates the Markdown source', () => {
    const markdown = '# Título real\n\nTexto **intacto**';
    const before = markdown;

    expect(noteTitle(markdown)).toBe('Título real');
    expect(markdown).toBe(before);
  });
});
