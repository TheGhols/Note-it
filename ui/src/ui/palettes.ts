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

/**
 * Text colours, each dark enough to stay readable on every paper colour and on
 * every highlight in the palette below. Orange, yellow and green were darkened
 * for that reason: the lighter shades dropped under a readable contrast on the
 * pale highlights, and on the yellow paper.
 */
export const TEXT_COLORS: readonly PaletteEntry[] = [
  { label: 'Padrão', value: null },
  { label: 'Cinza', value: '#64748B' },
  { label: 'Vermelho', value: '#DC2626' },
  { label: 'Laranja', value: '#C2410C' },
  { label: 'Amarelo', value: '#A16207' },
  { label: 'Verde', value: '#15803D' },
  { label: 'Azul', value: '#2563EB' },
  { label: 'Roxo', value: '#7C3AED' },
  { label: 'Rosa', value: '#DB2777' },
];

/**
 * Foreground for highlighted text.
 *
 * Every highlight in the palette is pale, so highlighted text needs a dark
 * foreground whatever the paper colour is: on the dark paper the light default
 * text would otherwise sit on a pale highlight and be unreadable. Applied by
 * the highlight mark itself rather than by a stylesheet rule, because the mark
 * carries an inline style and an inline style always wins the cascade.
 */
export const HIGHLIGHT_TEXT_COLOR = '#1E293B';

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
