/**
 * Sanitization for the small raw-HTML subset supported inside Note-it Markdown.
 * Normal Markdown is kept as Markdown; only HTML fragments are inspected.
 */

import { isCompletedAtComment } from './taskMeta.ts';
import { normalizeTextSize } from '../editor/textSize.ts';

export const HEX_COLOR_REGEX = /^#(?:[0-9A-Fa-f]{3}|[0-9A-Fa-f]{6})$/;

export function isValidHexColor(value: unknown): value is string {
  return typeof value === 'string' && HEX_COLOR_REGEX.test(value);
}

export const ALLOWED_AUTOLINK_SCHEMES: ReadonlySet<string> = new Set([
  'https',
  'http',
  'mailto',
]);

type CustomTagAction =
  | { kind: 'open'; tag: 'u' | 'span' | 'mark'; canonical: string }
  | { kind: 'close'; tag: 'u' | 'span' | 'mark'; canonical: string };

function parseCustomTag(rawTag: string): CustomTagAction | null {
  if (/^<u\s*>$/i.test(rawTag)) return { kind: 'open', tag: 'u', canonical: '<u>' };
  if (/^<\/u\s*>$/i.test(rawTag)) return { kind: 'close', tag: 'u', canonical: '</u>' };
  if (/^<\/span\s*>$/i.test(rawTag)) return { kind: 'close', tag: 'span', canonical: '</span>' };
  if (/^<\/mark\s*>$/i.test(rawTag)) return { kind: 'close', tag: 'mark', canonical: '</mark>' };

  const parser = new DOMParser();
  const parsed = parser.parseFromString(`${rawTag}</span></mark>`, 'text/html');
  const element = parsed.body.firstElementChild as HTMLElement | null;
  if (!element) return null;

  const tagName = element.tagName.toLowerCase();
  if (tagName === 'span') {
    // A span is kept only for the controlled Note-it attributes, and only with
    // values from the corresponding whitelist.
    const attributes: string[] = [];
    const color = element.getAttribute('data-note-it-color');
    if (isValidHexColor(color)) {
      attributes.push(`data-note-it-color="${color}"`);
    }
    const fontSize = normalizeTextSize(element.getAttribute('data-note-it-font-size'));
    if (fontSize !== null) {
      attributes.push(`data-note-it-font-size="${fontSize}"`);
    }
    if (attributes.length === 0) return null;
    return { kind: 'open', tag: 'span', canonical: `<span ${attributes.join(' ')}>` };
  }
  if (tagName === 'mark') {
    const color = element.getAttribute('data-note-it-highlight');
    if (isValidHexColor(color)) {
      return { kind: 'open', tag: 'mark', canonical: `<mark data-note-it-highlight="${color}">` };
    }
    return null;
  }
  return null;
}

function parseOpeningCodeFence(
  line: string,
): { char: '`' | '~'; length: number } | null {
  const match = /^[ ]{0,3}((`{3,})([^`\r\n]*)|(~{3,})([^\r\n]*))$/.exec(line);
  if (!match) return null;
  if (match[2]) {
    return { char: '`', length: match[2].length };
  }
  if (match[4]) {
    return { char: '~', length: match[4].length };
  }
  return null;
}

