import { describe, expect, it } from 'vitest';
import { MathError } from '../src/math/errors.ts';
import { evaluate } from '../src/math/evaluate.ts';
import { formatNumber } from '../src/math/format.ts';
import { isValidName, tokenize, MAX_EXPRESSION_LENGTH } from '../src/math/lexer.ts';
import { isLiteral, parse } from '../src/math/parser.ts';

/** Evaluates an expression with an optional set of variables. */
function calc(expression: string, variables: Record<string, number> = {}): number {
  return evaluate(parse(expression), {
    variables: new Map(Object.entries(variables)),
    samples: [],
  });
}

/** The error code an expression fails with, or `null` if it does not fail. */
function failure(expression: string, variables: Record<string, number> = {}): string | null {
  try {
    calc(expression, variables);
    return null;
  } catch (error) {
    return error instanceof MathError ? error.code : 'not-a-math-error';
  }
}

describe('arithmetic', () => {
  it('adds, subtracts, multiplies and divides', () => {
    expect(calc('2 + 2')).toBe(4);
    expect(calc('10 - 4')).toBe(6);
    expect(calc('10 * 8')).toBe(80);
    expect(calc('9 / 3')).toBe(3);
  });

  it('gives multiplication and division precedence over addition', () => {
    expect(calc('2 + 3 * 4')).toBe(14);
    expect(calc('2 - 8 / 4')).toBe(0);
    expect(calc('1 + 2 * 3 - 4 / 2')).toBe(5);
  });

  it('associates to the left', () => {
    expect(calc('10 - 3 - 2')).toBe(5);
    expect(calc('100 / 5 / 2')).toBe(10);
  });

  it('lets parentheses override precedence, at any depth', () => {
    expect(calc('(10 + 5) * 2')).toBe(30);
    expect(calc('(100 + 50) / 3')).toBe(50);
    expect(calc('((2 + 3) * (4 - 1)) / 5')).toBe(3);
  });

  it('reads negative numbers, including as an operand', () => {
    expect(calc('-5')).toBe(-5);
    expect(calc('-5 + 3')).toBe(-2);
    expect(calc('10 * -2')).toBe(-20);
    expect(calc('--5')).toBe(5);
    expect(calc('+7')).toBe(7);
  });

  it('reads decimals written with either separator', () => {
    expect(calc('10.5')).toBe(10.5);
    expect(calc('10,5')).toBe(10.5);
    expect(calc('0.25 * 4')).toBe(1);
    expect(calc('0,25 * 4')).toBe(1);
    expect(calc('10,5 * 2')).toBe(21);
  });

  it('refuses a number with two separators rather than guessing a grouping', () => {
    // `1.234.567` is a thousand-grouped number in one convention and nonsense
    // in the other. Note-it accepts no thousands separator at all, and says so
    // instead of picking a reading.
    expect(failure('1.234.567')).toBe('invalid-expression');
    expect(failure('1,234,567')).toBe('invalid-expression');
    expect(failure('1,5.2')).toBe('invalid-expression');
    // A single separator is always the decimal one, whichever it is.
    expect(calc('1.000')).toBe(1);
    expect(calc('1,000')).toBe(1);
  });

  it('reports division by zero rather than infinity', () => {
    expect(failure('1 / 0')).toBe('division-by-zero');
    expect(failure('0 / 0')).toBe('division-by-zero');
    expect(failure('10 / (5 - 5)')).toBe('division-by-zero');
  });

  it('reports a malformed expression', () => {
    expect(failure('(2 + 3')).toBe('invalid-expression');
    expect(failure('2 +')).toBe('invalid-expression');
    expect(failure('* 2')).toBe('invalid-expression');
    expect(failure('2 2')).toBe('invalid-expression');
    expect(failure('')).toBe('invalid-expression');
    expect(failure('   ')).toBe('invalid-expression');
    expect(failure('2 + 3)')).toBe('invalid-expression');
    expect(failure('()')).toBe('invalid-expression');
  });
});

describe('percentages', () => {
  it('reads a bare percentage as a hundredth', () => {
    expect(calc('10%')).toBe(0.1);
    expect(calc('100%')).toBe(1);
    expect(calc('10% * 200')).toBe(20);
  });

  it('reads "X% de Y" as X percent of Y', () => {
    expect(calc('10% de 200')).toBe(20);
    expect(calc('25% de 80')).toBe(20);
    expect(calc('10% DE 200')).toBe(20);
  });

  it('reads an addition of a percentage as an increase', () => {
    expect(calc('200 + 10%')).toBe(220);
    expect(calc('50 + 100%')).toBe(100);
  });

  it('reads a subtraction of a percentage as a discount', () => {
    expect(calc('200 - 10%')).toBe(180);
    expect(calc('80 - 25%')).toBe(60);
  });

  it('keeps a percentage in a variable a plain hundredth', () => {
    // The contextual reading belongs to the `%` you can see on the line, not
    // to a value that once came from one.
    expect(calc('taxa * 200', { taxa: 0.1 })).toBe(20);
    expect(calc('200 + taxa', { taxa: 0.1 })).toBe(200.1);
  });

  it('applies the increase only to a percentage written as the right operand', () => {
    expect(calc('10% + 200')).toBe(200.1);
    expect(calc('100 + 10% * 2')).toBeCloseTo(100.2, 10);
  });

  it('refuses "de" with anything but a percentage on its left', () => {
    expect(failure('200 de 10')).toBe('invalid-expression');
    expect(failure('de 200')).toBe('invalid-expression');
    expect(failure('10% de')).toBe('invalid-expression');
  });

  it('has no modulo operator, so "%" always means percent', () => {
    // `10 % 3` would have to be both, and a symbol that means two things in a
    // note is a symbol nobody can rely on.
    expect(failure('10 % 3')).toBe('invalid-expression');
  });
});

