import { Fragment, Node as ProseMirrorNode } from '@tiptap/pm/model';
import { isManagedAsset } from '../markdown/assetReference.ts';

/**
 * Flashcards, read out of the note rather than kept beside it.
 *
 * A card here is a **projection**. Nothing is stored: no `flashcards.json`, no
 * database, no identifier hidden in a comment, no front-matter, and not one
 * byte written into the Markdown by anything in this file. What the reader
 * typed is the card, and this reads the document they typed it into.
 *
 * That is the whole design, and it is what makes editing work: change the
 * words and the card changes with them; delete the delimiter and the card
 * stops existing. There is no second copy to fall out of step, nothing to
 * migrate, and nothing to reconcile after a restore from a backup — a note
 * that comes back out of the trash brings its cards because it brings its
 * text.
 *
 * The input is the **ProseMirror document**, never the Markdown source. A
 * regular expression over the file would be shorter and would be wrong: it
 * cannot see that `::` is inside a fenced block, inside inline code, inside an
 * `<img data-note-it-align="…">`, or part of `https://` — and every one of
 * those appears in real notes. The document already knows what is code, what
 * is text and what is an image, so this asks it instead of guessing.
 */

/** How many directions a card is studied in. */
export type FlashcardMode = 'basic' | 'reversible';

/** Which way round one review item runs. */
export type FlashcardDirection = 'forward' | 'reverse';

/** Whether the card was written on one line or across blocks. */
export type FlashcardForm = 'inline' | 'block';

/**
 * One side of a card, as content of the note's own document.
 *
 * Block-level content, so both forms end up the same shape and the study panel
 * has one thing to render. The nodes are the document's own — the same marks,
 * the same image nodes, the same schema — which is what keeps bold bold and an
 * image an image without a second representation existing anywhere.
 */
export interface FlashcardSide {
  readonly content: Fragment;
}

/** A card as it is written in the note. */
export interface FlashcardSource {
  readonly front: FlashcardSide;
  readonly back: FlashcardSide;
  readonly mode: FlashcardMode;
  readonly form: FlashcardForm;
  /** Where the card begins in the document. Ordering, and nothing else. */
  readonly pos: number;
  /** The delimiter itself, so the editor can mark it without touching it. */
  readonly delimiter: { readonly from: number; readonly to: number };
}

/**
 * One thing to answer.
 *
 * A basic card is one of these; a reversible card is two. The distinction is
 * the reason the count says both numbers — five cards can be seven questions,
 * and a progress bar that says "1 de 5" while there are seven to get through
 * is lying about the shorter half.
 */
export interface ReviewItem {
  readonly question: FlashcardSide;
  readonly answer: FlashcardSide;
  readonly direction: FlashcardDirection;
  /** Index of the card this came from, in document order. */
  readonly source: number;
}

/** One direction. */
export const BASIC_DELIMITER = '::';
/** Both directions. */
export const REVERSIBLE_DELIMITER = ':::';

/**
 * What stands in for something that is not text while the delimiters are
 * looked for.
 *
 * One character per unit of document, so an index into the scanned string is
 * an offset into the block's content and the two never drift. It is not
 * whitespace on purpose: `[image]:: x` is not a card, for the same reason
 * `A::B` is not one.
 */
const OPAQUE = '\0';

function hasCodeMark(node: ProseMirrorNode): boolean {
  return node.marks.some((mark) => mark.type.name === 'code');
}

/**
 * One textblock as a string the delimiters can be looked for in.
 *
 * Text carrying the `code` mark is masked rather than included, which is how
 * `` `A :: B` `` stays a sentence about a delimiter instead of becoming a
 * card. A hard break contributes a newline, because a line ending is
 * whitespace and the reader who put `::` on its own line inside one paragraph
 * meant the same thing as the one who used two.
 */
function scanText(block: ProseMirrorNode): string {
  let text = '';
  block.content.forEach((child, offset) => {
    // A child that contributed nothing still has to contribute its size.
    while (text.length < offset) text += OPAQUE;

    if (child.isText && !hasCodeMark(child)) {
      text += child.text ?? '';
      return;
    }
    text += child.type.name === 'hardBreak' ? '\n' : OPAQUE.repeat(child.nodeSize);
  });
  return text;
}

interface DelimiterHit {
  readonly from: number;
  readonly to: number;
  readonly mode: FlashcardMode;
}

