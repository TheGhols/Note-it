/**
 * The cross-conformance fixture, asserted from the canonical side.
 *
 * `tests/fixtures/math-conformance.json` is one file that two implementations
 * answer to: this engine, which is the canonical one, and the Rust port in the
 * terminal interface. Neither is compared to the other directly — comparing
 * two implementations tells you they agree, not that either is right — so both
 * are compared to a written-down expectation, and either one drifting fails
 * its own suite.
 *
 * The fixture was generated from this engine, so a change here that is
 * deliberate updates the file and a change that is not shows up as a failure.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { classifyLine, evaluateNote, type MathSource } from '../src/math/document.ts';

interface ExpectedResult {
  readonly kind: 'none' | 'value' | 'error';
  readonly text?: string;
  readonly code?: string;
}

interface Fixture {
  readonly lines: ReadonlyArray<{
    readonly source: string;
    readonly kind: string;
    readonly result: ExpectedResult;
  }>;
  readonly notes: ReadonlyArray<{
    readonly sources: readonly MathSource[];
    readonly results: readonly ExpectedResult[];
  }>;
}

// Resolved from the repository root rather than from this file's URL: vitest
// serves modules over a dev server, so `import.meta.url` is not a file path.
const fixture: Fixture = JSON.parse(
  readFileSync(resolve(process.cwd(), '../tests/fixtures/math-conformance.json'), 'utf8'),
);

function shapeOf(result: ReturnType<typeof evaluateNote>[number]): ExpectedResult {
  if (result.kind === 'value') return { kind: 'value', text: result.text };
  if (result.kind === 'error') return { kind: 'error', code: result.code };
  return { kind: 'none' };
}

describe('math cross-conformance', () => {
  it('has a fixture with something in it', () => {
    expect(fixture.lines.length).toBeGreaterThan(40);
    expect(fixture.notes.length).toBeGreaterThan(5);
  });

  it.each(fixture.lines)('classifies and evaluates $source', (expected) => {
    expect(classifyLine(expected.source)).toBe(expected.kind);

    const [result] = evaluateNote([expected.source]);
    const shape = shapeOf(result);
    expect(shape.kind).toBe(expected.result.kind);
    if (expected.result.kind === 'value') expect(shape.text).toBe(expected.result.text);
    if (expected.result.kind === 'error') expect(shape.code).toBe(expected.result.code);
  });

  it.each(fixture.notes.map((note, index) => ({ index, ...note })))(
    'evaluates note $index top to bottom',
    (expected) => {
      const results = evaluateNote(expected.sources).map(shapeOf);
      expect(results.length).toBe(expected.results.length);
      results.forEach((result, line) => {
        const wanted = expected.results[line];
        expect(result.kind).toBe(wanted.kind);
        if (wanted.kind === 'value') expect(result.text).toBe(wanted.text);
        if (wanted.kind === 'error') expect(result.code).toBe(wanted.code);
      });
    },
  );
});
