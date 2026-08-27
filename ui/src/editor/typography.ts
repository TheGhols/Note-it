import { InputRule } from '@tiptap/core';
import { Extension } from '@tiptap/core';
import type { EditorState } from '@tiptap/pm/state';

/** Fires as soon as the `>` completes a `->` sequence. */
export const ARROW_INPUT_REGEX = /->$/;

export const ARROW_CHARACTER = '→';

/**
 * Whether the arrow substitution may run at `from`.
 *
 * Code is left exactly as typed: `->` inside an inline code span or a code
 * block is source, not prose. A preceding dash is also left alone so an
 * `-->` sequence is not turned into `-→`.
 */
export function canSubstituteArrow(state: EditorState, from: number): boolean {
  const $from = state.doc.resolve(from);

  // Code block, or any other node declaring itself as code.
  if ($from.parent.type.spec.code) return false;

  const codeMark = state.schema.marks.code;
  if (codeMark) {
    const active = codeMark.isInSet(state.storedMarks ?? $from.marks());
    if (active) return false;
    if (state.doc.rangeHasMark(Math.max(0, from - 1), from + 2, codeMark)) return false;
  }

  // The character preceding the matched `->`.
  if ($from.parent.textBetween(Math.max(0, $from.parentOffset - 1), $from.parentOffset) === '-') {
    return false;
  }

  return true;
}

/**
 * Replaces a typed `->` with a real arrow character.
 *
 * Done as an editor input rule rather than by switching to a font with
 * ligatures: the note stores the actual `→`, so it survives in any editor and
 * does not depend on which font renders it. One transaction, so a single undo
 * puts the two characters back.
 */
export const NoteItTypography = Extension.create({
  name: 'noteItTypography',

  addInputRules() {
    return [
      new InputRule({
        find: ARROW_INPUT_REGEX,
        handler: ({ state, range, chain }) => {
          // Returning null tells Tiptap the rule declined; returning nothing
          // after producing steps lets the substitution through.
          if (!canSubstituteArrow(state, range.from)) return null;
          chain().insertContentAt(range, ARROW_CHARACTER).run();
          return undefined;
        },
      }),
    ];
  },
});
