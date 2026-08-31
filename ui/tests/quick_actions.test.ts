import { afterEach, describe, expect, inject, it, vi } from 'vitest';
import {
  CLIPPER,
  HEADER_ICONS,
  INLINE_HEADER_ICONS,
  normalizeIconSvg,
  QUICK_ACTIONS,
  SEARCH,
} from '../src/ui/icons.ts';
import { MenuPanel, NoteMenu } from '../src/ui/menu.ts';
import { PaperColor } from '../src/bridge/types.ts';
import { contrastRatio } from './support/color.ts';
import { declarationIn, RULES, tokensIn } from './support/stylesheet.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];

/** The page exactly as the application loads it, icons and all. */
function renderedPage(): Document {
  // The module script is dropped: happy-dom would try to fetch it, and none of
  // this is about the bundle.
  return new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
}

/**
 * A menu wired to the real markup's buttons, the way `main.ts` wires it.
 *
 * The point of the exercise is that a quick action opens a panel this menu
 * already owns. Building it from the shipped page rather than from a fixture
 * means a button that is renamed or dropped fails here.
 */
function mountFromPage() {
  document.body.innerHTML = renderedPage().body.innerHTML;
  const trigger = document.getElementById('btn-menu')!;
  const mount = document.getElementById('note-controls-left')!;

  const handlers = {
    onSelectColor: vi.fn(),
    onSelectPaperType: vi.fn(),
    onSelectPaperIntensity: vi.fn(),
    onSelectTheme: vi.fn(),
    onToggleCollapsed: vi.fn(),
    onSelectTextSize: vi.fn(),
    onSelectTextColor: vi.fn(),
    onSelectHighlight: vi.fn(),
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn(),
    onResetZoom: vi.fn(),
    onUiScaleIn: vi.fn(),
    onUiScaleOut: vi.fn(),
    onResetUiScale: vi.fn(),
    onSelectLayerMode: vi.fn(),
    onToggleCodeBlock: vi.fn(),
    onSelectCodeLanguage: vi.fn(),
    onToggleBlockquote: vi.fn(),
    onSelectCallout: vi.fn(),
    onInsertComment: vi.fn(),
    onOpenGlobalSearch: vi.fn(),
    onOpenFind: vi.fn(),
    onOpenReplace: vi.fn(),
    onTrashNote: vi.fn(),
    onOpenTrash: vi.fn(),
    onCreateBackup: vi.fn(),
    onInsertImage: vi.fn(),
    onOpenStudy: vi.fn(),
    onOpenStudyHub: vi.fn(),
    onToggleAutoPaste: vi.fn(),
    onSelectCaptureDelimiter: vi.fn(),
    onOpen: vi.fn(),
    onClose: vi.fn(),
  };

  const quickTriggers: Partial<Record<MenuPanel, HTMLElement>> = {};
  for (const action of QUICK_ACTIONS) {
    quickTriggers[action.panel] = document.getElementById(action.buttonId)!;
  }

  const menu = new NoteMenu({ trigger, mount, colors: COLORS, handlers, quickTriggers });
  return { menu, handlers };
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

describe('the header quick actions', () => {
  let active: NoteMenu | null = null;

  afterEach(() => {
    active?.destroy();
    active = null;
    document.body.innerHTML = '';
  });

  it('keeps the five formatting actions grouped in their approved order', () => {
    expect(QUICK_ACTIONS.map((action) => action.label)).toEqual([
      'Cor da nota',
      'Tamanho do texto',
      'Cor do texto',
      'Marca-texto',
      'Blocos',
    ]);
    // Every one of them names the panel it opens, and none has logic of its
    // own. The paperclip beside them is not one: it opens nothing.
    expect(QUICK_ACTIONS.every((action) => typeof action.panel === 'string')).toBe(true);
    expect(Object.hasOwn(CLIPPER, 'panel')).toBe(false);
  });

  it('groups formatting, content, and the compact search fallback in DOM order', () => {
    const page = renderedPage();
    const buttons = page.querySelectorAll('.note-header .header-quick-action');
    expect(Array.from(buttons, (button) => button.id)).toEqual(
      HEADER_ICONS.map((icon) => icon.buttonId),
    );
    const ids = Array.from(page.querySelectorAll('.note-header .icon-btn'), (b) => b.id);
    expect(ids.indexOf('btn-insert-image')).toBeGreaterThan(ids.indexOf('btn-blocks'));
    expect(ids.indexOf(SEARCH.buttonId)).toBeGreaterThan(ids.indexOf('btn-flashcards'));
    expect(ids.indexOf('btn-insert-image')).toBeLessThan(ids.indexOf('btn-timer'));
  });

  it('each one is named for what it does, for the reader and for the screen reader', () => {
    const page = renderedPage();
    for (const action of QUICK_ACTIONS) {
      const button = page.getElementById(action.buttonId)!;
      expect(button.getAttribute('aria-label')).toBe(action.label);
      expect(button.getAttribute('title')).toBe(action.label);
      // Icons only: no permanent text label anywhere in the bar.
      expect(button.textContent?.trim()).toBe('');
    }
    const search = page.getElementById(SEARCH.buttonId)!;
    expect(search.getAttribute('aria-label')).toBe(SEARCH.label);
    expect(search.getAttribute('title')).toContain('Ctrl+K');
  });

  it('each one opens the panel the menu already builds, and nothing else', () => {
    const note = mountFromPage();
    active = note.menu;

    for (const action of QUICK_ACTIONS) {
      click(document.getElementById(action.buttonId)!);
      expect(note.menu.activePanel()).toBe(action.panel);
      expect(note.menu.isOpen()).toBe(true);
    }

    // One popover, six ways in. A quick action never builds a second surface
    // and never applies anything on its own.
    expect(document.querySelectorAll('.note-menu')).toHaveLength(1);
    expect(note.handlers.onSelectColor).not.toHaveBeenCalled();
    expect(note.handlers.onSelectTextSize).not.toHaveBeenCalled();
    expect(note.handlers.onSelectTextColor).not.toHaveBeenCalled();
    expect(note.handlers.onSelectHighlight).not.toHaveBeenCalled();
    expect(note.handlers.onOpenGlobalSearch).not.toHaveBeenCalled();
    expect(note.handlers.onToggleCodeBlock).not.toHaveBeenCalled();
  });

  it('Buscar still offers all three ways of looking for something', () => {
    const note = mountFromPage();
    active = note.menu;

    click(document.getElementById('btn-search')!);
    const rows = Array.from(
      note.menu.element.querySelectorAll<HTMLElement>('.note-menu-search .note-menu-item'),
    );
    expect(rows.map((row) => row.firstChild?.textContent)).toEqual([
      'Buscar em todas as notas',
      'Buscar nesta nota',
      'Localizar e substituir',
    ]);

    click(rows[0]);
    expect(note.handlers.onOpenGlobalSearch).toHaveBeenCalledTimes(1);

    click(document.getElementById('btn-search')!);
    click(
      note.menu.element.querySelectorAll<HTMLElement>('.note-menu-search .note-menu-item')[1],
    );
    expect(note.handlers.onOpenFind).toHaveBeenCalledTimes(1);

    click(document.getElementById('btn-search')!);
    click(
      note.menu.element.querySelectorAll<HTMLElement>('.note-menu-search .note-menu-item')[2],
    );
    expect(note.handlers.onOpenReplace).toHaveBeenCalledTimes(1);
  });

  it('a collapsed note hides all six and keeps only the menu, the title and the close', () => {
    // The rule the stylesheet applies, and the class every one of the six
    // carries so that rule reaches them.
    expect(declarationIn('body[data-collapsed="true"] .header-quick-action', 'display')).toBe(
      'none',
    );

    const collapsed = renderedPage();
    collapsed.body.setAttribute('data-collapsed', 'true');
    for (const action of QUICK_ACTIONS) {
      const button = collapsed.getElementById(action.buttonId)!;
      expect(button.matches('body[data-collapsed="true"] .header-quick-action')).toBe(true);
    }
    for (const id of ['btn-menu', 'btn-close']) {
      expect(
        collapsed.getElementById(id)!.matches('body[data-collapsed="true"] .header-quick-action'),
      ).toBe(false);
    }

    // Expanded, the same rule no longer reaches any of them.
    const expanded = renderedPage();
    expanded.body.setAttribute('data-collapsed', 'false');
    for (const action of QUICK_ACTIONS) {
      expect(
        expanded
          .getElementById(action.buttonId)!
          .matches('body[data-collapsed="true"] .header-quick-action'),
      ).toBe(false);
    }
  });
});

describe('the quick-action icons', () => {
  it('every one is drawn into the page rather than fetched', () => {
    const page = renderedPage();

    for (const action of QUICK_ACTIONS) {
      const holder = page.querySelector(`[data-quick-icon="${action.id}"]`);
      expect(holder, `${action.label} has no icon holder`).not.toBeNull();
      const svg = holder!.querySelector('svg');
      expect(svg, `${action.label} has no inline drawing`).not.toBeNull();
      expect(svg!.getAttribute('viewBox')).toBeTruthy();
      expect(svg!.getAttribute('aria-hidden')).toBe('true');
      // Sized by the stylesheet, so one file serves the bar and the icon can
      // never arrive at its own intrinsic 800px.
      expect(svg!.hasAttribute('width')).toBe(false);
      expect(svg!.hasAttribute('height')).toBe(false);
    }
  });

  it('nothing in the shipped page or stylesheet asks for a file', () => {
    const page = inject('renderedHtml');

    // The defect this replaces: a CSS mask pointing at an image. Under this
    // page's own `default-src 'self'` the fetch is refused and the button is
    // blank, which no stylesheet assertion could have caught.
    expect(page).not.toMatch(/mask-image/);
    expect(page).not.toMatch(/data:image/);
    expect(page).not.toMatch(/<img\b/);
    expect(page).not.toMatch(/IconesNote-it/);
    for (const rule of RULES) {
      expect(rule.body).not.toMatch(/mask-image/);
      expect(rule.body).not.toMatch(/url\(/);
    }
    // Nothing is loaded from anywhere but this directory, at all.
    expect(page).not.toMatch(/(?:src|href)="https?:/);
  });

  it('releases exactly the files it uses from the icon drop, and no others', () => {
    const gitignore = inject('gitignore');
    const released = Array.from(
      gitignore.matchAll(/^!IconesNote-it\/(.+)$/gm),
      (match) => match[1],
    );

    expect(gitignore).toContain('IconesNote-it/*');
    expect(released.sort()).toEqual(INLINE_HEADER_ICONS.map((icon) => icon.asset).sort());
    // One asset per button, and no asset serving two.
    expect(new Set(released).size).toBe(INLINE_HEADER_ICONS.length);
  });

  it('is drawn in the button colour, on any paper and either theme', () => {
    const icons = inject('headerIcons');

    for (const action of INLINE_HEADER_ICONS) {
      const normalized = normalizeIconSvg(icons[action.id]);
      // No literal colour survives: the whole drawing inherits the button's.
      expect(normalized).not.toMatch(/(?:fill|stroke)="#/);
      expect(normalized).not.toMatch(/(?:fill|stroke)="(?:black|white|rgb)/i);
      expect(normalized).toMatch(/currentColor/);
    }
    expect(declarationIn('.header-action-icon', 'color')).toBe('inherit');
    expect(declarationIn('.icon-btn', 'color')).toBe('var(--paper-muted)');
  });

  it('reads at full strength on every paper, black included', () => {
    const icons = inject('headerIcons');

    // Every part of every icon is opaque, so the whole drawing is the button's
    // own colour. The supplied files are two-tone at 40%, which at this size
    // reads as a fault and does not clear the contrast floor on pale paper.
    for (const action of QUICK_ACTIONS) {
      expect(normalizeIconSvg(icons[action.id])).not.toMatch(/opacity=/);
    }

    for (const paper of COLORS) {
      const tokens = tokensIn(`body[data-color="${paper}"]`);
      const ink = tokens.get('--paper-muted')!;
      const background = tokens.get('--paper-bg')!;
      // WCAG 1.4.11: a graphic that carries meaning needs 3:1.
      expect(contrastRatio(ink, background), `${paper} paper`).toBeGreaterThanOrEqual(3);
    }
  });

  it('carries no element ids, so six drawings in one document cannot collide', () => {
    // The supplied files name their groups `Search`, `Stroke 1` and the like.
    for (const drawing of renderedPage().querySelectorAll('[data-quick-icon] svg')) {
      expect(drawing.querySelectorAll('[id]')).toHaveLength(0);
      expect(drawing.hasAttribute('id')).toBe(false);
    }
  });

  it('refuses to ship a button with no drawing', () => {
    expect(() => normalizeIconSvg('<p>not an icon</p>')).toThrow();
  });
});
