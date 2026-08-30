import { Node, mergeAttributes } from '@tiptap/core';
import {
  clampImageWidth,
  DEFAULT_IMAGE_ALIGN,
  ImageAlign,
  imageDisplayUri,
  normalizeImageAlign,
} from '../markdown/assetReference.ts';
import { imageNodeView } from './imageView.ts';

export type { ImageAlign };
export {
  clampImageWidth,
  DEFAULT_IMAGE_ALIGN,
  IMAGE_ALIGNMENTS,
  imageDisplayUri,
  isManagedAsset,
  MAX_IMAGE_WIDTH,
  MIN_IMAGE_WIDTH,
  normalizeImageAlign,
} from '../markdown/assetReference.ts';

/**
 * An image a note holds.
 *
 * The bytes live in the store, under `assets/<note>/<asset>.<ext>`, and the
 * note refers to them by a path relative to `notes/`. Nothing about an image
 * is ever inlined into the Markdown: a note stays a file a person can read,
 * and a screenshot does not turn it into a megabyte of base64.
 *
 * The node carries four things and no more — where the picture is, its
 * alternative text, how wide the reader made it, and how it sits in the text.
 * Height is never stored, because it follows from the width and the picture's
 * own proportions, and storing it is how an image ends up stretched.
 */

/** What the reader is shown in place of a picture that will not load. */
export const MISSING_IMAGE_LABEL = 'Imagem indisponível';

interface ImageAttributes {
  src: string;
  alt: string;
  width: number | null;
  align: ImageAlign;
}

function readAttributes(attrs: Record<string, unknown> | undefined): ImageAttributes {
  return {
    src: typeof attrs?.src === 'string' ? attrs.src : '',
    alt: typeof attrs?.alt === 'string' ? attrs.alt : '',
    width: clampImageWidth(attrs?.width),
    align: normalizeImageAlign(attrs?.align),
  };
}

/**
 * Whether an image can be written as ordinary Markdown.
 *
 * It can when there is nothing to say beyond where the picture is: no width
 * chosen, the default alignment, and an alt and a source that the `![](…)`
 * form can carry without escaping. That is what a freshly inserted image is,
 * so the common case leaves the note file as plain as it has always been.
 */
function fitsPlainMarkdown(attributes: ImageAttributes): boolean {
  if (attributes.width !== null) return false;
  if (attributes.align !== DEFAULT_IMAGE_ALIGN) return false;
  if (/[()\s<>]/.test(attributes.src)) return false;
  if (/[[\]\\\n]/.test(attributes.alt)) return false;
  return true;
}

function escapeAttribute(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

/**
 * The stored form of one image.
 *
 * Plain `![alt](src)` whenever that can say everything, and a canonical
 * `<img>` when the reader has chosen a width or an alignment — which is the
 * one thing Markdown's own image syntax has nowhere to put. The tag carries
 * exactly the attributes that are set, always in this order, so the same image
 * always writes the same bytes and a save that changed nothing changes nothing
 * on disk.
 */
export function renderImageMarkdown(attrs: Record<string, unknown> | undefined): string {
  const attributes = readAttributes(attrs);
  if (attributes.src === '') return '';

  if (fitsPlainMarkdown(attributes)) {
    return `![${attributes.alt}](${attributes.src})`;
  }

  const parts = [
    `src="${escapeAttribute(attributes.src)}"`,
    `alt="${escapeAttribute(attributes.alt)}"`,
  ];
  if (attributes.width !== null) {
    parts.push(`data-note-it-width="${attributes.width}"`);
  }
  if (attributes.align !== DEFAULT_IMAGE_ALIGN) {
    parts.push(`data-note-it-align="${attributes.align}"`);
  }
  return `<img ${parts.join(' ')}>`;
}

function decodeAttribute(value: string): string {
  return value
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

function attributeOf(raw: string, name: string): string | null {
  const match = new RegExp(`\\s${name}="([^"]*)"`, 'i').exec(raw);
  return match ? decodeAttribute(match[1]) : null;
}

/** One `<img …>` as it is stored, read back into attributes. */
export function parseImageTag(raw: string): ImageAttributes | null {
  if (!/^<img\s[^>]*>$/i.test(raw)) return null;
  const src = attributeOf(raw, 'src');
  if (src === null || src === '') return null;
  return {
    src,
    alt: attributeOf(raw, 'alt') ?? '',
    width: clampImageWidth(attributeOf(raw, 'data-note-it-width')),
    align: normalizeImageAlign(attributeOf(raw, 'data-note-it-align')),
  };
}

/** One `![alt](src)`, read back into attributes. */
export function parseImageMarkdown(raw: string): ImageAttributes | null {
  const match = /^!\[([^\]]*)\]\(([^()\s]*)\)/.exec(raw);
  if (!match || match[2] === '') return null;
  return {
    src: match[2],
    alt: match[1],
    width: null,
    align: DEFAULT_IMAGE_ALIGN,
  };
}

/**
 * The width of an image at the start of `src`, in either stored form, or
 * `null` when there is no image there.
 */
function imageTokenWidth(src: string): { raw: string; attributes: ImageAttributes } | null {
  if (src.startsWith('<img')) {
    const end = src.indexOf('>');
    if (end === -1) return null;
    const raw = src.slice(0, end + 1);
    const attributes = parseImageTag(raw);
    return attributes ? { raw, attributes } : null;
  }
  if (src.startsWith('![')) {
    const match = /^!\[([^\]]*)\]\(([^()\s]*)\)/.exec(src);
    if (!match) return null;
    const attributes = parseImageMarkdown(match[0]);
    return attributes ? { raw: match[0], attributes } : null;
  }
  return null;
}

