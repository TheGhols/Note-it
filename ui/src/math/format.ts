/**
 * How a result is spelled for the reader.
 *
 * The value the engine carries and the value it shows are two different
 * things. Chained calculations use the full binary double — rounding a
 * variable to make it look nice would make everything below it wrong — and
 * only what is drawn beside the line goes through here.
 */

/** Enough to be exact for anything a note computes, few enough to hide the
 * noise binary floating point leaves behind: `0.1 + 0.2` shows as `0,3`. */
const SIGNIFICANT_DIGITS = 12;

/** A result is a number to read, not a measurement to report. */
const MAX_DECIMALS = 10;

/** Above this, `toFixed` switches to exponent form of its own accord. */
const FIXED_LIMIT = 1e21;

/**
 * Formats a result in pt-BR, with a comma for the decimal separator and no
 * thousands separator at all.
 *
 * The missing grouping is deliberate. `.` and `,` are both accepted as decimal
 * separators when reading a number, so printing `109.876.463` would produce a
 * result that this same engine reads back as something else. A result you can
 * copy into the next line is worth more than a result that is easier on the
 * eye.
 */
export function formatNumber(value: number): string {
  if (!Number.isFinite(value)) return '—';

  const precise = Number(value.toPrecision(SIGNIFICANT_DIGITS));
  const normalized = Object.is(precise, -0) ? 0 : precise;

  if (Math.abs(normalized) >= FIXED_LIMIT) {
    return normalized.toString().replace('.', ',');
  }

  if (Number.isInteger(normalized)) return normalized.toFixed(0);

  let text = normalized.toFixed(MAX_DECIMALS).replace(/0+$/, '');
  if (text.endsWith('.')) text = text.slice(0, -1);
  return text.replace('.', ',');
}
