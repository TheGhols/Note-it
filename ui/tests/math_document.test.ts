import { describe, expect, it } from 'vitest';
import { classifyLine, evaluateNote, MathSource } from '../src/math/document.ts';

/** The note's results, one per line, as the reader would see them. */
function shown(lines: readonly MathSource[]): (string | null)[] {
  return evaluateNote(lines).map((result) => {
    if (result.kind === 'value') return result.text;
    if (result.kind === 'error') return `!${result.code}`;
    return null;
  });
}

/** The result of the last line, which is what most of these are about. */
function last(lines: readonly MathSource[]): string | null {
  return shown(lines).at(-1) ?? null;
}

describe('which lines are calculations at all', () => {
  it('takes a line that starts with = and a line that declares with :=', () => {
    expect(classifyLine('= 2 + 2')).toBe('calculation');
    expect(classifyLine('=2+2')).toBe('calculation');
    expect(classifyLine('   = 2 + 2')).toBe('calculation');
    expect(classifyLine('preco := 120')).toBe('declaration');
    expect(classifyLine('preco:=120')).toBe('declaration');
  });

  it('leaves ordinary prose alone', () => {
    expect(classifyLine('2 + 2 = 4')).toBe('prose');
    expect(classifyLine('o total é 360')).toBe('prose');
    expect(classifyLine('nota: um valor')).toBe('prose');
    expect(classifyLine('- item := 2')).toBe('prose');
    expect(classifyLine('== ênfase ==')).toBe('prose');
    expect(classifyLine('')).toBe('prose');
    expect(classifyLine(null)).toBe('prose');
  });
});

describe('calculation lines', () => {
  it('shows the result of an explicit expression', () => {
    expect(shown(['= 2 + 2'])).toEqual(['4']);
    expect(shown(['= 10 * 8'])).toEqual(['80']);
    expect(shown(['= (100 + 50) / 3'])).toEqual(['50']);
  });

  it('shows an error beside the line and leaves everything else standing', () => {
    expect(shown(['texto', '= 1 / 0', '= 2 + 2'])).toEqual([
      null,
      '!division-by-zero',
      '4',
    ]);
    expect(last(['= nao_existe * 2'])).toBe('!unknown-variable');
    expect(last(['= (2 + 3'])).toBe('!invalid-expression');
  });
});

describe('variables', () => {
  it('declares, reuses and computes', () => {
    expect(
      shown(['preco := 120', 'quantidade := 3', '= preco * quantidade']),
    ).toEqual([null, null, '360']);
  });

  it('shows a computed declaration but not one that only restates a number', () => {
    expect(shown(['preco := 120', 'subtotal := preco * 3'])).toEqual([null, '360']);
    expect(shown(['taxa := 10%'])).toEqual([null]);
    expect(shown(['desconto := -5'])).toEqual([null]);
  });

  it('lets a later declaration replace an earlier one, from that line down', () => {
    expect(
      shown(['preco := 100', '= preco * 2', 'preco := 150', '= preco * 2']),
    ).toEqual([null, '200', null, '300']);
  });

  it('does not let a line use a variable declared below it', () => {
    expect(shown(['= preco * 2', 'preco := 100'])).toEqual([
      '!unknown-variable',
      null,
    ]);
  });

  it('resolves a chain of dependencies top-down', () => {
    expect(
      shown([
        'preco := 120',
        'quantidade := 3',
        'subtotal := preco * quantidade',
        'imposto := subtotal * 10%',
        '= subtotal + imposto',
      ]),
    ).toEqual([null, null, '360', '36', '396']);
  });

  it('cannot be made to cycle, because nothing looks downwards', () => {
    expect(shown(['a := b + 1', 'b := a + 1'])).toEqual([
      '!unknown-variable',
      '!unknown-variable',
    ]);
  });

  it('reports a name it cannot use, rather than reading the line as prose', () => {
    expect(shown(['12preco := 50'])).toEqual(['!invalid-name']);
    expect(shown(['preço := 50'])).toEqual(['!invalid-name']);
    expect(shown(['sum := 50'])).toEqual(['!invalid-name']);
  });

  it('un-declares a variable whose new declaration failed', () => {
    // From that line on there is no such value, and saying so is better than
    // answering with a definition the reader can no longer see.
    expect(
      shown(['preco := 100', '= preco', 'preco := nao_existe', '= preco']),
    ).toEqual([null, '100', '!unknown-variable', '!unknown-variable']);
  });

  it('carries a variable across anything that is not a calculation', () => {
    expect(
      shown(['preco := 100', null, 'um parágrafo qualquer', null, '= preco * 2']),
    ).toEqual([null, null, null, null, '200']);
  });
});

