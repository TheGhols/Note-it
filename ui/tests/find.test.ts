import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import {
  allMatches,
  clearFind,
  findStatus,
  replaceActive,
  replaceAll,
  setFindQuery,
  stepFind,
} from '../src/editor/find.ts';

const open: NoteEditor[] = [];

function mount(initial = ''): { note: NoteEditor; element: HTMLElement } {
  const element = document.createElement('div');
  document.body.append(element);
  const note = new NoteEditor({ element, initialContent: initial });
  open.push(note);
  return { note, element };
}

afterEach(() => {
  while (open.length) open.pop()!.destroy();
  document.body.innerHTML = '';
});

function view(note: NoteEditor) {
  return note.getRawEditor().view;
}

/** The text each match covers, read back out of the document. */
function matchedText(note: NoteEditor): string[] {
  const { state } = note.getRawEditor();
  return allMatches(state).map((match) => state.doc.textBetween(match.from, match.to));
}

function status(note: NoteEditor) {
  return findStatus(note.getRawEditor().state);
}

describe('finding occurrences', () => {
  it('finds one, several, or none', () => {
    const { note } = mount('fígado\n\nrim\n\nfígado\n\npulmão\n\nfígado');
    setFindQuery(view(note), 'rim');
    expect(status(note).total).toBe(1);

    setFindQuery(view(note), 'fígado');
    expect(status(note).total).toBe(3);

    setFindQuery(view(note), 'baço');
    expect(status(note).total).toBe(0);
    expect(status(note).current).toBe(0);
  });

  it('is case-insensitive and accent-sensitive', () => {
    const { note } = mount('Fígado FÍGADO fígado figado');
    setFindQuery(view(note), 'fígado');
    // The three accented spellings, in any case — but not the unaccented one.
    expect(matchedText(note)).toEqual(['Fígado', 'FÍGADO', 'fígado']);

    setFindQuery(view(note), 'figado');
    expect(matchedText(note)).toEqual(['figado']);
  });

  it('can be told to respect case', () => {
    const { note } = mount('Gato gato GATO');
    setFindQuery(view(note), 'gato', true);
    expect(matchedText(note)).toEqual(['gato']);
    setFindQuery(view(note), 'Gato', true);
    expect(matchedText(note)).toEqual(['Gato']);
  });

  it('treats the query as literal text, never as a pattern', () => {
    const { note } = mount('a.b acb a*b');
    setFindQuery(view(note), 'a.b');
    expect(matchedText(note)).toEqual(['a.b']);
    setFindQuery(view(note), '.*');
    expect(status(note).total).toBe(0);
  });

  it('never lets a match cross a block or a line break', () => {
    const { note } = mount('ab\n\ncd');
    setFindQuery(view(note), 'abcd');
    expect(status(note).total).toBe(0);

    const hard = mount('ab  \ncd').note;
    setFindQuery(view(hard), 'abcd');
    expect(findStatus(hard.getRawEditor().state).total).toBe(0);
  });

  it('searches every kind of block the note really holds', () => {
    const { note } = mount(
      [
        '# alvo no título',
        '',
        '- alvo na lista',
        '',
        '- [ ] alvo na tarefa',
        '',
        '> alvo na citação',
        '',
        '> [!NOTE]',
        '> alvo no callout',
        '',
        '```text',
        'alvo no código',
        '```',
        '',
        'texto com `alvo` embutido',
        '',
        '<!-- alvo no comentário -->',
      ].join('\n'),
    );
    setFindQuery(view(note), 'alvo');
    expect(status(note).total).toBe(8);
  });
});

describe('what find cannot see, because it is not there', () => {
  it('does not find a calculated result', () => {
    const { note, element } = mount('= 2 + 2');
    // The reader can see `4`...
    expect(element.textContent).toContain('4');
    // ...and there is no `4` in the document to find.
    setFindQuery(view(note), '4');
    expect(status(note).total).toBe(0);

    setFindQuery(view(note), '2 + 2');
    expect(status(note).total).toBe(1);
  });

  it('does not find a converted quantity', () => {
    const { note, element } = mount('= 10 km em m');
    expect(element.textContent).toContain('10000 m');
    setFindQuery(view(note), '10000');
    expect(status(note).total).toBe(0);

    setFindQuery(view(note), 'km');
    expect(status(note).total).toBe(1);
  });

  it('does not find a callout label or any other painted text', () => {
    const { note, element } = mount('> [!WARNING]\n> cuidado aqui');
    expect(element.querySelector('[data-callout-label]')).not.toBeNull();
    setFindQuery(view(note), 'WARNING');
    expect(status(note).total).toBe(0);
    setFindQuery(view(note), 'cuidado');
    expect(status(note).total).toBe(1);
  });
});

