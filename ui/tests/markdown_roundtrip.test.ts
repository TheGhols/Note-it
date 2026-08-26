import { describe, it, expect } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';

describe('Tiptap 3 Markdown Round-Trip', () => {
  function createEditor(input: string): { editor: NoteEditor; container: HTMLDivElement } {
    const container = document.createElement('div');
    document.body.appendChild(container);
    return {
      container,
      editor: new NoteEditor({ element: container, initialContent: input }),
    };
  }

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

  it.each([
    ['color', '<span data-note-it-color="#ff0000">seguro</span>', 'data-note-it-color="#ff0000"'],
    [
      'highlight',
      '<mark data-note-it-highlight="#ffff00">seguro</mark>',
      'data-note-it-highlight="#ffff00"',
    ],
  ])('accepts supported %s HTML in the real editor pipeline', (_name, input, expected) => {
    const { editor, container } = createEditor(input);
    expect(editor.getMarkdown()).toContain(expected);
    expect(container.querySelector('[onclick]')).toBeNull();
    editor.destroy();
    container.remove();
  });

  it.each([
    ['<script>alert(1)</script>', 'script'],
    ['<span onclick="alert(1)" data-note-it-color="#ff0000">teste</span>', 'onclick'],
    ['<span data-note-it-color="red;background:url(...)">teste</span>', 'background:url'],
  ])('neutralizes unsafe HTML in the real editor pipeline: %s', (input, dangerousText) => {
    const { editor, container } = createEditor(input);
    const output = editor.getMarkdown();
    expect(container.querySelector('script, iframe, object, embed, [onclick]')).toBeNull();
    expect(output.toLowerCase()).not.toContain(dangerousText.toLowerCase());
    if (input.includes('red;background')) {
      expect(container.querySelector('[data-note-it-color]')).toBeNull();
    }
    editor.destroy();
    container.remove();
  });
});
