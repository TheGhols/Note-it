export const MIN_ZOOM_PERCENT = 75;
export const MAX_ZOOM_PERCENT = 300;
export const ZOOM_STEP_PERCENT = 10;
export const DEFAULT_ZOOM_PERCENT = 100;

/**
 * Keeps a zoom request inside the supported range.
 *
 * Anything that is not a real percentage — NaN, Infinity, a negative value, a
 * string from stored state — falls back to 100% rather than scaling the note
 * by an absurd factor.
 */
export function clampZoom(value: unknown): number {
  const numeric = typeof value === 'string' ? Number(value) : value;
  if (typeof numeric !== 'number' || !Number.isFinite(numeric)) {
    return DEFAULT_ZOOM_PERCENT;
  }
  const rounded = Math.round(numeric);
  return Math.min(MAX_ZOOM_PERCENT, Math.max(MIN_ZOOM_PERCENT, rounded));
}

export function zoomIn(current: number): number {
  return clampZoom(clampZoom(current) + ZOOM_STEP_PERCENT);
}

export function zoomOut(current: number): number {
  return clampZoom(clampZoom(current) - ZOOM_STEP_PERCENT);
}
