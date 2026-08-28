/**
 * What a unit is, before any of it is written down.
 *
 * This module knows nothing about parsing, about ProseMirror, or about notes.
 * It is the vocabulary the registry is written in and the conversion is
 * performed with, and that separation is the point: a unit table is data, and
 * data that has to know about an editor is a table nobody can check.
 */

/**
 * The kinds of quantity Note-it converts.
 *
 * A conversion is only ever performed between two units of the same dimension.
 * There is deliberately no dimensional algebra here — no `length / time`
 * derived on the fly — because that is a physics library, not a note-taking
 * application. `speed` is a dimension in its own right with three members, and
 * if it ever needs a fourth that is one row in a table.
 *
 * Every dimension here is **deterministic and offline**: a metre is a metre
 * without asking anyone. See `convert.ts` for why a currency is not, and why
 * one must never be added to this list.
 */
export type Dimension =
  | 'length'
  | 'mass'
  | 'volume'
  | 'temperature'
  | 'time'
  | 'area'
  | 'digital'
  | 'speed';

/**
 * A unit, as one row of the registry.
 *
 * Most units are linear: a value is carried to the dimension's base unit by
 * multiplying, and back by dividing. `scale` says by how much, and that single
 * number is the whole conversion.
 *
 * Temperature is not linear — its scales have different zeroes — so those rows
 * carry `toBase` and `fromBase` instead. Nothing else in the engine has to
 * know which kind a unit is; `convert.ts` asks the row.
 */
export interface Unit {
  /** The canonical spelling, and the key the registry is keyed by. */
  readonly id: string;
  /** How a result is written: `m`, `°C`, `cm²`. */
  readonly symbol: string;
  /**
   * The spelling used when the displayed value is not exactly one.
   *
   * Only the two units whose natural Portuguese name is a word rather than a
   * symbol have this: `1 dia` and `7 dias` both have to read properly, and
   * `7 d` does not.
   */
  readonly plural?: string;
  readonly dimension: Dimension;
  /** Every other spelling accepted for this unit, matched exactly. */
  readonly aliases: readonly string[];
  /** Linear units: `base = value * scale`. Mutually exclusive with the pair below. */
  readonly scale?: number;
  readonly toBase?: (value: number) => number;
  readonly fromBase?: (value: number) => number;
}
