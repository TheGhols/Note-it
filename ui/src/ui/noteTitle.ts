const UNTITLED_NOTE = 'Nota sem título';
const MAX_TITLE_CHARACTERS = 80;

/**
 * A short, presentation-only name for a collapsed note.
 *
 * The Markdown remains the source of truth. We only unwrap the superficial
 * marks that can begin a textual line, collapse display whitespace and cap an
 * unusually long line; no result from here is ever sent back to the host.
 */
export function noteTitle(markdown: string): string {
  for (const sourceLine of markdown.split(/\r?\n/)) {
    let line = sourceLine.trim();
    if (line === '') continue;

    line = line
      .replace(/^#{1,6}\s+/, '')
      .replace(/^>\s*/, '')
      .replace(/^(?:[-+*]|\d+[.)])\s+/, '')
      .replace(/^\[[ xX]\]\s+/, '')
      .replace(/\s+/g, ' ')
      .trim();

    if (line === '' || /^(```|~~~|---+|___+|\*\*\*+)$/.test(line)) continue;
    if (line.length <= MAX_TITLE_CHARACTERS) return line;
    return `${line.slice(0, MAX_TITLE_CHARACTERS - 1).trimEnd()}…`;
  }

  return UNTITLED_NOTE;
}
