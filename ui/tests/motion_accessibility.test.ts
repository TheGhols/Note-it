import { describe, expect, inject, it } from 'vitest';
import { tokenIn } from './support/stylesheet.ts';

describe('short accessible motion', () => {
  it('uses one bounded token set and no continuous animation', () => {
    expect(tokenIn(':root', '--motion-fast')).toBe('100ms');
    expect(tokenIn(':root', '--motion-normal')).toBe('150ms');
    expect(tokenIn(':root', '--motion-panel')).toBe('180ms');
    expect(tokenIn(':root', '--motion-ease')).toContain('cubic-bezier');
    expect(inject('themeCss')).not.toMatch(/animation(?:-iteration-count)?\s*:[^;]*(?:infinite|loop)/i);
  });

  it('animates only open surfaces and leaves hidden as the immediate pointer authority', () => {
    const css = inject('themeCss');
    expect(css).toContain('.note-menu:not([hidden])');
    expect(css).toContain('.note-study-hub:not([hidden])');
    expect(css).toContain('.note-search:not([hidden])');
    expect(css).toMatch(/\.editor-wrapper\s*\{[\s\S]*transition:/);
    expect(css).toMatch(/\[hidden\][^{]*\{[^}]*display:\s*none/);
  });

  it('makes reduced motion immediate for panels, buttons and collapse content', () => {
    const css = inject('themeCss');
    const reduced = css.match(/@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*)\}\s*$/)?.[1];
    expect(reduced).toBeDefined();
    expect(reduced).toContain('animation: none !important');
    expect(reduced).toContain('transition: none !important');
    expect(reduced).toContain('transform: none !important');
  });
});
