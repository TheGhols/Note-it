import { afterEach, describe, expect, it, vi } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { NoteMenu } from '../src/ui/menu.ts';
import { PaperColor } from '../src/bridge/types.ts';
import { CALLOUT_TYPES } from '../src/editor/callout.ts';
import { CODE_LANGUAGES } from '../src/editor/codeBlock.ts';
import { contrastRatio, composite, hexToRgb } from './support/color.ts';
import { declarationIn, tokenIn, tokensIn } from './support/stylesheet.ts';

const open: NoteEditor[] = [];

function mount(initial = ''): { note: NoteEditor; element: HTMLElement } {
  const element = document.createElement('div');
  document.body.append(element);
  const note = new NoteEditor({ element, initialContent: initial });
  open.push(note);
  return { note, element };
}

afterEach(() => {
  while (open.length) open.pop()!.destroy();
  document.body.innerHTML = '';
});

/** One note holding every kind of block the editor knows about. */
const MIXED_NOTE = [
  '# Título da nota',
  '',
  'Texto com **negrito**, *itálico*, <u>sublinhado</u> e um <span data-note-it-color="#1D4ED8" style="color:#1D4ED8">trecho colorido</span>.',
  '',
  'E um <mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">trecho marcado</mark> com <span data-note-it-font-size="22" style="font-size:22px">tamanho próprio</span>.',
  '',
  '- [ ] tarefa aberta',
  '- [x] tarefa concluída <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->',
  '',
  '```python',
  'def soma(a, b):',
  '    return a + b',
  '```',
  '',
  '> uma citação comum',
  '',
  '> [!WARNING]',
  '> Um aviso com **ênfase**.',
  '>',
  '> - detalhe',
  '',
  '<!-- lembrete que não é conteúdo -->',
  '',
  'Parágrafo final.',
].join('\n');

describe('smart blocks in one note', () => {
  it('carries every block through a save and a reload unchanged', () => {
    const { note } = mount(MIXED_NOTE);

    const saved = note.getMarkdown().trim();
    expect(saved).toBe(MIXED_NOTE);

    // Saving, closing and reopening is exactly this: the stored Markdown,
    // parsed again and written again. No block may absorb another.
    const reopened = mount(saved).note.getMarkdown().trim();
    expect(reopened).toBe(MIXED_NOTE);
  });

  it('renders each block as its own structure', () => {
    const { element } = mount(MIXED_NOTE);

    expect(element.querySelectorAll('h1')).toHaveLength(1);
    expect(element.querySelectorAll('u')).toHaveLength(1);
    expect(element.querySelectorAll('mark')).toHaveLength(1);
    expect(element.querySelectorAll('[data-note-it-font-size="22"]')).toHaveLength(1);
    expect(element.querySelectorAll('ul[data-type="taskList"] li')).toHaveLength(2);
    expect(element.querySelectorAll('[data-completed-at]')).toHaveLength(1);
    expect(element.querySelectorAll('pre code.language-python')).toHaveLength(1);
    expect(element.querySelectorAll('[data-note-it-comment]')).toHaveLength(1);

    // Two quotes, exactly one of which is a callout.
    const quotes = element.querySelectorAll('blockquote');
    expect(quotes).toHaveLength(2);
    expect(element.querySelectorAll('blockquote[data-callout="WARNING"]')).toHaveLength(1);
    // The plain quote holds its own text and nothing from its neighbour.
    expect(quotes[0].textContent).toBe('uma citação comum');
  });

  it('keeps the code block out of every other block', () => {
    const { element } = mount(MIXED_NOTE);
    const code = element.querySelector('pre')!;

    expect(code.closest('blockquote')).toBeNull();
    expect(code.closest('li')).toBeNull();
    expect(code.textContent).toContain('def soma(a, b):');
    // The task comment did not leak into the code, nor the code into the note.
    expect(code.textContent).not.toContain('completed_at');
  });

  it('writes no HTML of its own into the stored Markdown', () => {
    const saved = mount(MIXED_NOTE).note.getMarkdown();
    expect(saved).not.toContain('<pre');
    expect(saved).not.toContain('<blockquote');
    expect(saved).not.toContain('<div');
    expect(saved).not.toContain('hljs');
    expect(saved).not.toContain('data-callout');
  });
});

