/**
 * Discrete font sizes selectable for a run of text.
 *
 * This is inline formatting and part of the note's content — distinct from the
 * note zoom, which scales the whole view without touching the document.
 */
export const TEXT_SIZES = [12, 14, 16, 18, 22, 26, 32] as const;

export type TextSize = (typeof TEXT_SIZES)[number];

/** Only whitelisted sizes are ever applied or accepted from stored content. */
export function isValidTextSize(value: unknown): value is TextSize {
  const numeric = typeof value === 'string' ? Number(value) : value;
  return (
    typeof numeric === 'number' &&
    Number.isInteger(numeric) &&
    (TEXT_SIZES as readonly number[]).includes(numeric)
  );
}

export function normalizeTextSize(value: unknown): TextSize | null {
  const numeric = typeof value === 'string' ? Number(value) : value;
  return isValidTextSize(numeric) ? (numeric as TextSize) : null;
}

/**
 * Next size up the scale. `null` means the theme default, which sits below the
 * smallest explicit size. Already at the top stays at the top.
 */
export function largerTextSize(current: TextSize | null): TextSize {
  if (current === null) return TEXT_SIZES[0];
  const index = TEXT_SIZES.indexOf(current);
  return TEXT_SIZES[Math.min(index + 1, TEXT_SIZES.length - 1)];
}

/** Next size down; stepping below the smallest returns to the theme default. */
export function smallerTextSize(current: TextSize | null): TextSize | null {
  if (current === null) return null;
  const index = TEXT_SIZES.indexOf(current);
  return index <= 0 ? null : TEXT_SIZES[index - 1];
}
