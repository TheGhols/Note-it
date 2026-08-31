import { afterEach, describe, expect, it } from 'vitest';
import { DOMSerializer } from '@tiptap/pm/model';
import { NoteEditor } from '../src/editor/editor.ts';
import { extractFlashcards, reviewItems } from '../src/flashcards/extract.ts';
import { buildGlobalCatalog } from '../src/study/catalog.ts';
import { emptyStudyState } from '../src/study/types.ts';

const A = '11111111-1111-4111-8111-111111111111';
const B = '22222222-2222-4222-8222-222222222222';
const C = '33333333-3333-4333-8333-333333333333';
const ASSET = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const editors: NoteEditor[] = [];

afterEach(() => {
  while (editors.length > 0) editors.pop()!.destroy();
  document.body.replaceChildren();
});

function localReviews(markdown: string) {
  const editor = new NoteEditor({ element: document.createElement('div'), initialContent: markdown });
  editors.push(editor);
  return reviewItems(extractFlashcards(editor.getView().state.doc));
}

function sideText(side: { content: { textBetween(from: number, to: number): string; size: number } }): string {
  return side.content.textBetween(0, side.content.size);
}

describe('the on-demand global catalog', () => {
  it('parses open and closed candidates with the exact normal extractor semantics', async () => {
    const notes = [
      { id: A, content: '# Nota A\n\nA1 :: A2\n\nA3 :: A4' },
      { id: B, content: '# Nota B\n\nFrente ::: Verso' },
      { id: C, content: '# Nota fechada\n\nPergunta\n\n::\n\n- item um\n- item dois' },
    ];
    const catalog = await buildGlobalCatalog(notes, emptyStudyState(), null, document);

    expect(catalog.sourceCards).toBe(4);
    expect(catalog.items).toHaveLength(5);
    expect(catalog.notesWithCards).toBe(3);
    expect(new Set(catalog.items.map((item) => item.noteId))).toEqual(new Set([A, B, C]));
    expect(catalog.items.filter((item) => item.noteId === B).map((item) => item.direction)).toEqual([
      'forward',
      'reverse',
    ]);

    for (const note of notes) {
      const local = localReviews(note.content);
      const global = catalog.items.filter((item) => item.noteId === note.id);
      expect(global.map((item) => sideText(item.question))).toEqual(
        local.map((item) => sideText(item.question)),
      );
      expect(global.map((item) => sideText(item.answer))).toEqual(
        local.map((item) => sideText(item.answer)),
      );
    }
  });

  it('replaces the host copy of the requesting note with its live unsaved Markdown', async () => {
    const catalog = await buildGlobalCatalog(
      [
        { id: A, content: '# Nota A\n\nVersão salva sem cartão' },
        { id: B, content: '# Nota B\n\nB :: C' },
      ],
      emptyStudyState(),
      { id: A, content: '# Nota A ao vivo\n\nNovo :: Agora' },
      document,
    );

    expect(catalog.items.map((item) => item.noteId)).toEqual([A, B]);
    expect(catalog.items[0].noteTitle).toBe('Nota A ao vivo');
    expect(sideText(catalog.items[0].question)).toBe('Novo');
  });

  it('renders a closed note image through note-it-asset without copying or fetching it', async () => {
    const managed = `../assets/${C}/${ASSET}.png`;
    const catalog = await buildGlobalCatalog(
      [{ id: C, content: `# Imagens\n\n<img src="${managed}" alt="ECG">\n\n::\n\nArritmia` }],
      emptyStudyState(),
      null,
      document,
    );
    const fragment = DOMSerializer.fromSchema(catalog.schema).serializeFragment(
      catalog.items[0].question.content,
      { document },
    );
    const image = fragment.querySelector('img');
    expect(image?.getAttribute('src')).toBe(`note-it-asset:/${C}/${ASSET}.png`);
    expect(image?.getAttribute('data-note-it-src')).toBe(managed);
    expect(image?.hasAttribute('data-note-it-width')).toBe(false);
    expect(image?.classList.contains('note-image')).toBe(true);
  });

  it('contains only the notes the host supplied, so trash is not a frontend side channel', async () => {
    const catalog = await buildGlobalCatalog(
      [{ id: A, content: '# Viva\n\nA :: B' }],
      emptyStudyState(),
      null,
      document,
    );
    expect(catalog.items.map((item) => item.noteId)).toEqual([A]);
  });
});
