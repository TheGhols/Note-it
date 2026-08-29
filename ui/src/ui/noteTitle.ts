import { visibleText } from '../markdown/visibleText.ts';

const UNTITLED_NOTE = 'Nota sem título';
const MAX_TITLE_CHARACTERS = 80;

/**
 * A short, presentation-only name for a collapsed note.
 *
 * The Markdown remains the source of truth and nothing here is ever sent back
 * to the host. What the bar shows is the note *as the reader sees it*, which is
 * the projection in `markdown/visibleText.ts`: a coloured phrase reads as the
 * phrase, a heading as its words, a marked passage as what was marked. The
 * storage — the `<span data-note-it-color=…>` around them — was never anything
 * anybody wrote and has no business in a title.
 *
 * Everything left to do here is presentation of the projected text: take the
 * first line that says something, collapse display whitespace, and cap an
 * unusually long line.
 */
export function noteTitle(markdown: string): string {
  for (const sourceLine of visibleText(markdown).split('\n')) {
    const line = sourceLine.replace(/\s+/g, ' ').trim();
    if (line === '') continue;
    if (line.length <= MAX_TITLE_CHARACTERS) return line;
    return `${line.slice(0, MAX_TITLE_CHARACTERS - 1).trimEnd()}…`;
  }

  return UNTITLED_NOTE;
}
