import { afterEach, describe, expect, inject, it, vi } from 'vitest';
import { NoteMenu } from '../src/ui/menu.ts';
import { HeaderReveal } from '../src/ui/headerReveal.ts';
import { CAPTURE_DELIMITERS } from '../src/capture/autoPaste.ts';
import { PaperColor } from '../src/bridge/types.ts';
import { declarationIn, RULES, ruleFor, rulesFor } from './support/stylesheet.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];
const THEME_CSS = inject('themeCss');

/** The page exactly as the application loads it, icons and all. */
function renderedPage(): Document {
  return new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
}

let active: NoteMenu | null = null;
let reveal: HeaderReveal | null = null;

afterEach(() => {
  active?.destroy();
  active = null;
  reveal?.destroy();
  reveal = null;
  document.body.innerHTML = '';
  document.body.removeAttribute('data-autopaste');
  document.body.removeAttribute('data-collapsed');
});

/** A menu wired to the real markup's buttons, the way `main.ts` wires it. */
function mount() {
  document.body.innerHTML = renderedPage().body.innerHTML;
  const trigger = document.getElementById('btn-menu')!;
  const mountPoint = document.getElementById('note-controls-left')!;
  const indicator = document.getElementById('btn-autopaste')!;

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
    onToggleAutoPaste: vi.fn(),
    onSelectCaptureDelimiter: vi.fn(),
    onOpen: vi.fn(),
    onClose: vi.fn(),
  };

  const menu = new NoteMenu({
    trigger,
    mount: mountPoint,
    colors: COLORS,
    handlers,
    quickTriggers: { capture: indicator },
  });
  active = menu;
  return { menu, handlers, indicator, trigger };
}

/**
 * Whether the header would actually draw `element`, resolved the way the
 * cascade resolves it: the last rule that both matches and sets `display`
 * wins, and the `hidden` attribute is the UA rule underneath them all.
 *
 * happy-dom has no layout and no cascade, so this does the browser's job for
 * one property — which is the property deciding whether a note that stopped
 * capturing still says it is.
 */
function visibleInHeader(element: HTMLElement): boolean {
  let display: string | null = element.hidden ? 'none' : null;
  for (const rule of RULES) {
    if (!rule.selectors.some((selector) => safeMatches(element, selector))) continue;
    const match = /(?:^|;)\s*display\s*:\s*([^;]+)/.exec(rule.body);
    if (match) display = match[1].trim();
  }
  return display !== 'none';
}

