const UNKNOWN_TIMESTAMP = '—';

function pad(value: number, length = 2): string {
  return String(value).padStart(length, '0');
}

/**
 * Formats an ISO timestamp as pt-BR `dd/MM/aaaa HH:mm` in the local time zone.
 *
 * The layout is written out instead of delegated to `Intl` so the note header
 * never falls back to a US ordering when the runtime locale data differs.
 * Returns an em dash when the timestamp is missing or unparseable, so an
 * unknown date is shown as unknown rather than as an invented one.
 */
export function formatNoteTimestamp(value: string | null | undefined): string {
  if (!value) return UNKNOWN_TIMESTAMP;

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return UNKNOWN_TIMESTAMP;

  const day = pad(date.getDate());
  const month = pad(date.getMonth() + 1);
  const year = pad(date.getFullYear(), 4);
  const hours = pad(date.getHours());
  const minutes = pad(date.getMinutes());

  return `${day}/${month}/${year} ${hours}:${minutes}`;
}

export { UNKNOWN_TIMESTAMP };
