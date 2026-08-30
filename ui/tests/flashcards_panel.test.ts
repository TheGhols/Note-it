import { afterEach, describe, expect, it, vi } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { reviewItems } from '../src/flashcards/extract.ts';
import { flashcardCountsIn, flashcardsIn } from '../src/editor/flashcardMark.ts';
import { FlashcardPanel } from '../src/ui/flashcardPanel.ts';
import { NoteMenu } from '../src/ui/menu.ts';
import { PaperColor } from '../src/bridge/types.ts';
import { appendCapture } from '../src/capture/autoPaste.ts';
import { declarationIn, ruleFor } from './support/stylesheet.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];
const NOTE = '11111111-1111-4111-8111-111111111111';
const ASSET = '22222222-2222-4222-8222-222222222222';
const IMAGE = `../assets/${NOTE}/${ASSET}.png`;

const FOUR = 'A :: B\n\nC :: D\n\nE :: F\n\nG :: H';

const open: Array<{ destroy(): void }> = [];

afterEach(() => {
  while (open.length > 0) open.pop()!.destroy();
  document.body.innerHTML = '';
});

function mount(markdown: string, random?: () => number) {
  const element = document.createElement('div');
  document.body.append(element);
  const note = new NoteEditor({ element, initialContent: markdown });
  open.push(note);

  const host = document.createElement('div');
  document.body.append(host);
  const handlers = { onClose: vi.fn() };
  const panel = new FlashcardPanel({ mount: host, handlers, random });
  open.push(panel);

  const state = note.getView().state;
  const sources = flashcardsIn(state);
  return {
    note,
    panel,
    handlers,
    sources,
    request: {
      items: reviewItems(sources),
      cards: sources.length,
      schema: state.schema,
    },
  };
}

function press(panel: FlashcardPanel, key: string, target?: HTMLElement): void {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
  (target ?? panel.element()).dispatchEvent(event);
}

function button(panel: FlashcardPanel, selector: string): HTMLButtonElement {
  return panel.element().querySelector<HTMLButtonElement>(selector)!;
}

function question(panel: FlashcardPanel): string {
  return panel.element().querySelector('.note-study-question')!.textContent ?? '';
}

function answer(panel: FlashcardPanel): string {
  return panel.element().querySelector('.note-study-answer')!.textContent ?? '';
}

function progress(panel: FlashcardPanel): string {
  return panel.element().querySelector('.note-study-progress')!.textContent ?? '';
}

