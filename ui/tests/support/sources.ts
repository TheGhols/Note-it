import { inject } from 'vitest';

declare module 'vitest' {
  export interface ProvidedContext {
    /** Every `src/math/*.ts` and `src/units/*.ts`, by path, supplied by the
     *  Vitest config. */
    mathSources: Record<string, string>;
    /** `docs/features.md`, supplied by the Vitest config. */
    featuresDoc: string;
  }
}

/**
 * The calculating engine's own source, read back as data.
 *
 * A test can then assert what the engine does *not* contain — no `eval`, no
 * `Function`, nothing that defers or imports, nothing that reaches the network
 * — against the files the application ships rather than against a promise in a
 * comment. Both the math modules and the unit registry are covered, because a
 * unit table is exactly the kind of file where a convenient dynamic lookup or
 * a "just fetch the rate" would one day look reasonable.
 */
export const MATH_SOURCES: Record<string, string> = inject('mathSources');

/**
 * The feature documentation, read back as data.
 *
 * The unit table is written out in `docs/features.md` because "supports unit
 * conversion" tells a reader nothing they can act on. A documented table that
 * drifts from the real one is worse than none, so the two are compared.
 */
export const FEATURES_DOC: string = inject('featuresDoc');