export const NoteItImage = Node.create({
  name: 'noteItImage',
  group: 'inline',
  inline: true,
  atom: true,
  // Not draggable on purpose. The note itself is dragged by its header bar,
  // and a picture that could also be dragged around the document would put two
  // different drags a few pixels apart — one of which resizes it. Moving an
  // image is cut and paste, which already works.
  draggable: false,

  addAttributes() {
    return {
      src: {
        default: '',
        parseHTML: (element: HTMLElement) =>
          element.getAttribute('data-note-it-src') ?? element.getAttribute('src') ?? '',
        renderHTML: () => ({}),
      },
      alt: {
        default: '',
        parseHTML: (element: HTMLElement) => element.getAttribute('alt') ?? '',
        renderHTML: () => ({}),
      },
      width: {
        default: null,
        parseHTML: (element: HTMLElement) =>
          clampImageWidth(element.getAttribute('data-note-it-width')),
        renderHTML: () => ({}),
      },
      align: {
        default: DEFAULT_IMAGE_ALIGN,
        parseHTML: (element: HTMLElement) =>
          normalizeImageAlign(element.getAttribute('data-note-it-align')),
        renderHTML: () => ({}),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'img[src]' }, { tag: 'img[data-note-it-src]' }];
  },

  /**
   * The element written into the document.
   *
   * The stored reference goes into `data-note-it-src`, and the `src` the
   * browser actually loads is only ever the host's own asset scheme. A
   * reference that is not one of this store's managed assets — a remote URL, a
   * path somebody typed — resolves to nothing, so the element carries no
   * source and no request is made. Nothing is fetched by displaying a note.
   */
  renderHTML({ node, HTMLAttributes }: { node: any; HTMLAttributes: Record<string, unknown> }) {
    const attributes = readAttributes(node.attrs);
    const uri = imageDisplayUri(attributes.src);
    const rendered: Record<string, unknown> = {
      'data-note-it-src': attributes.src,
      'data-note-it-align': attributes.align,
      alt: attributes.alt,
      class: 'note-image',
      draggable: 'false',
    };
    if (uri !== null) rendered.src = uri;
    if (attributes.width !== null) {
      rendered['data-note-it-width'] = String(attributes.width);
      rendered.style = `width:${attributes.width}px`;
    }
    return ['img', mergeAttributes(HTMLAttributes, rendered)];
  },

  addNodeView() {
    return imageNodeView;
  },

  addCommands() {
    return {
      /** Puts one image into the document at the current selection. */
      setNoteItImage:
        (attributes: { src: string; alt?: string }) =>
        ({ commands }: { commands: any }) =>
          commands.insertContent({
            type: 'noteItImage',
            attrs: {
              src: attributes.src,
              alt: attributes.alt ?? '',
              width: null,
              align: DEFAULT_IMAGE_ALIGN,
            },
          }),
    } as any;
  },

  markdownTokenName: 'noteItImage',

  markdownTokenizer: {
    name: 'noteItImage',
    level: 'inline',
    start(src: string) {
      const tag = src.indexOf('<img');
      const markdown = src.indexOf('![');
      if (tag === -1) return markdown === -1 ? undefined : markdown;
      if (markdown === -1) return tag;
      return Math.min(tag, markdown);
    },
    tokenize(src: string) {
      const found = imageTokenWidth(src);
      if (!found) return;
      return {
        type: 'noteItImage',
        raw: found.raw,
        text: found.attributes.alt,
        attrs: found.attributes,
      };
    },
  },

  parseMarkdown(token: any, helpers: any) {
    const attributes = readAttributes(token?.attrs);
    if (attributes.src === '') return null;
    return helpers.createNode('noteItImage', attributes, []);
  },

  renderMarkdown(node: any) {
    return renderImageMarkdown(node?.attrs);
  },
} as any);
