/**
 * Projecting a stored note onto the text a reader actually sees.
 *
 * A note is stored as Markdown, and Markdown says two things at once: the
 * words, and how they are dressed. `# ` names a heading, `**` names emphasis,
 * and `<span data-note-it-color="#64748B">` names a colour Note-it applied.
 * Inside the note that second half is invisible — the editor renders it — but
 * the collapsed title is not the editor. It was showing the file, and the file
 * spells a coloured phrase `<span data-note-it-color="#64748B"
 * style="color:#64748B">teste de verdade</span>`, which is not something
 * anybody wrote.
 *
 * So this module answers one question: **given what is stored, what does the
 * reader see?** Nothing here writes: the Markdown remains the source of truth,
 * is never rewritten to make a label look better, and no result from here is
 * ever sent back to the host.
 *
 * It is not a Markdown renderer and it is not a pattern that happens to fit the
 * examples that were reported. It is a scanner over the forms Note-it's own
 * serializer produces — enumerated by round-tripping documents through the real
 * editor — and it reads a foreign `.md` under the same rules, declining
 * anything it cannot recognise rather than guessing. A delimiter with no
 * partner stays the character it is, which is why a note saying
 * `contém .* mesmo assim` still says that.
 *
 * The host carries the same projection in `src/visible_text.rs`, for the search
 * palette and the trash. The two are kept in step by testing both against the
 * same cases; they cannot be one file because a search must read a thousand
 * notes with no WebView at all.
 */

/**
 * Callout kinds Note-it writes as a `[!KIND]` marker on a line of its own.
 *
 * A whitelist deliberately: a marker this version does not know is not a
 * marker, it is the text somebody typed, and it stays visible — the same
 * failure mode the editor itself has.
 */
const CALLOUT_KINDS = ['NOTE', 'TIP', 'IMPORTANT', 'WARNING', 'CAUTION'];

/**
 * Note-it's own task metadata, which is machine bookkeeping rather than text.
 * It travels in an HTML comment on the task's line so the file stays ordinary
 * Markdown, and the reader never sees it.
 */
const TASK_METADATA_PREFIX = 'note-it:';

/** The stored note as the reader sees it: one visible line per stored line. */
export function visibleText(stored: string): string {
  const lines = stored.split('\n').map((line) => (line.endsWith('\r') ? line.slice(0, -1) : line));
  const out: string[] = [];
  let index = 0;
  let fence: { character: string; width: number } | null = null;

  while (index < lines.length) {
    const line = lines[index];
    index += 1;
    const trimmed = line.trim();

    // Inside a fence every line is the code somebody typed. It is shown
    // exactly as written, so it is projected exactly as written.
    if (fence) {
      const marker = fenceMarker(trimmed);
      if (marker && marker.character === fence.character && marker.width >= fence.width) {
        fence = null;
      } else {
        out.push(line.replace(/\s+$/, ''));
      }
      continue;
    }

    const marker = fenceMarker(trimmed);
    if (marker) {
      // The fence and its info string are how a code block is written down,
      // not part of it.
      fence = marker;
      continue;
    }
    if (trimmed === '' || isThematicBreak(trimmed)) {
      out.push('');
      continue;
    }
    if (trimmed.startsWith('<!--')) {
      const comment = readBlockComment(lines, index - 1);
      index = comment.resumed;
      out.push(comment.body ?? '');
      continue;
    }

    out.push(renderInline(stripBlockMarkers(trimmed)).trim());
  }

  return out.join('\n').replace(/\n+$/, '');
}

function fenceMarker(trimmed: string): { character: string; width: number } | null {
  for (const character of ['`', '~']) {
    const width = runLength(trimmed, 0, character);
    if (width >= 3) return { character, width };
  }
  return null;
}

/** A horizontal rule: a line that draws something and says nothing. */
function isThematicBreak(trimmed: string): boolean {
  const dense = trimmed.replace(/\s+/g, '');
  return (
    dense.length >= 3 &&
    ['-', '*', '_'].some((character) => [...dense].every((one) => one === character))
  );
}

