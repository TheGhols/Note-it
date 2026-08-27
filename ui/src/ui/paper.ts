import { PaperIntensity, PaperType } from '../bridge/types.ts';

/**
 * The note's paper: a background pattern and how strongly it is drawn.
 *
 * This module owns the vocabulary — the identifiers, their pt-BR labels and
 * what counts as a valid value — and nothing else. Every visual number lives
 * in `styles/theme.css` as a custom property, so spacing, ink and opacity are
 * defined once and there is nothing here that could drift away from what is
 * actually painted.
 */
export interface PaperOption<T> {
  id: T;
  label: string;
}

export const DEFAULT_PAPER_TYPE: PaperType = 'blank';
export const DEFAULT_PAPER_INTENSITY: PaperIntensity = 'normal';

export const PAPER_TYPES: readonly PaperOption<PaperType>[] = [
  { id: 'blank', label: 'Liso' },
  { id: 'lined', label: 'Pautado' },
  { id: 'dotted', label: 'Pontilhado' },
  { id: 'grid-small', label: 'Quadriculado pequeno' },
  { id: 'grid-large', label: 'Quadriculado grande' },
];

export const PAPER_INTENSITIES: readonly PaperOption<PaperIntensity>[] = [
  { id: 'subtle', label: 'Suave' },
  { id: 'normal', label: 'Normal' },
  { id: 'strong', label: 'Forte' },
];

/**
 * Resolves a stored paper pattern to the supported set.
 *
 * A note written before the paper existed carries nothing at all, and a
 * hand-edited one can carry anything; both have to open, on plain paper,
 * rather than leave the note unrenderable.
 */
export function normalizePaperType(value: unknown): PaperType {
  return PAPER_TYPES.some((option) => option.id === value)
    ? (value as PaperType)
    : DEFAULT_PAPER_TYPE;
}

/** Same contract as {@link normalizePaperType}, for the pattern intensity. */
export function normalizePaperIntensity(value: unknown): PaperIntensity {
  return PAPER_INTENSITIES.some((option) => option.id === value)
    ? (value as PaperIntensity)
    : DEFAULT_PAPER_INTENSITY;
}

export function paperTypeLabel(type: PaperType): string {
  return PAPER_TYPES.find((option) => option.id === type)?.label ?? type;
}

export function paperIntensityLabel(intensity: PaperIntensity): string {
  return PAPER_INTENSITIES.find((option) => option.id === intensity)?.label ?? intensity;
}

/**
 * Points the page at one paper. Two data attributes are all the stylesheet
 * needs: the type selects a pattern and its spacing, the intensity selects the
 * opacity it is drawn with. The intensity is applied even for plain paper,
 * where it simply has no pattern to act on, so switching back and forth never
 * loses the choice.
 */
export function applyPaper(body: HTMLElement, type: PaperType, intensity: PaperIntensity): void {
  body.setAttribute('data-paper', type);
  body.setAttribute('data-paper-intensity', intensity);
}
