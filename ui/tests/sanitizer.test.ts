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

  it('canonicalizes custom HTML and rejects non-hex colors', () => {
    expect(
      sanitizeMarkdown('<span onclick="alert(1)" data-note-it-color="#ff0000">ok</span>'),
    ).toBe('<span data-note-it-color="#ff0000">ok</span>');
    const invalid = sanitizeMarkdown(
      '<mark data-note-it-highlight="red;background:url(...)" onmouseover="x">bad</mark>',
    );
    expect(invalid).not.toContain('data-note-it-highlight');
    expect(invalid).not.toContain('onmouseover');
    expect(invalid).not.toContain('background:url');
  });
});