describe('the study panel', () => {
  it('is closed, and holds nothing, until studying starts', () => {
    const { panel } = mount(FOUR);
    expect(panel.isOpen()).toBe(false);
    expect(panel.element().hidden).toBe(true);
    expect(question(panel)).toBe('');
  });

  it('refuses to open an empty sitting', () => {
    const { panel, request } = mount('Texto comum');
    panel.openPanel(request);

    expect(panel.isOpen()).toBe(false);
    expect(panel.element().hidden).toBe(true);
    expect(document.activeElement).not.toBe(panel.element());
  });

  it('opens on the first question with the answer out of sight', () => {
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);

    expect(panel.isOpen()).toBe(true);
    expect(panel.element().hidden).toBe(false);
    expect(question(panel)).toBe('A');
    expect(panel.isAnswerVisible()).toBe(false);
    expect(answer(panel)).toBe('');
    expect(button(panel, '.note-study-reveal').hidden).toBe(false);
  });

  it('says where in the sitting the reader is, and how big it is', () => {
    const { panel, request } = mount('A :: B\n\nC ::: D\n\nE :: F');
    panel.openPanel(request);

    // Three cards, one of them reversible: four questions. Both numbers are
    // shown because they differ, and only one of them is the progress.
    expect(panel.element().querySelector('.note-study-summary')!.textContent).toBe(
      '3 cartões · 4 revisões',
    );
    expect(progress(panel)).toBe('1 de 4');
    button(panel, '.note-study-next').click();
    expect(progress(panel)).toBe('2 de 4');
  });

  it('shows the answer when it is asked for', () => {
    const { panel, request } = mount('Pergunta :: Resposta');
    panel.openPanel(request);

    button(panel, '.note-study-reveal').click();
    expect(panel.isAnswerVisible()).toBe(true);
    expect(answer(panel)).toBe('Resposta');
    // The button has done its job and stops taking up the room.
    expect(button(panel, '.note-study-reveal').hidden).toBe(true);
  });

  it('moves between questions and hides the answer again each time', () => {
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);

    button(panel, '.note-study-reveal').click();
    expect(panel.isAnswerVisible()).toBe(true);

    button(panel, '.note-study-next').click();
    expect(question(panel)).toBe('C');
    expect(progress(panel)).toBe('2 de 4');
    expect(panel.isAnswerVisible()).toBe(false);

    button(panel, '.note-study-previous').click();
    expect(question(panel)).toBe('A');
    expect(panel.isAnswerVisible()).toBe(false);
  });

  it('disables the ends rather than wrapping silently', () => {
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);

    expect(button(panel, '.note-study-previous').disabled).toBe(true);
    expect(button(panel, '.note-study-next').disabled).toBe(false);

    for (let step = 0; step < 3; step += 1) button(panel, '.note-study-next').click();
    expect(progress(panel)).toBe('4 de 4');
    expect(button(panel, '.note-study-next').disabled).toBe(true);
    expect(button(panel, '.note-study-previous').disabled).toBe(false);
  });

  it('shuffles into the order the randomness dictates and starts again', () => {
    const { panel, request } = mount(FOUR, () => 0);
    panel.openPanel(request);
    button(panel, '.note-study-next').click();
    button(panel, '.note-study-reveal').click();

    button(panel, '.note-study-shuffle').click();
    expect(progress(panel)).toBe('1 de 4');
    expect(panel.isAnswerVisible()).toBe(false);
    expect(question(panel)).toBe('C');

    const seen = [question(panel)];
    for (let step = 0; step < 3; step += 1) {
      button(panel, '.note-study-next').click();
      seen.push(question(panel));
    }
    expect(seen).toEqual(['C', 'E', 'G', 'A']);
  });

  it('has nothing to shuffle with one question', () => {
    const { panel, request } = mount('A :: B');
    panel.openPanel(request);
    expect(button(panel, '.note-study-shuffle').disabled).toBe(true);
  });
});

