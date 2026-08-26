import { describe, it, expect } from 'vitest';
import { sanitizeHtml } from '../src/markdown/sanitizer.ts';

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
    expect(clean).toContain('style="color: #D32F2F"');
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
});
