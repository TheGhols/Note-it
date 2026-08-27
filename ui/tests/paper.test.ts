import { afterEach, describe, expect, it } from 'vitest';
import { PaperIntensity, PaperType } from '../src/bridge/types.ts';
import {
  applyPaper,
  DEFAULT_PAPER_INTENSITY,
  DEFAULT_PAPER_TYPE,
  normalizePaperIntensity,
  normalizePaperType,
  paperIntensityLabel,
  paperTypeLabel,
  PAPER_INTENSITIES,
  PAPER_TYPES,
} from '../src/ui/paper.ts';
import {
  composite,
  contrastRatio,
  hexToRgb,
  lightness,
  Rgb,
} from './support/color.ts';
import {
  declarationsOf,
  numberIn,
  ruleFor,
  rulesFor,
  RULES,
  tokenIn,
} from './support/stylesheet.ts';

const PAPER_COLORS = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'] as const;

/** Rules painting the given paper type's surface. */
function patternRulesFor(type: string) {
  return rulesFor(`body[data-paper="${type}"] .editor-wrapper`);
}

function patternAlpha(paper: string, intensity: PaperIntensity): number {
  const base = numberIn(`body[data-paper-intensity="${intensity}"]`, '--paper-pattern-alpha');
  const gain =
    paper === 'black'
      ? numberIn('body[data-color="black"]', '--paper-pattern-gain')
      : numberIn(':root', '--paper-pattern-gain');
  return base * gain;
}

function patternInk(paper: string): Rgb {
  const raw =
    paper === 'black'
      ? tokenIn('body[data-color="black"]', '--paper-pattern-ink')
      : tokenIn(':root', '--paper-pattern-ink');
  const channels = raw.split(',').map((part) => Number.parseInt(part.trim(), 10));
  return [channels[0], channels[1], channels[2]];
}

function paperBackground(paper: string): Rgb {
  return hexToRgb(tokenIn(`body[data-color="${paper}"]`, '--paper-bg'));
}

function paperText(paper: string): Rgb {
  return hexToRgb(tokenIn(`body[data-color="${paper}"]`, '--paper-text'));
}

describe('paper vocabulary', () => {
  it('offers exactly the five papers, labelled in Portuguese', () => {
    expect(PAPER_TYPES.map((option) => option.id)).toEqual([
      'blank',
      'lined',
      'dotted',
      'grid-small',
      'grid-large',
    ]);
    expect(PAPER_TYPES.map((option) => option.label)).toEqual([
      'Liso',
      'Pautado',
      'Pontilhado',
      'Quadriculado pequeno',
      'Quadriculado grande',
    ]);
  });

  it('offers exactly the three intensities, labelled in Portuguese', () => {
    expect(PAPER_INTENSITIES.map((option) => option.id)).toEqual([
      'subtle',
      'normal',
      'strong',
    ]);
    expect(PAPER_INTENSITIES.map((option) => option.label)).toEqual([
      'Suave',
      'Normal',
      'Forte',
    ]);
  });

  it('defaults a note with no paper of its own to plain paper', () => {
    expect(DEFAULT_PAPER_TYPE).toBe('blank');
    expect(DEFAULT_PAPER_INTENSITY).toBe('normal');
    // What a note written before the paper existed carries: nothing.
    expect(normalizePaperType(undefined)).toBe('blank');
    expect(normalizePaperIntensity(undefined)).toBe('normal');
  });

  it('degrades an unknown or hand-edited value instead of failing', () => {
    for (const unknown of ['', 'hexagonal', 'GRID-SMALL', 'lined ', 42, null, {}]) {
      expect(normalizePaperType(unknown)).toBe(DEFAULT_PAPER_TYPE);
      expect(normalizePaperIntensity(unknown)).toBe(DEFAULT_PAPER_INTENSITY);
    }
  });

  it('keeps every supported value untouched', () => {
    for (const option of PAPER_TYPES) {
      expect(normalizePaperType(option.id)).toBe(option.id);
      expect(paperTypeLabel(option.id)).toBe(option.label);
    }
    for (const option of PAPER_INTENSITIES) {
      expect(normalizePaperIntensity(option.id)).toBe(option.id);
      expect(paperIntensityLabel(option.id)).toBe(option.label);
    }
  });
});

