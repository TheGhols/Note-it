import { afterEach, describe, expect, it } from 'vitest';
import {
  applyUiScale,
  clampUiScale,
  DEFAULT_UI_SCALE_PERCENT,
  MAX_UI_SCALE_PERCENT,
  MIN_UI_SCALE_PERCENT,
  UI_SCALE_STEP_PERCENT,
  uiScaleIn,
  uiScaleOut,
} from '../src/ui/uiScale.ts';

afterEach(() => {
  document.documentElement.removeAttribute('style');
  document.body.removeAttribute('data-ui-scale');
});

describe('global interface scale', () => {
  it('has the explicit compact range and deterministic invalid fallback', () => {
    expect(DEFAULT_UI_SCALE_PERCENT).toBe(100);
    expect(MIN_UI_SCALE_PERCENT).toBe(90);
    expect(MAX_UI_SCALE_PERCENT).toBe(160);
    expect(UI_SCALE_STEP_PERCENT).toBe(10);
    for (const value of [Number.NaN, Infinity, -Infinity, 'large', null, undefined]) {
      expect(clampUiScale(value)).toBe(100);
    }
  });

  it('accepts every supported stored value and clamps numeric overflow', () => {
    for (const value of [90, 110, 120, 140, 160]) expect(clampUiScale(value)).toBe(value);
    expect(clampUiScale(0)).toBe(90);
    expect(clampUiScale(10_000)).toBe(160);
    expect(uiScaleOut(90)).toBe(90);
    expect(uiScaleIn(150)).toBe(160);
    expect(uiScaleIn(160)).toBe(160);
  });

  it('changes chrome variables without touching note zoom or document content', () => {
    document.documentElement.style.setProperty('--note-zoom', '2.5');
    const note = document.createElement('p');
    note.textContent = 'Texto da nota';
    document.body.append(note);
    const before = document.body.textContent;

    expect(applyUiScale(document.documentElement, document.body, 140)).toBe(140);
    expect(document.documentElement.style.getPropertyValue('--ui-scale')).toBe('1.4');
    expect(document.body.dataset.uiScale).toBe('140');
    expect(document.documentElement.style.getPropertyValue('--note-zoom')).toBe('2.5');
    expect(document.body.textContent).toBe(before);
  });
});
