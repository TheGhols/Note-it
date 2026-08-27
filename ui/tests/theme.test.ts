import { afterEach, describe, expect, it, vi } from 'vitest';
import { ThemePreference } from '../src/bridge/types.ts';
import {
  DEFAULT_THEME,
  normalizeTheme,
  resolveTheme,
  ThemeController,
  themeLabel,
  THEMES,
} from '../src/ui/theme.ts';
import { contrastRatio } from './support/color.ts';
import { ruleFor, tokensIn } from './support/stylesheet.ts';

/** A window whose colour-scheme preference the test controls. */
function fakeView(prefersDark: boolean) {
  const listeners: Array<() => void> = [];
  const query = {
    matches: prefersDark,
    addEventListener: vi.fn((_: string, listener: () => void) => listeners.push(listener)),
    removeEventListener: vi.fn((_: string, listener: () => void) => {
      const index = listeners.indexOf(listener);
      if (index !== -1) listeners.splice(index, 1);
    }),
  };
  const view = { matchMedia: vi.fn(() => query) } as unknown as Window;
  return {
    view,
    query,
    /** Simulates the desktop switching its colour scheme. */
    setPrefersDark(value: boolean) {
      query.matches = value;
      for (const listener of [...listeners]) listener();
    },
    listenerCount: () => listeners.length,
  };
}

describe('theme vocabulary', () => {
  it('offers exactly system, light and dark, labelled in Portuguese', () => {
    expect(THEMES.map((option) => option.id)).toEqual(['system', 'light', 'dark']);
    expect(THEMES.map((option) => option.label)).toEqual(['Sistema', 'Claro', 'Escuro']);
    for (const option of THEMES) {
      expect(themeLabel(option.id)).toBe(option.label);
    }
  });

  it('defaults to following the environment', () => {
    expect(DEFAULT_THEME).toBe('system');
    for (const unknown of ['', 'solarized', 'DARK', 'light ', 7, null, undefined, {}]) {
      expect(normalizeTheme(unknown)).toBe('system');
    }
    for (const option of THEMES) {
      expect(normalizeTheme(option.id)).toBe(option.id);
    }
  });

  it('resolves an explicit choice without consulting the environment', () => {
    for (const prefersDark of [true, false]) {
      expect(resolveTheme('light', prefersDark)).toBe('light');
      expect(resolveTheme('dark', prefersDark)).toBe('dark');
    }
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });
});

describe('ThemeController', () => {
  let active: ThemeController | null = null;

  afterEach(() => {
    active?.destroy();
    active = null;
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.removeAttribute('data-theme-preference');
  });

  it('records the preference and the theme actually painted', () => {
    const environment = fakeView(true);
    const root = document.documentElement;
    active = new ThemeController(root, environment.view);

    for (const [preference, resolved] of [
      ['system', 'dark'],
      ['light', 'light'],
      ['dark', 'dark'],
    ] as Array<[ThemePreference, string]>) {
      active.setPreference(preference);
      expect(root.getAttribute('data-theme-preference')).toBe(preference);
      expect(root.getAttribute('data-theme')).toBe(resolved);
      expect(active.preference()).toBe(preference);
    }
  });

  it('follows the desktop switching scheme, but only under "Sistema"', () => {
    const environment = fakeView(false);
    const root = document.documentElement;
    active = new ThemeController(root, environment.view);

    active.setPreference('system');
    expect(root.getAttribute('data-theme')).toBe('light');

    environment.setPrefersDark(true);
    expect(root.getAttribute('data-theme')).toBe('dark');

    // An explicit choice is the user's, and the environment stops mattering.
    active.setPreference('light');
    environment.setPrefersDark(false);
    expect(root.getAttribute('data-theme')).toBe('light');
    environment.setPrefersDark(true);
    expect(root.getAttribute('data-theme')).toBe('light');
  });

  it('paints a theme even where no colour scheme is reported', () => {
    // A WebView without `matchMedia` must still be fully styled rather than
    // left with no theme attribute at all.
    const root = document.createElement('div');
    const controller = new ThemeController(root, {} as Window);

    expect(root.getAttribute('data-theme')).toBe('light');
    controller.setPreference('dark');
    expect(root.getAttribute('data-theme')).toBe('dark');
    controller.setPreference('system');
    expect(root.getAttribute('data-theme')).toBe('light');
    controller.destroy();
  });

  it('stops watching the environment once destroyed', () => {
    const environment = fakeView(false);
    const controller = new ThemeController(document.createElement('div'), environment.view);
    expect(environment.listenerCount()).toBe(1);
    controller.destroy();
    expect(environment.listenerCount()).toBe(0);
  });

  it('normalizes whatever the host sends', () => {
    const environment = fakeView(false);
    const root = document.createElement('div');
    active = new ThemeController(root, environment.view);

    active.setPreference('nonsense' as ThemePreference);
    expect(active.preference()).toBe('system');
    expect(root.getAttribute('data-theme-preference')).toBe('system');
  });
});