describe('moving between occurrences', () => {
  it('starts on the first and steps forwards', () => {
    const { note } = mount('um alvo, dois alvo, três alvo');
    setFindQuery(view(note), 'alvo');
    expect(status(note)).toMatchObject({ total: 3, current: 1 });

    stepFind(view(note), 1);
    expect(status(note).current).toBe(2);
    stepFind(view(note), 1);
    expect(status(note).current).toBe(3);
  });

  it('wraps from the last to the first and back again', () => {
    const { note } = mount('alvo alvo alvo');
    setFindQuery(view(note), 'alvo');
    stepFind(view(note), 1);
    stepFind(view(note), 1);
    expect(status(note).current).toBe(3);

    stepFind(view(note), 1);
    expect(status(note).current).toBe(1);

    stepFind(view(note), -1);
    expect(status(note).current).toBe(3);
  });

  it('does nothing at all when there is nothing to move between', () => {
    const { note } = mount('nada aqui');
    setFindQuery(view(note), 'ausente');
    stepFind(view(note), 1);
    stepFind(view(note), -1);
    expect(status(note)).toMatchObject({ total: 0, current: 0 });
  });

  it('closes back to nothing highlighted', () => {
    const { note } = mount('alvo alvo');
    setFindQuery(view(note), 'alvo');
    expect(status(note).total).toBe(2);
    clearFind(view(note));
    expect(status(note)).toMatchObject({ query: '', total: 0 });
  });
});

describe('finding changes nothing', () => {
  it('leaves the Markdown, the undo history and the update callback alone', () => {
    const updates: string[] = [];
    const element = document.createElement('div');
    document.body.append(element);
    const note = new NoteEditor({
      element,
      initialContent: 'alvo alvo alvo',
      onUpdate: (markdown) => updates.push(markdown),
    });
    open.push(note);

    const before = note.getMarkdown();
    setFindQuery(view(note), 'alvo');
    stepFind(view(note), 1);
    stepFind(view(note), 1);
    stepFind(view(note), -1);
    clearFind(view(note));

    expect(note.getMarkdown()).toBe(before);
    expect(updates).toEqual([]);
    expect(note.hasPendingSave()).toBe(false);

    // Nothing to undo means the document is exactly where it started.
    note.getRawEditor().commands.undo();
    expect(note.getMarkdown()).toBe(before);
  });

  it('highlights with decorations rather than with content', () => {
    const { note, element } = mount('alvo alvo');
    setFindQuery(view(note), 'alvo');
    expect(element.querySelectorAll('.note-find-match')).toHaveLength(2);
    expect(element.querySelectorAll('.note-find-match-active')).toHaveLength(1);
    expect(note.getRawEditor().getHTML()).not.toContain('note-find-match');
    expect(note.getMarkdown()).toBe('alvo alvo');
  });
});

describe('replacing the occurrence you are on', () => {
  it('replaces one and moves to the next', () => {
    const { note } = mount('gato gato gato');
    setFindQuery(view(note), 'gato');
    expect(replaceActive(view(note), 'cachorro')).toBe(true);
    expect(note.getMarkdown()).toBe('cachorro gato gato');
    expect(status(note).total).toBe(2);

    replaceActive(view(note), 'cachorro');
    expect(note.getMarkdown()).toBe('cachorro cachorro gato');
  });

  it('is one undo step per replacement', () => {
    const { note } = mount('gato gato gato');
    setFindQuery(view(note), 'gato');
    replaceActive(view(note), 'cachorro');
    replaceActive(view(note), 'cachorro');
    expect(note.getMarkdown()).toBe('cachorro cachorro gato');

    note.getRawEditor().commands.undo();
    expect(note.getMarkdown()).toBe('cachorro gato gato');
    note.getRawEditor().commands.undo();
    expect(note.getMarkdown()).toBe('gato gato gato');
  });

  it('answers false when there is nothing to replace', () => {
    const { note } = mount('nada');
    setFindQuery(view(note), 'ausente');
    expect(replaceActive(view(note), 'algo')).toBe(false);
    expect(note.getMarkdown()).toBe('nada');
  });
});

