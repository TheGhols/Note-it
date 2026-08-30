/**
 * Sanitization for the small raw-HTML subset supported inside Note-it Markdown.
 * Normal Markdown is kept as Markdown; only HTML fragments are inspected.
 */

import { normalizeTextSize } from '../editor/textSize.ts';

import {
  clampImageWidth,
  DEFAULT_IMAGE_ALIGN,
  isManagedAsset,
  normalizeImageAlign,
} from './assetReference.ts';

export const HEX_COLOR_REGEX = /^#(?:[0-9A-Fa-f]{3}|[0-9A-Fa-f]{6})$/;

export function isValidHexColor(value: unknown): value is string {
  return typeof value === 'string' && HEX_COLOR_REGEX.test(value);
}

export const ALLOWED_AUTOLINK_SCHEMES: ReadonlySet<string> = new Set([
  'https',
  'http',
  'mailto',
]);

/**
 * The one place that decides whether a string may become a link.
 *
 * Every feature that turns text into a link asks here — the autolink policy
 * and, since Phase 3.8, pasting a URL over a selection. A second opinion about
 * what a safe URL is would eventually be a second answer, and the whole point
 * of an allowlist is that there is one.
 *
 * Returns the URL as it will be stored, or `null`. `javascript:`, `data:`,
 * `file:` and everything else are not on the list; whitespace and control
 * characters are refused outright, because a "URL" that needs trimming to look
 * like one is not a URL somebody meant to paste.
 */
export function safeLinkUrl(candidate: string): string | null {
  const text = candidate.trim();
  if (text === '' || /\s/.test(text) || Array.from(text).some((c) => c < ' ' || c === '\u007f')) {
    return null;
  }

  let parsed: URL;
  try {
    parsed = new URL(text);
  } catch {
    return null;
  }

  const scheme = parsed.protocol.replace(/:$/, '').toLowerCase();
  if (!ALLOWED_AUTOLINK_SCHEMES.has(scheme)) return null;
  // `http://` with nothing after it is a scheme, not a destination.
  if ((scheme === 'http' || scheme === 'https') && parsed.hostname === '') return null;
  if (scheme === 'mailto' && parsed.pathname === '') return null;

  return text;
}

type CustomTagAction =
  | { kind: 'open'; tag: 'u' | 'span' | 'mark'; canonical: string }
  | { kind: 'close'; tag: 'u' | 'span' | 'mark'; canonical: string }
  | { kind: 'void'; tag: 'img'; canonical: string };

/**
 * One `<img>` as Note-it stores it, rewritten to exactly the form Note-it
 * writes — or refused.
 *
 * The rules are the ones the `span` above follows, for the same reason. Only
 * four attributes survive, always in this order, and each is validated rather
 * than copied: the source must be one of this store's own managed assets, the
 * width must be a number inside the supported range, and the alignment must be
 * one of three words. An `onerror`, a `style`, a `srcset`, a `javascript:` src
 * or a path climbing out of the assets directory is not rewritten and not
 * escaped — the tag is simply not one of ours, and it is dropped, exactly as a
 * `<span>` with no Note-it attribute is.
 *
 * A remote image is not refused as such: it is written `![alt](url)` like any
 * other Markdown image, and stays that. What cannot exist is an `<img>` tag in
 * a note carrying anything this application did not put there.
 */
function canonicalImageTag(rawTag: string): string | null {
  const parser = new DOMParser();
  const element = parser
    .parseFromString(rawTag, 'text/html')
    .body.querySelector('img');
  if (!element) return null;

  const src = element.getAttribute('src') ?? '';
  if (!isManagedAsset(src)) return null;

  const attributes = [`src="${src}"`, `alt="${escapeAttributeValue(element.getAttribute('alt') ?? '')}"`];

  const width = clampImageWidth(element.getAttribute('data-note-it-width'));
  if (width !== null) attributes.push(`data-note-it-width="${width}"`);

  const align = element.getAttribute('data-note-it-align');
  if (align !== null && align !== DEFAULT_IMAGE_ALIGN) {
    const normalized = normalizeImageAlign(align);
    if (normalized !== DEFAULT_IMAGE_ALIGN) {
      attributes.push(`data-note-it-align="${normalized}"`);
    }
  }

  return `<img ${attributes.join(' ')}>`;
}

function escapeAttributeValue(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function parseCustomTag(rawTag: string): CustomTagAction | null {
  // An image is a void element: it opens nothing, so it closes nothing.
  if (/^<img\b/i.test(rawTag)) {
    const canonical = canonicalImageTag(rawTag);
    return canonical ? { kind: 'void', tag: 'img', canonical } : null;
  }
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
      // 3a. HTML comment. A comment is inert data, never executable markup,
      // and since Phase 3.5 it is content the note keeps: the editor shows it
      // as a labelled block and writes it back unchanged. Dropping it here
      // would delete part of the file on every save.
      if (markdown.startsWith('<!--', i)) {
        const commentEnd = markdown.indexOf('-->', i + 4);
        if (commentEnd !== -1) {
          output += markdown.slice(i, commentEnd + 3);
          i = commentEnd + 3;
        } else {
          // Unterminated: there is no comment here, only an opening that never
          // closes. It is escaped rather than dropped, so the rest of the note
          // survives as the text it always was.
          output += '&lt;!--';
          i += 4;
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
          if (parsed.kind === 'void') {
            output += parsed.canonical;
          } else if (parsed.kind === 'open') {
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
