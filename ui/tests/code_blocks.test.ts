import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import {
  CODE_LANGUAGES,
  canHighlight,
  codeLanguageLabel,
} from '../src/editor/codeBlock.ts';

const open: NoteEditor[] = [];

function mount(initial = ''): { note: NoteEditor; element: HTMLElement } {
  const element = document.createElement('div');
  document.body.append(element);
  const note = new NoteEditor({ element, initialContent: initial });
  open.push(note);
  return { note, element };
}

/** Markdown in, Markdown out — the trip a note takes on every save. */
function roundTrip(markdown: string): string {
  return mount(markdown).note.getMarkdown().trim();
}

/** ...and again, which is what a save, close and reopen amounts to. */
function reopen(markdown: string): string {
  return roundTrip(roundTrip(markdown));
}

afterEach(() => {
  while (open.length) open.pop()!.destroy();
  document.body.innerHTML = '';
});

describe('code blocks', () => {
  it('round-trips a fence with no language', () => {
    const source = '```\nsem linguagem\n```';
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);
  });

  it.each(['python', 'javascript', 'rust', 'typescript', 'json', 'sql'])(
    'round-trips a `%s` fence with its language intact',
    (language) => {
      const source = `\`\`\`${language}\ncorpo\n\`\`\``;
      expect(roundTrip(source)).toBe(source);
      expect(reopen(source)).toBe(source);
    },
  );

  it('keeps a language it cannot highlight exactly as written', () => {
    // Nothing here knows `brainfuck`, and the note must not be edited for it.
    const source = '```brainfuck\n+++[->+<]\n```';
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);

    const { element } = mount(source);
    expect(element.querySelector('code')?.className).toBe('language-brainfuck');
    // Unhighlighted, rather than guessed at.
    expect(element.querySelectorAll('[class*="hljs-"]')).toHaveLength(0);
  });

  it('never guesses a language for a fence that has none', () => {
    const { element } = mount('```\ndef f(): return 1\n```');
    expect(element.querySelector('code')?.className).toBe('');
    expect(element.querySelectorAll('[class*="hljs-"]')).toHaveLength(0);
  });

  it('preserves indentation, blank lines and trailing structure', () => {
    const source = [
      '```python',
      'def f(x):',
      '    if x:',
      '        return 1',
      '',
      '    return 0',
      '```',
    ].join('\n');
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);
  });

  it('keeps HTML inside the block as literal code', () => {
    const source = '```html\n<script>alert(1)</script>\n<div class="x">&amp;</div>\n```';
    expect(roundTrip(source)).toBe(source);

    const { element } = mount(source);
    const code = element.querySelector('code')!;
    // The angle brackets are text. Nothing was parsed into an element, and no
    // script reached the document.
    expect(code.textContent).toContain('<script>alert(1)</script>');
    expect(element.querySelector('script')).toBeNull();
    expect(element.querySelector('code div')).toBeNull();
  });

  it('keeps backticks inside a longer fence', () => {
    const source = '````markdown\n```js\nconst a = 1;\n```\n````';
    expect(roundTrip(source)).toBe(source);
    expect(reopen(source)).toBe(source);
  });

  it('leaves the arrow substitution and inline formatting out of code', () => {
    // `->` becomes a real arrow in prose. Inside a block it is source.
    const source = '```c\nint *p = a->b;\n```';
    expect(roundTrip(source)).toBe(source);
    expect(roundTrip(source)).not.toContain('➜');
  });

  it('highlights a known language without touching the stored Markdown', () => {
    const source = '```python\ndef soma(a, b):\n    return a + b\n```';
    const { note, element } = mount(source);

    expect(element.querySelectorAll('[class*="hljs-"]').length).toBeGreaterThan(0);
    // Decorations only: the note is a plain fence.
    expect(note.getMarkdown().trim()).toBe(source);
    expect(note.getMarkdown()).not.toContain('hljs');
    expect(note.getMarkdown()).not.toContain('<span');
  });

  it('highlights every language the menu offers', () => {
    for (const entry of CODE_LANGUAGES) {
      expect(canHighlight(entry.id), entry.id).toBe(true);
    }
  });

  it.each([
    ['js', 'JavaScript'],
    ['ts', 'TypeScript'],
    ['py', 'Python'],
    ['sh', 'Bash / Shell'],
    ['cpp', 'C++'],
    ['yml', 'YAML'],
  ])('resolves the alias `%s` to %s', (alias, label) => {
    expect(canHighlight(alias)).toBe(true);
    expect(codeLanguageLabel(alias)).toBe(label);
    // The alias is what the note keeps; only the label is resolved.
    const source = `\`\`\`${alias}\ncorpo\n\`\`\``;
    expect(roundTrip(source)).toBe(source);
  });

  it('reports an unknown language as itself rather than inventing one', () => {
    expect(codeLanguageLabel('brainfuck')).toBe('brainfuck');
    expect(codeLanguageLabel(null)).toBe('Sem linguagem');
    expect(codeLanguageLabel('')).toBe('Sem linguagem');
    expect(canHighlight('brainfuck')).toBe(false);
  });

  it('changes the language from the menu without touching the code', () => {
    const { note } = mount('```python\nprint(1)\n```');
    note.setCodeLanguage('rust');
    expect(note.getMarkdown().trim()).toBe('```rust\nprint(1)\n```');

    note.setCodeLanguage(null);
    expect(note.getMarkdown().trim()).toBe('```\nprint(1)\n```');
    expect(note.currentBlock().codeLanguage).toBeNull();
  });

  it('reports the block under the cursor to the menu', () => {
    const { note } = mount('```python\nprint(1)\n```');
    expect(note.currentBlock()).toMatchObject({
      codeBlock: true,
      codeLanguage: 'python',
      blockquote: false,
    });
  });

  it('turns a paragraph into a code block and back', () => {
    const { note } = mount('texto comum');
    note.toggleCodeBlock();
    expect(note.currentBlock().codeBlock).toBe(true);
    expect(note.getMarkdown().trim()).toBe('```\ntexto comum\n```');

    note.toggleCodeBlock();
    expect(note.currentBlock().codeBlock).toBe(false);
    expect(note.getMarkdown().trim()).toBe('texto comum');
  });

  it('survives a save, close and reopen with its language', () => {
    const source = '```typescript\nconst a: number = 1;\n```';
    const { note } = mount(source);
    const saved = note.getMarkdown();

    // Reopening the note is exactly this: the stored Markdown, loaded again.
    const reopened = mount(saved);
    expect(reopened.note.getMarkdown().trim()).toBe(source);
    expect(reopened.element.querySelector('code')?.className).toBe('language-typescript');
  });

  it('opens Markdown written elsewhere', () => {
    const foreign = [
      '# Documento externo',
      '',
      'Instale com:',
      '',
      '```bash',
      'npm install --save-dev vitest',
      '```',
      '',
      'E rode:',
      '',
      '```sh',
      'npm test',
      '```',
    ].join('\n');
    expect(roundTrip(foreign)).toBe(foreign);
  });
});
