import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  PaperColor,
  PaperIntensity,
  PaperType,
  ThemePreference,
  WebviewToHostMessage,
} from '../src/bridge/types.ts';

/** The two messages this wiring can produce, so payloads stay readable. */
type AppearanceMessage = Extract<
  WebviewToHostMessage,
  { type: 'paper_changed' } | { type: 'theme_changed' }
>;
import { NoteEditor } from '../src/editor/editor.ts';
import { NoteMenu } from '../src/ui/menu.ts';
import { applyPaper, normalizePaperIntensity, normalizePaperType } from '../src/ui/paper.ts';
import { normalizeTheme, ThemeController } from '../src/ui/theme.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];

/**
 * Mirrors the paper and theme wiring from main.ts: choosing from the menu
 * applies the change at once and asks the host to persist it, while loading a
 * note applies it without sending anything back.
 */
function buildNote() {
  document.body.innerHTML = '';
  const app = document.createElement('div');
  app.id = 'app';
  const left = document.createElement('div');
  left.id = 'note-controls-left';
  const btnMenu = document.createElement('button');
  btnMenu.id = 'btn-menu';
  left.append(btnMenu);
  app.append(left);
  document.body.append(app);

  const root = document.createElement('div');
  const sent: AppearanceMessage[] = [];
  const themeController = new ThemeController(root, {} as Window);

  const state = {
    id: 'a2b0c1d2-0000-4000-8000-000000000001',
    paperType: 'blank' as PaperType,
    paperIntensity: 'normal' as PaperIntensity,
    theme: 'system' as ThemePreference,
  };

  function setPaper(type: PaperType, intensity: PaperIntensity, persist: boolean): void {
    const changed = type !== state.paperType || intensity !== state.paperIntensity;
    state.paperType = type;
    state.paperIntensity = intensity;
    applyPaper(document.body, type, intensity);
    menu.setPaper(type, intensity);
    if (persist && changed) {
      sent.push({
        type: 'paper_changed',
        payload: { id: state.id, paperType: type, paperIntensity: intensity },
      });
    }
  }

  function setTheme(theme: ThemePreference, persist: boolean): void {
    const changed = theme !== state.theme;
    state.theme = theme;
    themeController.setPreference(theme);
    menu.setTheme(theme);
    if (persist && changed) {
      sent.push({ type: 'theme_changed', payload: { theme } });
    }
  }

  const handlers = {
    onSelectColor: vi.fn(),
    onSelectPaperType: vi.fn((type: PaperType) => setPaper(type, state.paperIntensity, true)),
    onSelectPaperIntensity: vi.fn((intensity: PaperIntensity) =>
      setPaper(state.paperType, intensity, true),
    ),
    onSelectTheme: vi.fn((theme: ThemePreference) => setTheme(theme, true)),
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
  };
  const menu = new NoteMenu({ trigger: btnMenu, mount: left, colors: COLORS, handlers });

  /** The `load_note` path: applied, never echoed back to the host. */
  function loadNote(payload: { paperType?: unknown; paperIntensity?: unknown; theme?: unknown }) {
    setPaper(
      normalizePaperType(payload.paperType),
      normalizePaperIntensity(payload.paperIntensity),
      false,
    );
    setTheme(normalizeTheme(payload.theme), false);
  }

  return { menu, btnMenu, root, sent, state, loadNote, setTheme, themeController };
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

/** The class the menu gives each panel's body. */
const PANEL_CLASS: Record<string, string> = {
  paperType: 'note-menu-paper-type',
  paperIntensity: 'note-menu-paper-intensity',
  theme: 'note-menu-theme',
};

/** Opens the menu, walks into a submenu and clicks the option by its label. */
function chooseFrom(note: ReturnType<typeof buildNote>, panel: string, label: string): void {
  click(note.btnMenu);
  click(note.menu.element.querySelector(`[data-panel="${panel}"]`)!);
  const option = Array.from(
    note.menu.element.querySelectorAll<HTMLElement>(
      `.${PANEL_CLASS[panel]} .note-menu-option`,
    ),
  ).find((candidate) => candidate.textContent?.trim() === label);
  if (!option) throw new Error(`no "${label}" in ${panel}`);
  click(option);
}

describe('choosing a paper from the menu', () => {
  let note: ReturnType<typeof buildNote> | null = null;

  afterEach(() => {
    note?.menu.destroy();
    note?.themeController.destroy();
    note = null;
    document.body.innerHTML = '';
    document.body.removeAttribute('data-paper');
    document.body.removeAttribute('data-paper-intensity');
  });

  it('applies immediately and asks the host to persist it, once', () => {
    note = buildNote();

    chooseFrom(note, 'paperType', 'Pautado');

    // Applied to the page before anything is sent, so the note changes now.
    expect(document.body.getAttribute('data-paper')).toBe('lined');
    expect(note.sent).toEqual([
      {
        type: 'paper_changed',
        payload: {
          id: note.state.id,
          paperType: 'lined',
          paperIntensity: 'normal',
        },
      },
    ]);
  });

  it('carries both halves of the paper whichever one was chosen', () => {
    note = buildNote();

    chooseFrom(note, 'paperType', 'Quadriculado grande');
    chooseFrom(note, 'paperIntensity', 'Suave');

    expect(note.sent.map((message) => message.payload)).toEqual([
      { id: note.state.id, paperType: 'grid-large', paperIntensity: 'normal' },
      { id: note.state.id, paperType: 'grid-large', paperIntensity: 'subtle' },
    ]);
    expect(document.body.getAttribute('data-paper')).toBe('grid-large');
    expect(document.body.getAttribute('data-paper-intensity')).toBe('subtle');
  });

  it('says nothing when the choice is the one already in force', () => {
    note = buildNote();
    chooseFrom(note, 'paperType', 'Pontilhado');
    note.sent.length = 0;

    chooseFrom(note, 'paperType', 'Pontilhado');
    expect(note.sent).toEqual([]);
  });

  it('applies a loaded note without echoing it back to the host', () => {
    note = buildNote();

    note.loadNote({ paperType: 'grid-small', paperIntensity: 'strong', theme: 'dark' });

    expect(document.body.getAttribute('data-paper')).toBe('grid-small');
    expect(document.body.getAttribute('data-paper-intensity')).toBe('strong');
    expect(note.root.getAttribute('data-theme')).toBe('dark');
    expect(note.sent).toEqual([]);
  });

  it('opens a note that predates the paper on plain paper', () => {
    note = buildNote();

    note.loadNote({});

    expect(document.body.getAttribute('data-paper')).toBe('blank');
    expect(document.body.getAttribute('data-paper-intensity')).toBe('normal');
    expect(note.sent).toEqual([]);
  });
});

describe('choosing a theme from the menu', () => {
  let note: ReturnType<typeof buildNote> | null = null;

  afterEach(() => {
    note?.menu.destroy();
    note?.themeController.destroy();
    note = null;
    document.body.innerHTML = '';
  });

  it('applies immediately and asks the host to share it', () => {
    note = buildNote();

    chooseFrom(note, 'theme', 'Escuro');

    expect(note.root.getAttribute('data-theme')).toBe('dark');
    expect(note.sent).toEqual([{ type: 'theme_changed', payload: { theme: 'dark' } }]);
  });

  it('accepts a theme another note chose, without sending it back', () => {
    note = buildNote();
    // What `set_theme` does when it arrives from the host's broadcast.
    note.setTheme('dark', false);

    expect(note.root.getAttribute('data-theme')).toBe('dark');
    expect(note.sent).toEqual([]);
  });

  it('never changes the note’s own paper', () => {
    note = buildNote();
    note.loadNote({ paperType: 'lined', paperIntensity: 'strong', theme: 'light' });

    chooseFrom(note, 'theme', 'Escuro');

    // The whole separation: the theme dresses the chrome, the note keeps its
    // paper.
    expect(document.body.getAttribute('data-paper')).toBe('lined');
    expect(document.body.getAttribute('data-paper-intensity')).toBe('strong');
    expect(note.state.paperType).toBe('lined');
  });
});

describe('the paper never reaches the document', () => {
  let editors: NoteEditor[] = [];

  afterEach(() => {
    for (const editor of editors) editor.destroy();
    editors = [];
    document.body.innerHTML = '';
    document.body.removeAttribute('data-paper');
    document.body.removeAttribute('data-paper-intensity');
  });

  it('round-trips the Markdown unchanged through every paper', () => {
    const element = document.createElement('div');
    document.body.append(element);
    const note = new NoteEditor({ element, initialContent: '' });
    editors.push(note);

    const source = [
      '# Reunião',
      '',
      '- [ ] Comprar <mark data-note-it-highlight="#FDE68A">material</mark>',
      '- [x] Enviar relatório',
      '',
      'Seguir ➜ <u>este ponto</u>.',
    ].join('\n');
    note.setMarkdown(source);
    const before = note.getMarkdown();

    for (const type of ['lined', 'dotted', 'grid-small', 'grid-large', 'blank'] as PaperType[]) {
      for (const intensity of ['subtle', 'normal', 'strong'] as PaperIntensity[]) {
        applyPaper(document.body, type, intensity);
        const after = note.getMarkdown();
        expect(after, `${type}/${intensity}`).toBe(before);
        // No decoration is smuggled in as markup either.
        expect(after).not.toContain('data-paper');
        expect(after).not.toContain('class="grid"');
      }
    }
  });
});
