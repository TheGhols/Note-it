/**
 * Everything the math engine can refuse to answer, and the words the note
 * shows for it.
 *
 * There are four, deliberately. A calculation that fails is a discreet note
 * beside the line, not a diagnosis: the reader needs to know which of their
 * lines is not producing a number and roughly why, and nothing else. No
 * internal detail, no offending token, no stack trace ever reaches the note —
 * the strings below are constants, so no note content can be echoed back into
 * the document through an error message.
 */
export type MathErrorCode =
  | 'invalid-expression'
  | 'unknown-variable'
  | 'division-by-zero'
  | 'invalid-name';

const MESSAGES: Record<MathErrorCode, string> = {
  'invalid-expression': 'expressão inválida',
  'unknown-variable': 'variável desconhecida',
  'division-by-zero': 'divisão por zero',
  'invalid-name': 'nome inválido',
};

export function mathErrorMessage(code: MathErrorCode): string {
  return MESSAGES[code];
}

/** A failure the reader is meant to see, as opposed to a defect in this code. */
export class MathError extends Error {
  readonly code: MathErrorCode;

  constructor(code: MathErrorCode) {
    super(MESSAGES[code]);
    this.name = 'MathError';
    this.code = code;
  }
}
