import { TextSelection } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import { safeLinkUrl } from '../markdown/sanitizer.ts';

/**
 * Pasting a URL over selected text makes that text the link.
 *
 * Select `site oficial`, paste `https://example.com`, and the note holds
 * `[site oficial](https://example.com)`. It is the one paste where what the
 * reader means is unmistakable — they chose the words first — and the one
 * where the ordinary behaviour, replacing the words with a URL, throws away
 * the thing they chose.
 *
 * Nothing is fetched. There is no title lookup, no favicon, no preview, no
 * OpenGraph and therefore no network, no tracking and no waiting: the
 * clipboard already holds everything this needs. A note stays something you
 * can write on a train.
 *
 * The URL is judged by [`safeLinkUrl`], which is the same allowlist the
 * autolink policy uses. A `javascript:` or `data:` clipboard is not a link
 * here; it is text, and pasting it does what pasting text does.
 */
export function handleAutoPaste(view: EditorView, event: ClipboardEvent): boolean {
  const clipboard = event.clipboardData?.getData('text/plain') ?? '';
  const href = safeLinkUrl(clipboard);
  if (!href) return false;

  const linkType = view.state.schema.marks.link;
  if (!linkType) return false;

  const { selection } = view.state;
  if (!(selection instanceof TextSelection) || selection.empty) return false;

  const { $from, $to } = selection;
  // A link is an inline mark, so it has to live inside one block. A selection
  // spanning two paragraphs is a structure, and wrapping it would be inventing
  // one.
  if (!$from.sameParent($to)) return false;

  // Code is source. A URL pasted into it is characters, not a destination.
  if ($from.parent.type.spec.code) return false;
  const codeMark = view.state.schema.marks.code;
  if (codeMark && view.state.doc.rangeHasMark(selection.from, selection.to, codeMark)) {
    return false;
  }

  // One transaction, so one `Ctrl+Z` puts the plain text back.
  view.dispatch(
    view.state.tr
      .addMark(selection.from, selection.to, linkType.create({ href }))
      .scrollIntoView(),
  );
  return true;
}
