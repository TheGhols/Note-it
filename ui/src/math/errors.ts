/**
 * Everything the math engine can refuse to answer, and the words the note
 * shows for it.
 *
 * There are seven, and each one tells the reader which of their lines is not
 * producing a number and roughly why. Nothing else: no internal detail, no
 * offending token, no stack trace ever reaches the note — the strings below
 * are constants, so no note content can be echoed back into the document
 * through an error message.
 *
 * The last three arrived with conversions. They are separate codes rather than
 * one because they call for three different corrections: a spelling nobody
 * recognises, two units that were never comparable, and a conversion that is
 * well formed but has no answer.
 */
export type MathErrorCode =
  | 'invalid-expression'
  | 'unknown-variable'
  | 'division-by-zero'
  | 'invalid-name'
  | 'unknown-unit'
  | 'incompatible-units'
  | 'invalid-conversion';

const MESSAGES: Record<MathErrorCode, string> = {
  'invalid-expression': 'expressão inválida',
  'unknown-variable': 'variável desconhecida',
  'division-by-zero': 'divisão por zero',
  'invalid-name': 'nome inválido',
  'unknown-unit': 'unidade desconhecida',
  'incompatible-units': 'unidades incompatíveis',
  'invalid-conversion': 'conversão inválida',
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