describe('what studying draws', () => {
  it('keeps the formatting the note gave the words', () => {
    const { panel, request } = mount('**Bold** :: *Italic*');
    panel.openPanel(request);

    expect(panel.element().querySelector('.note-study-question strong')).not.toBeNull();
    button(panel, '.note-study-reveal').click();
    expect(panel.element().querySelector('.note-study-answer em')).not.toBeNull();
  });

  it('draws a list answer as a list', () => {
    const { panel, request } = mount('Tríade\n\n::\n\n- Febre\n- Icterícia\n- Dor');
    panel.openPanel(request);
    button(panel, '.note-study-reveal').click();

    expect(panel.element().querySelectorAll('.note-study-answer li')).toHaveLength(3);
  });

  it('draws a picture through the store’s own reference and fetches nothing', () => {
    const { panel, request } = mount(`![](${IMAGE}) :: Fibrilação atrial`);
    panel.openPanel(request);

    const image = panel.element().querySelector<HTMLImageElement>('.note-study-question img')!;
    expect(image).not.toBeNull();
    expect(image.getAttribute('src')).toBe(`note-it-asset:/${NOTE}/${ASSET}.png`);
    expect(image.getAttribute('data-note-it-src')).toBe(IMAGE);
    // Nothing remote, and nothing inlined: the same file the editor draws.
    expect(image.getAttribute('src')).not.toMatch(/^(https?|data|file):/);
  });

  it('gives a picture no controls, because studying is reading', () => {
    const { panel, request } = mount(`![](${IMAGE}) :: Fibrilação atrial`);
    panel.openPanel(request);

    const drawn = panel.element().querySelector('.note-study-question')!;
    // The frame, the handles and the alignment buttons belong to the editor's
    // node view. A serializer does not run node views, so there is nothing to
    // hide — and nothing to accidentally leave behind either.
    expect(drawn.querySelector('.note-image-frame')).toBeNull();
    expect(drawn.querySelector('.note-image-handle')).toBeNull();
    expect(drawn.querySelector('.note-image-controls')).toBeNull();
    expect(drawn.querySelector('[contenteditable]')).toBeNull();
    expect(drawn.querySelector('button')).toBeNull();
  });

  it('bounds a picture by the panel, whatever width the note stored', () => {
    const { panel, request } = mount(
      `<img src="${IMAGE}" alt="" data-note-it-width="900"> :: largo`,
    );
    panel.openPanel(request);

    const image = panel.element().querySelector<HTMLImageElement>('img')!;
    // The stored width is a preference and travels with the node...
    expect(image.getAttribute('style')).toContain('width:');
    // ...and the stylesheet is what stops it leaving the card.
    expect(declarationIn('.note-study-side .note-image', 'max-width')).toBe('100%');
    expect(declarationIn('.note-study-side .note-image', 'height')).toBe('auto');
  });

  it('never turns a card into markup', () => {
    // Two answers, and both matter. A script never reaches a card at all: the
    // sanitizer took it out on the way into the document, long before anything
    // here saw it, so there is not even a card to draw.
    const dangerous = mount('Perigo :: <script>alert(1)</script>');
    expect(dangerous.request.items).toHaveLength(0);
    expect(dangerous.note.getMarkdown()).not.toContain('script');

    // And the same characters written as code — which is text somebody meant
    // to keep — are drawn as text. This panel has no path that could make them
    // anything else: it serializes nodes, and never assigns markup.
    const quoted = mount('Perigo :: `<script>alert(1)</script>`');
    quoted.panel.openPanel(quoted.request);
    button(quoted.panel, '.note-study-reveal').click();

    const drawn = quoted.panel.element().querySelector('.note-study-answer')!;
    expect(drawn.querySelector('script')).toBeNull();
    expect(drawn.querySelector('code')).not.toBeNull();
    expect(drawn.textContent).toContain('alert(1)');
  });
});

describe('studying with the keyboard', () => {
  it('takes the keyboard when it opens and gives it back when it closes', () => {
    const { panel, request, handlers } = mount(FOUR);
    const invoker = document.createElement('button');
    document.body.append(invoker);

    panel.openPanel({ ...request, invoker });
    expect(document.activeElement).toBe(panel.element());

    panel.close();
    expect(document.activeElement).toBe(invoker);
    // The editor is not grabbed at random when there is somewhere to go back to.
    expect(handlers.onClose).not.toHaveBeenCalled();
  });

  it('asks for the caret back when whatever opened it has gone', () => {
    const { panel, request, handlers } = mount(FOUR);
    panel.openPanel(request);
    panel.close();
    expect(handlers.onClose).toHaveBeenCalledTimes(1);
  });

  it('closes on Escape', () => {
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);

    press(panel, 'Escape');
    expect(panel.isOpen()).toBe(false);
    expect(panel.element().hidden).toBe(true);
  });

  it('moves with the arrows and reveals with space', () => {
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);

    press(panel, ' ');
    expect(panel.isAnswerVisible()).toBe(true);

    press(panel, 'ArrowRight');
    expect(question(panel)).toBe('C');
    expect(panel.isAnswerVisible()).toBe(false);

    press(panel, 'ArrowLeft');
    expect(question(panel)).toBe('A');

    press(panel, 'Enter');
    expect(panel.isAnswerVisible()).toBe(true);
  });

  it('does not act twice when the key is already activating a button', () => {
    // The browser turns Enter and Space on a focused button into a click. A
    // panel handler that also acted would reveal an answer and skip past it in
    // the same press.
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);

    const reveal = button(panel, '.note-study-reveal');
    reveal.focus();
    press(panel, 'Enter', reveal);
    expect(panel.isAnswerVisible()).toBe(false);
    expect(progress(panel)).toBe('1 de 4');

    reveal.click();
    expect(panel.isAnswerVisible()).toBe(true);
    expect(progress(panel)).toBe('1 de 4');
  });

  it('keeps the keys inside the panel', () => {
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);

    const seen: string[] = [];
    document.addEventListener('keydown', (event) => seen.push(event.key));
    press(panel, 'ArrowRight');
    press(panel, 'Escape');
    // Stopped at the panel: an arrow moves to the next question, it does not
    // move the caret in the note behind it.
    expect(seen).toEqual([]);
  });

  it('means nothing at all while studying is closed', () => {
    const { panel, request } = mount(FOUR);
    press(panel, 'ArrowRight');
    press(panel, 'Escape');
    expect(panel.isOpen()).toBe(false);

    panel.openPanel(request);
    expect(progress(panel)).toBe('1 de 4');
  });

  it('is a dialog, and every control on it has a name', () => {
    const { panel, request } = mount(FOUR);
    panel.openPanel(request);
    const root = panel.element();

    expect(root.getAttribute('role')).toBe('dialog');
    expect(root.getAttribute('aria-label')).toBe('Flashcards');
    expect(root.tabIndex).toBe(-1);
    expect(panel.element().querySelector('.note-study-progress')!.getAttribute('role')).toBe(
      'status',
    );

    const names = Array.from(root.querySelectorAll('button'), (control) =>
      (control.getAttribute('aria-label') ?? control.textContent ?? '').trim(),
    );
    expect(names).toEqual([
      'Fechar',
      'Mostrar resposta',
      'Anterior',
      'Próximo',
      'Embaralhar',
    ]);
    for (const name of names) expect(name).not.toBe('');
  });
});

