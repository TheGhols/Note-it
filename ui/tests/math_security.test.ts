import { describe, expect, it } from 'vitest';
import { evaluateNote } from '../src/math/document.ts';
import { MAX_EXPRESSION_LENGTH, MAX_TOKENS } from '../src/math/lexer.ts';
import { MATH_SOURCES } from './support/sources.ts';

/** The engine's answer to one calculation line. */
function answer(expression: string): string {
  const [result] = evaluateNote([`= ${expression}`]);
  if (result.kind === 'error') return result.code;
  if (result.kind === 'value') return `value:${result.text}`;
  return 'none';
}

/**
 * The engine has no evaluator to escape from.
 *
 * These are not filtered inputs. There is no `eval`, no `Function`, no
 * property access, no call syntax and no host object anywhere in the engine:
 * an expression is tokens the lexer knows, a tree of six node shapes, and
 * arithmetic. The tests below are here so that stays true — anything that
 * started resolving one of these names to a real value would fail loudly.
 */
describe('nothing in a note can reach the runtime', () => {
  it('has no evaluator in the source at all', () => {
    expect(Object.keys(MATH_SOURCES).length).toBeGreaterThanOrEqual(5);
    for (const [name, source] of Object.entries(MATH_SOURCES)) {
      expect(source, name).not.toMatch(/\beval\s*\(/);
      expect(source, name).not.toMatch(/\bnew\s+Function\b/);
      expect(source, name).not.toMatch(/\bsetTimeout\s*\(|\bsetInterval\s*\(/);
      expect(source, name).not.toMatch(/\bimport\s*\(/);
    }
  });

  it('treats a global as an unknown variable and nothing more', () => {
    expect(answer('window')).toBe('unknown-variable');
    expect(answer('globalThis')).toBe('unknown-variable');
    expect(answer('document')).toBe('unknown-variable');
    expect(answer('process')).toBe('unknown-variable');
    expect(answer('fetch')).toBe('unknown-variable');
    expect(answer('alert')).toBe('unknown-variable');
  });

  it('cannot reach an inherited property, because the scope is a Map', () => {
    // A plain object would answer every one of these with a real function.
    expect(answer('constructor')).toBe('unknown-variable');
    expect(answer('__proto__')).toBe('unknown-variable');
    expect(answer('prototype')).toBe('unknown-variable');
    expect(answer('toString')).toBe('unknown-variable');
    expect(answer('valueOf')).toBe('unknown-variable');
    expect(answer('hasOwnProperty')).toBe('unknown-variable');
  });

  it('has no syntax for a property access or a call', () => {
    expect(answer('window.location')).toBe('invalid-expression');
    expect(answer('process.exit()')).toBe('invalid-expression');
    expect(answer('fetch("https://exemplo")')).toBe('invalid-expression');
    expect(answer('constructor.constructor("return 1")()')).toBe('invalid-expression');
    expect(answer('alert(1)')).toBe('invalid-expression');
    expect(answer('[].constructor')).toBe('invalid-expression');
    expect(answer('this')).toBe('unknown-variable');
    expect(answer('(() => 1)()')).toBe('invalid-expression');
  });

  it('cannot be made to write into a shared object', () => {
    // `__proto__` is a perfectly ordinary identifier under the ASCII name rule,
    // so what keeps it away from the runtime is not the name check: it is that
    // the scope is a `Map`, which has no prototype chain to walk into.
    evaluateNote(['__proto__ := 1', 'constructor := 2', '= __proto__ + constructor']);
    expect(Object.prototype).not.toHaveProperty('polluted');
    expect(({} as Record<string, unknown>).constructor).toBe(Object);

    const results = evaluateNote(['__proto__ := 1', '= __proto__']);
    expect(results[1]).toEqual({ kind: 'value', value: 1, text: '1' });
  });

  it('refuses malformed and hostile strings without throwing out of the engine', () => {
    for (const source of [
      '<script>alert(1)</script>',
      '"; DROP TABLE notas; --',
      ' ',
      '../../etc/passwd',
      '${process.env}',
      '\\x41',
      '2 + 2; = 3',
      // Digits that are digits to a reader and not to the lexer.
      '\u{1D7DA} + \u{1D7DA}',
      '\u0660\u0661 + \u0665',
      // ...and bytes no note should ever be able to smuggle through.
      '\u0000',
      '\u202E2 + 2',
    ]) {
      expect(() => answer(source)).not.toThrow();
      expect(answer(source)).toBe('invalid-expression');
    }
  });

  it('answers an extremely long line in bounded time', () => {
    const long = `${'1+'.repeat(200_000)}1`;
    const started = performance.now();
    expect(answer(long)).toBe('invalid-expression');
    expect(performance.now() - started).toBeLessThan(500);
  });

  it('caps the number of tokens as well as the number of characters', () => {
    const many = '1'.repeat(MAX_TOKENS + 20).split('').join('+');
    expect(answer(many)).toBe('invalid-expression');
  });

  it('refuses deep nesting rather than exhausting the stack', () => {
    const nested = `${'('.repeat(100)}1${')'.repeat(100)}`;
    expect(nested.length).toBeLessThan(MAX_EXPRESSION_LENGTH);
    expect(() => answer(nested)).not.toThrow();
    expect(answer(nested)).toBe('invalid-expression');
  });

  it('never produces infinity or a value that is not a number', () => {
    expect(answer('99999999999999999999 * 99999999999999999999')).toMatch(/^value:/);
    expect(answer(`9${'9'.repeat(400)}`)).toBe('invalid-expression');
  });
});