describe('the chrome the theme dresses', () => {
  const CHROME_TOKENS = [
    '--ui-surface',
    '--ui-surface-hover',
    '--ui-text',
    '--ui-text-muted',
    '--ui-border',
    '--ui-shadow',
    '--ui-focus-ring',
  ];

  it('defines the whole light palette on :root, so an unthemed page is painted', () => {
    const light = tokensIn(':root');
    for (const token of CHROME_TOKENS) {
      expect(light.has(token), token).toBe(true);
    }
  });

  it('redefines exactly the same tokens for the dark theme', () => {
    const dark = tokensIn(':root[data-theme="dark"]');
    expect([...dark.keys()].sort()).toEqual([...CHROME_TOKENS].sort());
  });

  it('never touches the paper, so a note keeps the colour it was given', () => {
    // This is the whole separation: a yellow note stays yellow under the dark
    // theme, and a black one stays black under the light theme.
    const dark = ruleFor(':root[data-theme="dark"]').body;
    for (const paperToken of ['--paper-bg', '--paper-text', '--paper-muted', '--selection-bg']) {
      expect(dark, paperToken).not.toContain(paperToken);
    }
    expect(dark).not.toContain('--paper-pattern');
  });

  it('keeps menu text readable on its own surface, in both themes', () => {
    for (const selector of [':root', ':root[data-theme="dark"]']) {
      const tokens = tokensIn(selector);
      const surface = tokens.get('--ui-surface')!;
      expect(contrastRatio(tokens.get('--ui-text')!, surface), selector).toBeGreaterThanOrEqual(
        4.5,
      );
      // Auxiliary text is smaller but still has to be read.
      expect(
        contrastRatio(tokens.get('--ui-text-muted')!, surface),
        selector,
      ).toBeGreaterThanOrEqual(3);
      // The focus ring has to be findable against the surface it sits on.
      expect(
        contrastRatio(tokens.get('--ui-focus-ring')!, surface),
        selector,
      ).toBeGreaterThanOrEqual(3);
    }
  });

  it('dresses the popovers from the theme rather than from the paper', () => {
    // The defect this prevents: a popover coloured from the note would take a
    // dark surface with the yellow paper's dark text, and be unreadable.
    for (const selector of ['.note-menu', '.note-tooltip']) {
      const body = ruleFor(selector).body;
      expect(body, selector).toContain('var(--ui-surface)');
      expect(body, selector).toContain('var(--ui-text)');
      expect(body, selector).not.toContain('var(--paper-');
    }
  });

  it('leaves the note content and the header on the paper', () => {
    // The theme stops at the chrome: everything drawn on the paper keeps
    // taking its colour from the paper.
    for (const selector of ['.icon-btn', 'body']) {
      expect(ruleFor(selector).body, selector).toContain('var(--paper-');
    }
    expect(ruleFor('#app').body).toContain('var(--paper-bg)');
  });
});
