import { afterEach, describe, expect, inject, it, vi } from 'vitest';
import { bindHeaderShortcuts, updateZoomShortcutState } from '../src/ui/headerShortcuts.ts';
import { declarationIn, ruleFor } from './support/stylesheet.ts';

afterEach(() => document.body.replaceChildren());

function renderedPage(): Document {
  return new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
}

describe('the Phase 3.14 header shortcuts', () => {
  it('ships the deck, zoom controls, and trash with explicit accessible names', () => {
    const page = renderedPage();
    for (const [id, label] of [
      ['btn-flashcards', 'Central de estudos'],
      ['btn-zoom-out', 'Diminuir zoom'],
      ['btn-zoom-in', 'Aumentar zoom'],
      ['btn-trash-note', 'Mover nota para a lixeira'],
    ]) {
      const button = page.getElementById(id);
      expect(button?.getAttribute('title')).toBe(label);
      expect(button?.getAttribute('aria-label')).toBe(label);
      expect(button?.querySelector('svg')).not.toBeNull();
    }
    const right = page.querySelector('.note-controls-right');
    expect(Array.from(right!.querySelectorAll('button'), (button) => button.id)).toEqual([
      'btn-trash-note',
      'btn-close',
    ]);
  });

  it('routes one click to the existing actions and gives trash no delete callback', () => {
    const buttons = {
      study: document.createElement('button'),
      zoomOut: document.createElement('button'),
      zoomIn: document.createElement('button'),
      trash: document.createElement('button'),
    };
    document.body.append(...Object.values(buttons));
    const actions = {
      openStudyHub: vi.fn(),
      zoomOut: vi.fn(),
      zoomIn: vi.fn(),
      openTrashConfirmation: vi.fn(),
    };
    const destroy = bindHeaderShortcuts(buttons, actions);

    buttons.study.click();
    buttons.zoomOut.click();
    buttons.zoomIn.click();
    buttons.trash.click();
    expect(actions.openStudyHub).toHaveBeenCalledWith(buttons.study);
    expect(actions.zoomOut).toHaveBeenCalledTimes(1);
    expect(actions.zoomIn).toHaveBeenCalledTimes(1);
    expect(actions.openTrashConfirmation).toHaveBeenCalledWith(buttons.trash);
    expect(Object.keys(actions)).not.toContain('trashNote');

    destroy();
    buttons.study.click();
    expect(actions.openStudyHub).toHaveBeenCalledTimes(1);
  });

  it('disables only the zoom control at its canonical limit', () => {
    const out = document.createElement('button');
    const into = document.createElement('button');
    updateZoomShortcutState(out, into, 75, 75, 200);
    expect(out.disabled).toBe(true);
    expect(into.disabled).toBe(false);
    updateZoomShortcutState(out, into, 100, 75, 200);
    expect(out.disabled).toBe(false);
    expect(into.disabled).toBe(false);
    updateZoomShortcutState(out, into, 200, 75, 200);
    expect(out.disabled).toBe(false);
    expect(into.disabled).toBe(true);
  });

  it('fits the requested widths with the longest timer and AutoPaste active', () => {
    const css = inject('themeCss');
    const page = renderedPage();
    const hiddenAt = (width: number): Set<string> => {
      const hidden = new Set<string>();
      for (const media of css.matchAll(/@media \(max-width: (\d+)px\) \{([\s\S]*?)(?=\n\}\n(?:\n|\/\*))/g)) {
        if (width > Number(media[1])) continue;
        for (const rule of media[2].matchAll(/([^{}]+)\{([^}]*)\}/g)) {
          if (!/display:\s*none/.test(rule[2])) continue;
          for (const selector of rule[1].split(',').map((part) => part.trim())) {
            const suffix = selector.match(/#[\w-]+|\.[\w-]+$/)?.[0];
            if (!suffix) continue;
            for (const button of page.querySelectorAll(`.note-header .icon-btn${suffix}`)) {
              hidden.add(button.id);
            }
          }
        }
      }
      return hidden;
    };
    const iconPadding = Number.parseFloat(declarationIn('.icon-btn', 'padding'));
    const quickIcon = Number.parseFloat(
      ruleFor(':root').body.match(/--header-action-size:\s*([\d.]+)px/)![1],
    );
    const headerPadding = Number.parseFloat(
      declarationIn('.note-header', 'padding').split(/\s+/)[1],
    );
    const clock =
      Number.parseFloat(declarationIn('.header-timer-readout', 'font-size')) * 0.75 * 7 + 3;

    for (const width of [220, 260, 300, 320, 360, 420, 600, 900]) {
      const hidden = hiddenAt(width);
      let used = headerPadding * 2;
      for (const button of page.querySelectorAll('.note-header .icon-btn')) {
        if (hidden.has(button.id)) continue;
        const intrinsic = button.querySelector('svg')?.getAttribute('width');
        used += (intrinsic ? Number.parseFloat(intrinsic) : quickIcon) + iconPadding * 2;
      }
      if (width > 300) used += clock;
      expect(used, `${width}px with H:MM:SS and AutoPaste`).toBeLessThanOrEqual(width);
    }
  });

  it('keeps collapsed title, menu, active timer, AutoPaste, and close while hiding new shortcuts', () => {
    const css = inject('themeCss');
    expect(css).toMatch(/body\[data-collapsed="true"\] \.header-quick-action\s*\{[^}]*display:\s*none/s);
    expect(css).toMatch(/body\[data-collapsed="true"\] \.header-study-action,[\s\S]*\.header-trash-action\s*\{[^}]*display:\s*none/s);
    expect(css).toMatch(/body\[data-collapsed="true"\]\[data-timer="running"\] \.header-timer-action/);
    expect(css).toMatch(/body\[data-collapsed="true"\] \.note-title\s*\{[^}]*display:\s*block/s);
    expect(css).not.toMatch(/body\[data-collapsed="true"\][^{]*#btn-menu[^}]*display:\s*none/s);
    expect(css).not.toMatch(/body\[data-collapsed="true"\][^{]*#btn-close[^}]*display:\s*none/s);
  });
});