describe('applying a paper to the page', () => {
  afterEach(() => {
    document.body.removeAttribute('data-paper');
    document.body.removeAttribute('data-paper-intensity');
    document.body.removeAttribute('data-color');
  });

  it('drives the stylesheet from two attributes and nothing else', () => {
    for (const type of PAPER_TYPES) {
      for (const intensity of PAPER_INTENSITIES) {
        applyPaper(document.body, type.id, intensity.id);
        expect(document.body.getAttribute('data-paper')).toBe(type.id);
        expect(document.body.getAttribute('data-paper-intensity')).toBe(intensity.id);
        // No inline styling: the pattern is a stylesheet concern throughout.
        expect(document.body.getAttribute('style')).toBeNull();
      }
    }
  });

  it('keeps the intensity when the paper becomes plain again', () => {
    applyPaper(document.body, 'grid-large', 'strong');
    applyPaper(document.body, 'blank', 'strong');

    // Plain paper has no pattern for the intensity to act on, but the choice
    // survives, so going back to a grid does not silently reset it.
    expect(document.body.getAttribute('data-paper')).toBe('blank');
    expect(document.body.getAttribute('data-paper-intensity')).toBe('strong');
  });

  it('leaves the note colour alone', () => {
    document.body.setAttribute('data-color', 'black');
    applyPaper(document.body, 'lined', 'subtle');
    expect(document.body.getAttribute('data-color')).toBe('black');
  });

  it('gives three notes three independent papers', () => {
    const notes = ['a', 'b', 'c'].map(() => document.createElement('div'));
    const chosen: Array<[PaperType, PaperIntensity]> = [
      ['lined', 'subtle'],
      ['dotted', 'normal'],
      ['grid-large', 'strong'],
    ];
    notes.forEach((note, index) => applyPaper(note, ...chosen[index]));

    // Changing the first note reaches only the first note.
    applyPaper(notes[0], 'grid-small', 'strong');

    expect(notes.map((note) => note.getAttribute('data-paper'))).toEqual([
      'grid-small',
      'dotted',
      'grid-large',
    ]);
    expect(notes.map((note) => note.getAttribute('data-paper-intensity'))).toEqual([
      'strong',
      'normal',
      'strong',
    ]);
  });
});

describe('the stylesheet the paper drives', () => {
  it('draws a pattern for every type except plain paper', () => {
    for (const type of PAPER_TYPES) {
      const draws = patternRulesFor(type.id).some((rule) =>
        rule.body.includes('background-image'),
      );
      // Plain paper declares nothing at all, which is what notes had before.
      expect(draws, type.id).toBe(type.id !== 'blank');
    }
  });

  it('draws both grids from one shared rule rather than two copies', () => {
    const shared = patternRulesFor('grid-small').find((rule) =>
      rule.body.includes('background-image'),
    );
    expect(shared?.selectors).toContain('body[data-paper="grid-large"] .editor-wrapper');
  });

  it('gives each pattern its own spacing, in pixels so zoom cannot move it', () => {
    const spacing: Record<string, number> = {};
    for (const type of PAPER_TYPES) {
      if (type.id === 'blank') continue;
      const raw = tokenIn(`body[data-paper="${type.id}"]`, '--paper-pattern-spacing');
      expect(raw, type.id).toMatch(/^\d+(\.\d+)?px$/);
      spacing[type.id] = Number.parseFloat(raw);
    }

    // The ruled spacing follows the note's default line box, 15px at 1.55.
    expect(spacing.lined).toBe(24);
    expect(spacing['grid-large']).toBeGreaterThan(spacing['grid-small']);
  });

  it('scales nothing with the editor font size or the view zoom', () => {
    // A pattern tied to either would blur or explode as the view is zoomed.
    for (const { name, value } of declarationsOf('--paper-pattern-')) {
      expect(value, name).not.toContain('--note-zoom');
      expect(value, name).not.toContain('--note-font-size');
      expect(value, name).not.toMatch(/\d\s*(em|rem)\b/);
    }
  });

  it('paints the pattern on the scrolling surface so it travels with the text', () => {
    expect(ruleFor('.editor-wrapper').body).toContain('background-attachment: local');
  });

  it('composes the pattern colour where its inputs are already final', () => {
    // The defect this covers is a cascade one, not an arithmetic one.
    // `var()` is substituted where the declaration sits, using that element's
    // own values. Composing the colour on `:root` froze the root's ink and
    // opacity into it, so every intensity painted at "normal" and the dark
    // paper painted with the *light* papers' dark ink — invisible on #18181B.
    const composing = RULES.filter((rule) => rule.body.includes('--paper-pattern-color:'));
    expect(composing).toHaveLength(1);

    const [rule] = composing;
    // It must sit on the element that paints the pattern, below every
    // override, never on the root above them.
    expect(rule.selectors).toEqual(['.editor-wrapper']);
    for (const selector of rule.selectors) {
      expect(selector).not.toBe(':root');
    }

    // And every override really does live further down the tree than it.
    const overriding = RULES.filter((candidate) =>
      /--paper-pattern-(ink|gain|alpha)\s*:/.test(candidate.body),
    ).flatMap((candidate) => candidate.selectors);
    expect(overriding).toContain('body[data-color="black"]');
    expect(overriding).toContain('body[data-paper-intensity="subtle"]');
  });

  it('orders the three intensities from faint to firm', () => {
    const alphas = PAPER_INTENSITIES.map((option) =>
      numberIn(`body[data-paper-intensity="${option.id}"]`, '--paper-pattern-alpha'),
    );
    expect(alphas[0]).toBeLessThan(alphas[1]);
    expect(alphas[1]).toBeLessThan(alphas[2]);
    // Still a background at its firmest: never an opaque overlay.
    expect(alphas[2]).toBeLessThan(0.35);
  });

  it('changes only the pattern, never the paper colour or the text', () => {
    for (const option of PAPER_INTENSITIES) {
      const properties = (
        ruleFor(`body[data-paper-intensity="${option.id}"]`).body.match(/[a-z-]+\s*:/g) ?? []
      ).map((property) => property.replace(':', '').trim());
      expect(properties, option.id).toEqual(['--paper-pattern-alpha']);
    }
  });
});

