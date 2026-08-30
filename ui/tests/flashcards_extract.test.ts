import { describe, expect, it } from 'vitest';
import { Fragment, Node as ProseMirrorNode } from '@tiptap/pm/model';
import { NoteEditor } from '../src/editor/editor.ts';
import {
  BASIC_DELIMITER,
  countFlashcards,
  extractFlashcards,
  FlashcardSide,
  REVERSIBLE_DELIMITER,
  reviewItems,
} from '../src/flashcards/extract.ts';

const NOTE = '11111111-1111-4111-8111-111111111111';
const ASSET = '22222222-2222-4222-8222-222222222222';
const OTHER = '33333333-3333-4333-8333-333333333333';
const IMAGE = `../assets/${NOTE}/${ASSET}.png`;
const SECOND_IMAGE = `../assets/${NOTE}/${OTHER}.jpg`;

/** The document a note holds, built the way the application builds it. */
function docOf(markdown: string): ProseMirrorNode {
  const editor = new NoteEditor({
    element: document.createElement('div'),
    initialContent: markdown,
  });
  return editor.getView().state.doc;
}

function cardsIn(markdown: string) {
  return extractFlashcards(docOf(markdown));
}

/** What one side reads as, ignoring how it is marked up. */
function text(side: FlashcardSide): string {
  let out = '';
  side.content.forEach((node) => {
    out += node.textContent;
  });
  return out;
}

/** Every node in a side, so a test can ask what it is made of. */
function nodesIn(fragment: Fragment): ProseMirrorNode[] {
  const found: ProseMirrorNode[] = [];
  const walk = (content: Fragment): void => {
    content.forEach((child) => {
      found.push(child);
      walk(child.content);
    });
  };
  walk(fragment);
  return found;
}

function marksOn(side: FlashcardSide, word: string): string[] {
  for (const node of nodesIn(side.content)) {
    if (node.isText && (node.text ?? '').includes(word)) {
      return node.marks.map((mark) => mark.type.name).sort();
    }
  }
  return [];
}

function imagesIn(side: FlashcardSide): string[] {
  return nodesIn(side.content)
    .filter((node) => node.type.name === 'noteItImage')
    .map((node) => String(node.attrs.src));
}

describe('what is a flashcard', () => {
  it('finds nothing in a note that has none', () => {
    expect(cardsIn('')).toHaveLength(0);
    expect(cardsIn('Uma nota comum, com duas frases. E nada mais.')).toHaveLength(0);
    expect(countFlashcards(cardsIn(''))).toEqual({ cards: 0, reviews: 0 });
  });

  it('reads a question and its answer off one line', () => {
    const cards = cardsIn(
      'Qual é a principal bactéria da pneumonia comunitária? :: Streptococcus pneumoniae',
    );

    expect(cards).toHaveLength(1);
    expect(cards[0].mode).toBe('basic');
    expect(cards[0].form).toBe('inline');
    expect(text(cards[0].front)).toBe(
      'Qual é a principal bactéria da pneumonia comunitária?',
    );
    expect(text(cards[0].back)).toBe('Streptococcus pneumoniae');
  });

  it('reads a term and its definition as one card studied both ways', () => {
    const cards = cardsIn('Metformina ::: Biguanida');

    expect(cards).toHaveLength(1);
    expect(cards[0].mode).toBe('reversible');
    expect(text(cards[0].front)).toBe('Metformina');
    expect(text(cards[0].back)).toBe('Biguanida');
    // One card, written once. The second direction is not a second card.
    expect(countFlashcards(cards)).toEqual({ cards: 1, reviews: 2 });
  });

  it('takes the longest delimiter, never the shorter one twice', () => {
    // `:::` is a run of three. Reading it as `::` plus a stray colon would make
    // every reversible card a basic one whose answer starts with a colon, and
    // it is the kind of defect that looks like a content problem for weeks.
    const cards = cardsIn('Termo ::: Definição');
    expect(cards[0].mode).toBe('reversible');
    expect(text(cards[0].back)).toBe('Definição');
    expect(text(cards[0].back).startsWith(':')).toBe(false);
    expect(REVERSIBLE_DELIMITER.length).toBeGreaterThan(BASIC_DELIMITER.length);
  });

  it('keeps the delimiter out of both sides', () => {
    const cards = cardsIn('A :: B');
    expect(text(cards[0].front)).toBe('A');
    expect(text(cards[0].back)).toBe('B');
    expect(text(cards[0].front)).not.toContain(':');
    expect(text(cards[0].back)).not.toContain(':');
  });

  it('finds every card in the note, in the order they are written', () => {
    const cards = cardsIn('A :: B\n\nC ::: D\n\nE :: F');

    expect(cards.map((card) => text(card.front))).toEqual(['A', 'C', 'E']);
    expect(cards.map((card) => card.mode)).toEqual(['basic', 'reversible', 'basic']);
    expect(countFlashcards(cards)).toEqual({ cards: 3, reviews: 4 });
  });
});