describe('the Blocos menu section', () => {
  const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];
  let active: NoteMenu | null = null;

  afterEach(() => {
    active?.destroy();
    active = null;
  });

  function mountMenu() {
    const mountPoint = document.createElement('div');
    const trigger = document.createElement('button');
    // Blocos is a header quick action now, so this is how the section is
    // reached. It is still the same panel and the same handlers.
    const blocksTrigger = document.createElement('button');
    blocksTrigger.id = 'btn-blocks';
    mountPoint.append(trigger, blocksTrigger);
    document.body.append(mountPoint);

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
    };
    const menu = new NoteMenu({
      trigger,
      mount: mountPoint,
      colors: COLORS,
      handlers,
      quickTriggers: { blocks: blocksTrigger },
    });
    active = menu;
    return { menu, trigger, blocksTrigger, handlers };
  }

  function click(element: Element): void {
    element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  }

  it('offers the four blocks in one section of the existing menu', () => {
    const { menu, blocksTrigger } = mountMenu();
    click(blocksTrigger);

    expect(menu.activePanel()).toBe('blocks');
    const labels = Array.from(
      menu.element.querySelectorAll<HTMLElement>('.note-menu-blocks .note-menu-item'),
    ).map((item) => item.firstChild?.textContent);
    expect(labels).toEqual([
      'Bloco de código',
      'Linguagem',
      'Callout',
      'Citação',
      'Comentário',
    ]);
  });

  it('offers every language and every callout kind', () => {
    const { menu, blocksTrigger } = mountMenu();
    menu.setBlockState({
      codeBlock: true,
      codeLanguage: 'python',
      blockquote: false,
      callout: null,
      comment: false,
    });
    click(blocksTrigger);
    click(menu.element.querySelector('[data-panel="codeLanguage"]')!);

    const languages = Array.from(
      menu.element.querySelectorAll<HTMLElement>('.note-menu-code-language .note-menu-option'),
    ).map((item) => item.dataset.value);
    expect(languages).toEqual(['', ...CODE_LANGUAGES.map((entry) => entry.id)]);

    click(menu.element.querySelector('[data-panel="callout"]')!);
    const callouts = Array.from(
      menu.element.querySelectorAll<HTMLElement>('.note-menu-callout .note-menu-option'),
    ).map((item) => item.dataset.value);
    expect(callouts).toEqual(['', ...CALLOUT_TYPES.map((entry) => entry.id)]);
  });

  it('shows what the cursor is sitting in', () => {
    const { menu } = mountMenu();
    const languageRow = menu.element.querySelector<HTMLButtonElement>(
      '[data-panel="codeLanguage"]',
    )!;
    const value = () => languageRow.querySelector('.note-menu-value')?.textContent;

    // Outside a code block the language row has nothing to say, and says so.
    menu.setBlockState({
      codeBlock: false,
      codeLanguage: null,
      blockquote: false,
      callout: null,
      comment: false,
    });
    expect(languageRow.disabled).toBe(true);
    expect(value()).toBe('—');

    menu.setBlockState({
      codeBlock: true,
      codeLanguage: 'py',
      blockquote: false,
      callout: null,
      comment: false,
    });
    expect(languageRow.disabled).toBe(false);
    // An alias is shown under the language it belongs to.
    expect(value()).toBe('Python');

    menu.setBlockState({
      codeBlock: false,
      codeLanguage: null,
      blockquote: true,
      callout: 'TIP',
      comment: false,
    });
    const calloutRow = menu.element.querySelector('[data-panel="callout"]')!;
    expect(calloutRow.querySelector('.note-menu-value')?.textContent).toBe('Dica');
  });

  it('reports each choice through the existing handler contract', () => {
    const { menu, blocksTrigger, handlers } = mountMenu();
    menu.setBlockState({
      codeBlock: true,
      codeLanguage: null,
      blockquote: false,
      callout: null,
      comment: false,
    });
    click(blocksTrigger);

    const item = (label: string) =>
      Array.from(
        menu.element.querySelectorAll<HTMLElement>('.note-menu-blocks .note-menu-item'),
      ).find((node) => node.firstChild?.textContent === label)!;

    click(item('Comentário'));
    expect(handlers.onInsertComment).toHaveBeenCalledTimes(1);
    expect(menu.isOpen()).toBe(false);

    click(blocksTrigger);
    click(item('Citação'));
    expect(handlers.onToggleBlockquote).toHaveBeenCalledTimes(1);

    click(blocksTrigger);
    click(item('Bloco de código'));
    expect(handlers.onToggleCodeBlock).toHaveBeenCalledTimes(1);

    click(blocksTrigger);
    click(menu.element.querySelector('[data-panel="callout"]')!);
    click(menu.element.querySelector('.note-menu-callout [data-value="TIP"]')!);
    expect(handlers.onSelectCallout).toHaveBeenCalledWith('TIP');

    click(blocksTrigger);
    click(menu.element.querySelector('[data-panel="codeLanguage"]')!);
    click(menu.element.querySelector('.note-menu-code-language [data-value=""]')!);
    expect(handlers.onSelectCodeLanguage).toHaveBeenCalledWith(null);
  });
});

