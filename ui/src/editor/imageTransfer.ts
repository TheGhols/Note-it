/**
 * Reading an image out of a paste or a drop.
 *
 * The gesture hands the page a `File`; the page hands the host its bytes. No
 * path is ever named — a dropped file's real location is not something the
 * page should be able to tell the host to go and read, and a pasted screenshot
 * has no location at all.
 *
 * Base64 only for the length of one message. What reaches the disk is the
 * bytes, and what the note stores is a path: nothing here ever ends up inside
 * a `.md`.
 */

/** The formats a note takes in. Anything else is left to whatever else wants it. */
export const IMAGE_MIME_TYPES: readonly string[] = [
  'image/png',
  'image/jpeg',
  'image/webp',
  'image/gif',
];

/**
 * The image in a transfer, if there is one.
 *
 * Deliberately narrow. A paste carrying text *and* an image — which is what
 * copying from a browser usually produces — is a text paste, because that is
 * what the reader almost always meant and it is what Note-it has always done.
 * Only a transfer whose files are an image, with no text alongside, is an
 * image paste.
 */
export function imageFileIn(transfer: DataTransfer | null): File | null {
  if (!transfer) return null;
  const files = Array.from(transfer.files ?? []);
  const image = files.find((file) => IMAGE_MIME_TYPES.includes(file.type));
  return image ?? null;
}

/** Whether a paste should be treated as an image rather than as text. */
export function isImagePaste(transfer: DataTransfer | null): boolean {
  if (!transfer) return false;
  if (imageFileIn(transfer) === null) return false;
  // Text alongside the picture means the reader copied something that has
  // both, and pasting text is what pasting has always done here.
  const text = transfer.getData('text/plain');
  return typeof text !== 'string' || text.trim() === '';
}

/** Base64 for one `ArrayBuffer`, in chunks so a large image does not blow the stack. */
export function encodeBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  const CHUNK = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + CHUNK));
  }
  return btoa(binary);
}

/** The bytes of the image in a transfer, encoded for the wire, or `null`. */
export async function imageBytesFromTransfer(
  transfer: DataTransfer | null,
): Promise<string | null> {
  const file = imageFileIn(transfer);
  if (!file) return null;
  return encodeBase64(await file.arrayBuffer());
}
