import StarterKit from '@tiptap/starter-kit';
import Underline from '@tiptap/extension-underline';
import Highlight from '@tiptap/extension-highlight';
import { TextStyle } from '@tiptap/extension-text-style';
import Color from '@tiptap/extension-color';
import TaskList from '@tiptap/extension-task-list';
import Placeholder from '@tiptap/extension-placeholder';
import { Markdown } from '@tiptap/markdown';
import { isValidHexColor } from '../markdown/sanitizer.ts';
import { HIGHLIGHT_TEXT_COLOR } from '../ui/palettes.ts';
import { NoteItBlockquote } from './callout.ts';
import { NoteItCodeBlock } from './codeBlock.ts';
import { NoteItComment } from './comment.ts';
import { NoteItMath } from './math.ts';
import { NoteItTaskItem } from './taskItem.ts';
import { NoteItTypography } from './typography.ts';
import { normalizeTextSize } from './textSize.ts';

// Custom Underline serialized to <u>...</u>
const NoteItUnderline = Underline.extend({
  renderMarkdown(node: any, helpers: any) {
    return `<u>${helpers.renderChildren(node)}</u>`;
  },
  markdownTokenizer: {
    name: 'underline',
    level: 'inline',
    start(src: string) {
      return src.indexOf('<u>');
    },
    tokenize(src: string, _tokens: any, lexer: any) {
      const match = /^(<u>)([\s\S]+?)(<\/u>)/i.exec(src);
      if (!match) return;
      const innerContent = match[2];
      return {
        type: 'underline',
        raw: match[0],
        text: innerContent,
        tokens: lexer.inlineTokens(innerContent),
      };
    },
  },
});

// Custom Highlight serialized to <mark data-note-it-highlight="..." style="...">
const NoteItHighlight = Highlight.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      color: {
        ...(this.parent?.() as any)?.color,
        /**
         * Upstream renders `background-color: X; color: inherit`, and that
         * inline `color: inherit` beats any stylesheet rule — which is why
         * highlighted text stayed white on the dark paper. The foreground is
         * set here instead, on the same inline style, so it actually applies.
         * Nothing is written to the Markdown: this is rendering only.
         */
        renderHTML: (attributes: Record<string, unknown>) => {
          const color = attributes.color;
          if (!isValidHexColor(color)) {
            return { style: `color: ${HIGHLIGHT_TEXT_COLOR}` };
          }
          return {
            'data-color': color,
            style: `background-color: ${color}; color: ${HIGHLIGHT_TEXT_COLOR}`,
          };
        },
      },
    };
  },

  renderMarkdown(node: any, helpers: any) {
    const color = node.attrs?.color;
    if (isValidHexColor(color)) {
      return `<mark data-note-it-highlight="${color}" style="background-color:${color}">${helpers.renderChildren(node)}</mark>`;
    }
    return `<mark>${helpers.renderChildren(node)}</mark>`;
  },
  markdownTokenizer: {
    name: 'highlight',
    level: 'inline',
    start(src: string) {
      return src.indexOf('<mark');
    },
    tokenize(src: string, _tokens: any, lexer: any) {
      const match = /^<mark(?:[^>]*?data-note-it-highlight=["']([^"']+)["'][^>]*)?>([\s\S]+?)<\/mark>/i.exec(src);
      if (!match) return;
      const color = match[1];
      const innerContent = match[2];
      if (color && !isValidHexColor(color)) return;
      return {
        type: 'highlight',
        raw: match[0],
        text: innerContent,
        attrs: color ? { color } : {},
        tokens: lexer.inlineTokens(innerContent),
      };
    },
  },
  parseMarkdown(token: any, helpers: any) {
    return helpers.applyMark(
      'highlight',
      helpers.parseInline(token.tokens || []),
      token.attrs || {},
    );
  },
});

/**
 * Finds a `<span>` carrying `attribute`, returning its inner content and the
 * exact source it spans.
 *
 * Colour and text size are both serialized as spans and can nest, so a lazy
 * regex would stop at the inner `</span>` and swallow the nested mark. This
 * walks the source tracking depth to find the matching close tag.
 */