describe('studying writes nothing', () => {
  it('leaves the note byte for byte as it was', () => {
    // The proof this phase turns on. A whole sitting — open, reveal, forward,
    // back, shuffle, close — over a note whose Markdown is compared before and
    // after, and whose document is the same object throughout.
    const { note, panel, request } = mount(
      `A :: B\n\nC ::: D\n\n![](${IMAGE}) :: Fibrilação atrial\n\nTexto comum.`,
      () => 0,
    );
    const before = note.getMarkdown();
    const documentBefore = note.getView().state.doc;

    panel.openPanel(request);
    button(panel, '.note-study-reveal').click();
    button(panel, '.note-study-next').click();
    button(panel, '.note-study-reveal').click();
    button(panel, '.note-study-previous').click();
    button(panel, '.note-study-shuffle').click();
    press(panel, ' ');
    press(panel, 'ArrowRight');
    panel.close();

    expect(note.getMarkdown()).toBe(before);
    // Not merely equal text: the very same document. No transaction was
    // dispatched, so there is no undo step and nothing to autosave.
    expect(note.getView().state.doc).toBe(documentBefore);
    expect(note.hasPendingSave()).toBe(false);
  });

  it('cannot reach the document even in principle', () => {
    // Structural, not a promise: what the panel is given is a list of sides
    // and a schema. There is no editor, no view and no dispatch on it.
    const { panel } = mount(FOUR);
    const held = Object.values(panel as unknown as Record<string, unknown>);
    const writable = held.some(
      (value) =>
        typeof value === 'object' &&
        value !== null &&
        ('dispatch' in value || 'state' in value || 'commands' in value),
    );
    expect(writable).toBe(false);
    expect(Object.keys(panel).join(' ')).not.toMatch(/editor|view/i);
  });
});

describe('the sitting is a snapshot', () => {
  it('ignores an AutoPaste capture until it is closed and opened again', () => {
    const { note, panel, request } = mount('A :: B\n\nC :: D');
    panel.openPanel(request);

    expect(progress(panel)).toBe('1 de 2');
    expect(appendCapture(note.getView(), 'Novo :: Cartão', 'blankLine')).toBe(true);
    expect(flashcardCountsIn(note.getView().state)).toEqual({ cards: 3, reviews: 3 });

    button(panel, '.note-study-next').click();
    expect(progress(panel)).toBe('2 de 2');
    expect(button(panel, '.note-study-next').disabled).toBe(true);

    panel.close();
    const refreshed = flashcardsIn(note.getView().state);
    panel.openPanel({
      items: reviewItems(refreshed),
      cards: refreshed.length,
      schema: note.getView().state.schema,
    });

    expect(progress(panel)).toBe('1 de 3');
    button(panel, '.note-study-next').click();
    button(panel, '.note-study-next').click();
    expect(question(panel)).toBe('Novo');
  });
});

