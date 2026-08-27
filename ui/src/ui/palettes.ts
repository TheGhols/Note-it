/**
 * Inline formatting palettes.
 *
 * Deliberately small and fixed: the values double as the whitelist the
 * Markdown sanitizer accepts back, so stored content can never introduce an
 * arbitrary colour.
 */
export interface PaletteEntry {
  label: string;
  /** `null` clears the mark and returns to the theme default. */
  value: string | null;
}

export const TEXT_COLORS: readonly PaletteEntry[] = [
  { label: 'Padrão', value: null },
  { label: 'Cinza', value: '#64748B' },
  { label: 'Vermelho', value: '#DC2626' },
  { label: 'Laranja', value: '#EA580C' },
  { label: 'Amarelo', value: '#CA8A04' },
  { label: 'Verde', value: '#16A34A' },
  { label: 'Azul', value: '#2563EB' },
  { label: 'Roxo', value: '#7C3AED' },
  { label: 'Rosa', value: '#DB2777' },
];

/**
 * Highlight colours are kept pale so the note's own text stays readable on
 * every paper colour, including the dark one.
 */
export const HIGHLIGHT_COLORS: readonly PaletteEntry[] = [
  { label: 'Sem marca-texto', value: null },
  { label: 'Amarelo', value: '#FDE68A' },
  { label: 'Verde', value: '#BBF7D0' },
  { label: 'Azul', value: '#BFDBFE' },
  { label: 'Rosa', value: '#FBCFE8' },
  { label: 'Roxo', value: '#DDD6FE' },
];