/**
 * Every delimiter in one textblock, as offsets into its content.
 *
 * Runs of colons are matched whole, which is the longest-match rule stated
 * once rather than as an ordering between two patterns: `:::` is a run of
 * three and is never read as `::` followed by `:`, and `::::` is a run of four
 * and is nothing at all. A single colon is left alone, so `12:30` and
 * `https://example.com` are the times and addresses they are.
 *
 * Whitespace on both sides is required. `namespace::method` and `A::B` are
 * ordinary technical writing, and a note is full of it; demanding the spaces
 * costs the reader nothing and stops the detector reaching into code they
 * happened to paste. The edges of the block count as whitespace: a delimiter
 * with nothing on one side of it is refused later, for having an empty side,
 * rather than twice for two different reasons.
 */
function delimitersIn(block: ProseMirrorNode): DelimiterHit[] {
  const text = scanText(block);
  const hits: DelimiterHit[] = [];
  const runs = /:+/g;

  let match = runs.exec(text);
  while (match !== null) {
    const from = match.index;
    const to = from + match[0].length;
    const length = match[0].length;

    if (length === 2 || length === 3) {
      const before = from === 0 ? ' ' : text[from - 1];
      const after = to === text.length ? ' ' : text[to];
      if (/\s/.test(before) && /\s/.test(after)) {
        hits.push({ from, to, mode: length === 3 ? 'reversible' : 'basic' });
      }
    }
    match = runs.exec(text);
  }
  return hits;
}

/**
 * Whether a side has anything on it.
 *
 * Text is content when it is not only whitespace, and a **managed image is
 * content on its own** — a card whose front is an ECG and whose back is the
 * diagnosis is the reason this phase exists, and judging a side by its
 * `textContent` would throw it away. A reference the store does not manage is
 * not content: it draws nothing, so a side holding only that is a blank side.
 */
function hasMeaning(fragment: Fragment): boolean {
  let meaningful = false;
  fragment.forEach((child) => {
    if (meaningful) return;
    if (child.isText) {
      if ((child.text ?? '').trim() !== '') meaningful = true;
      return;
    }
    if (child.type.name === 'noteItImage') {
      if (isManagedAsset(child.attrs?.src)) meaningful = true;
      return;
    }
    if (child.type.name === 'hardBreak') return;
    if (child.isLeaf || child.isAtom) {
      meaningful = true;
      return;
    }
    if (hasMeaning(child.content)) meaningful = true;
  });
  return meaningful;
}

/**
 * The same fragment without the whitespace the delimiter was written with.
 *
 * Marks survive, because a boundary text node is cut rather than rebuilt: the
 * bold that ran up to the delimiter is still bold on the card.
 */
function trimInline(fragment: Fragment): Fragment {
  const children: ProseMirrorNode[] = [];
  fragment.forEach((child) => children.push(child));

  while (children.length > 0) {
    const first = children[0];
    if (first.isText) {
      const text = first.text ?? '';
      const trimmed = text.replace(/^\s+/, '');
      if (trimmed === '') {
        children.shift();
        continue;
      }
      if (trimmed.length !== text.length) children[0] = first.cut(text.length - trimmed.length);
      break;
    }
    if (first.type.name === 'hardBreak') {
      children.shift();
      continue;
    }
    break;
  }

  while (children.length > 0) {
    const last = children[children.length - 1];
    if (last.isText) {
      const text = last.text ?? '';
      const trimmed = text.replace(/\s+$/, '');
      if (trimmed === '') {
        children.pop();
        continue;
      }
      if (trimmed.length !== text.length) {
        children[children.length - 1] = last.cut(0, trimmed.length);
      }
      break;
    }
    if (last.type.name === 'hardBreak') {
      children.pop();
      continue;
    }
    break;
  }

  return Fragment.fromArray(children);
}

/**
 * A paragraph that is nothing but a delimiter, and which delimiter it is.
 *
 * `null` for everything else, including a paragraph holding a delimiter and
 * anything at all besides — an image, a second word — and one whose delimiter
 * is inline code. Only a top-level paragraph is ever asked, so a `::` inside a
 * quote or a list item is not a marker.
 */
function markerMode(node: ProseMirrorNode): FlashcardMode | null {
  if (node.type.name !== 'paragraph') return null;

  let text = '';
  let plain = true;
  node.content.forEach((child) => {
    if (!child.isText || hasCodeMark(child)) {
      plain = false;
      return;
    }
    text += child.text ?? '';
  });
  if (!plain) return null;

  const trimmed = text.trim();
  if (trimmed === REVERSIBLE_DELIMITER) return 'reversible';
  if (trimmed === BASIC_DELIMITER) return 'basic';
  return null;
}

