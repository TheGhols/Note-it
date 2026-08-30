import { describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { extractFlashcards, FlashcardSide, ReviewItem, reviewItems } from '../src/flashcards/extract.ts';
import { StudySession } from '../src/flashcards/session.ts';

function itemsOf(markdown: string): ReviewItem[] {
  const editor = new NoteEditor({
    element: document.createElement('div'),
    initialContent: markdown,
  });
  return reviewItems(extractFlashcards(editor.getView().state.doc));
}

function text(side: FlashcardSide | undefined): string {
  if (!side) return '';
  let out = '';
  side.content.forEach((node) => {
    out += node.textContent;
  });
  return out;
}

function question(session: StudySession): string {
  return text(session.current?.question);
}

/**
 * A source of randomness a test can state rather than hope about.
 *
 * Shuffling has to be testable without being flaky, and the only way to have
 * both is for the randomness to come from outside.
 */
function scripted(values: number[]): () => number {
  let index = 0;
  return () => {
    const value = values[index] ?? 0;
    index += 1;
    return value;
  };
}

const FOUR = 'A :: B\n\nC :: D\n\nE :: F\n\nG :: H';

describe('a sitting with the cards', () => {
  it('starts on the first question with its answer hidden', () => {
    const session = new StudySession(itemsOf(FOUR));

    expect(session.total).toBe(4);
    expect(session.position).toBe(1);
    expect(session.isRevealed).toBe(false);
    expect(question(session)).toBe('A');
  });

  it('shows the answer when it is asked for, and only then', () => {
    const session = new StudySession(itemsOf('A :: B'));

    expect(session.isRevealed).toBe(false);
    session.reveal();
    expect(session.isRevealed).toBe(true);
    expect(text(session.current?.answer)).toBe('B');
    // Asking twice is asking once.
    session.reveal();
    expect(session.isRevealed).toBe(true);
  });

  it('moves forward and back, hiding the answer again each time', () => {
    const session = new StudySession(itemsOf(FOUR));

    session.reveal();
    expect(session.next()).toBe(true);
    expect(session.position).toBe(2);
    expect(question(session)).toBe('C');
    expect(session.isRevealed).toBe(false);

    session.reveal();
    expect(session.previous()).toBe(true);
    expect(session.position).toBe(1);
    expect(question(session)).toBe('A');
    expect(session.isRevealed).toBe(false);
  });

  it('stops at both ends instead of wrapping round', () => {
    // Coming back to the first question without saying so is how somebody
    // answers the same card twice believing they are making progress.
    const session = new StudySession(itemsOf(FOUR));

    expect(session.hasPrevious).toBe(false);
    expect(session.previous()).toBe(false);
    expect(session.position).toBe(1);

    while (session.hasNext) session.next();
    expect(session.position).toBe(4);
    expect(session.next()).toBe(false);
    expect(session.position).toBe(4);
  });

  it('counts questions rather than cards', () => {
    // Five cards, two of them reversible: seven things to answer. A progress
    // counter that said "de 5" would be wrong for the whole sitting.
    const session = new StudySession(itemsOf('A :: B\n\nC :: D\n\nE :: F\n\nG ::: H\n\nI ::: J'));
    expect(session.total).toBe(7);

    const seen: string[] = [question(session)];
    while (session.hasNext) {
      session.next();
      seen.push(question(session));
    }
    expect(seen).toEqual(['A', 'C', 'E', 'G', 'H', 'I', 'J']);
  });

  it('holds an empty sitting without falling over', () => {
    const session = new StudySession([]);
    expect(session.total).toBe(0);
    expect(session.position).toBe(0);
    expect(session.current).toBeNull();
    expect(session.hasNext).toBe(false);
    expect(session.hasPrevious).toBe(false);
    session.reveal();
    expect(session.isRevealed).toBe(false);
  });

  it('is a snapshot: the note goes on changing and the sitting does not', () => {
    const editor = new NoteEditor({
      element: document.createElement('div'),
      initialContent: 'A :: B\n\nC :: D',
    });
    const session = new StudySession(reviewItems(extractFlashcards(editor.getView().state.doc)));
    expect(session.total).toBe(2);

    // Whatever arrives now — a keystroke, a capture — belongs to the next
    // sitting. Somebody on question two stays on question two.
    editor.setMarkdown('A :: B\n\nC :: D\n\nE :: F\n\nNovo :: Cartão');
    session.next();

    expect(session.total).toBe(2);
    expect(question(session)).toBe('C');
    expect(session.hasNext).toBe(false);
    // ...and the next sitting sees everything.
    expect(reviewItems(extractFlashcards(editor.getView().state.doc))).toHaveLength(4);
  });
});

describe('shuffling', () => {
  it('keeps every question exactly once', () => {
    const session = new StudySession(itemsOf(FOUR), scripted([0.9, 0.1, 0.5]));
    session.shuffle();

    const order = [...session.sequence()].sort((a, b) => a - b);
    expect(order).toEqual([0, 1, 2, 3]);
    expect(session.total).toBe(4);
  });

  it('produces the order the randomness dictates, and no other', () => {
    // Fisher-Yates walks from the end, swapping each position with one chosen
    // from those at or below it. With every choice landing on index 0 the
    // permutation is fully determined, so this is a test of the algorithm and
    // not of chance.
    const session = new StudySession(itemsOf(FOUR), scripted([0, 0, 0]));
    session.shuffle();
    expect(session.sequence()).toEqual([1, 2, 3, 0]);

    const seen: string[] = [question(session)];
    while (session.hasNext) {
      session.next();
      seen.push(question(session));
    }
    expect(seen).toEqual(['C', 'E', 'G', 'A']);
  });

  it('leaves the order alone when every choice is the position itself', () => {
    // The identity case. A shuffle built on a random comparator could not
    // promise this, which is part of why it is not one.
    const session = new StudySession(itemsOf(FOUR), scripted([0.999, 0.999, 0.999]));
    session.shuffle();
    expect(session.sequence()).toEqual([0, 1, 2, 3]);
  });

  it('goes back to the first question with its answer hidden', () => {
    const session = new StudySession(itemsOf(FOUR), scripted([0, 0, 0]));
    session.next();
    session.next();
    session.reveal();

    session.shuffle();
    expect(session.position).toBe(1);
    expect(session.isRevealed).toBe(false);
    expect(session.hasPrevious).toBe(false);
  });

  it('is a no-op on one question and on none', () => {
    const single = new StudySession(itemsOf('A :: B'), scripted([0]));
    single.shuffle();
    expect(single.sequence()).toEqual([0]);

    const none = new StudySession([], scripted([0]));
    none.shuffle();
    expect(none.sequence()).toEqual([]);
    expect(none.position).toBe(0);
  });
});
