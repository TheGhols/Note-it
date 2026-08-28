import { MathError, MathErrorCode } from './errors.ts';
import { formatNumber } from './format.ts';
import { evaluate } from './evaluate.ts';
import { isValidName } from './lexer.ts';
import { isLiteral, MathNode, parse } from './parser.ts';

/**
 * A line of a note, as the math engine sees it.
 *
 * `null` means "this line can never be a calculation" — it is inside a code
 * block, it carries an inline-code span, it is a heading, a list item, a
 * comment. The engine still receives it, because it has to know a line was
 * there: an opaque line breaks a block of values the same way a paragraph of
 * prose does.
 */
export type MathSource = string | null;

export type MathLineKind = 'calculation' | 'declaration' | 'prose';

/** What, if anything, gets drawn beside a line. */
export type MathLineResult =
  | { readonly kind: 'none' }
  | { readonly kind: 'value'; readonly value: number; readonly text: string }
  | { readonly kind: 'error'; readonly code: MathErrorCode };

const NOTHING: MathLineResult = { kind: 'none' };

/**
 * A calculation: a line beginning with `=`.
 *
 * The marker is required, and that is the whole point. Without it every
 * "2 + 2 = ?" written in prose, every date, every version number would be a
 * candidate, and the engine would spend its life deciding which numbers in a
 * note are arithmetic. `==` is excluded so a line of emphasis is never a
 * calculation.
 */
const CALCULATION = /^[ \t]*=(?!=)(.*)$/;

/**
 * A declaration: one bare token, `:=`, and an expression.
 *
 * `:=` rather than `=` because `=` already means "calculate this", and because
 * a line of ordinary prose almost never contains `:=`. The name is captured as
 * whatever was written before the operator so an unusable one is reported
 * rather than silently read as prose — the reader typed `:=` and meant it.
 */
const DECLARATION = /^[ \t]*([^\s:=]+)[ \t]*:=(.*)$/;

export function classifyLine(source: MathSource): MathLineKind {
  if (source === null) return 'prose';
  if (CALCULATION.test(source)) return 'calculation';
  if (DECLARATION.test(source)) return 'declaration';
  return 'prose';
}

function errorOf(failure: unknown): MathLineResult {
  if (failure instanceof MathError) return { kind: 'error', code: failure.code };
  // A defect in this code, not in the note. It is reported to the reader as an
  // invalid expression and left visible on stderr for whoever maintains it.
  console.error('note-it: math engine failure', failure);
  return { kind: 'error', code: 'invalid-expression' };
}

/**
 * A result, spelled for the reader.
 *
 * A conversion carries its target unit, because `10000` on its own answers a
 * different question from `10000 m`. Everything else is a bare number, exactly
 * as it was before conversions existed. The unit is the registry's own symbol
 * — `°C`, `cm²`, `km/h` — and never anything the note wrote, so a spelling in
 * a note can reach the screen only by being one of the table's own.
 */
function valueOf(value: number, node: MathNode): MathLineResult {
  if (node.kind !== 'conversion') {
    return { kind: 'value', value, text: formatNumber(value) };
  }
  const { symbol, plural } = node.to;
  const unit = plural !== undefined && Math.abs(value) !== 1 ? plural : symbol;
  return { kind: 'value', value, text: `${formatNumber(value)} ${unit}` };
}

/**
 * Evaluates a whole note, top to bottom, and returns one result per line.
 *
 * Top-down is the entire dependency model. A variable exists from the line
 * that declares it onwards and nowhere else, which makes `= preco * 2` above
 * `preco := 100` an unknown variable rather than a puzzle, and makes cycles
 * impossible without a graph resolver to prevent them: `a := b + 1` above
 * `b := a + 1` fails on the first line because `b` is not there yet, and the
 * second fails because the first one did.
 *
 * Variables are note-wide and cross anything — headings, code blocks, lists.
 * The only thing contiguity decides is which values an aggregator adds up.
 */
export function evaluateNote(lines: readonly MathSource[]): MathLineResult[] {
  const variables = new Map<string, number>();
  const results: MathLineResult[] = [];

  /**
   * The run of consecutive calculation lines directly above the current one,
   * which is what `sum`, `avg` and `count` operate on.
   *
   * Only `= …` lines that produced a value are in it. Prose, a heading, a
   * failed calculation or a declaration of anything but an aggregate all end
   * the run, so a number that happens to be sitting in a sentence is never
   * added to anything and two blocks of values separated by a line of text
   * stay two blocks.
   */
  let run: number[] = [];

  /**
   * Whether the line just above was an aggregator.
   *
   * An aggregator reads the block and leaves it where it is, so the three of
   * them stacked under one block each answer about that same block. The block
   * ends at the next line that is not an aggregator: a value under a `= sum`
   * starts a new one, which is what keeps two totals in a note two totals and
   * not one running into the other.
   */
  let afterAggregate = false;

  for (const source of lines) {
    const kind = classifyLine(source);

    if (kind === 'prose' || source === null) {
      results.push(NOTHING);
      run = [];
      afterAggregate = false;
      continue;
    }

    if (kind === 'calculation') {
      const expression = CALCULATION.exec(source)![1];
      let node: MathNode | null = null;
      let result: MathLineResult;
      try {
        node = parse(expression);
        result = valueOf(evaluate(node, { variables, samples: run }), node);
      } catch (failure) {
        result = errorOf(failure);
      }
      results.push(result);

      if (node?.kind === 'aggregate') {
        afterAggregate = true;
      } else if (node?.kind === 'conversion') {
        // A converted quantity ends the block. `sum`, `avg` and `count` add up
        // plain numbers and know nothing about units, so letting `10 km em m`
        // into a block would total ten thousand of something against five of
        // something else and present the answer as a fact. Aggregating over
        // units is a real feature; silently aggregating across them is a bug.
        run = [];
        afterAggregate = false;
      } else if (result.kind === 'value') {
        if (afterAggregate) {
          run = [result.value];
          afterAggregate = false;
        } else {
          run.push(result.value);
        }
      } else {
        run = [];
        afterAggregate = false;
      }
      continue;
    }

    const [, name, expression] = DECLARATION.exec(source)!;
    const samples = run;

    if (!isValidName(name)) {
      // The name is unusable, so whatever it used to hold is not this. A note
      // that kept the old value here would silently answer with a variable the
      // reader can no longer see the definition of.
      variables.delete(name);
      results.push({ kind: 'error', code: 'invalid-name' });
      run = [];
      afterAggregate = false;
      continue;
    }

    let declared: MathNode | null = null;
    try {
      declared = parse(expression);
      const value = evaluate(declared, { variables, samples });
      // A variable holds a number and only a number. `metros := 10 km em m`
      // stores 10000, not "10000 metres": carrying a unit through a variable
      // would mean every value in the engine becoming a quantity, and with it
      // percentages, aggregation and every existing rule. See ADR-025.
      variables.set(name, value);
      // The value of `preco := 120` is already on the line. Only a declaration
      // that computes something gets a result drawn beside it.
      results.push(isLiteral(declared) ? NOTHING : valueOf(value, declared));
    } catch (failure) {
      // A declaration that failed declares nothing. Everything below that used
      // the name reports an unknown variable, which is the truth: from this
      // line on, there is no such value.
      variables.delete(name);
      results.push(errorOf(failure));
    }

    // A declaration reads the block above it the same way `= sum` does, and
    // leaves it alone the same way; anything else it declares ends the block.
    if (declared?.kind === 'aggregate') {
      afterAggregate = true;
    } else {
      run = [];
      afterAggregate = false;
    }
  }

  return results;
}
