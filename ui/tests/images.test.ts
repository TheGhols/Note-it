import { describe, expect, it } from 'vitest';
import { redo, undo } from '@tiptap/pm/history';
import { TextSelection } from '@tiptap/pm/state';
import { NoteEditor } from '../src/editor/editor.ts';
import { commitImageAttributes, commitImageRemoval } from '../src/editor/imageView.ts';
import { noteTitle } from '../src/ui/noteTitle.ts';
import { visibleText } from '../src/markdown/visibleText.ts';
import { sanitizeMarkdown } from '../src/markdown/sanitizer.ts';
import {
  clampImageWidth,
  DEFAULT_IMAGE_ALIGN,
  IMAGE_ALIGNMENTS,
  imageDisplayUri,
  isManagedAsset,
  MAX_IMAGE_WIDTH,
  MIN_IMAGE_WIDTH,
  normalizeImageAlign,
} from '../src/markdown/assetReference.ts';

const NOTE = '11111111-1111-4111-8111-111111111111';
const ASSET = '22222222-2222-4222-8222-222222222222';
const OTHER = '33333333-3333-4333-8333-333333333333';
const A = `../assets/${NOTE}/${ASSET}.png`;
const B = `../assets/${NOTE}/${OTHER}.jpg`;

function note(markdown = ''): NoteEditor {
  return new NoteEditor({
    element: document.createElement('div'),
    initialContent: markdown,
  });
}

/** The image nodes in the document, in order, with their attributes. */
function images(editor: NoteEditor): Record<string, unknown>[] {
  const found: Record<string, unknown>[] = [];
  editor.getView().state.doc.descendants((node) => {
    if (node.type.name === 'noteItImage') found.push(node.attrs);
  });
  return found;
}

/** Where one image is in the document, and what it currently says. */
function imageAt(editor: NoteEditor, index: number): { pos: number; node: any } {
  const view = editor.getView();
  let seen = 0;
  let target: { pos: number; node: any } | null = null;
  view.state.doc.descendants((node, pos) => {
    if (node.type.name !== 'noteItImage') return;
    if (seen === index) target = { pos, node };
    seen += 1;
  });
  if (!target) throw new Error(`no image at ${index}`);
  return target;
}

/**
 * Changes one image the way its own controls do — through the very function
 * the node view calls, so what the tests exercise is what the buttons run.
 */
function setImageAttrs(
  editor: NoteEditor,
  index: number,
  attrs: Record<string, unknown>,
): boolean {
  const { pos, node } = imageAt(editor, index);
  return commitImageAttributes(editor.getView(), pos, node, attrs);
}

/** Removes one image the way its own control does. */
function removeImageAt(editor: NoteEditor, index: number): void {
  const { pos, node } = imageAt(editor, index);
  commitImageRemoval(editor.getView(), pos, node);
}

describe('what a note may say about a picture', () => {
  it('recognises only its own managed references', () => {
    expect(isManagedAsset(A)).toBe(true);
    expect(isManagedAsset(B)).toBe(true);
    for (const foreign of [
      'https://exemplo.com/a.png',
      '/home/alguem/foto.png',
      'foto.png',
      '../assets/../../etc/passwd',
      `../assets/${NOTE}/${ASSET}.svg`,
      `../assets/${NOTE}/${ASSET}.exe`,
      `assets/${NOTE}/${ASSET}.png`,
      `../assets/nao-e-uuid/${ASSET}.png`,
      '',
      null,
      42,
    ]) {
      expect(isManagedAsset(foreign), String(foreign)).toBe(false);
    }
  });

  it('loads a managed reference through the host and nothing else through anything', () => {
    // The scheme is the only source an image element ever gets. A remote URL
    // resolves to nothing, so no request is made — the answer the page's own
    // Content-Security-Policy would give, arrived at before the request.
    expect(imageDisplayUri(A)).toBe(`note-it-asset:/${NOTE}/${ASSET}.png`);
    expect(imageDisplayUri('https://exemplo.com/a.png')).toBeNull();
    expect(imageDisplayUri('/etc/passwd')).toBeNull();
    expect(imageDisplayUri(`../assets/${NOTE}/${ASSET}.svg`)).toBeNull();
    expect(imageDisplayUri(A)).not.toContain('file:');
    expect(imageDisplayUri(A)).not.toContain('home');
  });

  it('clamps every width rather than believing one', () => {
    expect(clampImageWidth(320)).toBe(320);
    expect(clampImageWidth('320')).toBe(320);
    expect(clampImageWidth(320.4)).toBe(320);
    // Below the floor and above the ceiling are pulled in, not refused: a drag
    // that overshoots should stop at the limit rather than do nothing.
    expect(clampImageWidth(1)).toBe(MIN_IMAGE_WIDTH);
    expect(clampImageWidth(999_999)).toBe(MAX_IMAGE_WIDTH);
    // Nothing at all is a real answer: the stylesheet then sizes it.
    for (const nothing of [null, undefined, 0, -10, Number.NaN, Infinity, 'abc', {}, []]) {
      expect(clampImageWidth(nothing), String(nothing)).toBeNull();
    }
  });

  it('resolves every alignment to one of three words', () => {
    for (const entry of IMAGE_ALIGNMENTS) {
      expect(normalizeImageAlign(entry.id)).toBe(entry.id);
    }
    for (const unknown of ['justify', 'LEFT', '', null, undefined, 7, {}]) {
      expect(normalizeImageAlign(unknown)).toBe(DEFAULT_IMAGE_ALIGN);
    }
    expect(DEFAULT_IMAGE_ALIGN).toBe('center');
  });
});

