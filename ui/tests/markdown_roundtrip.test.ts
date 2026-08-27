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

  function typeText(editor: NoteEditor, text: string): void {
    const rawEditor = editor.getRawEditor();
    for (const character of text) {
      const { from, to } = rawEditor.state.selection;
      const handled = rawEditor.view.someProp('handleTextInput', (handler) =>
        handler(
          rawEditor.view,
          from,
          to,
          character,
          () => rawEditor.state.tr.insertText(character, from, to),
        ),
      );
      if (!handled) {
        rawEditor.view.dispatch(rawEditor.state.tr.insertText(character, from, to));
      }
    }
  }

  function appendBlock(editor: NoteEditor, markdownPrefix: string, text: string): void {
    const rawEditor = editor.getRawEditor();
    rawEditor.commands.focus('end');
    rawEditor.commands.enter();
    typeText(editor, `${markdownPrefix}${text}`);
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

  it('round-trips H1 through H6 without losing heading levels', () => {
    const input = [1, 2, 3, 4, 5, 6]
      .map((level) => `${'#'.repeat(level)} Heading ${level}`)
      .join('\n\n');
    const { editor, container } = createEditor(input);

    for (let level = 1; level <= 6; level += 1) {
      expect(container.querySelector(`h${level}`)?.textContent).toBe(`Heading ${level}`);
      expect(editor.getMarkdown()).toContain(`${'#'.repeat(level)} Heading ${level}`);
    }

    editor.setMarkdown(editor.getMarkdown());
    for (let level = 1; level <= 6; level += 1) {
      expect(container.querySelector(`h${level}`)?.textContent).toBe(`Heading ${level}`);
    }

    editor.destroy();
    container.remove();
  });

  it.each([1, 2, 3, 4, 5, 6])(
    'creates H%s by input rule in a new block after existing content',
    (level) => {
      const { editor, container } = createEditor('Existing paragraph before the heading.');
      appendBlock(editor, `${'#'.repeat(level)} `, `Typed H${level}`);

      expect(container.querySelector(`h${level}`)?.textContent).toBe(`Typed H${level}`);
      expect(editor.getMarkdown()).toContain(`${'#'.repeat(level)} Typed H${level}`);

      editor.destroy();
      container.remove();
    },
  );

  it('applies inline Markdown input rules in the middle and after earlier content', () => {
    const { editor, container } = createEditor('Before  after');
    const rawEditor = editor.getRawEditor();
    rawEditor.commands.setTextSelection(8);
    typeText(editor, '**bold**');
    rawEditor.commands.focus('end');
    typeText(editor, ' then *italic* and ~~strike~~');

    expect(container.querySelector('strong')?.textContent).toBe('bold');
    expect(container.querySelector('em')?.textContent).toBe('italic');
    expect(container.querySelector('s')?.textContent).toBe('strike');
    const markdown = editor.getMarkdown();
    expect(markdown).toContain('**bold**');
    expect(markdown).toContain('*italic*');
    expect(markdown).toContain('~~strike~~');

    editor.destroy();
    container.remove();
  });

  it('applies bold and italic input rules in the first paragraph', () => {
    const { editor, container } = createEditor('');
    typeText(editor, '**first bold** and *first italic*');

    expect(container.querySelector('strong')?.textContent).toBe('first bold');
    expect(container.querySelector('em')?.textContent).toBe('first italic');
    expect(editor.getMarkdown()).toContain('**first bold** and *first italic*');

    editor.destroy();
    container.remove();
  });

  it('keeps a hash literal in the middle of a paragraph', () => {
    const { editor, container } = createEditor('Text before ');
    editor.getRawEditor().commands.focus('end');
    typeText(editor, '# not a heading');

    expect(container.querySelector('h1, h2, h3, h4, h5, h6')).toBeNull();
    expect(editor.getMarkdown()).toContain('Text before # not a heading');

    editor.destroy();
    container.remove();
  });

  it('creates lists after several existing paragraphs and preserves three nested levels', () => {
    const existing = ['First paragraph.', 'Second paragraph.', 'Third paragraph.'].join('\n\n');
    const { editor, container } = createEditor(existing);
    appendBlock(editor, '- ', 'Later bullet');
    expect(container.querySelector('ul > li')?.textContent).toContain('Later bullet');

    const nested = [
      '1. First',
      '  1. Second',
      '    1. Third',
      '',
      '- Bullet first',
      '  - Bullet second',
      '    - Bullet third',
    ].join('\n');
    editor.setMarkdown(nested);

    expect(container.querySelectorAll('ol ol ol')).toHaveLength(1);
    expect(container.querySelectorAll('ul ul ul')).toHaveLength(1);
    expect(editor.getMarkdown()).toContain('    1. Third');
    expect(editor.getMarkdown()).toContain('    - Bullet third');

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

  it('preserves code blocks and inline code with HTML characters across Tiptap round-trip', () => {
    const input = [
      '# Code Integrity Test',
      '',
      'Inline: `<script>alert("inline")</script>` and `<span data-note-it-color="#ff0000">`',
      '',
      '```html',
      '<script>alert("fenced")</script>',
      '<div onclick="test()">example</div>',
      '```',
    ].join('\n');

    const { editor, container } = createEditor(input);
    const output = editor.getMarkdown();

    // Verify markdown serialization retains exact code content
    expect(output).toContain('`<script>alert("inline")</script>`');
    expect(output).toContain('`<span data-note-it-color="#ff0000">`');
    expect(output).toContain('<script>alert("fenced")</script>');
    expect(output).toContain('<div onclick="test()">example</div>');

    // Verify DOM structure does not contain executable elements
    expect(container.querySelectorAll('script').length).toBe(0);
    expect(container.querySelector('[onclick]')).toBeNull();

    // Verify DOM renders as pre/code text
    const codeElements = container.querySelectorAll('pre code, p code');
    expect(codeElements.length).toBeGreaterThanOrEqual(1);

    editor.destroy();
    container.remove();
  });

  it('preserves allowed autolinks and unsupported autolinks text in round-trip pipeline', () => {
    const input = [
      'Allowed: <https://example.com> and <mailto:user@example.com>',
      'Unsupported: <ftp://example.com> and <ssh://example.com> and <obsidian://open?vault=test>',
    ].join('\n\n');

    const { editor, container } = createEditor(input);
    const output = editor.getMarkdown();

    // Verify allowed schemes are preserved
    expect(output).toContain('https://example.com');
    expect(output).toContain('mailto:user@example.com');

    // Verify unsupported schemes preserved textual content without data loss
    expect(output).toContain('ftp://example.com');
    expect(output).toContain('ssh://example.com');
    expect(output).toContain('obsidian://open?vault=test');

    // Verify DOM safety: no script, no dangerous elements
    expect(container.querySelectorAll('script').length).toBe(0);

    editor.destroy();
    container.remove();
  });
});
