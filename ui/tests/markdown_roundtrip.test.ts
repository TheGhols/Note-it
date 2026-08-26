import { describe, it, expect } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';

describe('Tiptap 3 Markdown Round-Trip', () => {
  it('parses formatted markdown and serializes back correctly', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);

    const input = '# Heading 1\n\nThis is **bold** and *italic* text.';
    const editor = new NoteEditor({
      element: container,
      initialContent: input,
    });

    const output = editor.getMarkdown().trim();
    expect(output).toContain('# Heading 1');
    expect(output).toContain('**bold**');
    expect(output).toContain('*italic*');

    editor.destroy();
    container.remove();
  });

  it('handles task lists round-trip', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);

    const input = '- [ ] Task pending\n- [x] Task completed';
    const editor = new NoteEditor({
      element: container,
      initialContent: input,
    });

    const output = editor.getMarkdown().trim();
    expect(output).toContain('- [ ] Task pending');
    expect(output).toContain('- [x] Task completed');

    editor.destroy();
    container.remove();
  });

  it('handles underline and inline formatting', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);

    const input = 'Here is <u>underlined</u> text.';
    const editor = new NoteEditor({
      element: container,
      initialContent: input,
    });

    const output = editor.getMarkdown().trim();
    expect(output).toContain('<u>underlined</u>');

    editor.destroy();
    container.remove();
  });
});