function safeMatches(element: Element, selector: string): boolean {
  try {
    return element.matches(selector);
  } catch {
    return false;
  }
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

function capturePanel(menu: NoteMenu): HTMLElement {
  return menu.element.querySelector<HTMLElement>('.note-menu-capture')!;
}

function switchRow(menu: NoteMenu): HTMLButtonElement {
  return capturePanel(menu).querySelector<HTMLButtonElement>('.note-menu-option')!;
}

describe('where AutoPaste is switched on', () => {
  it('lives in the note menu rather than as another button in the bar', () => {
    // The bar is already carrying the menu, six quick actions, the timer and
    // the close cross. Switching clipboard observation on is a decision, not a
    // quick action, and there is no room for another permanent control.
    const page = renderedPage();
    const permanent = page.querySelectorAll('.note-header .icon-btn:not([hidden])');
    expect(permanent).toHaveLength(9);

    const note = mount();
    click(note.trigger);
    expect(
      Array.from(
        note.menu.element.querySelectorAll<HTMLElement>('.note-menu-panel:not([class*=" "]) .note-menu-submenu'),
      ).map((item) => item.dataset.panel),
    ).toContain('capture');
  });

  it('says what the switch will do before it is used', () => {
    const note = mount();
    const hint = capturePanel(note.menu).querySelector('.note-menu-capture-hint');
    expect(hint?.textContent).toBe(
      'Enquanto ativo, todo novo texto copiado será adicionado a esta nota.',
    );
    // A sentence, not a modal, and not a confirmation for every capture.
    expect(capturePanel(note.menu).querySelectorAll('dialog')).toHaveLength(0);
  });

  it('offers exactly the three ways of separating captures', () => {
    const note = mount();
    note.menu.openMenu();
    const panel = note.menu.element.querySelector<HTMLElement>('.note-menu-capture-delimiter')!;
    const options = panel.querySelectorAll<HTMLElement>('.note-menu-option');

    expect(Array.from(options).map((option) => option.dataset.value)).toEqual([
      'line',
      'blankLine',
      'separator',
    ]);
    expect(Array.from(options).map((option) => option.firstChild?.textContent)).toEqual(
      CAPTURE_DELIMITERS.map((entry) => entry.label),
    );
    // No template field, no regex, no placeholder language.
    expect(panel.querySelectorAll('input, textarea')).toHaveLength(0);
  });
});

describe('the switch says which state it is in', () => {
  it('carries a pressed state and the state in words', () => {
    const note = mount();
    const row = switchRow(note.menu);

    note.menu.setAutoPaste(false, 'blankLine');
    expect(row.getAttribute('aria-pressed')).toBe('false');
    expect(row.querySelector('.note-menu-value')?.textContent).toBe('Desativado');
    expect(row.getAttribute('aria-label')).toBe('AutoPaste desativado. Ativar');

    note.menu.setAutoPaste(true, 'blankLine');
    expect(row.getAttribute('aria-pressed')).toBe('true');
    expect(row.querySelector('.note-menu-value')?.textContent).toBe('Ativo');
    expect(row.getAttribute('aria-label')).toBe('AutoPaste ativo. Desativar');
    expect(note.menu.isAutoPasteActive()).toBe(true);
  });

  it('is readable without seeing a colour at all', () => {
    // The row states the mode in words; nothing about it is only a tint.
    const note = mount();
    note.menu.setAutoPaste(true, 'blankLine');
    const row = switchRow(note.menu);
    expect(row.textContent).toContain('AutoPaste');
    expect(row.textContent).toContain('Ativo');
    expect(rulesFor('.note-menu-capture .note-menu-option[aria-pressed="true"]')).toHaveLength(
      0,
    );
    expect(
      ruleFor('.note-menu-capture .note-menu-option[aria-pressed="true"] .note-menu-value').body,
    ).toContain('font-weight');
  });

  it('shows the delimiter in force without opening the submenu', () => {
    const note = mount();
    note.menu.setAutoPaste(false, 'separator');
    expect(capturePanel(note.menu).textContent).toContain('Separador');
  });
});

describe('asking the host, rather than deciding', () => {
  it('requests the opposite of what it is showing', () => {
    const note = mount();
    note.menu.setAutoPaste(false, 'blankLine');
    click(switchRow(note.menu));
    expect(note.handlers.onToggleAutoPaste).toHaveBeenCalledWith(true);

    note.menu.setAutoPaste(true, 'blankLine');
    click(switchRow(note.menu));
    expect(note.handlers.onToggleAutoPaste).toHaveBeenLastCalledWith(false);
  });

  it('does not switch itself on before the host has agreed', () => {
    // The target is exclusive across the application. A note that flipped its
    // own switch would claim a target another note may still hold.
    const note = mount();
    note.menu.setAutoPaste(false, 'blankLine');
    click(switchRow(note.menu));
    expect(note.menu.isAutoPasteActive()).toBe(false);
    expect(switchRow(note.menu).getAttribute('aria-pressed')).toBe('false');
  });

  it('keeps the panel open so the sentence is still in front of the reader', () => {
    const note = mount();
    note.menu.openMenu();
    note.menu.setAutoPaste(false, 'blankLine');
    click(switchRow(note.menu));
    expect(note.menu.isOpen()).toBe(true);
  });

  it('asks for a delimiter from the closed set and nothing else', () => {
    const note = mount();
    note.menu.openMenu();
    const options = note.menu.element.querySelectorAll<HTMLElement>(
      '.note-menu-capture-delimiter .note-menu-option',
    );
    for (const option of options) {
      click(option);
      expect(note.handlers.onSelectCaptureDelimiter).toHaveBeenLastCalledWith(
        option.dataset.value,
      );
    }
    expect(note.handlers.onSelectCaptureDelimiter).toHaveBeenCalledTimes(3);
  });
});

describe('the indicator on the note being captured into', () => {
  it('is absent from the bar until the note is the target', () => {
    const page = renderedPage();
    const indicator = page.getElementById('btn-autopaste')!;
    // Absent rather than dimmed: its presence is the signal.
    expect(indicator.hasAttribute('hidden')).toBe(true);
    expect(indicator.getAttribute('aria-label')).toBe('AutoPaste ativo. Abrir Captura');
    expect(indicator.getAttribute('title')).toBe('AutoPaste ativo');
    expect(indicator.getAttribute('aria-pressed')).toBe('true');
    // Outside the drag region, so pressing it can never move the window.
    expect(indicator.closest('.drag-region')).toBeNull();
    expect(indicator.closest('.note-controls-left')).not.toBeNull();
  });

  it('opens the panel that switches it off', () => {
    // A second way into a panel the menu already builds, never a second
    // implementation — the rule the six quick actions follow.
    const note = mount();
    note.indicator.hidden = false;
    click(note.indicator);
    expect(note.menu.isOpen()).toBe(true);
    expect(note.menu.activePanel()).toBe('capture');
    expect(note.handlers.onToggleAutoPaste).not.toHaveBeenCalled();
  });

  it('stays on the bar of a collapsed note, and only while it is the target', () => {
    // A note recolhida must not have to be expanded to find out that
    // everything copied is being filed into it — and a note that has *lost*
    // the target must stop saying it has it. The first version of this pinned
    // the indicator open when collapsed, which out-specified `[hidden]` and
    // left a released note claiming to be capturing.
    expect(declarationIn('body[data-collapsed="true"] .header-quick-action', 'display')).toBe(
      'none',
    );
    // The indicator is not a quick action, so nothing hides it when the note
    // collapses and no rule is needed to bring it back.
    expect(rulesFor('body[data-collapsed="true"] .header-autopaste')).toHaveLength(0);
    expect(declarationIn('.header-autopaste[hidden]', 'display')).toBe('none');

    // Resolved the way the cascade resolves it, in both collapse states.
    for (const collapsed of [true, false]) {
      const note = mount();
      document.body.setAttribute('data-collapsed', String(collapsed));

      note.menu.setAutoPaste(true, 'blankLine');
      note.indicator.hidden = false;
      expect(visibleInHeader(note.indicator)).toBe(true);

      note.indicator.hidden = true;
      expect(visibleInHeader(note.indicator)).toBe(false);
      active?.destroy();
      active = null;
    }
  });

  it('never lets the digits or the icons change the width of the row', () => {
    expect(declarationIn('.header-autopaste', 'flex')).toBe('0 0 auto');
    expect(declarationIn('.header-autopaste[hidden]', 'display')).toBe('none');
  });
});

describe('the chrome stays out while the clipboard is being watched', () => {
  it('holds the bar open for capturing, independently of the menu', () => {
    const header = document.createElement('div');
    document.body.append(header);
    reveal = new HeaderReveal({ header, body: document.body });

    expect(reveal.isRevealed()).toBe(false);

    reveal.setCapturing(true);
    expect(reveal.isRevealed()).toBe(true);
    expect(document.body.getAttribute('data-header-revealed')).toBe('true');

    // Opening and closing the menu must not take the indicator away with it.
    reveal.setHeld(true);
    reveal.setHeld(false);
    expect(reveal.isRevealed()).toBe(true);

    reveal.setCapturing(false);
    expect(reveal.isRevealed()).toBe(false);
  });

  it('does not reopen the defect the bar was rebuilt to fix', () => {
    // Holding the bar out is only safe because it paints the paper under
    // exactly the gutter — the strip that is always the note's own and never
    // a line's. Both stops of the fill are still at the gutter.
    const fill = declarationIn('.note-header', 'background-image');
    expect((fill.match(/var\(--note-chrome-gutter\)/g) ?? []).length).toBe(2);
    expect(fill).toContain('transparent var(--note-chrome-gutter)');
    expect(declarationIn('.note-header', 'opacity')).toBe('0');
  });
});

describe('the header bar still fits with everything on it', () => {
  /** The row's fixed width, from the markup and the stylesheet. */
  function controlRowWidth(includeHidden: boolean): number {
    const page = renderedPage();
    const iconPadding = Number.parseFloat(declarationIn('.icon-btn', 'padding'));
    const quickIcon = Number.parseFloat(
      ruleFor(':root').body.match(/--header-action-size:\s*([\d.]+)px/)![1],
    );
    const headerPadding = Number.parseFloat(
      declarationIn('.note-header', 'padding').split(/\s+/)[1],
    );

    let width = headerPadding * 2;
    for (const button of page.querySelectorAll('.note-header .icon-btn')) {
      if (!includeHidden && button.hasAttribute('hidden')) continue;
      const intrinsic = button.querySelector('svg')?.getAttribute('width');
      width += (intrinsic ? Number.parseFloat(intrinsic) : quickIcon) + iconPadding * 2;
    }
    return width;
  }

  it('fits every control, capture included, at the narrowest a note can be', () => {
    // 220 px, and the close cross must still be on the note. This is why the
    // indicator is not a permanent button: it costs its width only while the
    // note is actually the capture target.
    const floor = inject('minNoteWidth');
    expect(controlRowWidth(false)).toBeLessThan(floor);
    expect(controlRowWidth(true)).toBeLessThan(floor);
  });

  it('fits the timer clock beside it wherever that clock is shown', () => {
    // The digits are hidden on an expanded note below the breakpoint, so the
    // widest the row ever gets is at that breakpoint and above.
    const breakpoint = Number.parseFloat(
      /@media \(max-width: (\d+)px\)/.exec(THEME_CSS)![1],
    );
    const readoutFont = Number.parseFloat(
      declarationIn('.header-timer-readout', 'font-size'),
    );
    // `H:MM:SS`, at a generous upper bound for a tabular digit's advance.
    const widestClock = readoutFont * 0.75 * 7 + 3;

    expect(controlRowWidth(true) + widestClock).toBeLessThanOrEqual(breakpoint);
    // ...and with the digits hidden, the row fits the floor with room to spare
    // for the note's own name.
    expect(controlRowWidth(true)).toBeLessThan(inject('minNoteWidth'));
  });

  it('keeps the close control and the collapsed title whatever else is on', () => {
    const page = renderedPage();
    // Close is in its own group at the far end, and never shrinks.
    expect(declarationIn('.note-controls-right', 'flex')).toBe('0 0 auto');
    expect(page.querySelector('.note-controls-right #btn-close')).not.toBeNull();
    // The name is the one thing that yields, by ellipsis rather than by
    // pushing anything off the bar.
    expect(declarationIn('.note-title', 'min-width')).toBe('0');
    expect(declarationIn('.note-title', 'text-overflow')).toBe('ellipsis');
  });
});

describe('the indicator is chrome, not content', () => {
  it('lives in the header and never in the document', () => {
    const page = renderedPage();
    const indicator = page.getElementById('btn-autopaste')!;
    expect(indicator.closest('#editor-container')).toBeNull();
    expect(indicator.closest('.note-header')).not.toBeNull();
    // Nothing in the page spells a marker a note could come to contain.
    const markup = inject('renderedHtml');
    for (const forbidden of ['[AutoPaste', 'autopaste=', '<!-- autopaste']) {
      expect(markup).not.toContain(forbidden);
    }
  });
});
