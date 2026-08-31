import { describe, expect, inject, it } from 'vitest';
import { HOLD_ZONE_PX, REVEAL_ZONE_PX } from '../src/ui/headerReveal.ts';
import { HEADER_ICONS } from '../src/ui/icons.ts';
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
  it('carries the menu, the drawn actions, the drag region and the close control', () => {
    const page = noteIn({});
    const headers = page.querySelectorAll('.note-header');

    expect(headers).toHaveLength(1);
    expect(headers[0].querySelector('#btn-menu')).not.toBeNull();
    // Every established action still uses the reviewed inline-SVG pipeline.
    expect(headers[0].querySelectorAll('.header-quick-action')).toHaveLength(
      HEADER_ICONS.length,
    );
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

    const duration = Number.parseFloat(tokenIn(':root', '--motion-fast'));
    expect(duration).toBeGreaterThanOrEqual(100);
    expect(duration).toBeLessThanOrEqual(150);
  });

  it('reserves only the gutter for the editor, not the whole bar', () => {
    // The old rule was `calc(var(--note-header-height) + 8px)`: the bar was
    // overlaid but the note still paid for every pixel of it.
    expect(declarationIn('.editor-wrapper', 'padding')).toBe(
      'var(--note-chrome-gutter) 14px 14px 14px',
    );
    const px = (value: string): number => Number(value.match(/([\d.]+)px/)![1]);
    const gutter = px(tokenIn(':root', '--note-chrome-gutter'));
    const bar = px(tokenIn(':root', '--note-header-height'));
    expect(gutter).toBeLessThan(bar);
  });

  it('never lets a line of text sit under the strip that is always live', () => {
    // The two numbers that make both halves of the requirement hold at once:
    // the strip the note is dragged by is always a pointer target, the pointer
    // reaching it is what reveals the chrome, and the editor starts exactly
    // below it. If these ever drift, either the first line loses clicks or the
    // controls stop coming out.
    expect(tokenIn(':root', '--note-chrome-gutter')).toContain(`${REVEAL_ZONE_PX}px`);
    expect(tokenIn(':root', '--note-header-height')).toContain(`${HOLD_ZONE_PX}px`);
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

  it('carries its own paper under the controls, so nothing can be read through them', () => {
    // 3.9UX.R.2. The bar painted nothing at all, and the claim that the gutter
    // kept text out of the strip held only at the top of the note: the editor
    // is the scroll container and its top padding scrolls away with the text,
    // so a scrolled note put the reader's own words behind the icons.
    const fill = declarationIn('.note-header', 'background-image');
    expect(fill).toContain('var(--paper-bg)');
    // The paper's own colour, so the bar is never a foreign panel; painted on
    // the header itself, so it fades with the chrome and an absent bar still
    // paints nothing.
    expect(declarationIn('.note-header', 'opacity')).toBe('0');
    expect(declarationIn('.note-header', 'transition')).toBe(
      'opacity var(--note-header-reveal)',
    );
  });

  it('paints exactly the strip that is always the note\'s own', () => {
    const fill = declarationIn('.note-header', 'background-image');
    // Both stops at the gutter: a hard edge, nothing interpolated, and never a
    // pixel of a line the reader has at rest.
    const stops = fill.match(/var\(--note-chrome-gutter\)/g) ?? [];
    expect(stops).toHaveLength(2);
    expect(fill).toContain('transparent var(--note-chrome-gutter)');
  });

  it('covers every pixel a control occupies', () => {
    // The band is only worth anything if the whole control row sits inside it.
    const px = (value: string): number => Number(value.match(/([\d.]+)px/)![1]);
    const gutter = px(tokenIn(':root', '--note-chrome-gutter'));
    const control = px(tokenIn(':root', '--header-control-size'));
    expect(control).toBeLessThanOrEqual(gutter);
  });

  it('puts no title text on the row the controls live on', () => {
    // The expanded note names itself nowhere in the bar. The title element is
    // the collapsed note's, and only the collapsed note displays it.
    expect(declarationIn('.note-title', 'display')).toBe('none');
    expect(declarationIn('body[data-collapsed="true"] .note-title', 'display')).toBe('block');
    expect(rulesFor('.note-title:hover')).toHaveLength(0);
    expect(rulesFor('.note-header:hover .note-title')).toHaveLength(0);
  });

  it('shows the note information below the controls and never over them', () => {
    // The one thing the bar reveals on hover. It hangs off the bottom of the
    // control block rather than sharing the row with it, and it is never a
    // pointer target, so it cannot take a click meant for a button.
    expect(declarationIn('.note-tooltip', 'position')).toBe('absolute');
    expect(declarationIn('.note-tooltip', 'top')).toBe('calc(100% + 4px)');
    expect(declarationIn('.note-tooltip', 'pointer-events')).toBe('none');
    expect(declarationIn('.note-controls-left', 'position')).toBe('relative');
  });

  it('never lets a title take room away from a control', () => {
    // Only the drag region flexes. Both control blocks are fixed, so a title
    // of any length is cut by the title and never by the buttons.
    expect(declarationIn('.note-controls-left', 'flex')).toBe('0 0 auto');
    expect(declarationIn('.note-controls-right', 'flex')).toBe('0 0 auto');
    expect(declarationIn('.drag-region', 'flex')).toBe('1');
    expect(declarationIn('.drag-region', 'min-width')).toBe('0');
    expect(declarationIn('.note-title', 'min-width')).toBe('0');
    expect(declarationIn('.note-title', 'max-width')).toBe('100%');
    expect(declarationIn('.note-title', 'overflow')).toBe('hidden');
    expect(declarationIn('.note-title', 'white-space')).toBe('nowrap');
    expect(declarationIn('.note-title', 'text-overflow')).toBe('ellipsis');
  });

  it('keeps every control in the bar however long the title gets', () => {
    const page = noteIn({ collapsed: true, revealed: true });
    const title = page.querySelector('#note-title')!;
    title.textContent = `${'A'.repeat(78)}🎉…`;

    // The title is written into the drag region and nowhere else; the controls
    // are its siblings, not its container.
    expect(title.closest('.drag-region')).not.toBeNull();
    expect(title.closest('.note-controls-left')).toBeNull();
    expect(title.closest('.note-controls-right')).toBeNull();
    expect(page.querySelector('#btn-menu')).not.toBeNull();
    expect(page.querySelector('#btn-close')).not.toBeNull();
    expect(isPointerTarget(page.querySelector('#btn-menu')!)).toBe(true);
    expect(isPointerTarget(page.querySelector('#btn-close')!)).toBe(true);
    // ...and the title still cannot take a pointer event from either of them.
    expect(isPointerTarget(title)).toBe(false);
  });

  it('keeps every control named for a reader who cannot see the icon', () => {
    // The bug was painted, so the fix is painted. Nothing here gives up an
    // accessible name to buy visual room.
    const page = noteIn({ revealed: true });
    for (const button of page.querySelectorAll('.note-header .icon-btn')) {
      expect(button.getAttribute('aria-label')).toBeTruthy();
      expect(button.getAttribute('title')).toBeTruthy();
    }
  });

  it('names every drawn action in the markup the application loads', () => {
    const page = noteIn({});
    for (const icon of HEADER_ICONS) {
      const button = page.getElementById(icon.buttonId);
      expect(button?.getAttribute('aria-label')).toBe(icon.label);
      expect(button?.getAttribute('title')).toContain(icon.label);
    }
  });
});
