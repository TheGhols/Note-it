import { describe, it, expect } from 'vitest';
import { sanitizeHtml, sanitizeMarkdown } from '../src/markdown/sanitizer.ts';

describe('HTML Sanitizer', () => {
  it('strips script tags and executable content', () => {
    const dirty = '<p>Hello <script>alert("xss")</script>world</p>';
    const clean = sanitizeHtml(dirty);
    expect(clean).not.toContain('<script>');
    expect(clean).not.toContain('alert');
    expect(clean).toContain('Hello');
    expect(clean).toContain('world');
  });

  it('allows safe inline underline tags', () => {
    const input = '<p>This is <u>underlined</u> text</p>';
    const clean = sanitizeHtml(input);
    expect(clean).toContain('<u>underlined</u>');
  });

  it('allows safe span with data-note-it-color and hex color', () => {
    const input = '<p>Text with <span data-note-it-color="#D32F2F" style="color:#D32F2F">red color</span></p>';
    const clean = sanitizeHtml(input);
    expect(clean).toContain('data-note-it-color="#D32F2F"');
    expect(clean).toContain('style="color: #D32F2F;"');
  });

  it('allows safe mark with data-note-it-highlight', () => {
    const input = '<p>Text with <mark data-note-it-highlight="#FFF59D" style="background-color:#FFF59D">highlight</mark></p>';
    const clean = sanitizeHtml(input);
    expect(clean).toContain('data-note-it-highlight="#FFF59D"');
  });

  it('strips dangerous event handlers', () => {
    const input = '<p><span onclick="fetch(\'http://evil.com\')">Click</span></p>';
    const clean = sanitizeHtml(input);
    expect(clean).not.toContain('onclick');
  });

  it('keeps normal Markdown intact while sanitizing embedded HTML', () => {
    const input = '# Heading\n\n**bold** and <u>underline</u>\n\n- [x] task';
    expect(sanitizeMarkdown(input)).toBe(input);
  });

  it('preserves inline code containing script tags literally', () => {
    const input = 'Use `<script>alert(1)</script>` as code';
    expect(sanitizeMarkdown(input)).toBe('Use `<script>alert(1)</script>` as code');
  });

  it('preserves multi-backtick inline code with HTML chars literally', () => {
    const input = '``<script>`alert(1)`</script>``';
    expect(sanitizeMarkdown(input)).toBe('``<script>`alert(1)`</script>``');
  });

  it('preserves fenced code block containing script and dangerous handlers', () => {
    const input = '```html\n<script>alert("example")</script>\n<div onclick="test()">example</div>\n```';
    expect(sanitizeMarkdown(input)).toBe(input);
  });

  it('preserves fenced code block with tildes containing HTML literally', () => {
    const input = '~~~html\n<span onclick="alert(1)">code</span>\n~~~';
    expect(sanitizeMarkdown(input)).toBe(input);
  });

  it('preserves https autolinks in markdown', () => {
    const input = 'Check <https://example.com> and <http://example.com/path?a=1&b=2>';
    expect(sanitizeMarkdown(input)).toBe(input);
  });

  it('preserves email autolinks in markdown', () => {
    const input = 'Contact <user@example.com> or <john.doe+tag@example.org>';
    expect(sanitizeMarkdown(input)).toBe(input);
  });

  it('preserves custom Note-it HTML inside inline code literally', () => {
    const input = 'Use `<span data-note-it-color="#ff0000">` and `<mark data-note-it-highlight="#ffff00">`';
    expect(sanitizeMarkdown(input)).toBe(input);
  });

  it('preserves custom Note-it HTML inside fenced code literally', () => {
    const input = '```markdown\n<span data-note-it-color="#ff0000">raw</span>\n```';
    expect(sanitizeMarkdown(input)).toBe(input);
  });

  it('canonicalizes custom HTML outside code and removes event handlers', () => {
    expect(
      sanitizeMarkdown('<span onclick="alert(1)" data-note-it-color="#ff0000">ok</span>'),
    ).toBe('<span data-note-it-color="#ff0000">ok</span>');

    expect(
      sanitizeMarkdown('<mark onmouseover="bad()" data-note-it-highlight="#ffff00">ok</mark>'),
    ).toBe('<mark data-note-it-highlight="#ffff00">ok</mark>');

    expect(
      sanitizeMarkdown('<u>underlined</u>'),
    ).toBe('<u>underlined</u>');
  });

  it('neutralizes dangerous HTML tags and blocks outside code', () => {
    expect(sanitizeMarkdown('<script>alert(1)</script>')).toBe('');
    expect(sanitizeMarkdown('<iframe src="https://evil.com"></iframe>')).toBe('');
    expect(sanitizeMarkdown('<style>body { display: none; }</style>')).toBe('');
    expect(sanitizeMarkdown('<div onclick="alert(1)">hello</div>')).toBe('hello');
    expect(sanitizeMarkdown('<!-- comment -->text')).toBe('text');
  });

  it('neutralizes dangerous javascript: autolinks outside code', () => {
    expect(sanitizeMarkdown('<javascript:alert(1)>')).toBe('');
  });

  it('canonicalizes custom HTML and rejects non-hex colors', () => {
    const invalidSpan = sanitizeMarkdown(
      '<span data-note-it-color="red;background:url(...)">bad</span>',
    );
    expect(invalidSpan).toBe('bad');

    const invalidMark = sanitizeMarkdown(
      '<mark data-note-it-highlight="red;background:url(...)" onmouseover="x">bad</mark>',
    );
    expect(invalidMark).toBe('bad');
  });
});
