import { MathError } from './errors.ts';
import { MathNode } from './parser.ts';

/**
 * Everything an expression is allowed to see.
 *
 * `variables` is a `Map`, not an object, and that is a security property rather
 * than a style choice: a plain object would answer `constructor`, `__proto__`
 * and `toString` with real JavaScript values, so a note writing
 * `= constructor` would be reaching into the runtime. A `Map` has no inherited
 * keys at all, so an unknown name is unknown whatever it is called.
 *
 * `samples` is the block of values an aggregator on this line operates on, as
 * decided by the document layer.
 */
export interface MathScope {
  readonly variables: ReadonlyMap<string, number>;
  readonly samples: readonly number[];
}

export const EMPTY_SCOPE: MathScope = { variables: new Map(), samples: [] };

/**
 * Evaluates a parsed expression.
 *
 * Nothing here interprets text: it walks a tree of six node shapes and does
 * arithmetic. There is no path from a note to a function call, a property
 * access or a global — those are not nodes this tree can hold.
 */
export function evaluate(node: MathNode, scope: MathScope): number {
  return finite(evaluateNode(node, scope));
}

function finite(value: number): number {
  // Overflow and `0/0` are not results; a number the reader cannot use is a
  // failed calculation, not a calculation that produced `Infinity`.
  if (!Number.isFinite(value)) throw new MathError('invalid-expression');
  return value === 0 ? 0 : value;
}

function evaluateNode(node: MathNode, scope: MathScope): number {
  switch (node.kind) {
    case 'number':
      return node.value;

    case 'percent':
      // A percentage is a hundredth. The contextual readings of `+`, `-` and
      // `de` are applied by their operators, which can see that the operand
      // was written with a `%`; on its own it is just the number it names.
      return evaluateNode(node.operand, scope) / 100;

    case 'negate':
      return -evaluateNode(node.operand, scope);

    case 'variable': {
      const value = scope.variables.get(node.name);
      if (value === undefined) throw new MathError('unknown-variable');
      return value;
    }

    case 'aggregate':
      return aggregate(node.name, scope.samples);

    case 'binary':
      return binary(node, scope);
  }
}

function binary(
  node: Extract<MathNode, { kind: 'binary' }>,
  scope: MathScope,
): number {
  const left = evaluateNode(node.left, scope);

  // `X% de Y` is the only reading of `de`, and the parser has already refused
  // it for anything but a percentage on the left.
  if (node.operator === 'de') return left * evaluateNode(node.right, scope);

  const right = evaluateNode(node.right, scope);

  switch (node.operator) {
    case '+':
      // `200 + 10%` is a ten percent increase, because that is what the line
      // means to everyone who writes it. The rule is tied to a `%` written
      // right there, not to a value that happens to have come from one: a
      // variable holding `10%` holds `0.1`, and `200 + taxa` adds `0.1`. What
      // you can see is what applies.
      return node.right.kind === 'percent' ? left + left * right : left + right;
    case '-':
      return node.right.kind === 'percent' ? left - left * right : left - right;
    case '*':
      return left * right;
    case '/':
      if (right === 0) throw new MathError('division-by-zero');
      return left / right;
  }
}

function aggregate(name: 'sum' | 'avg' | 'count', samples: readonly number[]): number {
  if (name === 'count') return samples.length;

  const total = samples.reduce((sum, value) => sum + value, 0);
  if (name === 'sum') return total;

  // The average of nothing is `0 / 0`, and saying so is more honest than
  // answering zero.
  if (samples.length === 0) throw new MathError('division-by-zero');
  return total / samples.length;
}