describe('what is not a flashcard', () => {
  it('needs whitespace around the delimiter', () => {
    // Technical writing is full of these, and a note is full of technical
    // writing. The spaces cost the reader nothing and are what keeps the
    // detector out of everything below.
    for (const source of ['A::B', 'A:::B', 'namespace::method', 'std::vector<int>']) {
      expect(cardsIn(source), source).toHaveLength(0);
    }
  });

  it('leaves single colons alone, so times and addresses survive', () => {
    for (const source of [
      'https://example.com',
      'http://example.com/a:b',
      'A reunião é 12:30 na sala 4',
      'Veja: o resultado foi outro',
    ]) {
      expect(cardsIn(source), source).toHaveLength(0);
    }
  });

  it('recognises two colons and three, and nothing longer', () => {
    for (const source of ['A :::: B', 'A ::::: B', 'cinco::::pontos', '::::']) {
      expect(cardsIn(source), source).toHaveLength(0);
    }
  });

  it('never reads inside a code block', () => {
    // Code is code. A snippet pasted into a note is not a deck of cards.
    expect(cardsIn('```\nA :: B\n```')).toHaveLength(0);
    expect(cardsIn('```ts\nconst a: Map<string, string> = x;\nA ::: B\n```')).toHaveLength(0);
    expect(cardsIn('Antes\n\n```\nA :: B\n```\n\nDepois')).toHaveLength(0);
  });

  it('never reads inside inline code, which is how a delimiter is written about', () => {
    // The escape hatch, and it is one the note already had: whatever is in
    // backticks is quoted rather than interpreted.
    expect(cardsIn('`A :: B`')).toHaveLength(0);
    expect(cardsIn('Escreva `Pergunta :: Resposta` para criar um cartão.')).toHaveLength(0);
    expect(cardsIn('`A ::: B` não é um cartão')).toHaveLength(0);
  });

  it('still finds the card beside a quoted delimiter', () => {
    const cards = cardsIn('`A :: B` :: é a sintaxe de um cartão');
    expect(cards).toHaveLength(1);
    expect(text(cards[0].front)).toBe('A :: B');
    expect(text(cards[0].back)).toBe('é a sintaxe de um cartão');
  });

  it('declines a line with more than one delimiter instead of guessing', () => {
    // `A :: B :: C` has two readings and nothing to choose between them.
    // Producing one of them silently is worse than producing none.
    for (const source of ['A :: B :: C', 'A ::: B ::: C', 'A :: B ::: C', 'A ::: B :: C']) {
      expect(cardsIn(source), source).toHaveLength(0);
    }
  });

  it('refuses a side with nothing on it', () => {
    for (const source of [':: resposta', 'pergunta ::', '::: resposta', 'pergunta :::']) {
      expect(cardsIn(source), source).toHaveLength(0);
    }
  });

  it('does not count whitespace as content', () => {
    expect(cardsIn('   :: resposta')).toHaveLength(0);
    expect(cardsIn('pergunta ::   ')).toHaveLength(0);
  });

  it('sees nothing in what an image is stored as', () => {
    // The reference, the alt text, the width and the alignment are attributes
    // of a node. None of them is text in the document, so none of them can
    // contribute a delimiter — which is exactly what a regular expression over
    // the Markdown would get wrong.
    for (const source of [
      `![](${IMAGE})`,
      `![A :: B](${IMAGE})`,
      `<img src="${IMAGE}" alt="A :: B">`,
      `<img src="${IMAGE}" alt="" data-note-it-width="320" data-note-it-align="left">`,
    ]) {
      expect(cardsIn(source), source).toHaveLength(0);
    }
  });

  it('sees nothing in technical HTML attributes', () => {
    // Attributes are structure, not visible document text. A Markdown-wide
    // search would find this delimiter and create a card from an implementation
    // detail; the ProseMirror tree has no such text to offer.
    expect(cardsIn('<span data-example="A :: B">texto comum</span>')).toHaveLength(0);
    expect(cardsIn('<div title="Termo ::: Definição">conteúdo comum</div>')).toHaveLength(0);
  });

  it('sees nothing in the metadata a task carries', () => {
    const source =
      '- [x] Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->';
    expect(cardsIn(source)).toHaveLength(0);
  });
});

