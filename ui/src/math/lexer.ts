import { MathError } from './errors.ts';

/**
 * The whole vocabulary of the math engine.
 *
 * There is no token for anything that could reach a JavaScript value: no dot,
 * no bracket, no string, no call syntax. An expression is a sequence of these
 * and nothing else, so `window.location`, `constructor.constructor(1)` and
 * `fetch(...)` are not "dangerous input to be filtered" — they are simply not
 * spellable in this grammar, and stop at the first character that is not one
 * of the shapes below.
 */
export type TokenType =
  | 'number'
  | 'identifier'
  | 'de'
  | 'plus'
  | 'minus'
  | 'star'
  | 'slash'
  | 'percent'
  | 'lparen'
  | 'rparen';

export interface Token {
  readonly type: TokenType;
  /** Numbers carry their value; identifiers carry their name. */
  readonly value: number;
  readonly text: string;
}

/**
 * The names the grammar keeps for itself, matched without regard to case.
 *
 * `de` is the percentage preposition and the three others are the aggregators.
 * None of them can be a variable name, which is what keeps `= sum` unambiguous
 * whatever else the note declares.
 */
export const RESERVED_NAMES: ReadonlySet<string> = new Set(['de', 'sum', 'avg', 'count']);

export const AGGREGATE_NAMES: ReadonlySet<string> = new Set(['sum', 'avg', 'count']);

export function isReservedName(name: string): boolean {
  return RESERVED_NAMES.has(name.toLowerCase());
}

/**
 * A name a declaration may use: ASCII letters, digits and `_`, never starting
 * with a digit.
 *
 * ASCII only, on purpose. Accepting Unicode would mean deciding what `preço`
 * and `preço` are — the same variable or two — and every answer to that
 * is either a normalisation policy nobody asked for or a note where two
 * identical-looking names quietly disagree. A name outside this set is not
 * silently ignored: the line still says `:=`, so it is reported as an invalid
 * name rather than treated as prose.
 */
export const NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;

export function isValidName(name: string): boolean {
  return NAME_PATTERN.test(name) && !isReservedName(name);
}

/**
 * Ceilings that exist so a hostile or accidental input costs a fixed amount.
 *
 * A note is a text file and anything at all can be pasted into it. These are
 * far above any expression a person writes and far below anything that could
 * make the editor stutter.
 */
export const MAX_EXPRESSION_LENGTH = 1000;
export const MAX_TOKENS = 512;

const DIGIT = /[0-9]/;
const NAME_START = /[A-Za-z_]/;
const NAME_PART = /[A-Za-z0-9_]/;

const SINGLE_CHARACTER: Record<string, TokenType> = {
  '+': 'plus',
  '-': 'minus',
  '*': 'star',
  '/': 'slash',
  '%': 'percent',
  '(': 'lparen',
  ')': 'rparen',
};

function token(type: TokenType, text: string, value = 0): Token {
  return { type, value, text };
}

/**
 * Reads a number, and refuses the one spelling that could mean two things.
 *
 * `.` is the canonical decimal separator and `,` is accepted as one, because a
 * pt-BR keyboard writes `10,5` without thinking about it. What is never
 * accepted is a second separator: `1.234.567` is a thousands-grouped number in
 * one convention and nonsense in the other, and guessing between them is
 * exactly the silent reinterpretation this engine must not do. It is reported
 * as an invalid expression instead, which the reader sees immediately.
 */
function readNumber(source: string, start: number): { token: Token; next: number } {
  let index = start;
  while (index < source.length && DIGIT.test(source[index])) index += 1;

  let text = source.slice(start, index);
  const separator = source[index];
  if ((separator === '.' || separator === ',') && DIGIT.test(source[index + 1] ?? '')) {
    index += 1;
    const fractionStart = index;
    while (index < source.length && DIGIT.test(source[index])) index += 1;
    text = `${text}.${source.slice(fractionStart, index)}`;
  }

  // A separator still ahead is a second one: `1.234.567`, or `1,5.2`.
  const trailing = source[index];
  if ((trailing === '.' || trailing === ',') && DIGIT.test(source[index + 1] ?? '')) {
    throw new MathError('invalid-expression');
  }

  return { token: token('number', text, Number.parseFloat(text)), next: index };
}

/**
 * Turns an expression into tokens, or refuses it.
 *
 * Every character has to be one the grammar knows. There is no "skip what I do
 * not understand" path, because that is how a parser starts accepting things
 * nobody designed.
 */
export function tokenize(source: string): Token[] {
  if (source.length > MAX_EXPRESSION_LENGTH) throw new MathError('invalid-expression');

  const tokens: Token[] = [];
  let index = 0;

  while (index < source.length) {
    const character = source[index];

    if (character === ' ' || character === '\t') {
      index += 1;
      continue;
    }

    if (tokens.length >= MAX_TOKENS) throw new MathError('invalid-expression');

    const single = SINGLE_CHARACTER[character];
    if (single) {
      tokens.push(token(single, character));
      index += 1;
      continue;
    }

    if (DIGIT.test(character)) {
      const read = readNumber(source, index);
      tokens.push(read.token);
      index = read.next;
      continue;
    }

    if (NAME_START.test(character)) {
      const start = index;
      while (index < source.length && NAME_PART.test(source[index])) index += 1;
      const name = source.slice(start, index);
      tokens.push(name.toLowerCase() === 'de' ? token('de', name) : token('identifier', name));
      continue;
    }

    throw new MathError('invalid-expression');
  }

  return tokens;
}