describe('what is written into the note', () => {
  it('writes plain Markdown while there is nothing else to say', () => {
    const editor = note('');
    editor.insertImage(A);
    // No width chosen, default alignment: the file stays as plain as it has
    // always been, and the reference is relative.
    expect(editor.getMarkdown()).toBe(`![](${A})`);
    expect(editor.getMarkdown()).not.toContain('<img');
    expect(editor.getMarkdown()).not.toContain('base64');
    expect(editor.getMarkdown()).not.toContain('/home/');
  });

  it('writes a canonical tag once a width or an alignment is chosen', () => {
    const editor = note('');
    editor.insertImage(A);

    setImageAttrs(editor, 0, { width: 320 });
    expect(editor.getMarkdown()).toBe(`<img src="${A}" alt="" data-note-it-width="320">`);

    setImageAttrs(editor, 0, { align: 'left' });
    expect(editor.getMarkdown()).toBe(
      `<img src="${A}" alt="" data-note-it-width="320" data-note-it-align="left">`,
    );

    // Back to the default alignment and no width: back to plain Markdown.
    setImageAttrs(editor, 0, { align: 'center', width: null });
    expect(editor.getMarkdown()).toBe(`![](${A})`);
  });

  it('never stores a height, because the width and the picture decide it', () => {
    const editor = note('');
    editor.insertImage(A);
    setImageAttrs(editor, 0, { width: 240, align: 'right' });
    expect(editor.getMarkdown()).not.toContain('height');
  });

  it('round-trips both forms byte for byte', () => {
    for (const stored of [
      `![](${A})`,
      `<img src="${A}" alt="" data-note-it-width="320">`,
      `<img src="${A}" alt="" data-note-it-align="left">`,
      `<img src="${A}" alt="" data-note-it-align="right">`,
      `<img src="${A}" alt="legenda" data-note-it-width="120" data-note-it-align="left">`,
      `texto antes ![](${A}) texto depois`,
      `# Título\n\n![](${A})\n\nDepois`,
      `![](${A})\n\n![](${B})`,
    ]) {
      expect(note(stored).getMarkdown(), stored).toBe(stored);
    }
  });

  it('leaves a hand-written image exactly as it was', () => {
    // The one image case the shared corpus already had. Nothing about this
    // phase rewrites a note somebody typed themselves.
    const stored = '![diagrama do fígado](https://exemplo.com/a.png)';
    expect(note(stored).getMarkdown()).toBe(stored);
  });

  it('normalises the default alignment away, so one image is always one form', () => {
    // A save that changed nothing must change nothing on disk.
    const editor = note(`<img src="${A}" alt="" data-note-it-align="center">`);
    expect(editor.getMarkdown()).toBe(`![](${A})`);
    expect(note(editor.getMarkdown()).getMarkdown()).toBe(`![](${A})`);
  });
});

