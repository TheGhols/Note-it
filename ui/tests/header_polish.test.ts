import { describe, expect, inject, it, vi } from 'vitest';
import {
  applyHeaderActionMetadata,
  HEADER_ACTIONS,
} from '../src/ui/actionMetadata.ts';
import { bindSearchEntries } from '../src/ui/searchEntry.ts';
import { declarationIn } from './support/stylesheet.ts';

function page(): Document {
  return new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
}

describe('the reorganized header', () => {
  it('preserves every established id in semantic groups and keeps trash before close', () => {
    const doc = page();
    expect(doc.querySelector('main#app')).not.toBeNull();
    expect(doc.querySelector('.note-header')?.getAttribute('role')).toBe('toolbar');
    expect(doc.querySelector('.note-header')?.getAttribute('aria-label')).toBe('Ferramentas da nota');
    expect(Array.from(doc.querySelectorAll('.header-action-group'), (node) => [
      node.getAttribute('role'),
      node.getAttribute('aria-label'),
    ])).toEqual([
      ['group', 'Nota'],
      ['group', 'Texto'],
      ['group', 'Conteúdo'],
      ['group', 'Visualização e ferramentas'],
    ]);
    expect(Array.from(doc.querySelectorAll('.header-note-group button'), (node) => node.id))
      .toEqual(['btn-menu', 'btn-note-color']);
    expect(Array.from(doc.querySelectorAll('.header-text-group button'), (node) => node.id))
      .toEqual(['btn-text-size', 'btn-text-color', 'btn-highlight', 'btn-blocks']);
    expect(Array.from(doc.querySelectorAll('.header-content-group button'), (node) => node.id))
      .toEqual(['btn-insert-image', 'btn-flashcards']);
    expect(Array.from(doc.querySelectorAll('.header-view-group button'), (node) => node.id))
      .toEqual(['btn-zoom-out', 'btn-zoom-in', 'btn-timer', 'btn-autopaste']);
    expect(Array.from(doc.querySelectorAll('.note-controls-right button'), (node) => node.id))
      .toEqual(['btn-trash-note', 'btn-close']);
    expect(doc.querySelectorAll('.header-group-separator').length).toBeGreaterThanOrEqual(2);
  });

  it('centres a button-shaped search entry and retains one compact fallback', () => {
    const doc = page();
    const pill = doc.getElementById('btn-search-pill');
    const fallback = doc.getElementById('btn-search');
    expect(pill?.tagName).toBe('BUTTON');
    expect(fallback?.tagName).toBe('BUTTON');
    expect(declarationIn('.header-search-pill', 'left')).toBe('50%');
    expect(declarationIn('.header-search-pill', 'transform')).toBe('translateX(-50%)');
    expect(declarationIn('.header-search-fallback', 'display')).toBe('none');
    const css = inject('themeCss');
    expect(css).toMatch(/@media \(max-width: 719px\)[\s\S]*header-search-pill-label-compact/);
    expect(css).toMatch(/@media \(max-width: 539px\)[\s\S]*header-search-pill[\s\S]*display:\s*none/);
    expect(css).toMatch(/@media \(max-width: 539px\)[\s\S]*header-search-fallback[\s\S]*inline-flex/);
  });

  it('routes pill and fallback to the same search callback without reaching drag', () => {
    const pill = document.createElement('button');
    const fallback = document.createElement('button');
    const open = vi.fn();
    const drag = vi.fn();
    document.body.append(pill, fallback);
    document.body.addEventListener('pointerdown', drag);
    const destroy = bindSearchEntries([pill, fallback], open);

    pill.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    pill.click();
    fallback.click();
    expect(drag).not.toHaveBeenCalled();
    expect(open).toHaveBeenCalledTimes(2);
    destroy();
  });

  it('uses one shortcut source for title, accessible name and aria-keyshortcuts', () => {
    const doc = page();
    applyHeaderActionMetadata(doc);
    for (const [id, action] of [
      ['btn-search-pill', HEADER_ACTIONS.search],
      ['btn-search', HEADER_ACTIONS.search],
      ['btn-zoom-out', HEADER_ACTIONS.zoomOut],
      ['btn-zoom-in', HEADER_ACTIONS.zoomIn],
      ['btn-close', HEADER_ACTIONS.close],
    ] as const) {
      const button = doc.getElementById(id)!;
      expect(button.title).toContain(action.label);
      expect(button.title).toContain(action.shortcut.display);
      expect(button.getAttribute('aria-label')).toBe(action.label);
      expect(button.getAttribute('aria-keyshortcuts')).toBe(action.shortcut.aria);
    }
    expect(doc.getElementById('btn-insert-image')?.title).toBe('Inserir imagem');
    expect(doc.getElementById('btn-insert-image')?.hasAttribute('aria-keyshortcuts')).toBe(false);
  });
});