/**
 * Reads a whole-block Note-it comment starting at `start`.
 *
 * A comment is stored but it is not hidden: the editor shows it as a small
 * labelled block, so its words are words the reader sees. What goes is the
 * `<!--` and `-->` around them — and the task metadata comment in its
 * entirety, because that one is never shown at all.
 */
function readBlockComment(
  lines: string[],
  start: number,
): { body: string | null; resumed: number } {
  const collected: string[] = [];
  let index = start;

  while (index < lines.length) {
    const line = lines[index];
    index += 1;
    const end = line.indexOf('-->');
    if (end === -1) {
      collected.push(line);
      continue;
    }
    collected.push(line.slice(0, end));
    const body = collected
      .join('\n')
      .replace(/^\s*<!--/, '')
      .trim()
      .replace(/--&gt;/g, '-->');
    return { body: body.startsWith(TASK_METADATA_PREFIX) ? null : body, resumed: index };
  }

  // A comment nobody closed. The file is still the file; what it holds is
  // shown rather than swallowed, minus the opener.
  const body = collected
    .join('\n')
    .replace(/^\s*<!--/, '')
    .trim()
    .replace(/--&gt;/g, '-->');
  return { body, resumed: index };
}

/**
 * Removes the markers that name a line's *kind* rather than saying anything:
 * quote and callout, heading, list and task box.
 */
function stripBlockMarkers(line: string): string {
  let text = line;

  for (;;) {
    const stripped = text.replace(/^\s+/, '');
    if (!stripped.startsWith('>')) {
      text = stripped;
      break;
    }
    text = stripped.slice(1);
  }

  text = stripCalloutMarker(text) ?? text;
  const heading = stripHeadingMarker(text);
  if (heading !== null) return heading.trim();

  const list = stripListMarker(text);
  if (list !== null) text = stripTaskMarker(list) ?? list;

  return text.trim();
}

/**
 * `[!WARNING]` alone on its line. The kind is decoration Note-it draws as a
 * coloured label; putting the identifier in a title would be showing the reader
 * a word they never typed.
 */
function stripCalloutMarker(text: string): string | null {
  const match = /^\s*\[!([A-Za-z]+)\]\s*$/.exec(text);
  if (!match || !CALLOUT_KINDS.includes(match[1].toUpperCase())) return null;
  return '';
}

/** `#` through `######`, and only when a space follows: `#tag` is a word. */
function stripHeadingMarker(text: string): string | null {
  const hashes = runLength(text, 0, '#');
  if (hashes < 1 || hashes > 6) return null;
  const rest = text.slice(hashes);
  if (rest === '') return rest;
  return /^[ \t]/.test(rest) ? rest.slice(1) : null;
}

/**
 * A bullet or an ordered marker, each of which must be followed by a space to
 * be a marker at all — which is what keeps `*grifado*` from being a list.
 */
function stripListMarker(text: string): string | null {
  const match = /^(?:[-*+]|\d{1,9}[.)])(?:[ \t]+|$)/.exec(text);
  return match ? text.slice(match[0].length) : null;
}

/** `[ ]` or `[x]`: the box a task is drawn with, never words. */
function stripTaskMarker(text: string): string | null {
  const match = /^\[[ xX]\](?:[ \t]+|$)/.exec(text);
  return match ? text.slice(match[0].length) : null;
}

