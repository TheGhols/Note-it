import { Editor } from '@tiptap/core';
import { editorExtensions } from '../editor/extensions.ts';
import { extractFlashcards } from '../flashcards/extract.ts';
import { sanitizeMarkdown } from '../markdown/sanitizer.ts';
import { noteTitle } from '../ui/noteTitle.ts';
import { identifyReviews } from './identity.ts';
import type { GlobalCatalog, StudyCatalogNote, StudyState } from './types.ts';

/**
 * Parses every note on demand with one reusable Tiptap editor and the exact
 * extension/schema stack the visible note uses. No regex and no background
 * index. The current note's unsaved Markdown replaces the host copy first.
 */
export async function buildGlobalCatalog(
  notes: readonly StudyCatalogNote[],
  study: StudyState,
  current?: { id: string; content: string } | null,
  document: Document = window.document,
): Promise<GlobalCatalog> {
  const element = document.createElement('div');
  const editor = new Editor({
    element,
    extensions: editorExtensions,
    content: '',
    contentType: 'markdown',
    autofocus: false,
  });

  const items = [];
  let sourceCards = 0;
  let notesWithCards = 0;
  let order = 0;
  try {
    for (const note of notes) {
      const markdown = current?.id === note.id ? current.content : note.content;
      editor.commands.setContent(sanitizeMarkdown(markdown), {
        contentType: 'markdown',
        emitUpdate: false,
      });
      const sources = extractFlashcards(editor.state.doc);
      if (sources.length === 0) continue;
      sourceCards += sources.length;
      notesWithCards += 1;
      const identified = await identifyReviews(
        note.id,
        noteTitle(markdown),
        sources,
        study.cards,
        order,
      );
      items.push(...identified);
      order += identified.length;
    }
    return { items, sourceCards, notesWithCards, schema: editor.schema };
  } finally {
    editor.destroy();
  }
}