describe('a card written across blocks', () => {
  it('takes the block before the marker and the block after it', () => {
    const cards = cardsIn('Pergunta\n\n::\n\nResposta');

    expect(cards).toHaveLength(1);
    expect(cards[0].form).toBe('block');
    expect(cards[0].mode).toBe('basic');
    expect(text(cards[0].front)).toBe('Pergunta');
    expect(text(cards[0].back)).toBe('Resposta');
  });

  it('is reversible when the marker is', () => {
    const cards = cardsIn('Termo\n\n:::\n\nDefinição');

    expect(cards[0].mode).toBe('reversible');
    expect(countFlashcards(cards)).toEqual({ cards: 1, reviews: 2 });
  });

  it('takes a whole list as the answer', () => {
    const cards = cardsIn(
      'Quais são os componentes da tríade de Charcot?\n\n::\n\n- Febre\n- Icterícia\n- Dor em hipocôndrio direito',
    );

    expect(cards).toHaveLength(1);
    const back = nodesIn(cards[0].back.content);
    expect(back[0].type.name).toBe('bulletList');
    expect(text(cards[0].back)).toContain('Febre');
    expect(text(cards[0].back)).toContain('Icterícia');
    expect(text(cards[0].back)).toContain('Dor em hipocôndrio direito');
  });

  it('takes numbered and task lists as one structural side', () => {
    const numbered = cardsIn('Ordem\n\n::\n\n1. Primeiro\n2. Segundo');
    expect(nodesIn(numbered[0].back.content)[0].type.name).toBe('orderedList');
    expect(text(numbered[0].back)).toContain('Primeiro');

    const tasks = cardsIn('Checklist\n\n::\n\n- [x] Feito\n- [ ] Pendente');
    expect(nodesIn(tasks[0].back.content)[0].type.name).toBe('taskList');
    expect(text(tasks[0].back)).toContain('Feito');
    expect(text(tasks[0].back)).toContain('Pendente');
  });

  it('takes a quote, and a callout, as a side', () => {
    const quoted = cardsIn('Pergunta\n\n::\n\n> Uma resposta citada');
    expect(nodesIn(quoted[0].back.content)[0].type.name).toBe('blockquote');
    expect(text(quoted[0].back)).toContain('Uma resposta citada');

    const callout = cardsIn('Pergunta\n\n::\n\n> [!NOTE]\n> Uma resposta destacada');
    expect(callout).toHaveLength(1);
    expect(nodesIn(callout[0].back.content)[0].attrs.callout).toBe('NOTE');
  });

  it('takes a heading as a side and keeps it a heading', () => {
    const cards = cardsIn('# Pergunta\n\n::\n\nResposta');
    expect(nodesIn(cards[0].front.content)[0].type.name).toBe('heading');
  });

  it('keeps the lines of a multiline answer together', () => {
    // Hard breaks inside one block are one answer, not three.
    const cards = cardsIn('Tríade de Charcot\n\n::\n\nFebre\\\nIcterícia\\\nDor');
    expect(cards).toHaveLength(1);
    expect(text(cards[0].back)).toContain('Febre');
    expect(text(cards[0].back)).toContain('Dor');
  });

  it('needs a block on both sides of the marker', () => {
    expect(cardsIn('::\n\nResposta'), 'marker first').toHaveLength(0);
    expect(cardsIn('Pergunta\n\n::'), 'marker last').toHaveLength(0);
    expect(cardsIn('::'), 'marker alone').toHaveLength(0);
    expect(cardsIn(':::\n\nResposta')).toHaveLength(0);
  });

  it('is not a marker unless it is a paragraph of its own, at the top level', () => {
    // A `::` in a quote or inside a list item belongs to whoever wrote it
    // there. Only a top-level paragraph consumes the blocks around it.
    expect(cardsIn('Pergunta\n\n> ::\n\nResposta')).toHaveLength(0);
    expect(cardsIn('Pergunta\n\n- ::\n\nResposta')).toHaveLength(0);
    expect(cardsIn('Pergunta\n\n# ::\n\nResposta')).toHaveLength(0);
  });

  it('is not a marker when the delimiter is quoted as code', () => {
    expect(cardsIn('Pergunta\n\n`::`\n\nResposta')).toHaveLength(0);
  });

  it('is not a marker when it carries anything else', () => {
    expect(cardsIn(`Pergunta\n\n:: ![](${IMAGE})\n\nResposta`)).toHaveLength(0);
  });

  it('never makes a marker the answer to another marker', () => {
    expect(cardsIn('A\n\n::\n\n:::\n\nB')).toHaveLength(0);
  });

  it('refuses a blank block as a side', () => {
    // A note left with an empty paragraph either side of a marker is somebody
    // mid-sentence, not a card with an empty answer.
    const document = docOf('Pergunta\n\n::\n\nResposta');
    expect(extractFlashcards(document)).toHaveLength(1);
    expect(cardsIn('\n\n::\n\nResposta')).toHaveLength(0);
  });

  it('does not read a block again as a card of its own once it is a side', () => {
    // Deciding it once is what makes the result the same however it is read.
    const cards = cardsIn('A :: B\n\n::\n\nC');
    expect(cards).toHaveLength(1);
    expect(cards[0].form).toBe('block');
    expect(text(cards[0].front)).toBe('A :: B');
    expect(text(cards[0].back)).toBe('C');
  });
});

