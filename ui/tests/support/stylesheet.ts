import { inject } from 'vitest';

declare module 'vitest' {
  export interface ProvidedContext {
    /** `src/styles/theme.css`, supplied by the Vitest config. */
    themeCss: string;
    /** The real header markup shipped from `index.html`. */
    indexHtml: string;
    /** `index.html` after the build has written the icons into it. */
    renderedHtml: string;
    /** The chosen icon files, by quick-action id. */
    quickActionIcons: Record<string, string>;
    /** The repository's `.gitignore`, so the shipped assets can be checked. */
    gitignore: string;
    /** `MIN_NOTE_WIDTH` from `src/layer_shell.rs`: the narrowest a note can be. */
    minNoteWidth: number;
  }
}

const CSS = inject('themeCss');

/**
 * The stylesheet, read back as data.
 *
 * Every visual number — pattern spacing, pattern opacity, chrome colours —
 * is defined once in `theme.css`. The tests parse that file rather than
 * keeping a second copy, so an assertion can never pass against a value the
 * application no longer paints.
 */
export interface CssRule {
  selectors: string[];
  body: string;
}

function parse(css: string): CssRule[] {
  const rules: CssRule[] = [];
  const pattern = /([^{}]+)\{([^}]*)\}/g;
  const withoutComments = css.replace(/\/\*[\s\S]*?\*\//g, '');
  let match = pattern.exec(withoutComments);
  while (match !== null) {
    rules.push({
      selectors: match[1].split(',').map((selector) => selector.trim()).filter(Boolean),
      body: match[2],
    });
    match = pattern.exec(withoutComments);
  }
  return rules;
}

export const RULES: CssRule[] = parse(CSS);

/** Every rule whose selector list contains `selector`. */
export function rulesFor(selector: string): CssRule[] {
  return RULES.filter((rule) => rule.selectors.includes(selector));
}

export function ruleFor(selector: string): CssRule {
  const [rule] = rulesFor(selector);
  if (!rule) throw new Error(`no rule for ${selector}`);
  return rule;
}

/** Every custom property the rule for `selector` defines. */
export function tokensIn(selector: string): Map<string, string> {
  const tokens = new Map<string, string>();
  const pattern = /(--[a-z-]+)\s*:\s*([^;]+);/g;
  const { body } = ruleFor(selector);
  let match = pattern.exec(body);
  while (match !== null) {
    tokens.set(match[1], match[2].trim());
    match = pattern.exec(body);
  }
  return tokens;
}

export function tokenIn(selector: string, property: string): string {
  const value = tokensIn(selector).get(property);
  if (value === undefined) throw new Error(`${selector} does not set ${property}`);
  return value;
}

export function numberIn(selector: string, property: string): number {
  return Number.parseFloat(tokenIn(selector, property));
}

/**
 * An ordinary declaration of the rule for `selector`.
 *
 * [`tokenIn`] reads custom properties; this reads the properties they are
 * spent on, so a test can check what a rule actually paints and not only what
 * the palette holds.
 */
export function declarationIn(selector: string, property: string): string {
  const { body } = ruleFor(selector);
  const pattern = new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]+)`, 'i');
  const match = pattern.exec(body);
  if (!match) throw new Error(`${selector} does not set ${property}`);
  return match[1].trim();
}

/** Every declaration of a custom property whose name starts with `prefix`. */
export function declarationsOf(prefix: string): Array<{ name: string; value: string }> {
  const declarations: Array<{ name: string; value: string }> = [];
  const pattern = new RegExp(`(${prefix}[a-z-]*)\\s*:\\s*([^;]+);`, 'g');
  for (const rule of RULES) {
    let match = pattern.exec(rule.body);
    while (match !== null) {
      declarations.push({ name: match[1], value: match[2].trim() });
      match = pattern.exec(rule.body);
    }
  }
  return declarations;
}