describe('smart blocks stay cheap', () => {
  /** A note with many code blocks, which is the case highlighting could ruin. */
  function bigNote(blocks: number): string {
    const parts = ['# Nota grande', ''];
    for (let index = 0; index < blocks; index += 1) {
      parts.push(`Parágrafo ${index} com algum texto de contexto.`, '');
      parts.push('```python', `def f${index}(a, b):`, '    return a + b', '```', '');
    }
    parts.push('Parágrafo final onde vou digitar.');
    return parts.join('\n');
  }

  it('does not make typing expensive in a note full of code', () => {
    // The budgets are an order of magnitude above what this measures today
    // (roughly 70ms and 30ms), so they flag a highlighter that started
    // re-parsing the whole note on every keystroke without failing on the
    // ordinary variance of a shared machine.
    const element = document.createElement('div');
    document.body.append(element);

    const openedAt = Date.now();
    const note = new NoteEditor({ element, initialContent: bigNote(20) });
    open.push(note);
    expect(Date.now() - openedAt).toBeLessThan(2000);

    const editor = note.getRawEditor();
    editor.commands.focus('end');
    const typedAt = Date.now();
    for (let index = 0; index < 40; index += 1) editor.commands.insertContent('a');
    expect(Date.now() - typedAt).toBeLessThan(2000);

    // Every block is still there, and still highlighted.
    expect(element.querySelectorAll('pre')).toHaveLength(20);
    expect(element.querySelectorAll('[class*="hljs-"]').length).toBeGreaterThan(20);
  });

  it('loads only the grammars the menu offers', async () => {
    // A full `highlight.js` build carries nearly two hundred languages and
    // would cost more than the editor itself.
    const { default: hljs } = await import('highlight.js/lib/core');
    expect(hljs.listLanguages().length).toBeLessThanOrEqual(CODE_LANGUAGES.length + 4);
  });
});

/**
 * Readability of everything a smart block paints, measured against the paper
 * it is actually painted on rather than assumed.
 */
