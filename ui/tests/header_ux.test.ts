import { describe, expect, inject, it } from 'vitest';
import { declarationIn, rulesFor } from './support/stylesheet.ts';

describe('the one Note-it header', () => {
  it('contains exactly the menu, two quick actions, title drag region and close control', () => {
    const page = new DOMParser().parseFromString(inject('indexHtml'), 'text/html');
    const headers = page.querySelectorAll('.note-header');

    expect(headers).toHaveLength(1);
    expect(headers[0].querySelector('#btn-menu')).not.toBeNull();
    expect(headers[0].querySelector('#btn-note-color')?.getAttribute('aria-label')).toBe(
      'Cor da nota',
    );
    expect(headers[0].querySelector('#btn-text-size')?.getAttribute('aria-label')).toBe(
      'Tamanho do texto',
    );
    expect(headers[0].querySelector('.drag-region #note-title')).not.toBeNull();
    expect(headers[0].querySelector('#btn-close')).not.toBeNull();
  });

  it('lays the expanded header over the paper and reveals it only at the top or on focus', () => {
    expect(declarationIn('.note-header', 'position')).toBe('absolute');
    expect(declarationIn('.editor-wrapper', 'padding')).toBe(
      'calc(var(--note-header-height) + 8px) 14px 14px 14px',
    );
    expect(declarationIn('.note-header', 'opacity')).toBe('0.06');
    expect(declarationIn('.note-header:hover', 'opacity')).toBe('1');
    expect(declarationIn('.note-header:focus-within', 'opacity')).toBe('1');
    expect(rulesFor('#app:hover .note-header')).toHaveLength(0);
  });

  it('keeps the collapsed header and title visible while removing only quick actions', () => {
    expect(declarationIn('body[data-collapsed="true"] .note-header', 'opacity')).toBe('1');
    expect(declarationIn('body[data-collapsed="true"] .note-title', 'display')).toBe('block');
    expect(declarationIn('body[data-collapsed="true"] .header-quick-action', 'display')).toBe(
      'none',
    );
    expect(declarationIn('.note-title', 'text-overflow')).toBe('ellipsis');
    expect(declarationIn('.note-title', 'pointer-events')).toBe('none');
  });

  it('uses only the two reviewed icon files as colour-adapting masks', () => {
    expect(declarationIn('.header-action-icon-color', '--header-action-mask')).toContain(
      'palette-round-svgrepo-com.svg',
    );
    expect(declarationIn('.header-action-icon-text-size', '--header-action-mask')).toContain(
      'larger-text-svgrepo-com.svg',
    );
    expect(declarationIn('.header-action-icon', 'background-color')).toBe('currentColor');
  });
});
