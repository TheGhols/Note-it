import Blockquote from '@tiptap/extension-blockquote';

/**
 * Callout kinds, in the order the menu offers them.
 *
 * The identifiers are GitHub's alert syntax, which Obsidian reads too, so a
 * note written here opens as a callout in both and a callout written there
 * opens here. The labels are what the note itself shows.
 */
export const CALLOUT_TYPES = [
  { id: 'NOTE', label: 'Nota' },
  { id: 'TIP', label: 'Dica' },
  { id: 'IMPORTANT', label: 'Importante' },
  { id: 'WARNING', label: 'Atenção' },
  { id: 'CAUTION', label: 'Cuidado' },
] as const;

export type CalloutType = (typeof CALLOUT_TYPES)[number]['id'];

/**
 * Resolves a marker to a supported kind, or `null` for anything else.
 *
 * Everything that is not one of the five is `null`, which is simply a
 * blockquote — the marker line stays in the quote as the text it was, so an
 * unknown kind costs the note nothing but the decoration.
 */
export function calloutType(value: unknown): CalloutType | null {
  if (typeof value !== 'string') return null;
  const upper = value.toUpperCase();
  return CALLOUT_TYPES.some((entry) => entry.id === upper) ? (upper as CalloutType) : null;
}

export function calloutLabel(type: CalloutType): string {
  return CALLOUT_TYPES.find((entry) => entry.id === type)!.label;
}

/** A `[!KIND]` marker occupying a line of its own, and nothing after it. */
const MARKER = /^\[!([A-Za-z]+)\][ \t]*(?:\n|$)/;

/**
 * Reads the callout marker off a parsed blockquote.
 *
 * The marker arrives as the opening text of the first paragraph, because
 * `> [!NOTE]` and the line under it are one paragraph separated by a soft
 * break. Recognising it means removing exactly that much text and nothing
 * else; a paragraph left with nothing is dropped, unless it is the only one,
 * since a blockquote must hold at least one block.
 */
function readMarker(node: any): { type: CalloutType; content: any[] } | null {
  const content = Array.isArray(node?.content) ? node.content : [];
  const first = content[0];
  if (first?.type !== 'paragraph') return null;

  const inline = Array.isArray(first.content) ? first.content : [];
  const leading = inline[0];
  if (leading?.type !== 'text' || typeof leading.text !== 'string') return null;
  // A marker carrying formatting is not a marker; it is text someone styled.
  if (Array.isArray(leading.marks) && leading.marks.length > 0) return null;

  const match = MARKER.exec(leading.text);
  if (!match) return null;
  const type = calloutType(match[1]);
  if (!type) return null;

  const remainder = leading.text.slice(match[0].length);
  const rest =
    remainder === ''
      ? inline.slice(1)
      : [{ ...leading, text: remainder }, ...inline.slice(1)];

  const paragraph = { ...first, content: rest };
  const body =
    rest.length === 0 && content.length > 1 ? content.slice(1) : [paragraph, ...content.slice(1)];

  return { type, content: body };
}

/**
 * Blockquotes, which may also carry a callout kind.
 *
 * A callout is deliberately not a node of its own. It is the same blockquote
 * with one attribute, so it inherits the content model unchanged — several
 * paragraphs, lists, nested blocks, anything a quote can hold — and needs no
 * parallel set of commands, input rules or serialization. It also means the
 * failure mode is free: a marker this version does not know produces no
 * attribute, which is exactly a plain blockquote with the marker still in it.
 */
export const NoteItBlockquote = Blockquote.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      callout: {
        default: null,
        parseHTML: (element: HTMLElement) => calloutType(element.getAttribute('data-callout')),
        renderHTML: (attributes: Record<string, unknown>) => {
          const type = calloutType(attributes.callout);
          if (!type) return {};
          // Both values come from the whitelist above, never from the note, so
          // no note content can reach an attribute or a class name.
          return { 'data-callout': type, 'data-callout-label': calloutLabel(type) };
        },
      },
    };
  },

  parseMarkdown(this: any, token: any, helpers: any) {
    const node = this.parent?.(token, helpers);
    if (!node) return node;

    const marker = readMarker(node);
    if (!marker) return node;

    return {
      ...node,
      attrs: { ...(node.attrs ?? {}), callout: marker.type },
      content: marker.content,
    };
  },

  renderMarkdown(this: any, node: any, helpers: any) {
    // The quote itself is rendered by the parent, so the `>` prefixing, the
    // blank separator lines and nested blocks all keep behaving as they do for
    // an ordinary blockquote. Only the marker line is added here.
    const rendered = this.parent?.(node, helpers) ?? '';
    const type = calloutType(node.attrs?.callout);
    if (!type) return rendered;
    return rendered === '' ? `> [!${type}]` : `> [!${type}]\n${rendered}`;
  },
});
