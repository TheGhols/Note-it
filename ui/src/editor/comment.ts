import { Node } from '@tiptap/core';

/**
 * The only sequence that cannot appear inside an HTML comment, and the form it
 * is stored as.
 *
 * A `-->` typed into a comment would close it early and spill the rest of the
 * note into the document as ordinary text. Writing it escaped keeps the file
 * valid, is inert in every other tool, and reads back as what was typed.
 */
const COMMENT_TERMINATOR = /-->/g;
const ESCAPED_TERMINATOR = /--&gt;/g;

export function encodeCommentText(text: string): string {
  return text.replace(COMMENT_TERMINATOR, '--&gt;');
}

export function decodeCommentText(text: string): string {
  return text.replace(ESCAPED_TERMINATOR, '-->');
}

/** A whole-block HTML comment, as `marked` hands it over. */
const BLOCK_COMMENT = /^<!--([\s\S]*?)-->\s*$/;

export function parseCommentBody(raw: unknown): string | null {
  if (typeof raw !== 'string') return null;
  const match = BLOCK_COMMENT.exec(raw.trim());
  if (!match) return null;
  return decodeCommentText(match[1].trim());
}

/**
 * A note-to-self that is stored but not part of what the note says.
 *
 * Persisted as a plain `<!-- ... -->`, so the file stays ordinary Markdown and
 * any other tool treats it as the comment it is. In the editor it is a small
 * labelled block rather than invisible text: a WYSIWYG editor that hid it
 * would leave no way to edit or delete it, and a note whose file holds
 * something the window never shows is a note that loses things quietly.
 *
 * The content is text and only text. The node declares itself as code and
 * takes no marks, so nothing inside is ever interpreted — not as HTML, not as
 * Markdown, not as a typographic substitution.
 */
export const NoteItComment = Node.create({
  name: 'noteItComment',
  group: 'block',
  content: 'text*',
  marks: '',
  code: true,
  defining: true,
  whitespace: 'pre',

  addOptions() {
    return { HTMLAttributes: {} };
  },

  parseHTML() {
    return [{ tag: 'div[data-note-it-comment]', preserveWhitespace: 'full' as const }];
  },

  renderHTML() {
    // The label is a constant put on the element for CSS to show; the comment
    // text itself is the node's content and is never interpolated anywhere.
    return [
      'div',
      {
        'data-note-it-comment': '',
        class: 'note-comment',
        'data-note-it-comment-label': 'Comentário',
      },
      0,
    ];
  },

  addCommands() {
    return {
      setComment:
        (text = '') =>
        ({ commands }: { commands: any }) =>
          commands.insertContent({
            type: this.name,
            content: text ? [{ type: 'text', text }] : [],
          }),
    } as any;
  },

  markdownTokenName: 'html',

  parseMarkdown(token: any, helpers: any) {
    // Every other `html` token is left to the handlers that already deal with
    // it: returning null here declines, and the parser moves on.
    if (!token?.block) return null;
    const body = parseCommentBody(token.raw);
    if (body === null) return null;
    return helpers.createNode(
      'noteItComment',
      undefined,
      body ? [helpers.createTextNode(body)] : [],
    );
  },

  renderMarkdown(node: any) {
    const text = (node.content ?? [])
      .map((child: any) => (typeof child?.text === 'string' ? child.text : ''))
      .join('');
    const body = encodeCommentText(text).trim();
    return body ? `<!-- ${body} -->` : '<!--  -->';
  },
} as any);