describe('the way in to studying', () => {
  function buildMenu(counts: { cards: number; reviews: number }) {
    const left = document.createElement('div');
    const trigger = document.createElement('button');
    trigger.id = 'btn-menu';
    left.append(trigger);
    document.body.append(left);

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
      onOpenStudy: vi.fn(),
      onToggleAutoPaste: vi.fn(),
      onSelectCaptureDelimiter: vi.fn(),
    };
    const menu = new NoteMenu({ trigger, mount: left, colors: COLORS, handlers });
    open.push(menu);
    menu.setFlashcardCounts(counts);
    return { menu, trigger, handlers };
  }

  it('offers studying from the menu and from nowhere else in the bar', () => {
    const { menu } = buildMenu({ cards: 2, reviews: 3 });
    const row = menu.element.querySelector<HTMLElement>('[data-panel="study"]')!;

    expect(row).not.toBeNull();
    expect(row.textContent).toContain('Estudo');
    // No eighth permanent button: the bar is full, and studying is not
    // something anyone reaches for mid-sentence.
    expect(document.querySelectorAll('.note-header .icon-btn')).toHaveLength(0);
  });

  it('says how many cards the note holds before the panel is opened', () => {
    const { menu } = buildMenu({ cards: 5, reviews: 7 });
    expect(menu.element.querySelector('[data-panel="study"] .note-menu-value')!.textContent).toBe(
      '5 cartões',
    );
    expect(menu.element.querySelector('.note-menu-study-summary')!.textContent).toBe(
      '5 cartões · 7 revisões',
    );
  });

  it('counts one card in the singular', () => {
    const { menu } = buildMenu({ cards: 1, reviews: 1 });
    expect(menu.element.querySelector('.note-menu-study-summary')!.textContent).toBe(
      '1 cartão · 1 revisão',
    );
  });

  it('says so plainly when there are none, and offers nothing to open', () => {
    const { menu } = buildMenu({ cards: 0, reviews: 0 });

    expect(menu.element.querySelector('.note-menu-study-summary')!.textContent).toBe(
      'Nenhum flashcard nesta nota',
    );
    const action = menu.element.querySelector<HTMLButtonElement>('.note-menu-study .note-menu-item')!;
    expect(action.disabled).toBe(true);
    expect(menu.element.querySelector('[data-panel="study"] .note-menu-value')!.textContent).toBe(
      'Nenhum',
    );
  });

  it('asks for studying, and gets out of the way first', () => {
    const { menu, trigger, handlers } = buildMenu({ cards: 2, reviews: 2 });
    trigger.click();
    menu.element.querySelector<HTMLElement>('[data-panel="study"]')!.click();

    const action = menu.element.querySelector<HTMLButtonElement>('.note-menu-study .note-menu-item')!;
    expect(action.disabled).toBe(false);
    action.click();

    expect(handlers.onOpenStudy).toHaveBeenCalledTimes(1);
    expect(menu.isOpen()).toBe(false);
  });

  it('tells the reader the syntax where they would look for it', () => {
    const { menu } = buildMenu({ cards: 0, reviews: 0 });
    const hint = menu.element.querySelector('.note-menu-study-hint')!.textContent ?? '';
    expect(hint).toContain('::');
    expect(hint).toContain(':::');
  });
});

