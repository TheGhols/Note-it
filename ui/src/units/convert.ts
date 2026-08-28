import { Unit } from './types.ts';

/**
 * Why a conversion could not be performed, as far as this module is concerned.
 *
 * These are returned rather than thrown, so the caller decides what a reader
 * is told. The math engine maps them onto its own error vocabulary.
 */
export type ConversionFailure = 'incompatible' | 'impossible';

export type ConversionResult =
  | { readonly ok: true; readonly value: number }
  | { readonly ok: false; readonly failure: ConversionFailure };

/** Absolute zero, which is the floor of the temperature dimension. */
const ABSOLUTE_ZERO_KELVIN = 0;

function toBase(unit: Unit, value: number): number {
  return unit.toBase ? unit.toBase(value) : value * unit.scale!;
}

function fromBase(unit: Unit, base: number): number {
  return unit.fromBase ? unit.fromBase(base) : base / unit.scale!;
}

/**
 * Converts a value from one unit to another.
 *
 * Two units, one base, and arithmetic. A linear unit multiplies into the base
 * and divides back out of it; temperature carries its own pair of converters
 * because its scales have different zeroes, and `0 °C` is `32 °F` rather than
 * `0 °F` however hard you multiply.
 *
 * The whole of the conversion happens here. There is no lookup of anything by
 * a string the note supplied, no dynamic property access, and nothing that
 * could reach outside this function: the two units arrive already resolved
 * from the registry, and what they carry is a number or two closures written
 * in `registry.ts`.
 */
export function convertValue(value: number, from: Unit, to: Unit): ConversionResult {
  // Dimensions are static, so this is decided before any arithmetic happens.
  // A kilogram is not a kilometre and no amount of context makes it one.
  if (from.dimension !== to.dimension) return { ok: false, failure: 'incompatible' };

  const base = toBase(from, value);

  // A temperature under absolute zero is not a reading with an unusual sign;
  // it is not a temperature. Converting it would hand the reader a number that
  // cannot exist, dressed as an answer.
  if (from.dimension === 'temperature' && base < ABSOLUTE_ZERO_KELVIN) {
    return { ok: false, failure: 'impossible' };
  }

  const converted = fromBase(to, base);
  if (!Number.isFinite(converted)) return { ok: false, failure: 'impossible' };

  return { ok: true, value: converted === 0 ? 0 : converted };
}

/*
 * The boundary this module is on one side of.
 *
 * Everything in `registry.ts` is deterministic and offline. A kilometre is a
 * thousand metres on a machine that has never had a network interface, it was
 * a thousand metres last year, and a note written today converts identically
 * when it is reopened in ten years. That property is what makes it safe to
 * compute a conversion silently, as a decoration, with no cache and no
 * staleness to reason about.
 *
 * A currency has none of those properties. `USD em BRL` has no answer without
 * a rate, the rate is different every minute, and a rate written into this
 * table would be wrong before the commit that added it finished pushing. The
 * roadmap therefore keeps currencies for a later phase, "with the external
 * dependency isolated behind a boundary so the rest of the application never
 * depends on the network being there".
 *
 * That boundary is this module edge, and honouring it now costs nothing:
 *
 * - `Dimension` in `types.ts` lists only quantities that are constants. A
 *   currency is not one, and adding `currency` to that union is the change
 *   that must not be made quietly.
 * - `convertValue` is synchronous and total. It is handed two resolved units
 *   and returns an answer; it never waits, never fails to be reachable, and
 *   never needs a fallback for "the rate is not in yet". A rate-backed
 *   conversion is none of those things, so it does not belong in this
 *   function — it belongs behind an asynchronous provider of its own, with its
 *   own staleness, its own failure state and its own way of telling the reader
 *   how old the number is.
 * - The math engine resolves units at parse time, so a dimension that cannot
 *   answer synchronously would announce itself immediately rather than being
 *   discovered halfway through the editor's render.
 *
 * No provider interface is written here, because there is nothing behind it
 * yet and an empty abstraction is a worse guide to the future than a plain
 * statement of what the future has to look like. What this phase owes the next
 * one is the absence of a hardcoded rate, and that is what it delivers.
 */