describe('inserting', () => {
  it('puts an image into an empty note', () => {
    const editor = note('');
    editor.insertImage(A);
    expect(images(editor)).toHaveLength(1);
    expect(images(editor)[0]).toMatchObject({ src: A, alt: '', width: null, align: 'center' });
  });

  it('puts an image into a note that already says something', () => {
    const editor = note('# Biópsia hepática\n\nprimeiro parágrafo');
    // Wherever the caret is, which after opening a note is where the reader
    // left it. Put at the end here, as a menu insertion from the foot of a
    // note would be.
    const view = editor.getView();
    view.dispatch(
      view.state.tr.setSelection(
        TextSelection.create(view.state.doc, view.state.doc.content.size - 1),
      ),
    );
    editor.insertImage(A);
    const markdown = editor.getMarkdown();
    expect(markdown).toContain('# Biópsia hepática');
    expect(markdown).toContain('primeiro parágrafo');
    expect(markdown).toContain(`![](${A})`);
  });

  it('takes more than one image in the same note', () => {
    const editor = note('');
    editor.insertImage(A);
    editor.insertImage(B);
    expect(images(editor)).toHaveLength(2);
    expect(images(editor).map((attrs) => attrs.src)).toEqual([A, B]);

    // ...and they keep their own layout independently.
    setImageAttrs(editor, 0, { align: 'left', width: 120 });
    setImageAttrs(editor, 1, { align: 'right' });
    const markdown = editor.getMarkdown();
    expect(markdown).toContain(`data-note-it-width="120" data-note-it-align="left"`);
    expect(markdown).toContain(`src="${B}" alt="" data-note-it-align="right"`);
    expect(note(markdown).getMarkdown()).toBe(markdown);
  });

  it('survives being closed and opened again', () => {
    const editor = note('texto');
    editor.insertImage(A);
    setImageAttrs(editor, 0, { width: 200, align: 'left' });
    const stored = editor.getMarkdown();

    const reopened = note(stored);
    expect(images(reopened)).toHaveLength(1);
    expect(images(reopened)[0]).toMatchObject({ src: A, width: 200, align: 'left' });
    expect(reopened.getMarkdown()).toBe(stored);
  });
});

describe('history', () => {
  it('takes back an insertion and puts it back', () => {
    const editor = note('texto');
    const view = editor.getView();
    editor.insertImage(A);
    expect(images(editor)).toHaveLength(1);

    undo(view.state, view.dispatch);
    expect(images(editor)).toHaveLength(0);
    expect(editor.getMarkdown()).toBe('texto');

    redo(view.state, view.dispatch);
    expect(images(editor)).toHaveLength(1);
  });

  it('takes back a resize to the width before it', () => {
    const editor = note('');
    const view = editor.getView();
    editor.insertImage(A);
    setImageAttrs(editor, 0, { width: 320 });
    setImageAttrs(editor, 0, { width: 120 });

    undo(view.state, view.dispatch);
    expect(images(editor)[0].width).toBe(320);
    undo(view.state, view.dispatch);
    expect(images(editor)[0].width).toBeNull();
  });

  it('takes back an alignment', () => {
    const editor = note('');
    const view = editor.getView();
    editor.insertImage(A);
    setImageAttrs(editor, 0, { align: 'right' });
    expect(images(editor)[0].align).toBe('right');

    undo(view.state, view.dispatch);
    expect(images(editor)[0].align).toBe('center');
  });

  it('takes back a removal', () => {
    const editor = note('antes');
    const view = editor.getView();
    editor.insertImage(A);
    const withImage = editor.getMarkdown();

    removeImageAt(editor, 0);
    expect(images(editor)).toHaveLength(0);
    expect(editor.getMarkdown()).toBe('antes');

    undo(view.state, view.dispatch);
    expect(editor.getMarkdown()).toBe(withImage);
  });

  it('writes nothing at all for a change that changes nothing', () => {
    // Choosing the alignment a picture already has, or releasing a handle on
    // the width it started from, must not touch the document — so it does not
    // move the note's modification date and does not add an undo step.
    const editor = note('');
    const view = editor.getView();
    editor.insertImage(A);
    const stored = editor.getMarkdown();

    expect(setImageAttrs(editor, 0, { align: 'center' })).toBe(false);
    expect(setImageAttrs(editor, 0, { width: null })).toBe(false);
    expect(editor.getMarkdown()).toBe(stored);

    // One undo still reaches the state before the picture arrived.
    undo(view.state, view.dispatch);
    expect(images(editor)).toHaveLength(0);
  });
});