describe('smart block readability', () => {
  const PALE_PAPERS: Record<string, string> = {
    yellow: '#FEF9C3',
    blue: '#E0F2FE',
    green: '#DCFCE7',
    pink: '#FCE7F3',
    purple: '#F3E8FF',
    gray: '#F1F5F9',
  };
  const DARK_PAPER = '#18181B';

  /** `--block-bg` composited over a paper: what a block really sits on. */
  function blockGround(paper: string, ink: [number, number, number], alpha: number) {
    return composite(ink, alpha, hexToRgb(paper));
  }

  const PALE_INK: [number, number, number] = [15, 23, 42];
  const LIGHT_INK: [number, number, number] = [255, 255, 255];
  const PALE_ALPHA = 0.055;
  const DARK_ALPHA = 0.06;

  const paleTokens = tokensIn(':root');
  const darkTokens = tokensIn('body[data-color="black"]');

  const CODE_TOKENS = [
    '--code-comment',
    '--code-keyword',
    '--code-string',
    '--code-number',
    '--code-function',
    '--code-attribute',
    '--code-punctuation',
  ];
  const CALLOUT_TOKENS = [
    '--callout-note',
    '--callout-tip',
    '--callout-important',
    '--callout-warning',
    '--callout-caution',
  ];

  it('defines the block tint and both palettes once each', () => {
    expect(tokenIn(':root', '--block-ink')).toBe('15, 23, 42');
    expect(tokenIn('body[data-color="black"]', '--block-ink')).toBe('255, 255, 255');
    for (const token of [...CODE_TOKENS, ...CALLOUT_TOKENS]) {
      expect(paleTokens.get(token), `${token} on the pale papers`).toMatch(/^#[0-9A-Fa-f]{6}$/);
      expect(darkTokens.get(token), `${token} on the dark paper`).toMatch(/^#[0-9A-Fa-f]{6}$/);
      // The two palettes must actually differ, or one of the papers is
      // wearing colours chosen for the other.
      expect(paleTokens.get(token)).not.toBe(darkTokens.get(token));
    }
  });

  it.each([...CODE_TOKENS, ...CALLOUT_TOKENS])(
    '%s is readable on every pale paper',
    (token) => {
      const color = paleTokens.get(token)!;
      for (const [name, paper] of Object.entries(PALE_PAPERS)) {
        const ground = blockGround(paper, PALE_INK, PALE_ALPHA);
        expect(contrastRatio(color, ground), `${token} on ${name}`).toBeGreaterThanOrEqual(4.5);
      }
    },
  );

  it.each([...CODE_TOKENS, ...CALLOUT_TOKENS])('%s is readable on the dark paper', (token) => {
    const ground = blockGround(DARK_PAPER, LIGHT_INK, DARK_ALPHA);
    expect(contrastRatio(darkTokens.get(token)!, ground)).toBeGreaterThanOrEqual(4.5);
  });

  it('would have been unreadable with the other paper palette', () => {
    // The reason there are two: the pale palette disappears into the dark
    // paper, which is what a single palette would have shipped.
    const darkGround = blockGround(DARK_PAPER, LIGHT_INK, DARK_ALPHA);
    const failures = CODE_TOKENS.filter(
      (token) => contrastRatio(paleTokens.get(token)!, darkGround) < 4.5,
    );
    expect(failures.length).toBeGreaterThan(0);
  });

  it('paints the code, callout and comment grounds from the paper', () => {
    // None of the three brings a surface colour of its own, so a note keeps
    // its paper under every block.
    expect(declarationIn('.ProseMirror pre', 'background-color')).toBe('var(--block-bg)');
    expect(declarationIn('.ProseMirror blockquote[data-callout]', 'background-color')).toBe(
      'var(--block-bg)',
    );
    expect(declarationIn('.ProseMirror .note-comment', 'background-color')).toBe(
      'var(--block-bg)',
    );
    expect(tokenIn(':root', '--block-bg')).toBe('rgba(var(--block-ink), 0.055)');
    expect(tokenIn('body[data-color="black"]', '--block-bg')).toBe('rgba(var(--block-ink), 0.06)');
  });

  it('keeps code on the note own text colour and its lines intact', () => {
    expect(declarationIn('.ProseMirror pre code', 'color')).toBe('var(--paper-text)');
    expect(declarationIn('.ProseMirror pre', 'white-space')).toBe('pre');
  });

  it('draws the callout kind from the attribute, never from the note text', () => {
    expect(declarationIn('.ProseMirror blockquote[data-callout-label]::before', 'content')).toBe(
      'attr(data-callout-label)',
    );
    expect(declarationIn('.ProseMirror .note-comment::before', 'content')).toBe(
      'attr(data-note-it-comment-label)',
    );
  });
});
