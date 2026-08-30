/**
 * What a note is allowed to say about a picture it holds.
 *
 * The stored form is a markdown concern rather than an editor one, which is
 * why it lives here: the sanitizer has to recognise it on the way in, and the
 * editor has to write it on the way out, and neither of them should be the
 * place the rules are kept.
 */

/** The scheme the host serves a note's own images over. */
export const ASSET_SCHEME = 'note-it-asset';

/** The prefix every managed reference starts with, relative to `notes/`. */
export const ASSET_RELATIVE_PREFIX = '../assets/';

const UUID = '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}';

/** `../assets/<note-uuid>/<asset-uuid>.<ext>`, and nothing else at all. */
const STORED_ASSET = new RegExp(
  `^\\.\\./assets/(${UUID})/(${UUID})\\.(png|jpg|jpeg|webp|gif)$`,
);

/** How the image sits among the text. */
export type ImageAlign = 'left' | 'center' | 'right';

/**
 * The alignment an image has when nobody has chosen one.
 *
 * A block of its own, centred, with the text above and below it rather than
 * around it. Wrapping is something the reader opts into by choosing left or
 * right, because an inserted picture that immediately rearranged the paragraph
 * around it would be a surprise.
 */
export const DEFAULT_IMAGE_ALIGN: ImageAlign = 'center';

export const IMAGE_ALIGNMENTS: readonly { id: ImageAlign; label: string }[] = [
  { id: 'left', label: 'Esquerda' },
  { id: 'center', label: 'Centro' },
  { id: 'right', label: 'Direita' },
];

/**
 * Narrowest an image may be made: still a picture, and still large enough for
 * its own handle to be grabbable.
 */
export const MIN_IMAGE_WIDTH = 48;

/**
 * Widest a stored width may be.
 *
 * Not the widest an image is *drawn* — the stylesheet caps that at the note's
 * own usable width, so making a note narrower shows a smaller picture without
 * rewriting anything. This is the ceiling on what may be written down, so a
 * hand-edited or corrupted file cannot ask for a width that breaks layout.
 */
export const MAX_IMAGE_WIDTH = 4096;

/**
 * A width somebody could have asked for, or `null` for "as it comes".
 *
 * `null` is a real answer and the one a freshly inserted image has: the
 * stylesheet then sizes it, capped to the note. Everything else is clamped
 * rather than believed, because this arrives from a drag, from a stored file,
 * and from anything that has ever edited one.
 */
export function clampImageWidth(value: unknown): number | null {
  const width = typeof value === 'string' ? Number(value.trim()) : value;
  if (typeof width !== 'number' || !Number.isFinite(width)) return null;
  const rounded = Math.round(width);
  if (rounded <= 0) return null;
  return Math.min(Math.max(rounded, MIN_IMAGE_WIDTH), MAX_IMAGE_WIDTH);
}

export function normalizeImageAlign(value: unknown): ImageAlign {
  return IMAGE_ALIGNMENTS.some((entry) => entry.id === value)
    ? (value as ImageAlign)
    : DEFAULT_IMAGE_ALIGN;
}

/** Whether a reference is one of this store's own managed assets. */
export function isManagedAsset(src: unknown): src is string {
  return typeof src === 'string' && STORED_ASSET.test(src);
}

/**
 * What the page actually loads, for a reference the store manages.
 *
 * Only a managed asset resolves to anything. A hand-written `![](foto.png)`, a
 * remote URL or a path somebody typed comes back `null`, so the element is
 * drawn with no source and nothing is requested — the answer the page's own
 * Content-Security-Policy would give, arrived at before the request rather
 * than after it.
 */
export function imageDisplayUri(src: unknown): string | null {
  if (!isManagedAsset(src)) return null;
  return `${ASSET_SCHEME}:${src.slice('../assets'.length)}`;
}
