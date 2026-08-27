import { ThemePreference } from '../bridge/types.ts';

/**
 * The application's interface theme.
 *
 * A theme dresses Note-it's own chrome — menus, popovers, borders, focus
 * rings, auxiliary text — and never the paper a note is written on. A yellow
 * note stays yellow under the dark theme and a black one stays black under the
 * light one, because the paper is a property of the note and the theme is a
 * property of the application.
 */
export type ResolvedTheme = 'light' | 'dark';

export interface ThemeOption {
  id: ThemePreference;
  label: string;
}

export const DEFAULT_THEME: ThemePreference = 'system';

export const THEMES: readonly ThemeOption[] = [
  { id: 'system', label: 'Sistema' },
  { id: 'light', label: 'Claro' },
  { id: 'dark', label: 'Escuro' },
];

const DARK_SCHEME_QUERY = '(prefers-color-scheme: dark)';

/** Resolves a stored preference, falling back to following the environment. */
export function normalizeTheme(value: unknown): ThemePreference {
  return THEMES.some((option) => option.id === value)
    ? (value as ThemePreference)
    : DEFAULT_THEME;
}

export function themeLabel(theme: ThemePreference): string {
  return THEMES.find((option) => option.id === theme)?.label ?? theme;
}

/**
 * Turns a preference into the theme actually painted. `system` is the only one
 * that depends on anything outside the application.
 */
export function resolveTheme(preference: ThemePreference, prefersDark: boolean): ResolvedTheme {
  if (preference === 'light') return 'light';
  if (preference === 'dark') return 'dark';
  return prefersDark ? 'dark' : 'light';
}

/**
 * Keeps the document in step with the chosen theme.
 *
 * Two attributes are written: the preference, which is what the user picked
 * and what the menu marks, and the resolved theme, which is what the
 * stylesheet selects on. Under `system` the environment is watched live, so a
 * desktop switching to dark reaches an open note without a restart.
 *
 * `matchMedia` is optional throughout: a WebView that does not report a colour
 * scheme resolves `system` to the light theme rather than failing.
 */
export class ThemeController {
  private readonly query: MediaQueryList | null;
  private preferenceValue: ThemePreference = DEFAULT_THEME;
  private readonly listener = (): void => this.apply();

  public constructor(
    private readonly root: HTMLElement,
    view: Window = window,
  ) {
    this.query = view.matchMedia ? view.matchMedia(DARK_SCHEME_QUERY) : null;
    this.query?.addEventListener?.('change', this.listener);
    this.apply();
  }

  public setPreference(preference: ThemePreference): void {
    this.preferenceValue = normalizeTheme(preference);
    this.apply();
  }

  public preference(): ThemePreference {
    return this.preferenceValue;
  }

  public resolved(): ResolvedTheme {
    return resolveTheme(this.preferenceValue, this.query?.matches === true);
  }

  public destroy(): void {
    this.query?.removeEventListener?.('change', this.listener);
  }

  private apply(): void {
    this.root.setAttribute('data-theme-preference', this.preferenceValue);
    this.root.setAttribute('data-theme', this.resolved());
  }
}
