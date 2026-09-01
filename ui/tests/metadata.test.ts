import { afterEach, describe, expect, it, vi } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { MetadataPanel, NoteTagStrip } from '../src/ui/metadataPanel.ts';
import type { MetadataView } from '../src/bridge/types.ts';
import { contrastRatio } from './support/color.ts';
import { declarationIn, ruleFor } from './support/stylesheet.ts';

const metadata: MetadataView = {
  tags: [
    { value: 'Medicina', colour: 0 },
    { value: 'PBL', colour: 1 },
    { value: 'Urgência', colour: 2 },
    { value: 'Clínica Médica', colour: 3 },
    { value: 'Projeto', colour: 4 },
  ],
  properties: [
    { key: 'tipo', value: 'estudo' },
    { key: 'disciplina', value: 'cardiologia' },
  ],
};

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

function submit(form: HTMLFormElement): void {
  form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
}

function mountPanel() {
  const mount = document.createElement('div');
  const invoker = document.createElement('button');
  document.body.append(invoker, mount);
  const handlers = {
    requestCatalog: vi.fn(),
    requestSuggestions: vi.fn(),
    save: vi.fn(),
    onOpen: vi.fn(),
    onClose: vi.fn(),
  };
  const panel = new MetadataPanel(mount, handlers);
  panel.setMetadata(metadata);
  return { panel, handlers, invoker };
}