describe('pattern contrast on every paper', () => {
  it('is visible on all seven papers, at all three intensities', () => {
    for (const paper of PAPER_COLORS) {
      const background = paperBackground(paper);
      for (const intensity of PAPER_INTENSITIES) {
        const line = composite(
          patternInk(paper),
          patternAlpha(paper, intensity.id),
          background,
        );
        const delta = Math.abs(lightness(line) - lightness(background));
        expect(delta, `${paper} / ${intensity.id}`).toBeGreaterThan(3);
      }
    }
  });

  it('never competes with the note’s own text', () => {
    for (const paper of PAPER_COLORS) {
      const background = paperBackground(paper);
      const textContrast = contrastRatio(paperText(paper), background);
      for (const intensity of PAPER_INTENSITIES) {
        const line = composite(
          patternInk(paper),
          patternAlpha(paper, intensity.id),
          background,
        );
        const patternContrast = contrastRatio(line, background);
        // A decorative rule needs no text-level contrast; it must stay far
        // below the text, or reading the note becomes work.
        expect(patternContrast / textContrast, `${paper} / ${intensity.id}`).toBeLessThan(0.25);
      }
    }
  });

  it('lifts the dark paper instead of darkening it', () => {
    // Dark ink on #18181B would be invisible; the contrast rule above cannot
    // catch that on its own, because it only measures a difference.
    const background = paperBackground('black');
    const line = composite(
      patternInk('black'),
      patternAlpha('black', 'normal'),
      background,
    );
    expect(lightness(line)).toBeGreaterThan(lightness(background));

    for (const paper of PAPER_COLORS.filter((name) => name !== 'black')) {
      const paperRgb = paperBackground(paper);
      const paperLine = composite(
        patternInk(paper),
        patternAlpha(paper, 'normal'),
        paperRgb,
      );
      expect(lightness(paperLine), paper).toBeLessThan(lightness(paperRgb));
    }
  });

  it('reads at the same strength on the dark paper as on the pale ones', () => {
    // The same alpha lifts a near-black paper much further than it darkens a
    // pale one, so the dark paper carries a gain to bring the three
    // intensities back onto the strength they have everywhere else.
    for (const intensity of PAPER_INTENSITIES) {
      const deltas = PAPER_COLORS.map((paper) => {
        const background = paperBackground(paper);
        const line = composite(
          patternInk(paper),
          patternAlpha(paper, intensity.id),
          background,
        );
        return Math.abs(lightness(line) - lightness(background));
      });

      const smallest = Math.min(...deltas);
      const largest = Math.max(...deltas);
      expect(largest / smallest, intensity.id).toBeLessThan(1.25);
    }
  });
});
