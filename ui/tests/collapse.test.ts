import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { collapseTransition } from '../src/ui/collapse.ts';

/** Mirrors the presentation switch main.ts applies on collapse/expand. */
function setCollapsed(collapsed: boolean): void {
  document.body.setAttribute('data-collapsed', String(collapsed));
}

describe('collapsed presentation', () => {
  let editor: NoteEditor | null = null;

  afterEach(() => {
    editor?.destroy();
    editor = null;
    document.body.innerHTML = '';
    document.body.removeAttribute('data-collapsed');
  });

  it('publishes the collapsed flag the stylesheet hides the editor with', () => {
    const editorWrapper = document.createElement('div');
    editorWrapper.className = 'editor-wrapper';
    const resizeHandle = document.createElement('div');
    resizeHandle.className = 'resize-handle';
    document.body.append(editorWrapper, resizeHandle);

    setCollapsed(true);
    expect(document.body.matches('[data-collapsed="true"]')).toBe(true);
    expect(editorWrapper.matches('body[data-collapsed="true"] .editor-wrapper')).toBe(true);
    expect(resizeHandle.matches('body[data-collapsed="true"] .resize-handle')).toBe(true);

    setCollapsed(false);
    expect(editorWrapper.matches('body[data-collapsed="true"] .editor-wrapper')).toBe(false);
    expect(resizeHandle.matches('body[data-collapsed="true"] .resize-handle')).toBe(false);
  });

  it('collapsing hides the editor without destroying it or its content', () => {
    const container = document.createElement('div');
    container.className = 'editor-wrapper';
    document.body.append(container);

    editor = new NoteEditor({ element: container, initialContent: '# Reunião\n\nJoão não faltou.' });
    const instance = editor.getRawEditor();
    const before = editor.getMarkdown();
    expect(before).toContain('Reunião');

    setCollapsed(true);

    expect(document.body.getAttribute('data-collapsed')).toBe('true');
    // Same live Tiptap instance, still mounted, content untouched.
    expect(editor.getRawEditor()).toBe(instance);
    expect(instance.isDestroyed).toBe(false);
    expect(container.isConnected).toBe(true);
    expect(editor.getMarkdown()).toBe(before);

    setCollapsed(false);

    expect(document.body.getAttribute('data-collapsed')).toBe('false');
    expect(editor.getRawEditor()).toBe(instance);
    expect(editor.getMarkdown()).toBe(before);
  });

  it('keeps formatting and edits made before collapsing', () => {
    const container = document.createElement('div');
    container.className = 'editor-wrapper';
    document.body.append(container);

    editor = new NoteEditor({ element: container, initialContent: '' });
    editor.setMarkdown('## Tarefas\n\n- item **importante**\n');
    const before = editor.getMarkdown();

    setCollapsed(true);
    setCollapsed(false);

    const after = editor.getMarkdown();
    expect(after).toBe(before);
    expect(after).toContain('## Tarefas');
    expect(after).toContain('**importante**');
  });
});

/**
 * What a change of collapse state obliges the page to do.
 *
 * Collapsing hides the editor with `display: none`, and an element that stops
 * being displayed stops holding the selection. Before this, expanding brought
 * the note back looking ready and deaf: every keystroke went nowhere until the
 * note was clicked, and a note on the desktop layer sits behind every window,
 * so there may be no click available to give it. That is the shape the
 * "Recolher nota does not work on the desktop layer" report actually had.
 */
describe('the collapse transition', () => {
  it('closes everything that needs room to be typed into, on the way in', () => {
    expect(collapseTransition(false, true)).toEqual({
      closePanels: true,
      restoreCaret: false,
    });
  });

  it('gives the caret back on the way out', () => {
    expect(collapseTransition(true, false)).toEqual({
      closePanels: false,
      restoreCaret: true,
    });
  });

  it('does not take the caret from a note that was never collapsed', () => {
    // The host sends the expanded state on every load, and a note being loaded
    // is not a note being expanded.
    expect(collapseTransition(false, false).restoreCaret).toBe(false);
  });

  it('a repeated collapse request changes nothing about where the caret is', () => {
    expect(collapseTransition(true, true).restoreCaret).toBe(false);
  });
});