function matchSpanWithAttribute(
  src: string,
  attribute: string,
): { raw: string; value: string; inner: string } | null {
  const openMatch = /^<span[^>]*>/i.exec(src);
  if (!openMatch) return null;

  const openTag = openMatch[0];
  const valueMatch = new RegExp(`${attribute}=["']([^"']+)["']`, 'i').exec(openTag);
  if (!valueMatch) return null;

  let depth = 1;
  let index = openTag.length;
  while (index < src.length) {
    const nextOpen = src.toLowerCase().indexOf('<span', index);
    const nextClose = src.toLowerCase().indexOf('</span>', index);
    if (nextClose === -1) return null;

    if (nextOpen !== -1 && nextOpen < nextClose) {
      depth += 1;
      index = nextOpen + '<span'.length;
      continue;
    }

    depth -= 1;
    if (depth === 0) {
      return {
        raw: src.slice(0, nextClose + '</span>'.length),
        value: valueMatch[1],
        inner: src.slice(openTag.length, nextClose),
      };
    }
    index = nextClose + '</span>'.length;
  }
  return null;
}

// Custom Color serialized to <span data-note-it-color="..." style="...">
const NoteItTextStyle = TextStyle.extend({
  markdownTokenName: 'color',
  renderMarkdown(node: any, helpers: any) {
    const color = node.attrs?.color;
    if (isValidHexColor(color)) {
      return `<span data-note-it-color="${color}" style="color:${color}">${helpers.renderChildren(node)}</span>`;
    }
    return helpers.renderChildren(node);
  },
  markdownTokenizer: {
    name: 'color',
    level: 'inline',
    start(src: string) {
      return src.indexOf('<span');
    },
    tokenize(src: string, _tokens: any, lexer: any) {
      const match = matchSpanWithAttribute(src, 'data-note-it-color');
      if (!match || !isValidHexColor(match.value)) return;
      return {
        type: 'color',
        raw: match.raw,
        text: match.inner,
        attrs: { color: match.value },
        tokens: lexer.inlineTokens(match.inner),
      };
    },
  },
  parseMarkdown(token: any, helpers: any) {
    return helpers.applyMark(
      'textStyle',
      helpers.parseInline(token.tokens || []),
      token.attrs || {},
    );
  },
});

// Discrete text size serialized to <span data-note-it-font-size="..." style="...">
const NoteItFontSize = TextStyle.extend({
  name: 'noteItFontSize',
  markdownTokenName: 'fontSize',
  addAttributes() {
    return {
      fontSize: {
        default: null,
        parseHTML: (element: HTMLElement) =>
          normalizeTextSize(element.getAttribute('data-note-it-font-size')),
        renderHTML: (attributes: Record<string, unknown>) => {
          const size = normalizeTextSize(attributes.fontSize);
          if (size === null) return {};
          // Only the data attribute is emitted; the stylesheet turns it into a
          // zoom-scaled size, so a sized run keeps its proportion to the
          // surrounding text at every zoom level. The stored Markdown still
          // carries a plain pixel value for other tools.
          return { 'data-note-it-font-size': String(size) };
        },
      },
    };
  },
  renderMarkdown(node: any, helpers: any) {
    const size = normalizeTextSize(node.attrs?.fontSize);
    if (size === null) return helpers.renderChildren(node);
    return `<span data-note-it-font-size="${size}" style="font-size:${size}px">${helpers.renderChildren(node)}</span>`;
  },
  markdownTokenizer: {
    name: 'fontSize',
    level: 'inline',
    start(src: string) {
      return src.indexOf('<span');
    },
    tokenize(src: string, _tokens: any, lexer: any) {
      const match = matchSpanWithAttribute(src, 'data-note-it-font-size');
      if (!match) return;
      const size = normalizeTextSize(match.value);
      if (size === null) return;
      return {
        type: 'fontSize',
        raw: match.raw,
        text: match.inner,
        attrs: { fontSize: size },
        tokens: lexer.inlineTokens(match.inner),
      };
    },
  },
  parseMarkdown(token: any, helpers: any) {
    return helpers.applyMark(
      'noteItFontSize',
      helpers.parseInline(token.tokens || []),
      token.attrs || {},
    );
  },
});

export const editorExtensions = [
  StarterKit.configure({
    heading: {
      levels: [1, 2, 3, 4, 5, 6],
    },
    undoRedo: {
      depth: 100,
    },
    underline: false,
    // Both are replaced below rather than configured: the code block gains
    // highlighting, and the blockquote gains the callout marker.
    blockquote: false,
    codeBlock: false,
  }),
  NoteItBlockquote,
  NoteItCodeBlock,
  NoteItComment,
  NoteItUnderline,
  NoteItHighlight.configure({
    multicolor: true,
  }),
  NoteItTextStyle,
  Color,
  NoteItFontSize,
  TaskList,
  NoteItTaskItem.configure({
    nested: true,
  }),
  NoteItTypography,
  NoteItMath,
  Placeholder.configure({
    placeholder: 'Type your note here...',
  }),
  Markdown.configure({
    indentation: {
      style: 'space',
      size: 2,
    },
  }),
];