describe('what a card carries', () => {
  it('keeps every mark the note put on the words', () => {
    const cards = cardsIn(
      '**Qual medicamento?** :: *Metformina*\n\n~~Riscado~~ :: comum\n\n<u>Sublinhado</u> :: comum\n\n<mark data-note-it-highlight="#FDE68A">Marcado</mark> :: comum',
    );

    expect(marksOn(cards[0].front, 'Qual medicamento?')).toEqual(['bold']);
    expect(marksOn(cards[0].back, 'Metformina')).toEqual(['italic']);
    expect(marksOn(cards[1].front, 'Riscado')).toEqual(['strike']);
    expect(marksOn(cards[2].front, 'Sublinhado')).toEqual(['underline']);
    expect(marksOn(cards[3].front, 'Marcado')).toEqual(['highlight']);
  });

  it('keeps a colour and a text size', () => {
    const cards = cardsIn(
      '<span data-note-it-color="#DC2626" style="color:#DC2626">Vermelho</span> :: cor\n\n<span data-note-it-font-size="22" style="font-size:22px">Grande</span> :: tamanho',
    );

    expect(marksOn(cards[0].front, 'Vermelho')).toEqual(['textStyle']);
    expect(marksOn(cards[1].front, 'Grande')).toEqual(['noteItFontSize']);
  });

  it('keeps a link a link', () => {
    const cards = cardsIn('[Referência](https://example.com) :: uma fonte');
    expect(marksOn(cards[0].front, 'Referência')).toEqual(['link']);
  });

  it('keeps inline code on the side it was written on', () => {
    const cards = cardsIn('O que faz `map`? :: transforma cada elemento');
    expect(marksOn(cards[0].front, 'map')).toEqual(['code']);
  });

  it('carries accents, emoji, combining marks and other scripts through unchanged', () => {
    const cards = cardsIn(
      'Biópsia :: Procedimento diagnóstico\n\n🫀 :: Coração\n\n心臓 :: coração\n\ncafé :: café',
    );

    expect(text(cards[0].front)).toBe('Biópsia');
    expect(text(cards[1].front)).toBe('🫀');
    expect(text(cards[1].back)).toBe('Coração');
    expect(text(cards[2].front)).toBe('心臓');
    // A combining acute is part of the word, not a boundary of it.
    expect(text(cards[3].front)).toBe('café');
  });

  it('finds a card written inside a list item or a quote', () => {
    const list = cardsIn('- Pergunta :: Resposta');
    expect(list).toHaveLength(1);
    expect(text(list[0].back)).toBe('Resposta');

    const quote = cardsIn('> Pergunta :: Resposta');
    expect(quote).toHaveLength(1);
    expect(text(quote[0].front)).toBe('Pergunta');
  });
});

