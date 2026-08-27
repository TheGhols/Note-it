/**
 * Note-it metadata carried alongside a Markdown task item.
 *
 * Standard Markdown has no syntax for when a task was completed, so the
 * timestamp travels in an HTML comment appended to the task's own line:
 *
 *     - [x] Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->
 *
 * The main line stays plain `- [x] …`, so the note remains readable in any
 * other Markdown tool, and the comment moves with the task when tasks are
 * reordered — the metadata is never associated by position.
 */

/** ISO 8601 with an explicit offset or `Z`; no bare local timestamps. */
const ISO_8601_WITH_ZONE =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/;

/** Matches the metadata comment anywhere in a task line. */
export const COMPLETED_AT_COMMENT_REGEX =
  /<!--\s*note-it:completed_at=([^\s]+?)\s*-->/;

export function isValidCompletedAt(value: unknown): value is string {
  if (typeof value !== 'string' || !ISO_8601_WITH_ZONE.test(value)) return false;
  return !Number.isNaN(new Date(value).getTime());
}

/** Renders the metadata comment, or an empty string for an unknown date. */
export function renderCompletedAtComment(value: unknown): string {
  return isValidCompletedAt(value) ? `<!-- note-it:completed_at=${value} -->` : '';
}

/**
 * Extracts the completion timestamp from a task line and returns the line
 * without it. An invalid or absent timestamp yields `null` — a task whose date
 * is unknown stays unknown rather than being given an invented one.
 */
export function extractCompletedAt(text: string): {
  completedAt: string | null;
  text: string;
} {
  const match = COMPLETED_AT_COMMENT_REGEX.exec(text);
  if (!match) return { completedAt: null, text };

  const candidate = match[1];
  const stripped = text.replace(COMPLETED_AT_COMMENT_REGEX, '').replace(/[ \t]+$/, '');
  return {
    completedAt: isValidCompletedAt(candidate) ? candidate : null,
    text: stripped,
  };
}

/** True when the raw HTML is exactly a well-formed Note-it task comment. */
export function isCompletedAtComment(rawHtml: string): boolean {
  const match = /^<!--\s*note-it:completed_at=([^\s]+?)\s*-->$/.exec(rawHtml.trim());
  return Boolean(match && isValidCompletedAt(match[1]));
}