/** Everything inside a line: emphasis, code, links, HTML and entities. */
function renderInline(source: string): string {
  let out = '';
  let index = 0;

  while (index < source.length) {
    const rest = source.slice(index);
    const character = source[index];

    // A backslash escape is how the serializer stores a character that would
    // otherwise be a mark. What the reader sees is the character.
    if (character === '\\') {
      const next = source[index + 1];
      if (next !== undefined && isAsciiPunctuation(next)) {
        out += next;
        index += 2;
      } else {
        out += '\\';
        index += 1;
      }
      continue;
    }

    if (character === '`') {
      const width = runLength(source, index, '`');
      const close = findCodeSpanClose(rest.slice(width), width);
      if (close === null) {
        out += rest.slice(0, width);
        index += width;
      } else {
        // Code is source: it is shown exactly as typed, so nothing inside it
        // is unwrapped, decoded or matched.
        out += rest.slice(width, width + close);
        index += width + close + width;
      }
      continue;
    }

    if (character === '<') {
      const comment = inlineCommentWidth(rest);
      if (comment !== null) {
        index += comment;
        continue;
      }
      const tag = htmlTagWidth(rest);
      if (tag !== null) {
        // A raw tag in a stored note is always Note-it's own serialization: a
        // `<` the reader typed is stored as `&lt;` and arrives here as text.
        index += tag;
        continue;
      }
      out += '<';
      index += 1;
      continue;
    }

    if (character === '&') {
      const entity = parseEntity(rest);
      if (entity) {
        out += entity.decoded;
        index += entity.width;
      } else {
        out += '&';
        index += 1;
      }
      continue;
    }

    // An image shows its alternative text and nothing else.
    if (character === '!' && source[index + 1] === '[') {
      const link = parseLink(rest.slice(1));
      if (link) {
        out += renderInline(link.text);
        index += 1 + link.width;
      } else {
        out += '!';
        index += 1;
      }
      continue;
    }

    // A link shows its words. The destination is not on screen — the editor's
    // own find cannot reach it either.
    if (character === '[') {
      const link = parseLink(rest);
      if (link) {
        out += renderInline(link.text);
        index += link.width;
      } else {
        out += '[';
        index += 1;
      }
      continue;
    }

    if (character === '*' || character === '_' || character === '~') {
      const width = runLength(source, index, character);
      const close = emphasisSpan(source, index, character, width);
      if (close === null) {
        out += rest.slice(0, width);
        index += width;
      } else {
        out += renderInline(rest.slice(width, width + close));
        index += width + close + width;
      }
      continue;
    }

    out += character;
    index += 1;
  }

  return out;
}

/**
 * Where the emphasis opened at `start` closes, or `null` when it never does.
 *
 * A run only opens if something follows it immediately — `a ** b` is two
 * asterisks somebody typed — and only closes if something precedes it. `_`
 * additionally may not open or close inside a word, which is what leaves
 * `note_it_config` alone.
 */
function emphasisSpan(
  source: string,
  start: number,
  delimiter: string,
  width: number,
): number | null {
  const allowed = delimiter === '~' ? width === 2 : width >= 1 && width <= 3;
  if (!allowed) return null;

  const inner = source.slice(start + width);
  if (inner === '' || /^\s/.test(inner)) return null;
  if (delimiter === '_' && isAlphanumeric(source[start - 1])) return null;

  return findEmphasisClose(inner, delimiter, width);
}

function findEmphasisClose(haystack: string, delimiter: string, width: number): number | null {
  let index = 0;

  while (index < haystack.length) {
    const character = haystack[index];

    if (character === '\\') {
      index += 2;
      continue;
    }
    if (character === '`') {
      const ticks = runLength(haystack, index, '`');
      const close = findCodeSpanClose(haystack.slice(index + ticks), ticks);
      index += close === null ? ticks : ticks + close + ticks;
      continue;
    }
    if (character === delimiter) {
      const run = runLength(haystack, index, delimiter);
      const closes =
        run === width &&
        index > 0 &&
        !/\s/.test(haystack[index - 1]) &&
        (delimiter !== '_' || !isAlphanumeric(haystack[index + run]));
      if (closes) return index;
      index += run;
      continue;
    }
    index += 1;
  }

  return null;
}

/**
 * Where a code span opened with `width` backticks closes, counted from just
 * after the opener. A run of a different length is part of the code.
 */
