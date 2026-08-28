import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import { mathErrorMessage } from '../math/errors.ts';
import { evaluateNote, MathLineResult, MathSource } from '../math/document.ts';

/**
 * One line of the document, and where its result belongs.
 *
 * `text` is `null` for a line no calculation may be read out of. `end` is the
 * document position the result attaches to — the end of the line, before the
 * break that terminates it — and is `null` for a whole block that produced no
 * line of its own.
 */
export interface ScannedLine {
  readonly text: MathSource;
  readonly end: number | null;
}

/**
 * Splits the document into lines the math engine can read.
 *
 * Only a paragraph that is a direct child of the document contributes lines.
 * Everything else — a heading, a list, a blockquote, a callout, a code block, a
 * comment — contributes one opaque line, so the block is skipped without the
 * engine losing track of the fact that something was between the paragraphs
 * around it.
 *
 * Restricting calculation to plain paragraphs is a first-version decision and
 * a deliberate one. A `- = 2 + 2` inside a task list, a `> = 2 + 2` inside a
 * quote and a fenced `= 2 + 2` all read as text that happens to start with an
 * equals sign, and half-supporting them would mean a note where the same line
 * calculates in one place and not in another for reasons nobody can see.
 *
 * A paragraph can hold several lines. A note written elsewhere with no blank
 * line between its lines arrives as one paragraph carrying newlines, and
 * Shift+Enter produces a hard break; both are line ends here.
 */
export function scanMathLines(doc: ProseMirrorNode): ScannedLine[] {
  const lines: ScannedLine[] = [];

  doc.forEach((block, offset) => {
    if (block.type.name !== 'paragraph') {
      lines.push({ text: null, end: null });
      return;
    }
    scanParagraph(block, offset, lines);
  });

  return lines;
}

function scanParagraph(
  paragraph: ProseMirrorNode,
  paragraphPos: number,
  lines: ScannedLine[],
): void {
  const contentStart = paragraphPos + 1;
  let text = '';
  let readable = true;
  let position = contentStart;

  const flush = (end: number): void => {
    lines.push({ text: readable ? text : null, end });
    text = '';
    readable = true;
  };

  paragraph.forEach((child, offset) => {
    const childStart = contentStart + offset;

    if (child.isText) {
      const value = child.text ?? '';
      // An inline code span is source, not arithmetic: `` `= 2 + 2` `` shows
      // the expression, it does not run it. One such span anywhere on the line
      // takes the whole line out.
      const isCode = child.marks.some((mark) => mark.type.name === 'code');
      let index = 0;
      for (;;) {
        const brk = value.indexOf('\n', index);
        const stop = brk === -1 ? value.length : brk;
        if (stop > index) {
          text += value.slice(index, stop);
          if (isCode) readable = false;
        }
        position = childStart + stop;
        if (brk === -1) break;
        flush(position);
        index = brk + 1;
        position = childStart + index;
      }
      return;
    }

    if (child.type.name === 'hardBreak') {
      flush(childStart);
      position = childStart + child.nodeSize;
      return;
    }

    // Anything else inline is not text the engine can read.
    readable = false;
    position = childStart + child.nodeSize;
  });

  flush(position);
}

/**
 * The element drawn beside a line.
 *
 * Built with `textContent` from either a formatted number or one of four
 * constant messages, so nothing from the note is ever interpreted as markup on
 * its way back to the screen. It is `contenteditable="false"` and unselectable,
 * so it is not part of the text, not part of a selection and not part of what
 * gets copied.
 */
export function renderResult(result: MathLineResult): HTMLElement {
  const element = document.createElement('span');
  element.className = 'note-math-result';
  element.setAttribute('contenteditable', 'false');

  if (result.kind === 'error') {
    element.setAttribute('data-note-it-math', 'error');
    element.textContent = mathErrorMessage(result.code);
  } else if (result.kind === 'value') {
    element.setAttribute('data-note-it-math', 'value');
    element.textContent = result.text;
  }

  return element;
}

/**
 * Every result in the document, as decorations.
 *
 * Decorations and not content, which is the decision the whole phase rests on.
 * A result written into the document would be serialized into the `.md`, so
 * the file would gain numbers nobody typed, `updated_at` would move because
 * something was recalculated, opening a note would be an edit, and a stale
 * result would be saved the moment a note was opened in another editor. As a
 * decoration it exists only in this window: the note on disk stays the note
 * that was written, and reopening it recomputes everything from the text.
 */
export function mathDecorations(doc: ProseMirrorNode): DecorationSet {
  const lines = scanMathLines(doc);
  const results = evaluateNote(lines.map((line) => line.text));
  const decorations: Decoration[] = [];

  results.forEach((result, index) => {
    if (result.kind === 'none') return;
    const end = lines[index].end;
    if (end === null) return;

    decorations.push(
      Decoration.widget(end, () => renderResult(result), {
        // Typed characters go before the result, so it stays at the end of the
        // line the reader is writing.
        side: 1,
        // The result is not a place the cursor can be.
        ignoreSelection: true,
        key: `${result.kind}:${result.kind === 'error' ? result.code : result.text}`,
      }),
    );
  });

  return DecorationSet.create(doc, decorations);
}

export const mathPluginKey = new PluginKey<DecorationSet>('noteItMath');

/**
 * Contextual calculation, recomputed whenever the document changes.
 *
 * The whole note is evaluated on each change rather than incrementally. It is
 * a plain scan and a small parser over a document that is one window's worth
 * of text; measured on a note far larger than a note-it note, it is a fraction
 * of a millisecond, which is less than the incremental bookkeeping it would
 * take to avoid it. Reactivity falls out of this for free: change a variable
 * and every line below it is recomputed in the same pass, with no dependency
 * tracking to go stale.
 *
 * The plugin never dispatches a transaction. It reads the document and paints
 * over it, so it adds no undo step, moves no cursor, and never marks the note
 * as edited.
 */
export const NoteItMath = Extension.create({
  name: 'noteItMath',

  addProseMirrorPlugins() {
    return [
      new Plugin<DecorationSet>({
        key: mathPluginKey,
        state: {
          init: (_config, state) => mathDecorations(state.doc),
          apply: (transaction, current, _oldState, newState) =>
            transaction.docChanged ? mathDecorations(newState.doc) : current,
        },
        props: {
          decorations(state) {
            return mathPluginKey.getState(state);
          },
        },
      }),
    ];
  },
});
