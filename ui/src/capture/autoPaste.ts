import { closeHistory } from '@tiptap/pm/history';
import { Fragment, Node as ProseMirrorNode, Schema } from '@tiptap/pm/model';
import type { EditorView } from '@tiptap/pm/view';

/**
 * AutoPaste, on the page: one capture, appended to the end of the note.
 *
 * The host decides *whether* a clipboard change is a capture; this decides
 * what the document becomes when one arrives. Nothing here reads a clipboard,
 * and nothing here has an opinion about focus: a capture happens while the
 * reader is in another application, so the note stays exactly as passive as it
 * was — same selection, same scroll, same window.
 */

/** What goes between the note's existing content and a new capture. */
export type CaptureDelimiter = 'line' | 'blankLine' | 'separator';

export const CAPTURE_DELIMITERS: readonly { id: CaptureDelimiter; label: string }[] = [
  { id: 'line', label: 'Linha' },
  { id: 'blankLine', label: 'Linha em branco' },
  { id: 'separator', label: 'Separador' },
];

export const DEFAULT_CAPTURE_DELIMITER: CaptureDelimiter = 'blankLine';

export function normalizeDelimiter(value: unknown): CaptureDelimiter {
  return CAPTURE_DELIMITERS.some((entry) => entry.id === value)
    ? (value as CaptureDelimiter)
    : DEFAULT_CAPTURE_DELIMITER;
}

export function delimiterLabel(delimiter: CaptureDelimiter): string {
  return CAPTURE_DELIMITERS.find((entry) => entry.id === delimiter)!.label;
}

/**
 * Captured text, split the way this editor splits a plain-text paste.
 *
 * Deliberately the same rule ProseMirror itself applies to `text/plain` —
 * `text.split(/(?:\r\n?|\n)+/)`, one block per run of newlines — because
 * AutoPaste is a paste. Anything cleverer would mean a capture and a `Ctrl+V`
 * of the same clipboard produced different documents.
 *
 * That also settles the line endings: a run of `\r\n` is a separator like any
 * other, so CRLF never survives into the document as a stray carriage return.
 * Leading and trailing runs give empty pieces, and those are dropped rather
 * than turned into blank paragraphs — a copy that happened to end in a newline
 * must not file a phantom line. Nothing inside a block is trimmed: the
 * reader's spacing is the reader's.
 */
export function splitCapture(text: string): string[] {
  const blocks = text.split(/(?:\r\n?|\n)+/);
  while (blocks.length > 0 && blocks[0] === '') blocks.shift();
  while (blocks.length > 0 && blocks[blocks.length - 1] === '') blocks.pop();
  return blocks;
}

/** Whether a capture carries anything worth filing. */
export function isCapturable(text: string): boolean {
  return text.trim() !== '';
}

/**
 * Whether the document is empty in the sense that matters for a first capture:
 * one textblock with nothing in it, which is what a new note holds.
 */
export function isDocumentEmpty(doc: ProseMirrorNode): boolean {
  return doc.childCount === 1 && doc.firstChild!.isTextblock && doc.firstChild!.content.size === 0;
}

/**
 * Whether a hard break can follow whatever the last block already holds.
 *
 * The `line` delimiter continues the previous paragraph, which is only
 * possible when that block is a textblock that accepts one. A note whose last
 * block is a horizontal rule, a code block or a list falls back to appending a
 * paragraph, because the alternative is refusing the capture.
 */
function canContinueLastBlock(doc: ProseMirrorNode, schema: Schema): boolean {
  const last = doc.lastChild;
  const hardBreak = schema.nodes.hardBreak;
  if (!last || !hardBreak || !last.isTextblock || last.type.spec.code) return false;
  return last.contentMatchAt(last.childCount).matchType(hardBreak) !== null;
}

/** The blocks a capture becomes, as paragraphs. */
function paragraphsFor(blocks: string[], schema: Schema): ProseMirrorNode[] {
  return blocks.map((block) =>
    schema.nodes.paragraph.create(null, block === '' ? null : schema.text(block)),
  );
}

/**
 * Appends one capture to the end of the document, as one transaction.
 *
 * One transaction is the whole point of the shape of this function: it makes a
 * capture one `Ctrl+Z`. `closeHistory` is what stops two captures arriving
 * seconds apart from being folded into a single undo step by the history
 * plugin's own grouping, so undoing removes the last capture and only the last.
 *
 * The text becomes text nodes directly. It is never handed to a Markdown
 * parser and never parsed as HTML, so `**literal**` stays asterisks and
 * `<script>` stays eleven characters — the same thing a plain-text paste does
 * here today, which is the contract AutoPaste is held to.
 *
 * What is deliberately absent: no `setSelection`, no `scrollIntoView`, no
 * `focus`. The transaction maps the reader's selection through an insertion
 * that happens entirely after it, so the caret does not move — and the note
 * does not scroll to show something the reader is not looking at.
 *
 * Returns `false` when there was nothing to file, so no empty transaction is
 * dispatched and nothing downstream sees a change that did not happen.
 */
export function appendCapture(
  view: EditorView,
  text: string,
  delimiter: CaptureDelimiter,
): boolean {
  if (!isCapturable(text)) return false;
  const blocks = splitCapture(text);
  if (blocks.length === 0) return false;

  const { schema, doc } = view.state;
  const tr = view.state.tr;
  // A capture is its own undo step, however close together two of them land.
  closeHistory(tr);

  if (isDocumentEmpty(doc)) {
    // A note with nothing in it takes the first capture as its content: there
    // is nothing for a delimiter to stand between.
    tr.replaceWith(0, doc.content.size, Fragment.fromArray(paragraphsFor(blocks, schema)));
    view.dispatch(tr);
    return true;
  }

  if (delimiter === 'line' && canContinueLastBlock(doc, schema)) {
    // The capture continues the last paragraph on the next line, and any
    // further blocks of the same capture follow as paragraphs of their own.
    const [first, ...rest] = blocks;
    const insideLastBlock = doc.content.size - 1;
    const inline: ProseMirrorNode[] = [schema.nodes.hardBreak.create()];
    if (first !== '') inline.push(schema.text(first));
    tr.insert(insideLastBlock, Fragment.fromArray(inline));
    if (rest.length > 0) {
      tr.insert(tr.doc.content.size, Fragment.fromArray(paragraphsFor(rest, schema)));
    }
    view.dispatch(tr);
    return true;
  }

  const nodes = paragraphsFor(blocks, schema);
  if (delimiter === 'separator' && schema.nodes.horizontalRule) {
    nodes.unshift(schema.nodes.horizontalRule.create());
  }
  tr.insert(doc.content.size, Fragment.fromArray(nodes));
  view.dispatch(tr);
  return true;
}
