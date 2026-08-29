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

  it('is presentation only and never mutates the Markdown source', () => {
    const markdown = '# Título real\n\nTexto **intacto**';
    const before = markdown;

    expect(noteTitle(markdown)).toBe('Título real');
    expect(markdown).toBe(before);
  });
});
