import { visibleText } from '../markdown/visibleText.ts';

const UNTITLED_NOTE = 'Nota sem título';
const MAX_TITLE_CHARACTERS = 80;

/**
 * The note's language, and the units a title is measured and cut in.
 *
 * A JavaScript string is indexed in UTF-16 code units, which is not what
 * anybody means by "eighty characters". `'🎉'.length` is 2, so a line of emoji
 * was called twice as long as it looked and then cut with `slice`, which is
 * free to land between the two halves of a surrogate pair and hand the bar a
 * lone surrogate — a replacement glyph in the middle of a note's name.
 *
 * Grapheme clusters rather than code points, because code points do not fix
 * all of it. Text reaches Note-it decomposed as well as precomposed — search
 * folds `o` + U+0301 exactly like `ó` for that reason — and cutting between a
 * letter and its combining accent leaves the accent to land on the ellipsis.
 * A flag or a joined emoji comes apart the same way. `Intl.Segmenter` is the
 * platform's own answer and needs no dependency; it is present both in the
 * JavaScriptCore WebKitGTK 6 ships and in the test runtime.
 */
const GRAPHEMES = new Intl.Segmenter('pt-BR', { granularity: 'grapheme' });

function graphemesOf(text: string): string[] {
  return Array.from(GRAPHEMES.segment(text), (piece) => piece.segment);
}

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
 * unusually long line. Only the projected string is cut; the stored Markdown
 * is never touched.
 */
export function noteTitle(markdown: string): string {
  for (const sourceLine of visibleText(markdown).split('\n')) {
    const line = sourceLine.replace(/\s+/g, ' ').trim();
    if (line === '') continue;

    const characters = graphemesOf(line);
    if (characters.length <= MAX_TITLE_CHARACTERS) return line;
    const kept = characters.slice(0, MAX_TITLE_CHARACTERS - 1).join('').trimEnd();
    return `${kept}…`;
  }

  return UNTITLED_NOTE;
}
