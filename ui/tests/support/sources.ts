import { inject } from 'vitest';

declare module 'vitest' {
  export interface ProvidedContext {
    /** Every `src/math/*.ts`, by file name, supplied by the Vitest config. */
    mathSources: Record<string, string>;
  }
}

/**
 * The math engine's own source, read back as data.
 *
 * A test can then assert what the engine does *not* contain — no `eval`, no
 * `Function`, nothing that defers or imports — against the files the
 * application ships rather than against a promise in a comment.
 */
export const MATH_SOURCES: Record<string, string> = inject('mathSources');
