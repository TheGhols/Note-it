import { describe, expect, inject, it } from 'vitest';
import { HOLD_ZONE_PX, REVEAL_ZONE_PX } from '../src/ui/headerReveal.ts';
import { QUICK_ACTIONS } from '../src/ui/icons.ts';
import { declarationIn, RULES, ruleFor, rulesFor, tokenIn } from './support/stylesheet.ts';

/** A note in a given state, so a selector can be asked what it applies to. */
function noteIn(state: { collapsed?: boolean; revealed?: boolean }): Document {
  const page = new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
  page.body.setAttribute('data-collapsed', String(state.collapsed ?? false));
  page.body.setAttribute('data-header-revealed', String(state.revealed ?? false));
  return page;
}

/**
 * Whether `element` is a pointer target, resolved the way the cascade resolves
 * it: the last rule that both matches the element and sets `pointer-events`
 * wins, and an element with no rule at all inherits from its nearest ancestor
 * that has one.
 *
 * happy-dom has no layout and no cascade, so this walks the stylesheet by hand.
 * It is doing the browser's job for one property, which is exactly the property
 * that decides whether an invisible control can steal a click from the text
 * underneath it.
 */
function isPointerTarget(element: Element): boolean {
  for (let node: Element | null = element; node; node = node.parentElement) {
    let value: string | null = null;
    for (const rule of RULES) {
      if (!rule.selectors.some((selector) => safeMatches(node!, selector))) continue;
      const match = /(?:^|;)\s*pointer-events\s*:\s*([^;]+)/.exec(rule.body);
      if (match) value = match[1].trim();
    }
    if (value !== null) return value !== 'none';
  }
  return true;
}

function safeMatches(element: Element, selector: string): boolean {
  try {
    return element.matches(selector);
  } catch {
    return false;
  }
}

describe('the one Note-it header', () => {
  it('carries the menu, the six quick actions, the drag region and the close control', () => {
    const page = noteIn({});
    const headers = page.querySelectorAll('.note-header');

    expect(headers).toHaveLength(1);
    expect(headers[0].querySelector('#btn-menu')).not.toBeNull();
    expect(headers[0].querySelectorAll('.header-quick-action')).toHaveLength(6);
    expect(headers[0].querySelector('.drag-region #note-title')).not.toBeNull();
    expect(headers[0].querySelector('#btn-close')).not.toBeNull();
  });

  it('is chrome the note wears rather than a row the note gives up', () => {
    // Laid over the paper, with nothing of it painted until something asks for
    // it. The rejected model kept the whole bar permanently visible at 6% and
    // lifted it on hover; neither half of that survives.
    expect(declarationIn('.note-header', 'position')).toBe('absolute');
    expect(declarationIn('.note-header', 'opacity')).toBe('0');
    expect(rulesFor('.note-header:hover')).toHaveLength(0);
    expect(rulesFor('.note-header:focus-within')).toHaveLength(0);
    // Hovering anywhere on the note would reveal the bar, which is not what a
    // strip at the top means.
    expect(rulesFor('#app:hover .note-header')).toHaveLength(0);
  });

  it('shows the controls when the reveal state says so, in about a tenth of a second', () => {
    expect(ruleFor('body[data-header-revealed="true"] .note-header').body).toContain(
      'opacity: 1',
    );
    expect(declarationIn('.note-header', 'transition')).toBe(
      'opacity var(--note-header-reveal)',
    );

    const duration = Number.parseFloat(tokenIn(':root', '--note-header-reveal'));
    expect(duration).toBeGreaterThanOrEqual(100);
    expect(duration).toBeLessThanOrEqual(150);
  });

  it('reserves only the gutter for the editor, not the whole bar', () => {
    // The old rule was `calc(var(--note-header-height) + 8px)`: the bar was
    // overlaid but the note still paid for every pixel of it.
    expect(declarationIn('.editor-wrapper', 'padding')).toBe(
      'var(--note-chrome-gutter) 14px 14px 14px',
    );
    const gutter = Number.parseFloat(tokenIn(':root', '--note-chrome-gutter'));
    const bar = Number.parseFloat(tokenIn(':root', '--note-header-height'));
    expect(gutter).toBeLessThan(bar);
  });

  it('never lets a line of text sit under the strip that is always live', () => {
    // The two numbers that make both halves of the requirement hold at once:
    // the strip the note is dragged by is always a pointer target, the pointer
    // reaching it is what reveals the chrome, and the editor starts exactly
    // below it. If these ever drift, either the first line loses clicks or the
    // controls stop coming out.
    expect(tokenIn(':root', '--note-chrome-gutter')).toBe(`${REVEAL_ZONE_PX}px`);
    expect(tokenIn(':root', '--note-header-height')).toBe(`${HOLD_ZONE_PX}px`);
    expect(declarationIn('.drag-region', 'height')).toBe('var(--note-chrome-gutter)');
    expect(declarationIn('.drag-region', 'align-self')).toBe('flex-start');
  });

  it('makes a hidden control take no click at all', () => {
    const page = noteIn({ revealed: false });

    // The header is never a target itself, so the gaps between its controls
    // belong to the text underneath.
    expect(isPointerTarget(page.querySelector('.note-header')!)).toBe(false);
    for (const button of page.querySelectorAll('.note-header .icon-btn')) {
      expect(isPointerTarget(button)).toBe(false);
    }
    // Dragging never requires revealing anything first.
    expect(isPointerTarget(page.querySelector('.drag-region')!)).toBe(true);
    // The editor is untouched by any of it.
    expect(isPointerTarget(page.querySelector('.editor-wrapper')!)).toBe(true);
  });

  it('makes every control clickable once it can be seen', () => {
    const page = noteIn({ revealed: true });

    for (const button of page.querySelectorAll('.note-header .icon-btn')) {
      expect(isPointerTarget(button)).toBe(true);
    }
    expect(isPointerTarget(page.querySelector('#btn-menu')!)).toBe(true);
    expect(isPointerTarget(page.querySelector('#btn-close')!)).toBe(true);
  });

  it('keeps a collapsed note fully usable without any reveal', () => {
    const page = noteIn({ collapsed: true, revealed: false });

    expect(ruleFor('body[data-collapsed="true"] .note-header').body).toContain('opacity: 1');
    expect(isPointerTarget(page.querySelector('#btn-menu')!)).toBe(true);
    expect(isPointerTarget(page.querySelector('#btn-close')!)).toBe(true);
    expect(declarationIn('body[data-collapsed="true"] .drag-region', 'height')).toBe('100%');
  });

  it('keeps the collapsed title and hides the quick actions with it', () => {
    expect(declarationIn('body[data-collapsed="true"] .note-title', 'display')).toBe('block');
    expect(declarationIn('body[data-collapsed="true"] .header-quick-action', 'display')).toBe(
      'none',
    );
    expect(declarationIn('.note-title', 'text-overflow')).toBe('ellipsis');
    expect(declarationIn('.note-title', 'pointer-events')).toBe('none');
  });

  it('leaves the popover clickable even though the bar around it is not', () => {
    const page = noteIn({ revealed: true });
    const menu = page.createElement('div');
    menu.className = 'note-menu';
    page.querySelector('#note-controls-left')!.append(menu);

    expect(isPointerTarget(menu)).toBe(true);
  });

  it('names every quick action in the markup the application loads', () => {
    const page = noteIn({});
    for (const action of QUICK_ACTIONS) {
      const button = page.getElementById(action.buttonId);
      expect(button?.getAttribute('aria-label')).toBe(action.label);
    }
  });
});