describe('metadata panel', () => {
  let panels: MetadataPanel[] = [];
  let editors: NoteEditor[] = [];

  afterEach(() => {
    for (const panel of panels) panel.destroy();
    for (const editor of editors) editor.destroy();
    panels = [];
    editors = [];
    document.body.innerHTML = '';
    document.body.removeAttribute('data-has-tags');
  });

  it('opens on Tags, asks for derived suggestions, and Escape returns focus', () => {
    const { panel, handlers, invoker } = mountPanel();
    panels.push(panel);
    panel.open('tags', invoker);

    expect(panel.isOpen()).toBe(true);
    expect(panel.activeSection()).toBe('tags');
    expect(handlers.requestCatalog).toHaveBeenCalledWith(1);
    expect(panel.element.querySelectorAll('.metadata-chip')).toHaveLength(5);

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(panel.isOpen()).toBe(false);
    expect(document.activeElement).toBe(invoker);
    expect(handlers.onClose).toHaveBeenCalledTimes(1);
  });

  it('adds and removes tags only through an explicit confirmed save', () => {
    const { panel, handlers } = mountPanel();
    panels.push(panel);
    panel.open('tags');
    const input = panel.element.querySelector<HTMLInputElement>('[aria-label="Nova tag"]')!;
    input.value = '#Saúde';
    expect(handlers.save).not.toHaveBeenCalled();
    submit(input.closest('form')!);
    expect(handlers.save).toHaveBeenCalledWith(
      2,
      expect.objectContaining({ tags: ['Medicina', 'PBL', 'Urgência', 'Clínica Médica', 'Projeto', '#Saúde'] }),
    );

    panel.resolveSave(2, true, 'Metadados salvos', {
      ...metadata,
      tags: [...metadata.tags, { value: 'Saúde', colour: 5 }],
    });
    click(panel.element.querySelector<HTMLButtonElement>('[aria-label="Remover tag PBL"]')!);
    expect(handlers.save).toHaveBeenLastCalledWith(
      3,
      expect.objectContaining({ tags: ['Medicina', 'Urgência', 'Clínica Médica', 'Projeto', 'Saúde'] }),
    );
  });

  it('adds, edits, and removes properties in the scrollable Properties section', () => {
    const { panel, handlers } = mountPanel();
    panels.push(panel);
    panel.open('properties');
    expect(panel.element.querySelectorAll('.note-metadata-property')).toHaveLength(2);

    const existingValues = panel.element.querySelectorAll<HTMLInputElement>(
      '.note-metadata-property input[placeholder="Valor"]',
    );
    existingValues[0].value = 'revisão';
    click(panel.element.querySelector<HTMLButtonElement>('.note-metadata-add-property')!);
    const rows = panel.element.querySelectorAll<HTMLElement>('.note-metadata-property');
    const addedInputs = rows[2].querySelectorAll<HTMLInputElement>('input');
    addedInputs[0].value = 'fonte';
    addedInputs[1].value = 'Harrison';
    click(rows[1].querySelector<HTMLButtonElement>('button')!);
    submit(panel.element.querySelector<HTMLFormElement>('.note-metadata-properties')!);

    expect(handlers.save).toHaveBeenCalledWith(2, {
      tags: metadata.tags.map((tag) => tag.value),
      properties: [
        { key: 'tipo', value: 'revisão' },
        { key: 'fonte', value: 'Harrison' },
      ],
    });
    expect(declarationIn('.note-metadata-body', 'overflow-y')).toBe('auto');
  });

  it('renders hostile metadata as text and never creates executable DOM', () => {
    const hostile: MetadataView = {
      tags: [{ value: '<script>alert(1)</script>', colour: 0 }],
      properties: [
        { key: '<img onerror=alert(1)>', value: '"</div><script>alert(2)</script>' },
      ],
    };
    const { panel } = mountPanel();
    panels.push(panel);
    panel.setMetadata(hostile);
    panel.open('tags');
    expect(panel.element.querySelector('script')).toBeNull();
    expect(panel.element.querySelector('img')).toBeNull();
    expect(panel.element.textContent).toContain('<script>alert(1)</script>');
    click(panel.element.querySelector<HTMLButtonElement>('[role="tab"]:last-of-type')!);
    expect(panel.element.querySelector('script')).toBeNull();
    expect(panel.element.querySelector<HTMLInputElement>('[placeholder="Chave"]')!.value).toBe(
      '<img onerror=alert(1)>',
    );
  });

  it('does not put metadata into ProseMirror or create an editor transaction', () => {
    const editorMount = document.createElement('div');
    document.body.append(editorMount);
    const editor = new NoteEditor({ element: editorMount, initialContent: '# Corpo\n\nTexto' });
    editors.push(editor);
    const before = editor.getMarkdown();
    const { panel } = mountPanel();
    panels.push(panel);
    panel.open('tags');
    expect(editor.getMarkdown()).toBe(before);
    expect(editorMount.textContent).not.toContain('Medicina');
    expect(editorMount.textContent).not.toContain('cardiologia');
  });

  it('offers host-matched catalog values without writing merely because a suggestion arrived', () => {
    const { panel, handlers } = mountPanel();
    panels.push(panel);
    panel.open('tags');
    panel.setCatalog(1, {
      tags: [{ tag: 'Hotel', noteCount: 6 }],
      propertyKeys: [{ key: 'status', noteCount: 9 }],
    });
    const tagInput = panel.element.querySelector<HTMLInputElement>('[aria-label="Nova tag"]')!;
    tagInput.value = 'hotel';
    tagInput.dispatchEvent(new Event('input', { bubbles: true }));
    expect(handlers.requestSuggestions).toHaveBeenCalledWith(1, 'tag', 'hotel');
    panel.setSuggestions(1, ['Hotel']);
    expect(panel.element.querySelector('[role="option"]')?.textContent).toBe('Hotel');
    expect(handlers.save).not.toHaveBeenCalled();
    click(panel.element.querySelectorAll<HTMLButtonElement>('[role="tab"]')[1]);
    const keyInput = panel.element.querySelector<HTMLInputElement>('[aria-label="Chave da propriedade"]')!;
    keyInput.value = 'STATUS';
    keyInput.dispatchEvent(new Event('input', { bubbles: true }));
    expect(handlers.requestSuggestions).toHaveBeenLastCalledWith(2, 'property_key', 'STATUS');
    panel.setSuggestions(2, ['status']);
    expect(panel.element.querySelector('[role="option"]')?.textContent).toBe('status');
    expect(handlers.save).not.toHaveBeenCalled();
  });

  it('keeps an unconfirmed draft intact when the asynchronous catalog arrives', () => {
    const { panel } = mountPanel();
    panels.push(panel);
    panel.open('tags');
    const input = panel.element.querySelector<HTMLInputElement>('[aria-label="Nova tag"]')!;
    input.value = 'rascunho ainda não confirmado';

    panel.setCatalog(1, {
      tags: [{ tag: 'Hotel', noteCount: 6 }],
      propertyKeys: [{ key: 'status', noteCount: 9 }],
    });

    expect(input.isConnected).toBe(true);
    expect(input.value).toBe('rascunho ainda não confirmado');
    expect(input.title).toBe('1 tags usadas nas notas vivas');
  });
});

