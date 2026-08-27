import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';

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
