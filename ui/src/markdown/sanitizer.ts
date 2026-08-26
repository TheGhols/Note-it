/**
 * Sanitization for the small raw-HTML subset supported inside Note-it Markdown.
 * Normal Markdown is kept as Markdown; only HTML fragments are inspected.
 */

export const HEX_COLOR_REGEX = /^#(?:[0-9A-Fa-f]{3}|[0-9A-Fa-f]{6})$/;

const DANGEROUS_BLOCK =
  /<(script|iframe|object|embed|style|form)\b[^>]*>[\s\S]*?<\/\1\s*>/gi;
const DANGEROUS_STANDALONE = /<\/?(?:script|iframe|object|embed|style|form)\b[^>]*>/gi;
const HTML_TAG = /<!--[\s\S]*?-->|<\/?[A-Za-z][^>]*>/g;

export function isValidHexColor(value: unknown): value is string {
  return typeof value === 'string' && HEX_COLOR_REGEX.test(value);
}

function canonicalCustomTag(rawTag: string): string | null {
  if (/^<u\s*>$/i.test(rawTag)) return '<u>';
  if (/^<\/u\s*>$/i.test(rawTag)) return '</u>';
  if (/^<\/span\s*>$/i.test(rawTag)) return '</span>';
  if (/^<\/mark\s*>$/i.test(rawTag)) return '</mark>';

  const parser = new DOMParser();
  const parsed = parser.parseFromString(`${rawTag}</span></mark>`, 'text/html');
  const element = parsed.body.firstElementChild as HTMLElement | null;
  if (!element) return null;

  const tagName = element.tagName.toLowerCase();
  if (tagName === 'span') {
    const color = element.getAttribute('data-note-it-color');
    return isValidHexColor(color) ? `<span data-note-it-color="${color}">` : null;
  }
  if (tagName === 'mark') {
    const color = element.getAttribute('data-note-it-highlight');
    return isValidHexColor(color) ? `<mark data-note-it-highlight="${color}">` : null;
  }
  return null;
}

/**
 * Sanitize raw HTML embedded in Markdown without parsing the whole document as HTML.
 * Dangerous blocks and unsupported tags are removed, while their ordinary text
 * content is retained where applicable.
 */
export function sanitizeMarkdown(markdown: string): string {
  if (!markdown) return '';

  const withoutDangerousBlocks = markdown
    .replace(DANGEROUS_BLOCK, '')
    .replace(DANGEROUS_STANDALONE, '');

  return withoutDangerousBlocks.replace(HTML_TAG, (rawTag) => {
    if (rawTag.startsWith('<!--')) return '';
    const canonical = canonicalCustomTag(rawTag);
    return canonical ?? '';
  });
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
      if (isValidHexColor(color)) {
        const span = document.createElement('span');
        span.setAttribute('data-note-it-color', color);
        span.style.color = color;
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
