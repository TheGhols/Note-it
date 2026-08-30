import { ReviewItem } from './extract.ts';

/**
 * One sitting with the cards a note held when it was opened.
 *
 * The list is a **snapshot**, and that is the point rather than an
 * optimisation. The note underneath goes on being a note: the reader can be
 * typing in it, AutoPaste can be filing something into it, and either of those
 * changes what the detector would say. If the session followed the document,
 * somebody on item 4 of 10 would find item 4 was now a different question
 * because a capture landed three paragraphs above it. So the document stays
 * live and the session stays still; what was written while studying is there
 * the next time studying starts.
 *
 * Nothing here persists. There is no identifier, no progress, no ordering and
 * no answer written anywhere — closing the panel ends the session, and that is
 * the whole of its lifetime. Scheduling is a later phase, and stable identity
 * is its problem, not this one's.
 */
export class StudySession {
  private readonly items: readonly ReviewItem[];
  private readonly random: () => number;
  /** Indices into `items`. Shuffling permutes this and never the list. */
  private order: number[];
  private cursor = 0;
  private revealed = false;

  public constructor(items: readonly ReviewItem[], random: () => number = Math.random) {
    this.items = items;
    this.random = random;
    this.order = items.map((_item, index) => index);
  }

  /** How many questions this sitting holds. */
  public get total(): number {
    return this.order.length;
  }

  /** Which one is on screen, counted from one. Zero when there are none. */
  public get position(): number {
    return this.total === 0 ? 0 : this.cursor + 1;
  }

  public get current(): ReviewItem | null {
    const index = this.order[this.cursor];
    return index === undefined ? null : (this.items[index] ?? null);
  }

  /** Whether the answer to the question on screen has been asked for. */
  public get isRevealed(): boolean {
    return this.revealed;
  }

  public get hasNext(): boolean {
    return this.cursor + 1 < this.total;
  }

  public get hasPrevious(): boolean {
    return this.cursor > 0;
  }

  /** Shows the answer. Asking twice is asking once. */
  public reveal(): void {
    if (this.total === 0) return;
    this.revealed = true;
  }

  /**
   * The next question, with its answer hidden again.
   *
   * The last one is the last one: there is no wrap. Coming back round to the
   * first card without saying so is how somebody answers the same question
   * twice believing they are making progress.
   */
  public next(): boolean {
    if (!this.hasNext) return false;
    this.cursor += 1;
    this.revealed = false;
    return true;
  }

  public previous(): boolean {
    if (!this.hasPrevious) return false;
    this.cursor -= 1;
    this.revealed = false;
    return true;
  }

  /**
   * A new order for the same questions, from the start.
   *
   * Fisher-Yates, which is the shuffle that gives every permutation the same
   * chance — sorting by a random comparator does not, and the bias is the sort
   * implementation's rather than anything a reader could reason about. The
   * source of randomness is injected so a test can state an order instead of
   * asserting something about chance.
   */
  public shuffle(): void {
    for (let index = this.order.length - 1; index > 0; index -= 1) {
      const swap = Math.floor(this.random() * (index + 1));
      const held = this.order[index];
      this.order[index] = this.order[swap];
      this.order[swap] = held;
    }
    this.cursor = 0;
    this.revealed = false;
  }

  /** The order the questions are currently in, for a test to inspect. */
  public sequence(): readonly number[] {
    return [...this.order];
  }
}