describe('responsive tag strip and presentation', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    document.body.removeAttribute('data-has-tags');
    document.body.removeAttribute('data-collapsed');
  });

  function mountStrip() {
    const root = document.createElement('div');
    const open = vi.fn();
    document.body.append(root);
    const strip = new NoteTagStrip(root, open);
    return { root, strip, open };
  }

  it('renders pills in one row, stable numeric colours, and +N overflow', () => {
    const { root, strip } = mountStrip();
    strip.setMetadata(metadata, 500, 400);
    expect(root.querySelectorAll('.metadata-chip')).toHaveLength(4);
    expect(root.querySelector('.note-tag-overflow')?.textContent).toBe('+1');
    expect(root.querySelector('.metadata-chip')?.getAttribute('data-colour')).toBe('0');
    expect(root.getAttribute('style')).toBeNull();
  });

  it('uses one compact counter for narrow or low notes and remains keyboard-openable', () => {
    const { root, strip, open } = mountStrip();
    strip.setMetadata(metadata, 220, 500);
    expect(root.textContent).toBe('5 tags');
    root.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(open).toHaveBeenCalledTimes(1);
    strip.render(500, 180);
    expect(root.textContent).toBe('5 tags');
  });

  it('is absent without tags and the collapsed stylesheet removes the whole row', () => {
    const { root, strip } = mountStrip();
    strip.setMetadata({ tags: [], properties: [] }, 500, 500);
    expect(root.hidden).toBe(true);
    expect(ruleFor('body[data-collapsed="true"] .note-tags-line').body).toContain('display: none');
  });

  it('keeps the editor clear of a visible row at both 90% and 160% UI scale', () => {
    expect(declarationIn('body[data-has-tags="true"] .editor-wrapper', 'padding-top')).toContain(
      '--ui-scale',
    );
    expect(declarationIn('.note-tags-line', 'overflow')).toBe('hidden');
    expect(declarationIn('.note-tags-line', 'height')).toContain('--ui-scale');
  });

  it('uses seven palette pairs with readable contrast on light and black papers in both themes', () => {
    const palette = [
      ['#1e3a8a', '#dbeafe'],
      ['#14532d', '#dcfce7'],
      ['#78350f', '#fef3c7'],
      ['#831843', '#fce7f3'],
      ['#4c1d95', '#ede9fe'],
      ['#164e63', '#cffafe'],
      ['#7f1d1d', '#fee2e2'],
    ];
    for (const [foreground, background] of palette) {
      expect(contrastRatio(foreground, background)).toBeGreaterThanOrEqual(4.5);
    }
    expect(declarationIn('.metadata-chip', 'color')).toBe('#1e3a8a');
    expect(declarationIn('.metadata-chip', 'background')).toBe('#dbeafe');
    expect(ruleFor('.note-metadata').body).toContain('--ui-scale');
    expect(ruleFor('.note-menu').body).toContain('--ui-surface');
  });
});