/** One whole block as a side, or `null` when there is nothing on it. */
function blockSide(node: ProseMirrorNode): FlashcardSide | null {
  const content = Fragment.from(node);
  return hasMeaning(content) ? { content } : null;
}

/**
 * Every card in the document, in the order they are written.
 *
 * Two forms, and the block form is decided first. A marker paragraph takes the
 * blocks on either side of it, and those blocks are then not read again as
 * cards of their own: a block cannot be both an answer and a card, and
 * deciding it once is what keeps the result the same however the note is read.
 */
export function extractFlashcards(doc: ProseMirrorNode): FlashcardSource[] {
  const found: FlashcardSource[] = [];
  const blocks: Array<{ node: ProseMirrorNode; pos: number }> = [];
  doc.forEach((node, pos) => blocks.push({ node, pos }));

  const consumed = new Set<number>();

  blocks.forEach((marker, index) => {
    const mode = markerMode(marker.node);
    if (mode === null) return;

    const before = blocks[index - 1];
    const after = blocks[index + 1];
    if (!before || !after) return;
    // A marker is never a side: `A / :: / ::: / B` is one reader's typo, not a
    // card whose answer is three colons.
    if (markerMode(before.node) !== null || markerMode(after.node) !== null) return;

    const front = blockSide(before.node);
    const back = blockSide(after.node);
    if (!front || !back) return;

    found.push({
      front,
      back,
      mode,
      form: 'block',
      pos: before.pos,
      delimiter: { from: marker.pos + 1, to: marker.pos + 1 + marker.node.content.size },
    });
    consumed.add(index - 1);
    consumed.add(index);
    consumed.add(index + 1);
  });

  const blocked = Array.from(consumed, (index) => blocks[index]).map((entry) => [
    entry.pos,
    entry.pos + entry.node.nodeSize,
  ]);

  doc.descendants((node, pos) => {
    if (blocked.some(([from, to]) => pos >= from && pos < to)) return false;
    if (!node.isTextblock) return true;
    // A fenced block is code. Everything in it is code, delimiters included.
    if (node.type.spec.code) return false;

    const hits = delimitersIn(node);
    // Exactly one, or nothing. `A :: B :: C` has two readings and no way to
    // choose between them, and guessing at one is worse than declining.
    if (hits.length !== 1) return false;

    const hit = hits[0];
    const front = trimInline(node.content.cut(0, hit.from));
    const back = trimInline(node.content.cut(hit.to));
    if (!hasMeaning(front) || !hasMeaning(back)) return false;

    // Wrapped in a paragraph, whatever block it was written in: the card is
    // the words, not the heading or the list item they happened to sit in.
    const paragraph = node.type.schema.nodes.paragraph;
    found.push({
      front: { content: Fragment.from(paragraph.create(null, front)) },
      back: { content: Fragment.from(paragraph.create(null, back)) },
      mode: hit.mode,
      form: 'inline',
      pos,
      delimiter: { from: pos + 1 + hit.from, to: pos + 1 + hit.to },
    });
    return false;
  });

  return found.sort((a, b) => a.pos - b.pos);
}

/**
 * The cards expanded into the things actually answered.
 *
 * A reversible card puts both of its directions where it is written rather
 * than collecting them at the end, so studying follows the note.
 */
export function reviewItems(sources: readonly FlashcardSource[]): ReviewItem[] {
  const items: ReviewItem[] = [];
  sources.forEach((source, index) => {
    items.push({
      question: source.front,
      answer: source.back,
      direction: 'forward',
      source: index,
    });
    if (source.mode === 'reversible') {
      items.push({
        question: source.back,
        answer: source.front,
        direction: 'reverse',
        source: index,
      });
    }
  });
  return items;
}

/** How many cards are written, and how many questions they come to. */
export interface FlashcardCounts {
  readonly cards: number;
  readonly reviews: number;
}

export function countFlashcards(sources: readonly FlashcardSource[]): FlashcardCounts {
  return {
    cards: sources.length,
    reviews: sources.reduce(
      (total, source) => total + (source.mode === 'reversible' ? 2 : 1),
      0,
    ),
  };
}