describe('variables in an expression', () => {
  it('reads a declared variable', () => {
    expect(calc('preco * 3', { preco: 120 })).toBe(360);
    expect(calc('(a + b) / 2', { a: 10, b: 20 })).toBe(15);
  });

  it('reports an undeclared one', () => {
    expect(failure('preco * 2')).toBe('unknown-variable');
    expect(failure('a + b', { a: 1 })).toBe('unknown-variable');
  });

  it('accepts the name shapes it documents and no others', () => {
    expect(isValidName('preco')).toBe(true);
    expect(isValidName('preco_unitario')).toBe(true);
    expect(isValidName('_x')).toBe(true);
    expect(isValidName('a1')).toBe(true);
    expect(isValidName('12preco')).toBe(false);
    expect(isValidName('preço')).toBe(false);
    expect(isValidName('meu preco')).toBe(false);
    expect(isValidName('')).toBe(false);
  });

  it('keeps its own words out of reach as variable names', () => {
    expect(isValidName('sum')).toBe(false);
    expect(isValidName('avg')).toBe(false);
    expect(isValidName('count')).toBe(false);
    expect(isValidName('de')).toBe(false);
    expect(isValidName('SUM')).toBe(false);
  });
});

describe('aggregators as expressions', () => {
  it('reads sum, avg and count standing alone', () => {
    const scope = { variables: new Map<string, number>(), samples: [10, 20, 30] };
    expect(evaluate(parse('sum'), scope)).toBe(60);
    expect(evaluate(parse('avg'), scope)).toBe(20);
    expect(evaluate(parse('count'), scope)).toBe(3);
    expect(evaluate(parse('SUM'), scope)).toBe(60);
  });

  it('answers zero for an empty sum and count, and refuses an empty average', () => {
    const scope = { variables: new Map<string, number>(), samples: [] };
    expect(evaluate(parse('sum'), scope)).toBe(0);
    expect(evaluate(parse('count'), scope)).toBe(0);
    expect(() => evaluate(parse('avg'), scope)).toThrow(MathError);
  });

  it('refuses an aggregator used as part of a larger expression', () => {
    expect(failure('sum * 2')).toBe('invalid-expression');
    expect(failure('1 + count')).toBe('invalid-expression');
    expect(failure('sum(1, 2)')).toBe('invalid-expression');
  });
});

describe('a literal is recognised as one', () => {
  it('calls a bare value a literal and a computation not', () => {
    expect(isLiteral(parse('120'))).toBe(true);
    expect(isLiteral(parse('12.50'))).toBe(true);
    expect(isLiteral(parse('-5'))).toBe(true);
    expect(isLiteral(parse('10%'))).toBe(true);
    expect(isLiteral(parse('preco'))).toBe(false);
    expect(isLiteral(parse('2 + 2'))).toBe(false);
    expect(isLiteral(parse('(120)'))).toBe(true);
  });
});

describe('how a result is spelled', () => {
  it('hides the noise binary floating point leaves behind', () => {
    expect(formatNumber(0.1 + 0.2)).toBe('0,3');
    expect(formatNumber(1.005 * 100)).toBe('100,5');
    expect(formatNumber(4)).toBe('4');
  });

  it('writes the decimal separator the way the note is written', () => {
    expect(formatNumber(10.5)).toBe('10,5');
    expect(formatNumber(-2.25)).toBe('-2,25');
  });

  it('never groups thousands, so a result reads back as itself', () => {
    // The engine accepts both `.` and `,` as decimal separators, so a grouped
    // result would be a number this same engine reads as something else.
    expect(formatNumber(109876463)).toBe('109876463');
    expect(formatNumber(1000)).toBe('1000');
  });

  it('never shows a negative zero', () => {
    expect(formatNumber(-0)).toBe('0');
    expect(formatNumber(0 * -1)).toBe('0');
  });

  it('keeps the value it carries apart from the value it shows', () => {
    // A third is shown rounded and used whole: rounding what a variable holds
    // would make every line under it wrong.
    const third = calc('1 / 3');
    expect(formatNumber(third)).toBe('0,3333333333');
    expect(third * 3).toBe(1);
  });
});

describe('the lexer refuses what the grammar cannot hold', () => {
  it('has no token for a dot, a bracket, a quote or a comma outside a number', () => {
    for (const source of ['a.b', 'a[0]', '"x"', 'a;b', 'a=>b', 'a && b', 'x!']) {
      expect(() => tokenize(source)).toThrow(MathError);
    }
  });

  it('caps how long an expression may be', () => {
    expect(() => tokenize('1+'.repeat(MAX_EXPRESSION_LENGTH))).toThrow(MathError);
  });
});
