import { MathError } from './errors.ts';
import { AGGREGATE_NAMES, Token, tokenize } from './lexer.ts';

export type AggregateName = 'sum' | 'avg' | 'count';

export type MathNode =
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'variable'; readonly name: string }
  | { readonly kind: 'percent'; readonly operand: MathNode }
  | { readonly kind: 'negate'; readonly operand: MathNode }
  | {
      readonly kind: 'binary';
      readonly operator: '+' | '-' | '*' | '/' | 'de';
      readonly left: MathNode;
      readonly right: MathNode;
    }
  | { readonly kind: 'aggregate'; readonly name: AggregateName };

/** Deep enough for any expression a person writes, shallow enough to be safe. */
export const MAX_DEPTH = 64;

/**
 * The grammar, written out once:
 *
 * ```text
 * expression     := additive
 * additive       := multiplicative (('+' | '-') multiplicative)*
 * multiplicative := unary (('*' | '/' | 'de') unary)*
 * unary          := ('-' | '+') unary | postfix
 * postfix        := primary '%'*
 * primary        := number | name | '(' expression ')'
 * ```
 *
 * plus one whole-expression form, `sum`, `avg` or `count` standing alone.
 *
 * `de` sits at the multiplicative level and is accepted only when what stands
 * to its left is a percentage — `10% de 200` reads, `200 de 10` does not. That
 * check is syntactic and happens here, so the reader is told the expression is
 * invalid rather than being handed a number nobody meant.
 */
export function parse(source: string): MathNode {
  const tokens = tokenize(source);
  if (tokens.length === 0) throw new MathError('invalid-expression');

  // An aggregator is the whole expression or it is not an expression. Allowing
  // `sum * 2` would mean deciding what the aggregated set is when the line also
  // does arithmetic, and there is no reading of that which is obvious.
  if (tokens.length === 1 && tokens[0].type === 'identifier') {
    const name = tokens[0].text.toLowerCase();
    if (AGGREGATE_NAMES.has(name)) {
      return { kind: 'aggregate', name: name as AggregateName };
    }
  }

  const parser = new Parser(tokens);
  const node = parser.parseExpression(0);
  parser.expectEnd();
  return node;
}

class Parser {
  private index = 0;

  constructor(private readonly tokens: readonly Token[]) {}

  private peek(): Token | undefined {
    return this.tokens[this.index];
  }

  private take(): Token {
    const next = this.tokens[this.index];
    if (!next) throw new MathError('invalid-expression');
    this.index += 1;
    return next;
  }

  expectEnd(): void {
    if (this.index !== this.tokens.length) throw new MathError('invalid-expression');
  }

  parseExpression(depth: number): MathNode {
    if (depth > MAX_DEPTH) throw new MathError('invalid-expression');
    return this.parseAdditive(depth);
  }

  private parseAdditive(depth: number): MathNode {
    let left = this.parseMultiplicative(depth);
    for (;;) {
      const next = this.peek();
      if (next?.type !== 'plus' && next?.type !== 'minus') return left;
      this.index += 1;
      const right = this.parseMultiplicative(depth);
      left = { kind: 'binary', operator: next.type === 'plus' ? '+' : '-', left, right };
    }
  }

  private parseMultiplicative(depth: number): MathNode {
    let left = this.parseUnary(depth);
    for (;;) {
      const next = this.peek();
      if (next?.type === 'star' || next?.type === 'slash') {
        this.index += 1;
        const right = this.parseUnary(depth);
        left = { kind: 'binary', operator: next.type === 'star' ? '*' : '/', left, right };
        continue;
      }
      if (next?.type === 'de') {
        // `de` reads "of", and only a percentage is "of" something.
        if (left.kind !== 'percent') throw new MathError('invalid-expression');
        this.index += 1;
        const right = this.parseUnary(depth);
        left = { kind: 'binary', operator: 'de', left, right };
        continue;
      }
      return left;
    }
  }

  private parseUnary(depth: number): MathNode {
    const next = this.peek();
    if (next?.type === 'minus') {
      this.index += 1;
      return { kind: 'negate', operand: this.parseUnary(depth + 1) };
    }
    if (next?.type === 'plus') {
      this.index += 1;
      return this.parseUnary(depth + 1);
    }
    return this.parsePostfix(depth);
  }

  private parsePostfix(depth: number): MathNode {
    let node = this.parsePrimary(depth);
    while (this.peek()?.type === 'percent') {
      this.index += 1;
      node = { kind: 'percent', operand: node };
    }
    return node;
  }

  private parsePrimary(depth: number): MathNode {
    const next = this.take();

    if (next.type === 'number') return { kind: 'number', value: next.value };

    if (next.type === 'identifier') {
      // Reached only when an aggregator is used as part of a larger
      // expression, which the whole-expression rule above has already ruled
      // out. `de` never gets here: it is its own token.
      if (AGGREGATE_NAMES.has(next.text.toLowerCase())) {
        throw new MathError('invalid-expression');
      }
      return { kind: 'variable', name: next.text };
    }

    if (next.type === 'lparen') {
      const inner = this.parseExpression(depth + 1);
      if (this.take().type !== 'rparen') throw new MathError('invalid-expression');
      return inner;
    }

    throw new MathError('invalid-expression');
  }
}

/**
 * Whether an expression is a bare literal, so its value is already on screen.
 *
 * `preco := 120` needs no result beside it; `subtotal := preco * 3` does. The
 * rule is deliberately about the shape of what was written and not about the
 * value, so the same line always decides the same way.
 */
export function isLiteral(node: MathNode): boolean {
  if (node.kind === 'number') return true;
  if (node.kind === 'percent' || node.kind === 'negate') return isLiteral(node.operand);
  return false;
}