describe('percentages in a note', () => {
  it('answers the four everyday forms', () => {
    expect(shown(['= 10% de 200'])).toEqual(['20']);
    expect(shown(['= 200 + 10%'])).toEqual(['220']);
    expect(shown(['= 200 - 10%'])).toEqual(['180']);
    expect(shown(['taxa := 10%', '= taxa * 200'])).toEqual([null, '20']);
  });
});

describe('aggregators over the lines above them', () => {
  const BLOCK = ['= 10', '= 20', '= 30'];

  it('sums, averages and counts the block directly above', () => {
    expect(last([...BLOCK, '= sum'])).toBe('60');
    expect(last([...BLOCK, '= avg'])).toBe('20');
    expect(last([...BLOCK, '= count'])).toBe('3');
  });

  it('takes only calculation lines, never a number sitting in prose', () => {
    expect(last(['10', '20', '30', '= sum'])).toBe('0');
    expect(last(['Gastei 10 reais e 20 no domingo', '= sum'])).toBe('0');
    expect(last(['= count'])).toBe('0');
    expect(last(['= avg'])).toBe('!division-by-zero');
  });

  it('stops at the first line that is not one of them', () => {
    expect(last(['= 99', 'um parágrafo', ...BLOCK, '= sum'])).toBe('60');
    expect(last(['= 99', null, ...BLOCK, '= sum'])).toBe('60');
    expect(last(['= 99', 'total := 1', ...BLOCK, '= sum'])).toBe('60');
  });

  it('stops at a failed calculation rather than quietly skipping it', () => {
    expect(last(['= 10', '= 1 / 0', '= 30', '= sum'])).toBe('30');
  });

  it('lets the three of them read the same block, one under the other', () => {
    expect(shown([...BLOCK, '= sum', '= avg', '= count'])).toEqual([
      '10',
      '20',
      '30',
      '60',
      '20',
      '3',
    ]);
  });

  it('starts a new block at the first value under an aggregator', () => {
    expect(shown(['= 10', '= sum', '= 5', '= 7', '= sum'])).toEqual([
      '10',
      '10',
      '5',
      '7',
      '12',
    ]);
  });

  it('treats an aggregator as the end of a block, so two blocks stay two', () => {
    expect(
      shown(['= 10', '= 20', '= sum', '= 5', '= 7', '= sum']),
    ).toEqual(['10', '20', '30', '5', '7', '12']);
  });

  it('lets a declaration take an aggregate of the block above it', () => {
    expect(shown(['= 10', '= 20', 'total := sum', '= total * 2'])).toEqual([
      '10',
      '20',
      '30',
      '60',
    ]);
  });

  it('aggregates values the note computed, not only ones it was given', () => {
    expect(
      last(['preco := 12', '= preco * 2', '= preco * 3', '= sum']),
    ).toBe('60');
  });
});

describe('reactivity is a property of evaluating the whole note', () => {
  it('recomputes every dependent line when a variable changes', () => {
    const before = ['preco := 100', '= preco * 2', '= preco + 50'];
    const after = ['preco := 150', '= preco * 2', '= preco + 50'];
    expect(shown(before)).toEqual([null, '200', '150']);
    expect(shown(after)).toEqual([null, '300', '200']);
  });

  it('invalidates the dependants when the declaration is removed', () => {
    expect(shown(['preco := 100', '= preco * 2'])).toEqual([null, '200']);
    expect(shown(['= preco * 2'])).toEqual(['!unknown-variable']);
  });

  it('recovers them when the declaration comes back', () => {
    expect(shown(['preco := 100', '= preco * 2'])).toEqual([null, '200']);
  });
});