describe('replacing every occurrence', () => {
  it('replaces them all', () => {
    const { note } = mount('gato gato gato');
    setFindQuery(view(note), 'gato');
    expect(replaceAll(view(note), 'cachorro')).toBe(3);
    expect(note.getMarkdown()).toBe('cachorro cachorro cachorro');
  });

  it('comes back with a single undo, and goes forward again with redo', () => {
    const many = Array.from({ length: 20 }, (_, index) => `linha ${index} com gato`).join('\n\n');
    const { note } = mount(many);
    setFindQuery(view(note), 'gato');
    expect(replaceAll(view(note), 'cachorro')).toBe(20);
    expect(note.getMarkdown()).not.toContain('gato');

    note.getRawEditor().commands.undo();
    expect(note.getMarkdown()).toBe(many);
    expect(note.getMarkdown().match(/gato/g)).toHaveLength(20);

    note.getRawEditor().commands.redo();
    expect(note.getMarkdown()).not.toContain('gato');
  });

  it('replaces nothing when nothing matches', () => {
    const { note } = mount('gato');
    setFindQuery(view(note), 'ausente');
    expect(replaceAll(view(note), 'x')).toBe(0);
    expect(note.getMarkdown()).toBe('gato');
  });

  it('accepts an empty replacement, a longer one and one in another script', () => {
    const removed = mount('a gato b').note;
    setFindQuery(view(removed), 'gato');
    replaceAll(view(removed), '');
    expect(removed.getMarkdown()).toBe('a  b');

    const longer = mount('gato').note;
    setFindQuery(view(longer), 'gato');
    replaceAll(view(longer), 'um cachorro bem grande');
    expect(longer.getMarkdown()).toBe('um cachorro bem grande');

    const unicode = mount('gato').note;
    setFindQuery(view(unicode), 'gato');
    replaceAll(view(unicode), '猫 🐈');
    expect(unicode.getMarkdown()).toBe('猫 🐈');
  });
});

describe('replacing preserves what surrounds it', () => {
  it('keeps the marks the text was wearing', () => {
    const { note } = mount('um **gato** em negrito e *gato* em itálico');
    setFindQuery(view(note), 'gato');
    replaceAll(view(note), 'cachorro');
    expect(note.getMarkdown()).toBe('um **cachorro** em negrito e *cachorro* em itálico');
  });

  it('keeps a link, a heading, a list and a task intact', () => {
    const source = [
      '# título com gato',
      '',
      '- item com gato',
      '',
      '- [ ] tarefa com gato',
      '',
      '> citação com gato',
      '',
      'um [gato](https://example.com) com link',
    ].join('\n');
    const { note } = mount(source);
    setFindQuery(view(note), 'gato');
    replaceAll(view(note), 'cachorro');

    expect(note.getMarkdown().trim()).toBe(source.replace(/gato/g, 'cachorro'));
  });

  it('keeps a code block a code block', () => {
    const source = '```python\ndef gato():\n    return gato\n```';
    const { note, element } = mount(source);
    setFindQuery(view(note), 'gato');
    expect(status(note).total).toBe(2);
    replaceAll(view(note), 'cachorro');

    expect(note.getMarkdown().trim()).toBe(
      '```python\ndef cachorro():\n    return cachorro\n```',
    );
    expect(element.querySelectorAll('pre code.language-python')).toHaveLength(1);
  });
});

describe('replacing beside the math and conversion engines', () => {
  it('renames a variable and the results follow', () => {
    const { note, element } = mount('preco := 100\n\n= preco * 2');
    const results = () =>
      Array.from(element.querySelectorAll('.note-math-result')).map((n) => n.textContent);
    expect(results()).toEqual(['200']);

    setFindQuery(view(note), 'preco');
    expect(replaceAll(view(note), 'valor')).toBe(2);

    expect(note.getMarkdown().trim()).toBe('valor := 100\n\n= valor \\* 2');
    expect(results()).toEqual(['200']);

    note.getRawEditor().commands.undo();
    expect(note.getMarkdown().trim()).toBe('preco := 100\n\n= preco \\* 2');
    expect(results()).toEqual(['200']);
  });

  it('renames a unit and the conversion follows', () => {
    const { note, element } = mount('distancia := 5\n\n= distancia km em m');
    const results = () =>
      Array.from(element.querySelectorAll('.note-math-result')).map((n) => n.textContent);
    expect(results()).toEqual(['5000 m']);

    setFindQuery(view(note), 'distancia');
    replaceAll(view(note), 'percurso');
    expect(results()).toEqual(['5000 m']);
    expect(note.getMarkdown()).toContain('percurso km em m');
  });
});
