import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import type { EditorState } from '@tiptap/pm/state';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import {
  countFlashcards,
  extractFlashcards,
  FlashcardCounts,
  FlashcardSource,
} from '../flashcards/extract.ts';

/**
 * The cards a note holds, as editor state that paints and never writes.
 *
 * Exactly what Find is, and for the same reason. A recognised `::` is marked
 * with a ProseMirror decoration, which is drawn over the document rather than
 * being part of it: marking a card creates no transaction, adds no undo step,
 * changes not one byte of Markdown and moves no modification date. The reader
 * keeps seeing the two characters they typed, in a document they can still
 * edit character by character — nothing is folded away behind a widget, and
 * nothing is rewritten into a tag the file did not have.
 *
 * Recomputed when the document changes and at no other time, so a card
 * appears the moment its delimiter is finished and disappears the moment it is
 * taken out, with no save, no reopen and no refresh in between.
 */

interface FlashcardPluginState {
  readonly sources: FlashcardSource[];
  /** The document the positions above were measured in. */
  readonly doc: ProseMirrorNode;
}

export const flashcardPluginKey = new PluginKey<FlashcardPluginState>('noteItFlashcards');

function stateFor(doc: ProseMirrorNode): FlashcardPluginState {
  return { sources: extractFlashcards(doc), doc };
}

/**
 * What is painted: the delimiter, and the line it sits on.
 *
 * Two hints, both faint, and neither of them hiding anything. The delimiter is
 * tinted so it reads as the marker it is, and the block carrying it gets a
 * quiet rule down its left edge so a card is recognisable while scrolling
 * past. A reader who has never heard of flashcards sees a slightly coloured
 * pair of colons in text they typed, which is the most this may cost them.
 */
function decorationsFor(state: FlashcardPluginState): DecorationSet | null {
  if (state.sources.length === 0) return null;

  const decorations: Decoration[] = [];
  for (const source of state.sources) {
    decorations.push(
      Decoration.inline(source.delimiter.from, source.delimiter.to, {
        class: 'note-flashcard-mark',
      }),
    );
    const block = state.doc.resolve(source.delimiter.from).before();
    const node = state.doc.nodeAt(block);
    if (node) {
      decorations.push(
        Decoration.node(block, block + node.nodeSize, { class: 'note-flashcard-line' }),
      );
    }
  }
  return DecorationSet.create(state.doc, decorations);
}

export const NoteItFlashcards = Extension.create({
  name: 'noteItFlashcards',

  addProseMirrorPlugins() {
    return [
      new Plugin<FlashcardPluginState>({
        key: flashcardPluginKey,
        state: {
          init: (_config, state) => stateFor(state.doc),
          apply(transaction, current, _old, newState) {
            if (!transaction.docChanged) return current;
            return stateFor(newState.doc);
          },
        },
        props: {
          decorations(state) {
            const plugin = flashcardPluginKey.getState(state);
            return plugin ? decorationsFor(plugin) : null;
          },
        },
      }),
    ];
  },
});

/** The cards in the document as it stands, in the order they are written. */
export function flashcardsIn(state: EditorState): FlashcardSource[] {
  return flashcardPluginKey.getState(state)?.sources ?? extractFlashcards(state.doc);
}

/** How many cards there are, and how many questions they come to. */
export function flashcardCountsIn(state: EditorState): FlashcardCounts {
  return countFlashcards(flashcardsIn(state));
}