describe('the count follows the note', () => {
  it('appears, changes and disappears as the delimiter is typed', () => {
    const note = new NoteEditor({
      element: document.createElement('div'),
      initialContent: 'A B',
    });
    open.push(note);

    expect(flashcardCountsIn(note.getView().state)).toEqual({ cards: 0, reviews: 0 });

    note.setMarkdown('A :: B');
    expect(flashcardCountsIn(note.getView().state)).toEqual({ cards: 1, reviews: 1 });

    // One more colon, and the same card is studied both ways.
    note.setMarkdown('A ::: B');
    expect(flashcardCountsIn(note.getView().state)).toEqual({ cards: 1, reviews: 2 });

    // Take the delimiter out and the card stops existing. No save, no reopen.
    note.setMarkdown('A B');
    expect(flashcardCountsIn(note.getView().state)).toEqual({ cards: 0, reviews: 0 });
  });

  it('tells the menu on every change to the document', () => {
    const seen: number[] = [];
    const note = new NoteEditor({
      element: document.createElement('div'),
      initialContent: '',
      onDocChange: () => seen.push(flashcardCountsIn(note.getView().state).cards),
    });
    open.push(note);

    note.setMarkdown('A :: B\n\nC :: D');
    note.setMarkdown('A :: B');
    expect(seen).toEqual([2, 1]);
    // Knowing the count saved nothing: loading a note is not an edit.
    expect(note.hasPendingSave()).toBe(false);
  });
});

describe('the mark the editor paints', () => {
  it('marks the delimiter without touching the text', () => {
    const note = new NoteEditor({
      element: document.createElement('div'),
      initialContent: 'A :: B',
    });
    open.push(note);

    const sources = flashcardsIn(note.getView().state);
    const { from, to } = sources[0].delimiter;
    expect(note.getView().state.doc.textBetween(from, to)).toBe('::');
    // The Markdown still says exactly what the reader typed.
    expect(note.getMarkdown()).toBe('A :: B');
  });

  it('is painted rather than written, on both forms', () => {
    for (const source of ['A :: B', 'A\n\n::\n\nB']) {
      const note = new NoteEditor({
        element: document.createElement('div'),
        initialContent: source,
      });
      open.push(note);
      const before = note.getMarkdown();
      const document_ = note.getView().state.doc;

      expect(flashcardsIn(note.getView().state)).toHaveLength(1);
      // Extraction and decoration are reads. Nothing was rewritten into a tag,
      // no `<flashcard>` exists, and the document is the same object.
      expect(note.getMarkdown()).toBe(before);
      expect(note.getView().state.doc).toBe(document_);
      expect(before).not.toContain('flashcard');
    }
  });

  it('has a style for the delimiter and the line, and both are faint', () => {
    expect(ruleFor('.ProseMirror .note-flashcard-mark')).not.toBeNull();
    expect(ruleFor('.ProseMirror .note-flashcard-line')).not.toBeNull();
    // Painted with the paper's own ink, so one rule serves all seven papers
    // and both themes rather than needing a palette of its own.
    expect(declarationIn('.ProseMirror .note-flashcard-mark', 'background-color')).toContain(
      'var(--paper-text)',
    );
    expect(declarationIn('.ProseMirror .note-flashcard-line', 'border-left')).toContain(
      'var(--paper-text)',
    );
  });
});

describe('the panel fits the note it is in', () => {
  it('is chrome, so it reads the same over every paper', () => {
    expect(declarationIn('.note-study', 'background-color')).toBe('var(--ui-surface)');
    expect(declarationIn('.note-study', 'color')).toBe('var(--ui-text)');
    // Fixed in pixels: a note's zoom scales the note, not the furniture.
    expect(declarationIn('.note-study', 'font-size')).toMatch(/px$/);
  });

  it('stays inside the note at every width, with the controls wrapping', () => {
    // 220px is the narrowest a note can be. The panel is positioned from both
    // edges rather than given a width, so there is no width at which it hangs
    // over the side, and the footer wraps instead of pushing a control off.
    expect(declarationIn('.note-study', 'left')).toBe('8px');
    expect(declarationIn('.note-study', 'right')).toBe('8px');
    expect(declarationIn('.note-study', 'box-sizing')).toBe('border-box');
    expect(declarationIn('.note-study-footer', 'flex-wrap')).toBe('wrap');
    expect(declarationIn('.note-study', 'top')).toContain('--note-header-height');
  });

  it('scrolls a long card instead of growing the note', () => {
    expect(declarationIn('.note-study-card', 'overflow-y')).toBe('auto');
    expect(declarationIn('.note-study', 'overflow')).toBe('hidden');
    expect(declarationIn('.note-study-card', 'min-height')).toBe('0');
  });
});