describe('what a picture must never become', () => {
  it('drops a tag pointing anywhere but at a managed asset', () => {
    for (const hostile of [
      '<img src="/etc/passwd" alt="">',
      '<img src="../../../etc/passwd" alt="">',
      '<img src="javascript:alert(1)" alt="">',
      '<img src="data:image/svg+xml,<svg onload=alert(1)>" alt="">',
      `<img src="../assets/${NOTE}/${ASSET}.svg" alt="">`,
      '<img src="https://exemplo.com/rastreador.gif" alt="">',
    ]) {
      // The tag does not survive: whatever is left is inert text, and no
      // picture, no element and no request comes of it.
      expect(sanitizeMarkdown(hostile), hostile).not.toContain('<img');
      const editor = note(hostile);
      expect(images(editor), hostile).toHaveLength(0);
      expect(editor.getView().dom.querySelector('img'), hostile).toBeNull();
      expect(editor.getView().dom.querySelector('[onload]'), hostile).toBeNull();
    }
  });

  it('keeps nothing from a tag but the four attributes it understands', () => {
    const hostile = `<img src="${A}" alt="" onerror="alert(1)" onload="alert(2)" srcset="x" style="position:fixed" class="roubar" data-note-it-width="150">`;
    const cleaned = sanitizeMarkdown(hostile);

    expect(cleaned).toBe(`<img src="${A}" alt="" data-note-it-width="150">`);
    for (const attribute of ['onerror', 'onload', 'srcset', 'style', 'class']) {
      expect(cleaned).not.toContain(attribute);
    }

    const editor = note(hostile);
    const rendered = editor.getView().dom.querySelector('img');
    expect(rendered?.getAttribute('onerror')).toBeNull();
    expect(rendered?.getAttribute('srcset')).toBeNull();
    expect(editor.getView().dom.querySelector('[onerror]')).toBeNull();
  });

  it('refuses a width that would break the note', () => {
    const editor = note(`<img src="${A}" alt="" data-note-it-width="99999">`);
    expect(images(editor)[0].width).toBe(MAX_IMAGE_WIDTH);

    for (const broken of ['0', '-40', 'NaN', 'Infinity', '', 'muito']) {
      const parsed = note(`<img src="${A}" alt="" data-note-it-width="${broken}">`);
      // Not a width at all: the picture is drawn at the stylesheet's own cap.
      expect(images(parsed)[0].width, broken).toBeNull();
    }
  });

  it('refuses an alignment that is not one of the three', () => {
    const editor = note(`<img src="${A}" alt="" data-note-it-align="fixed">`);
    expect(images(editor)[0].align).toBe('center');
    expect(editor.getMarkdown()).toBe(`![](${A})`);
  });

  it('never asks the network for anything', () => {
    // A remote image round-trips as the text it is and is drawn with no
    // source, so displaying a note fetches nothing.
    const editor = note('![remoto](https://exemplo.com/a.png)');
    const rendered = editor.getView().dom.querySelector('img');
    expect(rendered?.getAttribute('src')).toBeNull();
    expect(editor.getMarkdown()).toBe('![remoto](https://exemplo.com/a.png)');
  });
});

describe('a picture is not text', () => {
  it('leaves a note that holds only an image without a title', () => {
    // The alt is empty for every image this application inserts, so a note
    // that is one picture is still a note nobody has named.
    for (const stored of [
      `![](${A})`,
      `<img src="${A}" alt="" data-note-it-width="320" data-note-it-align="left">`,
    ]) {
      expect(visibleText(stored), stored).toBe('');
      expect(noteTitle(stored), stored).toBe('Nota sem título');
    }
  });

  it('names a note after its words, whatever pictures are in it', () => {
    const stored = `![](${A})\n\n# Biópsia hepática\n\n![](${B})`;
    expect(noteTitle(stored)).toBe('Biópsia hepática');
  });

  it('keeps every technical detail out of what the note reads as', () => {
    const stored = `<img src="${A}" alt="" data-note-it-width="320" data-note-it-align="left">\n\nencefalopatia hepática`;
    const visible = visibleText(stored);

    expect(visible).toContain('encefalopatia hepática');
    for (const technical of [
      NOTE,
      ASSET,
      'assets',
      '../',
      'data-note-it-width',
      'data-note-it-align',
      '320',
      'left',
      '<img',
      '.png',
    ]) {
      expect(visible, technical).not.toContain(technical);
    }
  });

  it('leaves the projection of a hand-written image exactly as it was', () => {
    // The corpus case, unchanged: an image somebody typed still reads as its
    // alternative text.
    expect(visibleText('![diagrama do fígado](https://exemplo.com/a.png)')).toBe(
      'diagrama do fígado',
    );
  });
});

describe('what changes the note and what does not', () => {
  it('changes the stored text when a picture arrives, is laid out, or leaves', () => {
    const editor = note('texto');
    const before = editor.getMarkdown();

    editor.insertImage(A);
    const inserted = editor.getMarkdown();
    expect(inserted).not.toBe(before);

    setImageAttrs(editor, 0, { align: 'left' });
    const aligned = editor.getMarkdown();
    expect(aligned).not.toBe(inserted);

    setImageAttrs(editor, 0, { width: 200 });
    expect(editor.getMarkdown()).not.toBe(aligned);
  });

  it('changes nothing when the picture is only selected', () => {
    const editor = note(`texto ![](${A})`);
    const before = editor.getMarkdown();
    const view = editor.getView();

    // The selection moves onto the image and over it; the document does not.
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 1)));
    view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, 3, 6)));

    expect(editor.getMarkdown()).toBe(before);
  });
});