function findCodeSpanClose(haystack: string, width: number): number | null {
  let index = 0;

  while (index < haystack.length) {
    if (haystack[index] === '`') {
      const run = runLength(haystack, index, '`');
      if (run === width) return index;
      index += run;
      continue;
    }
    index += 1;
  }

  return null;
}

/**
 * The width of `<!-- ... -->` appearing inside a line, which in a stored note
 * is Note-it's task metadata and is never on screen.
 */
function inlineCommentWidth(rest: string): number | null {
  if (!rest.startsWith('<!--')) return null;
  const end = rest.indexOf('-->');
  return end === -1 ? null : end + 3;
}

/**
 * The width of an HTML tag at the start of `rest`, or `null` if this `<` opens
 * no tag — `<https://exemplo.com>` and a bare `<` among words included.
 */
function htmlTagWidth(rest: string): number | null {
  const match = /^<\/?[A-Za-z][A-Za-z0-9-]*/.exec(rest);
  if (!match) return null;
  let index = match[0].length;

  const after = rest[index];
  if (after === '>') return index + 1;
  if (rest.startsWith('/>', index)) return index + 2;
  if (after === undefined || !/\s/.test(after)) return null;

  let quote: string | null = null;
  for (; index < rest.length; index += 1) {
    const character = rest[index];
    if (quote !== null) {
      if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") quote = character;
    else if (character === '>') return index + 1;
  }
  return null;
}

/**
 * The character an HTML entity stands for, and how much of the source it took.
 * Only the entities Note-it's serializer writes, plus numeric references. An
 * `&` that begins nothing recognisable is an ampersand somebody typed.
 */
function parseEntity(rest: string): { decoded: string; width: number } | null {
  const named: Record<string, string> = {
    '&amp;': '&',
    '&lt;': '<',
    '&gt;': '>',
    '&quot;': '"',
    '&apos;': "'",
    '&nbsp;': ' ',
  };
  for (const [entity, decoded] of Object.entries(named)) {
    if (rest.startsWith(entity)) return { decoded, width: entity.length };
  }

  const match = /^&#(x|X)?([0-9A-Fa-f]{1,8});/.exec(rest);
  if (!match) return null;
  const code = Number.parseInt(match[2], match[1] ? 16 : 10);
  if (!Number.isFinite(code) || code < 0 || code > 0x10ffff) return null;
  return { decoded: String.fromCodePoint(code), width: match[0].length };
}

/**
 * A `[text](destination)` or `[text][reference]` starting at `rest`, as its
 * visible text and the width it occupies.
 */
function parseLink(rest: string): { text: string; width: number } | null {
  if (!rest.startsWith('[')) return null;

  let depth = 0;
  let index = 0;
  let labelEnd = -1;
  while (index < rest.length) {
    const character = rest[index];
    if (character === '\\') {
      index += 2;
      continue;
    }
    if (character === '[') depth += 1;
    else if (character === ']') {
      depth -= 1;
      if (depth === 0) {
        labelEnd = index;
        break;
      }
    }
    index += 1;
  }
  if (labelEnd === -1) return null;

  const after = rest.slice(labelEnd + 1);
  // A pair of brackets with nothing addressed is a pair of brackets.
  const closing = after.startsWith('(') ? ')' : after.startsWith('[') ? ']' : null;
  if (closing === null) return null;
  const end = after.indexOf(closing);
  if (end === -1) return null;

  return { text: rest.slice(1, labelEnd), width: labelEnd + 1 + end + 1 };
}

function runLength(text: string, from: number, character: string): number {
  let width = 0;
  while (text[from + width] === character) width += 1;
  return width;
}

function isAsciiPunctuation(character: string): boolean {
  return /^[!-/:-@[-`{-~]$/.test(character);
}

function isAlphanumeric(character: string | undefined): boolean {
  return character !== undefined && /[\p{L}\p{N}]/u.test(character);
}
