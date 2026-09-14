/**
 * The flashcard cross-conformance fixture, asserted from the canonical side.
 *
 * `tests/fixtures/flashcard-conformance.json` is answered by two
 * implementations that read different things: this one reads the ProseMirror
 * document, and the terminal interface reads its lossless projection. They
 * cannot be compared to each other — they do not share an input — so both are
 * compared to the same written-down expectation, and either one drifting fails
 * its own suite with the note that drifted.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { Node as ProseMirrorNode } from '@tiptap/pm/model';
import { NoteEditor } from '../src/editor/editor.ts';
import { countFlashcards, extractFlashcards, reviewItems } from '../src/flashcards/extract.ts';

interface Fixture {
  readonly notes: ReadonlyArray<{
    readonly markdown: string;
    readonly counts: { readonly cards: number; readonly reviews: number };
    readonly cards: ReadonlyArray<{
      readonly front: string;
      readonly back: string;
      readonly mode: string;
      readonly form: string;
    }>;
    readonly reviews: ReadonlyArray<{ readonly direction: string; readonly source: number }>;
  }>;
}

const fixture: Fixture = JSON.parse(
  readFileSync(resolve(process.cwd(), '../tests/fixtures/flashcard-conformance.json'), 'utf8'),
);

function docOf(markdown: string): ProseMirrorNode {
  const editor = new NoteEditor({
    element: document.createElement('div'),
    initialContent: markdown,
  });
  return editor.getView().state.doc;
}

describe('flashcard cross-conformance', () => {
  it('has a fixture that covers the vocabulary', () => {
    expect(fixture.notes.length).toBeGreaterThan(20);
  });

  it.each(fixture.notes.map((note, index) => ({ index, ...note })))(
    'extracts note $index the same way',
    (expected) => {
      const sources = extractFlashcards(docOf(expected.markdown));

      expect(sources.length).toBe(expected.cards.length);
      sources.forEach((card, index) => {
        const wanted = expected.cards[index];
        const front = card.front.content
          .textBetween(0, card.front.content.size, '\n')
          .trim();
        const back = card.back.content.textBetween(0, card.back.content.size, '\n').trim();
        expect(front).toBe(wanted.front);
        expect(back).toBe(wanted.back);
        expect(card.mode).toBe(wanted.mode);
        expect(card.form).toBe(wanted.form);
      });

      expect(countFlashcards(sources)).toEqual(expected.counts);

      const items = reviewItems(sources);
      expect(items.length).toBe(expected.reviews.length);
      items.forEach((item, index) => {
        expect(item.direction).toBe(expected.reviews[index].direction);
        expect(item.source).toBe(expected.reviews[index].source);
      });
    },
  );
});