describe('a card made of pictures', () => {
  it('takes an image as the whole of a side', () => {
    // The front is an ECG and the back is the diagnosis. Judging a side by its
    // text would throw this away, and it is the reason images came first.
    const cards = cardsIn(`![](${IMAGE}) :: Fibrilação atrial`);

    expect(cards).toHaveLength(1);
    expect(imagesIn(cards[0].front)).toEqual([IMAGE]);
    expect(text(cards[0].back)).toBe('Fibrilação atrial');
  });

  it('takes an image as the answer', () => {
    const cards = cardsIn(`Qual é o traçado? :: ![](${IMAGE})`);
    expect(imagesIn(cards[0].back)).toEqual([IMAGE]);
  });

  it('keeps an image beside the words it was written with', () => {
    const cards = cardsIn(`![](${IMAGE}) Qual é o diagnóstico? :: Fibrilação atrial`);
    expect(imagesIn(cards[0].front)).toEqual([IMAGE]);
    expect(text(cards[0].front)).toContain('Qual é o diagnóstico?');
  });

  it('carries an image through the block form too', () => {
    const cards = cardsIn(`![](${IMAGE})\n\n::\n\nFibrilação atrial`);
    expect(cards).toHaveLength(1);
    expect(imagesIn(cards[0].front)).toEqual([IMAGE]);
  });

  it('makes both directions of a reversible card from one picture', () => {
    const cards = cardsIn(`![](${IMAGE}) ::: Carcinoma hepatocelular`);
    const items = reviewItems(cards);

    expect(items).toHaveLength(2);
    expect(imagesIn(items[0].question)).toEqual([IMAGE]);
    expect(imagesIn(items[1].answer)).toEqual([IMAGE]);
    // The same reference on both, because it is the same picture. Nothing is
    // copied, duplicated or re-stored to be studied from the other side.
    expect(imagesIn(items[0].question)).toEqual(imagesIn(items[1].answer));
  });

  it('keeps managed images on both sides without making another asset', () => {
    const cards = cardsIn(`![](${IMAGE}) ::: ![](${SECOND_IMAGE})`);
    const items = reviewItems(cards);

    expect(cards).toHaveLength(1);
    expect(items).toHaveLength(2);
    expect(imagesIn(items[0].question)).toEqual([IMAGE]);
    expect(imagesIn(items[0].answer)).toEqual([SECOND_IMAGE]);
    expect(imagesIn(items[1].question)).toEqual([SECOND_IMAGE]);
    expect(imagesIn(items[1].answer)).toEqual([IMAGE]);
  });

  it('keeps an image and text together in each structural block', () => {
    const cards = cardsIn(
      `![](${IMAGE}) Qual é o achado?\n\n::\n\nFibrilação atrial ![](${SECOND_IMAGE})`,
    );

    expect(imagesIn(cards[0].front)).toEqual([IMAGE]);
    expect(text(cards[0].front)).toContain('Qual é o achado?');
    expect(imagesIn(cards[0].back)).toEqual([SECOND_IMAGE]);
    expect(text(cards[0].back)).toContain('Fibrilação atrial');
  });

  it('carries several images on one side', () => {
    const cards = cardsIn(`![](${IMAGE}) ![](${SECOND_IMAGE}) :: dois achados`);
    expect(imagesIn(cards[0].front)).toEqual([IMAGE, SECOND_IMAGE]);
  });

  it('does not accept a reference the store does not manage as content', () => {
    // It draws nothing, so a side holding only that is a blank side.
    expect(cardsIn('![](https://example.com/foto.png) :: nada')).toHaveLength(0);
    expect(cardsIn('![](foto.png) :: nada')).toHaveLength(0);
  });
});

describe('the questions a card comes to', () => {
  it('makes one question from a basic card and two from a reversible one', () => {
    const basic = reviewItems(cardsIn('A :: B'));
    expect(basic).toHaveLength(1);
    expect(basic[0].direction).toBe('forward');
    expect(text(basic[0].question)).toBe('A');
    expect(text(basic[0].answer)).toBe('B');

    const both = reviewItems(cardsIn('A ::: B'));
    expect(both).toHaveLength(2);
    expect(both.map((item) => item.direction)).toEqual(['forward', 'reverse']);
    expect(text(both[1].question)).toBe('B');
    expect(text(both[1].answer)).toBe('A');
  });

  it('expands a reversible card where it is written, not at the end', () => {
    const items = reviewItems(cardsIn('A :: B\n\nC ::: D\n\nE :: F'));

    expect(items.map((item) => `${text(item.question)}→${text(item.answer)}`)).toEqual([
      'A→B',
      'C→D',
      'D→C',
      'E→F',
    ]);
    expect(items.map((item) => item.source)).toEqual([0, 1, 1, 2]);
  });

  it('counts cards and questions separately, because they differ', () => {
    const cards = cardsIn('A :: B\n\nC :: D\n\nE :: F\n\nG ::: H\n\nI ::: J');
    expect(countFlashcards(cards)).toEqual({ cards: 5, reviews: 7 });
    expect(reviewItems(cards)).toHaveLength(7);
  });

  it('keeps two identical cards as two cards', () => {
    // Somebody wrote it twice. Presuming that is a mistake and silently
    // dropping one is the detector deciding what the note says.
    const cards = cardsIn('A :: B\n\nA :: B');
    expect(cards).toHaveLength(2);
    expect(reviewItems(cards)).toHaveLength(2);
  });
});
