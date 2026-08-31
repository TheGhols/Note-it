import { afterEach, describe, expect, it } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { extractFlashcards } from '../src/flashcards/extract.ts';
import { canonicalSide, identifyReviews } from '../src/study/identity.ts';

const NOTE = '11111111-1111-4111-8111-111111111111';
const ASSET = '22222222-2222-4222-8222-222222222222';
const OTHER = '33333333-3333-4333-8333-333333333333';
const open: NoteEditor[] = [];

afterEach(() => {
  while (open.length > 0) open.pop()!.destroy();
  document.body.replaceChildren();
});

function sources(markdown: string) {
  const editor = new NoteEditor({ element: document.createElement('div'), initialContent: markdown });
  open.push(editor);
  return extractFlashcards(editor.getView().state.doc);
}

async function keys(markdown: string): Promise<string[]> {
  return (await identifyReviews(NOTE, 'Nota', sources(markdown), {})).map((item) => item.reviewKey);
}

describe('derived review identity', () => {
  it('is deterministic SHA-256 and exposes no note content', async () => {
    const first = await keys('Pergunta secreta :: Resposta secreta');
    const second = await keys('Pergunta secreta :: Resposta secreta');

    expect(first).toEqual(second);
    expect(first[0]).toMatch(/^[0-9a-f]{64}$/);
    expect(first[0]).not.toContain('Pergunta');
    expect(first[0]).not.toContain('Resposta');
  });

  it('survives moving the card inside the same note', async () => {
    expect(await keys('Antes\n\nA :: B\n\nDepois')).toEqual(
      await keys('Depois\n\nA :: B\n\nAntes'),
    );
  });

  it('ignores presentation marks while preserving semantic structure', async () => {
    const plain = await keys('Pergunta :: Resposta');
    const formatted = await keys(
      '**Pergunta** :: *Resposta*',
    );
    const coloured = await keys(
      '<span data-note-it-color="#DC2626" style="color:#DC2626">Pergunta</span> :: <mark data-note-it-highlight="#FDE68A">Resposta</mark>',
    );
    expect(formatted).toEqual(plain);
    expect(coloured).toEqual(plain);
  });

  it('ignores managed-image width and alignment but not its asset or alt text', async () => {
    const src = `../assets/${NOTE}/${ASSET}.png`;
    const sameAsset = await keys(`<img src="${src}" alt="ECG">\n\n::\n\nResposta`);
    const resized = await keys(
      `<img src="${src}" alt="ECG" data-note-it-width="420" data-note-it-align="right">\n\n::\n\nResposta`,
    );
    const changedAsset = await keys(
      `<img src="../assets/${NOTE}/${OTHER}.png" alt="ECG">\n\n::\n\nResposta`,
    );
    const changedAlt = await keys(`<img src="${src}" alt="Outro ECG">\n\n::\n\nResposta`);

    expect(resized).toEqual(sameAsset);
    expect(changedAsset).not.toEqual(sameAsset);
    expect(changedAlt).not.toEqual(sameAsset);
  });

  it('creates a new key for a semantic text edit', async () => {
    expect(await keys('Metformina :: Biguanida')).not.toEqual(
      await keys('Metformina é de qual classe? :: Biguanida'),
    );
  });

  it('keeps reversible directions independent', async () => {
    const identified = await identifyReviews(NOTE, 'Farmacologia', sources('A ::: B'), {});
    expect(identified).toHaveLength(2);
    expect(identified[0].direction).toBe('forward');
    expect(identified[1].direction).toBe('reverse');
    expect(identified[0].reviewKey).not.toBe(identified[1].reviewKey);
  });

  it('gives identical duplicates occurrence ordinals instead of deduplicating them', async () => {
    const identified = await identifyReviews(NOTE, 'Duplicatas', sources('A :: B\n\nA :: B'), {});
    expect(identified).toHaveLength(2);
    expect(new Set(identified.map((item) => item.reviewKey)).size).toBe(2);
    expect(canonicalSide(identified[0].question.content)).toBe(
      canonicalSide(identified[1].question.content),
    );
  });

  it('includes the note UUID so equal cards in two notes remain independent', async () => {
    const cards = sources('A :: B');
    const first = await identifyReviews(NOTE, 'A', cards, {});
    const second = await identifyReviews('99999999-9999-4999-8999-999999999999', 'B', cards, {});
    expect(first[0].reviewKey).not.toBe(second[0].reviewKey);
  });
});
