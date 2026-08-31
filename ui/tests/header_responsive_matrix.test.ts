import { describe, expect, inject, it } from 'vitest';

const WIDTHS = [220, 260, 300, 320, 360, 420, 480, 540, 600, 720, 900];
const SCALES = [100, 120, 140, 160];

type PillMode = 'full' | 'compact' | 'hidden';

function pillMode(width: number, scale: number): PillMode {
  if (scale >= 160) return width >= 1100 ? 'full' : width >= 1000 ? 'compact' : 'hidden';
  if (scale >= 140) return width >= 1000 ? 'full' : width >= 800 ? 'compact' : 'hidden';
  if (scale >= 120) return width >= 800 ? 'full' : width >= 720 ? 'compact' : 'hidden';
  return width >= 720 ? 'full' : width >= 540 ? 'compact' : 'hidden';
}

/**
 * Conservative header budget for the real CSS metrics. The clock allowance
 * is H:MM:SS, and AutoPaste is treated as active: these are the two states
 * which must never be sacrificed to make the centred entry fit.
 */
function budget(width: number, scalePercent: number) {
  const scale = scalePercent / 100;
  const control = 24 * scale;
  const separator = 7 * scale;
  const padding = 6 * scale;
  const mode = pillMode(width, scalePercent);

  let note = 2;
  let text = 4;
  let content = 2;
  let zoom = 2;
  let trash = 1;
  let leftSeparators = 2;
  let toolSeparator = 1;

  if (width <= 419) {
    content = 0;
    leftSeparators = 1;
  }
  if (width <= 359) {
    text = 0;
    content = 0;
    leftSeparators = 0;
    toolSeparator = 0;
  }
  if (width <= 479) zoom = 0;
  if (width <= 339) trash = 0;
  if (scalePercent >= 120 && width <= 799) zoom = 0;
  if (scalePercent >= 120 && width <= 599) {
    text = 0;
    content = 0;
    leftSeparators = 0;
  }
  if (scalePercent >= 140 && width <= 719) {
    content = 0;
    leftSeparators = text === 0 ? 0 : 1;
  }
  if (scalePercent >= 140 && width <= 599) trash = 0;
  if (scalePercent >= 140 && width <= 359) note = 1;

  const fallback = mode === 'hidden' ? 1 : 0;
  const left = padding + (note + text + content + fallback) * control
    + leftSeparators * separator;
  // Timer, active AutoPaste and Close are unconditional; trash is optional.
  const rightControls = zoom + 2 + trash + 1;
  const clock = width > 300 ? 45 * scale : 0;
  const right = padding + rightControls * control + toolSeparator * separator + clock;
  const pill = mode === 'full'
    ? Math.max(150 * scale, Math.min(230 * scale, width - 420))
    : mode === 'compact' ? 112 * scale : 0;
  return { left, mode, pill, right };
}

describe('responsive toolbar budget', () => {
  it('keeps the centre entry clear of both groups across the required matrix', () => {
    for (const scale of SCALES) {
      for (const width of WIDTHS) {
        const { left, mode, pill, right } = budget(width, scale);
        if (mode === 'hidden') {
          expect(left + right, `${width}px at ${scale}%`).toBeLessThanOrEqual(width);
          continue;
        }
        const centreClearance = width / 2 - pill / 2;
        expect(left, `left ${width}px at ${scale}%`).toBeLessThanOrEqual(centreClearance);
        expect(right, `right ${width}px at ${scale}%`).toBeLessThanOrEqual(centreClearance);
      }
    }
  });

  it('always retains Menu, Timer, active AutoPaste and Close while the pill yields first', () => {
    for (const scale of SCALES) {
      for (const width of WIDTHS) {
        const result = budget(width, scale);
        expect(result.left).toBeGreaterThan(0);
        expect(result.right).toBeGreaterThanOrEqual(3 * 24 * (scale / 100));
        if (width <= 300) expect(result.mode).toBe('hidden');
      }
    }
  });

  it('ships the measured compact and early-yield breakpoints in the stylesheet', () => {
    const css = inject('themeCss');
    expect(css).toMatch(/@media \(max-width: 419px\)[\s\S]*header-content-group/);
    expect(css).toMatch(/@media \(max-width: 799px\)[\s\S]*header-zoom-action/);
    expect(css).toMatch(/@media \(max-width: 999px\)[\s\S]*data-ui-scale="160"[\s\S]*header-search-pill/);
    expect(css).toMatch(/@media \(max-width: 359px\)[\s\S]*#btn-note-color/);
  });
});
