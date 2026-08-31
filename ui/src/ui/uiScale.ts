export const MIN_UI_SCALE_PERCENT = 90;
export const MAX_UI_SCALE_PERCENT = 160;
export const UI_SCALE_STEP_PERCENT = 10;
export const DEFAULT_UI_SCALE_PERCENT = 100;

/** Normalizes host, config and test inputs to the supported global scale. */
export function clampUiScale(value: unknown): number {
  const numeric = typeof value === 'string' ? Number(value) : value;
  if (typeof numeric !== 'number' || !Number.isFinite(numeric)) {
    return DEFAULT_UI_SCALE_PERCENT;
  }
  const rounded = Math.round(numeric);
  return Math.min(MAX_UI_SCALE_PERCENT, Math.max(MIN_UI_SCALE_PERCENT, rounded));
}

export function uiScaleIn(current: number): number {
  return clampUiScale(clampUiScale(current) + UI_SCALE_STEP_PERCENT);
}

export function uiScaleOut(current: number): number {
  return clampUiScale(clampUiScale(current) - UI_SCALE_STEP_PERCENT);
}

/**
 * Applies real layout metrics through CSS variables. It never scales the
 * document tree as painted pixels, so hit testing and window coordinates stay
 * in agreement with what the reader sees.
 */
export function applyUiScale(root: HTMLElement, body: HTMLElement, value: unknown): number {
  const percent = clampUiScale(value);
  root.style.setProperty('--ui-scale', String(percent / 100));
  body.dataset.uiScale = String(percent);
  return percent;
}