function isClosingCodeFence(
  line: string,
  fenceChar: '`' | '~',
  fenceLength: number,
): boolean {
  if (fenceChar === '`') {
    const match = /^[ ]{0,3}(`+)[ \t]*$/.exec(line);
    return Boolean(match && match[1].length >= fenceLength);
  } else {
    const match = /^[ ]{0,3}(~+)[ \t]*$/.exec(line);
    return Boolean(match && match[1].length >= fenceLength);
  }
}

/**
 * Sanitize raw HTML embedded in Markdown in a Markdown-aware manner.
 * Fenced code blocks, inline code spans, and Markdown autolinks are preserved
 * literally as code/syntax without corruption.
 * Only HTML tags outside literal code regions are inspected and sanitized.
 */
export function sanitizeMarkdown(markdown: string): string {
  if (!markdown) return '';

  let output = '';
  const openTagStack: Array<'u' | 'span' | 'mark'> = [];
  let i = 0;
  const len = markdown.length;

  while (i < len) {
    const isStartOfLine = i === 0 || markdown[i - 1] === '\n';

    // 1. Fenced code block at start of line
    if (isStartOfLine) {
      let lineEnd = markdown.indexOf('\n', i);
      if (lineEnd === -1) lineEnd = len;
      const line = markdown.slice(i, lineEnd);
      const openingFence = parseOpeningCodeFence(line);

      if (openingFence) {
        const lineWithBreak = markdown.slice(i, lineEnd === len ? len : lineEnd + 1);
        output += lineWithBreak;
        i = lineEnd === len ? len : lineEnd + 1;

        while (i < len) {
          let codeLineEnd = markdown.indexOf('\n', i);
          if (codeLineEnd === -1) codeLineEnd = len;
          const codeLine = markdown.slice(i, codeLineEnd);
          const isClosing = isClosingCodeFence(codeLine, openingFence.char, openingFence.length);
          const chunk = markdown.slice(i, codeLineEnd === len ? len : codeLineEnd + 1);
          output += chunk;
          i = codeLineEnd === len ? len : codeLineEnd + 1;
          if (isClosing) {
            break;
          }
        }
        continue;
      }
    }

    const char = markdown[i];

    // 2. Inline code span (runs of backticks)
    if (char === '`') {
      let backtickCount = 0;
      while (i + backtickCount < len && markdown[i + backtickCount] === '`') {
        backtickCount++;
      }

      let closeIndex = -1;
      let scan = i + backtickCount;
      while (scan < len) {
        if (markdown[scan] === '`') {
          let runLen = 0;
          while (scan + runLen < len && markdown[scan + runLen] === '`') {
            runLen++;
          }
          if (runLen === backtickCount) {
            closeIndex = scan;
            break;
          }
          scan += runLen;
        } else {
          scan++;
        }
      }

      if (closeIndex !== -1) {
        const inlineCodeSpan = markdown.slice(i, closeIndex + backtickCount);
        output += inlineCodeSpan;
        i = closeIndex + backtickCount;
        continue;
      } else {
        output += markdown.slice(i, i + backtickCount);
        i += backtickCount;
        continue;
      }
    }

    // 3. HTML comments, dangerous tags, autolinks, and custom tags outside code
    if (char === '<') {
      // 3a. HTML Comment. Note-it's own task metadata is the single form kept;
      // every other comment is still dropped.
      if (markdown.startsWith('<!--', i)) {
        const commentEnd = markdown.indexOf('-->', i + 4);
        if (commentEnd !== -1) {
          const rawComment = markdown.slice(i, commentEnd + 3);
          if (isCompletedAtComment(rawComment)) {
            output += rawComment;
          }
          i = commentEnd + 3;
        } else {
          i = len;
        }
        continue;
      }

      // 3b. Dangerous HTML block elements
      const dangerousBlockMatch = /^<(script|iframe|object|embed|style|form)\b[^>]*>/i.exec(
        markdown.slice(i),
      );
      if (dangerousBlockMatch) {
        const tagName = dangerousBlockMatch[1].toLowerCase();
        const closeRegex = new RegExp(`</${tagName}\\s*>`, 'i');
        const rest = markdown.slice(i + dangerousBlockMatch[0].length);
        const closeMatch = closeRegex.exec(rest);
        if (closeMatch) {
          i = i + dangerousBlockMatch[0].length + closeMatch.index + closeMatch[0].length;
        } else {
          i = i + dangerousBlockMatch[0].length;
        }
        continue;
      }

      // 3c. Dangerous standalone opening/closing tags
      const dangerousStandaloneMatch = /^<\/?(?:script|iframe|object|embed|style|form)\b[^>]*>/i.exec(
        markdown.slice(i),
      );
      if (dangerousStandaloneMatch) {
        i += dangerousStandaloneMatch[0].length;
        continue;
      }

      // 3d. Markdown Autolinks (URI and Email)
      const uriAutolinkMatch = /^<([a-zA-Z][a-zA-Z0-9+.-]*):([^<>\s]+)>/i.exec(
        markdown.slice(i),
      );
      if (uriAutolinkMatch) {
        const scheme = uriAutolinkMatch[1].toLowerCase();
        if (ALLOWED_AUTOLINK_SCHEMES.has(scheme)) {
          output += uriAutolinkMatch[0];
          i += uriAutolinkMatch[0].length;
          continue;
        } else {
          // Unsupported or dangerous URI scheme: escape < and > so it is rendered
          // as literal text without creating clickable <a> links or executable elements,
          // preserving the original text without data loss.
          output += `&lt;${uriAutolinkMatch[1]}:${uriAutolinkMatch[2]}&gt;`;
          i += uriAutolinkMatch[0].length;
          continue;
        }
      }

      const emailAutolinkMatch = /^<([a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*)>/i.exec(
        markdown.slice(i),
      );
      if (emailAutolinkMatch) {
        output += emailAutolinkMatch[0];
        i += emailAutolinkMatch[0].length;
        continue;
      }

      // 3e. Allowed Note-it custom tags or unsupported HTML tags
      const htmlTagMatch = /^<\/?[A-Za-z][^>]*>/.exec(markdown.slice(i));
      if (htmlTagMatch) {
        const rawTag = htmlTagMatch[0];
        const parsed = parseCustomTag(rawTag);
        if (parsed) {
          if (parsed.kind === 'open') {
            openTagStack.push(parsed.tag);
            output += parsed.canonical;
          } else {
            const lastIdx = openTagStack.lastIndexOf(parsed.tag);
            if (lastIdx !== -1) {
              openTagStack.splice(lastIdx, 1);
              output += parsed.canonical;
            }
          }
        }
        i += rawTag.length;
        continue;
      }
    }

    // 4. Regular characters
    output += char;
    i++;
  }

  return output;
}

/** Sanitize HTML received from the clipboard before ProseMirror parses it. */
export function sanitizeHtml(rawHtml: string): string {
  if (!rawHtml) return '';

  const parser = new DOMParser();
  const doc = parser.parseFromString(rawHtml, 'text/html');
  const output = document.createElement('div');

  function appendSafe(node: Node, parent: Node): void {
    if (node.nodeType === Node.TEXT_NODE) {
      parent.appendChild(document.createTextNode(node.textContent ?? ''));
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;

    const source = node as HTMLElement;
    const tagName = source.tagName.toLowerCase();
    if (['script', 'iframe', 'object', 'embed', 'style', 'form'].includes(tagName)) return;

    let destination: Node = parent;
    if (tagName === 'u') {
      destination = document.createElement('u');
      parent.appendChild(destination);
    } else if (tagName === 'span') {
      const color = source.getAttribute('data-note-it-color');
      const fontSize = normalizeTextSize(source.getAttribute('data-note-it-font-size'));
      if (isValidHexColor(color) || fontSize !== null) {
        const span = document.createElement('span');
        if (isValidHexColor(color)) {
          span.setAttribute('data-note-it-color', color);
          span.style.color = color;
        }
        if (fontSize !== null) {
          span.setAttribute('data-note-it-font-size', String(fontSize));
          span.style.fontSize = `${fontSize}px`;
        }
        destination = span;
        parent.appendChild(destination);
      }
    } else if (tagName === 'mark') {
      const color = source.getAttribute('data-note-it-highlight');
      if (isValidHexColor(color)) {
        const mark = document.createElement('mark');
        mark.setAttribute('data-note-it-highlight', color);
        mark.style.backgroundColor = color;
        destination = mark;
        parent.appendChild(destination);
      }
    }

    for (const child of Array.from(source.childNodes)) appendSafe(child, destination);
  }

  for (const child of Array.from(doc.body.childNodes)) appendSafe(child, output);
  return output.innerHTML;
}
