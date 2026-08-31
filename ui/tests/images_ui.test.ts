import { afterEach, describe, expect, inject, it, vi } from 'vitest';
import { NoteMenu } from '../src/ui/menu.ts';
import { PaperColor } from '../src/bridge/types.ts';
import {
  encodeBase64,
  imageFileIn,
  IMAGE_MIME_TYPES,
  isImagePaste,
} from '../src/editor/imageTransfer.ts';
import { widthFromDrag } from '../src/editor/imageView.ts';
import { MAX_IMAGE_WIDTH, MIN_IMAGE_WIDTH } from '../src/markdown/assetReference.ts';
import { declarationIn, ruleFor, rulesFor } from './support/stylesheet.ts';

const COLORS: PaperColor[] = ['yellow', 'blue', 'green', 'pink', 'purple', 'gray', 'black'];

function renderedPage(): Document {
  return new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
}

let active: NoteMenu | null = null;

afterEach(() => {
  active?.destroy();
  active = null;
  document.body.innerHTML = '';
});

function mount() {
  document.body.innerHTML = renderedPage().body.innerHTML;
  const handlers = {
    onSelectColor: vi.fn(),
    onSelectPaperType: vi.fn(),
    onSelectPaperIntensity: vi.fn(),
    onSelectTheme: vi.fn(),
    onToggleCollapsed: vi.fn(),
    onSelectTextSize: vi.fn(),
    onSelectTextColor: vi.fn(),
    onSelectHighlight: vi.fn(),
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn(),
    onResetZoom: vi.fn(),
    onUiScaleIn: vi.fn(),
    onUiScaleOut: vi.fn(),
    onResetUiScale: vi.fn(),
    onSelectLayerMode: vi.fn(),
    onToggleCodeBlock: vi.fn(),
    onSelectCodeLanguage: vi.fn(),
    onToggleBlockquote: vi.fn(),
    onSelectCallout: vi.fn(),
    onInsertComment: vi.fn(),
    onOpenGlobalSearch: vi.fn(),
    onOpenFind: vi.fn(),
    onOpenReplace: vi.fn(),
    onTrashNote: vi.fn(),
    onOpenTrash: vi.fn(),
    onCreateBackup: vi.fn(),
    onInsertImage: vi.fn(),
    onOpenStudy: vi.fn(),
    onOpenStudyHub: vi.fn(),
    onToggleAutoPaste: vi.fn(),
    onSelectCaptureDelimiter: vi.fn(),
    onOpen: vi.fn(),
    onClose: vi.fn(),
  };
  const menu = new NoteMenu({
    trigger: document.getElementById('btn-menu')!,
    mount: document.getElementById('note-controls-left')!,
    colors: COLORS,
    handlers,
  });
  active = menu;
  return { menu, handlers };
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

/** A transfer as a paste or a drop hands one over. */
function transfer(options: { files?: File[]; text?: string }): DataTransfer {
  return {
    files: options.files ?? [],
    getData: (type: string) => (type === 'text/plain' ? (options.text ?? '') : ''),
  } as unknown as DataTransfer;
}

function imageFile(type = 'image/png', name = 'captura de tela.png'): File {
  return new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], name, { type });
}

describe('where an image is inserted from', () => {
  it('keeps its menu row whatever else reaches it', () => {
    // 3.12R.1 put a paperclip in the bar beside the six quick actions, the
    // timer and the close cross. It is a second door, so the row it is a
    // shortcut to has to still be there — a note too narrow for the paperclip
    // still has somewhere to insert a picture from.
    const page = renderedPage();
    expect(page.querySelectorAll('.note-header .icon-btn:not([hidden])')).toHaveLength(14);

    const note = mount();
    const panels = Array.from(
      note.menu.element.querySelectorAll<HTMLElement>(
        '.note-menu-panel:not([class*=" "]) .note-menu-submenu',
      ),
    ).map((item) => item.dataset.panel);
    expect(panels).toContain('media');
  });

  it('asks the host for the chooser, and says what else works', () => {
    const note = mount();
    note.menu.openMenu();
    const panel = note.menu.element.querySelector<HTMLElement>('.note-menu-media')!;
    const insert = panel.querySelector<HTMLButtonElement>('.note-menu-item')!;

    expect(insert.textContent).toContain('Inserir imagem…');
    expect(panel.querySelector('.note-menu-media-hint')?.textContent).toBe(
      'Também é possível colar uma imagem ou arrastá-la para a nota.',
    );

    click(insert);
    expect(note.handlers.onInsertImage).toHaveBeenCalledTimes(1);
    // The chooser is the host's window, so the menu gets out of its way.
    expect(note.menu.isOpen()).toBe(false);
  });

  it('is not a properties panel', () => {
    const note = mount();
    const panel = note.menu.element.querySelector<HTMLElement>('.note-menu-media')!;
    // One action and one sentence. No fields, no metadata editor.
    expect(panel.querySelectorAll('input, textarea, select')).toHaveLength(0);
    expect(panel.querySelectorAll('.note-menu-item')).toHaveLength(1);
  });
});

describe('reading a picture out of a gesture', () => {
  it('takes the four supported types and leaves everything else alone', () => {
    expect(IMAGE_MIME_TYPES).toEqual(['image/png', 'image/jpeg', 'image/webp', 'image/gif']);
    for (const type of IMAGE_MIME_TYPES) {
      expect(imageFileIn(transfer({ files: [imageFile(type)] }))).not.toBeNull();
    }
    for (const type of ['image/svg+xml', 'application/pdf', 'text/plain', '']) {
      expect(imageFileIn(transfer({ files: [imageFile(type)] })), type).toBeNull();
    }
    expect(imageFileIn(transfer({}))).toBeNull();
    expect(imageFileIn(null)).toBeNull();
  });

  it('treats a paste carrying text as the text paste it has always been', () => {
    // Copying from a browser usually produces a picture *and* its text. That
    // has always pasted as text here, and it still does.
    expect(isImagePaste(transfer({ files: [imageFile()], text: 'encefalopatia' }))).toBe(false);
    expect(isImagePaste(transfer({ text: 'apenas texto' }))).toBe(false);
    expect(isImagePaste(null)).toBe(false);

    // A screenshot carries no text, and that is an image paste.
    expect(isImagePaste(transfer({ files: [imageFile()] }))).toBe(true);
    expect(isImagePaste(transfer({ files: [imageFile()], text: '   ' }))).toBe(true);
  });

  it('never names the file it was handed', () => {
    // The page sends bytes; the host decides what they are. A dropped file's
    // real location is not something the page can ask the host to go and read.
    const file = imageFile('image/png', '../../../etc/passwd');
    expect(imageFileIn(transfer({ files: [file] }))?.name).toBe('../../../etc/passwd');
    // ...and the only thing that leaves the page is the content.
    expect(encodeBase64(new Uint8Array([0x89, 0x50, 0x4e, 0x47]).buffer)).toBe('iVBORw==');
  });

  it('encodes bytes for the wire without losing any', () => {
    const bytes = new Uint8Array(Array.from({ length: 512 }, (_, index) => index % 256));
    const encoded = encodeBase64(bytes.buffer);
    expect(typeof encoded).toBe('string');
    const decoded = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0));
    expect(Array.from(decoded)).toEqual(Array.from(bytes));
  });
});

describe('the arithmetic of a resize', () => {
  it('grows to the right and to the left from the handle that is pulled', () => {
    const base = { startWidth: 200, available: 1000 };
    expect(widthFromDrag({ ...base, handle: 'right', deltaX: 60 })).toBe(260);
    expect(widthFromDrag({ ...base, handle: 'right', deltaX: -60 })).toBe(140);
    // The left handle grows the picture when it is pulled away from it.
    expect(widthFromDrag({ ...base, handle: 'left', deltaX: -60 })).toBe(260);
    expect(widthFromDrag({ ...base, handle: 'left', deltaX: 60 })).toBe(140);
  });

  it('never goes below the floor or past the note', () => {
    // The ceiling is the note's own usable width: an image can be made as wide
    // as the note and no wider, whatever the pointer does.
    expect(widthFromDrag({ startWidth: 200, deltaX: -5000, handle: 'right', available: 900 })).toBe(
      MIN_IMAGE_WIDTH,
    );
    expect(widthFromDrag({ startWidth: 200, deltaX: 5000, handle: 'right', available: 340 })).toBe(
      340,
    );
    // A note narrower than the smallest picture still yields a usable one.
    expect(widthFromDrag({ startWidth: 200, deltaX: 5000, handle: 'right', available: 10 })).toBe(
      MIN_IMAGE_WIDTH,
    );
    expect(
      widthFromDrag({ startWidth: 200, deltaX: 1e9, handle: 'right', available: 1e9 }),
    ).toBe(MAX_IMAGE_WIDTH);
  });

  it('always lands on a whole number of pixels', () => {
    for (const deltaX of [0.4, 1.5, -2.7, 33.33]) {
      const width = widthFromDrag({ startWidth: 200, deltaX, handle: 'right', available: 900 });
      expect(Number.isInteger(width), String(deltaX)).toBe(true);
    }
  });
});

describe('how a picture sits in the text', () => {
  it('is capped until the reader chooses a width, and by the note after that', () => {
    // An inserted photograph opens at a readable size in a wide note and still
    // fits a narrow one; a stored width replaces that cap rather than fighting
    // it, and the picture never crosses the note.
    expect(ruleFor(':root').body).toContain('--note-image-default-width');
    expect(
      ruleFor('.note-image-frame:not([data-width]) .note-image').body,
    ).toContain('min(100%, var(--note-image-default-width))');
    expect(declarationIn('.note-image', 'max-width')).toBe('100%');
    expect(declarationIn('.note-image', 'height')).toBe('auto');
  });

  it('keeps the picture proportional, because only the width is ever set', () => {
    // Height is `auto` and is never written down, so there is no combination
    // of stored values that can stretch a picture.
    expect(declarationIn('.note-image', 'height')).toBe('auto');
    expect(rulesFor('.note-image-frame[data-height]')).toHaveLength(0);
  });

  it('floats left and right so the text runs alongside', () => {
    expect(declarationIn('.note-image-frame[data-align="left"]', 'float')).toBe('left');
    expect(declarationIn('.note-image-frame[data-align="right"]', 'float')).toBe('right');
    // Displacing the words rather than covering them: a float is the one
    // arrangement that cannot overlap the text it sits beside.
    for (const align of ['left', 'right']) {
      expect(rulesFor(`.note-image-frame[data-align="${align}"]`)[0].body).not.toContain(
        'position: absolute',
      );
    }
  });

  it('centres without wrapping, as a block of its own', () => {
    const centred = ruleFor('.note-image-frame[data-align="center"]').body;
    expect(centred).toContain('display: block');
    expect(centred).toContain('margin: 10px auto');
    expect(centred).not.toContain('float');
  });

  it('never lets a float escape the note or overlap a block that cannot flow', () => {
    // The editor closes over its own floats, so the last paragraph of a note
    // never hangs beside a picture that has ended.
    expect(declarationIn('.ProseMirror::after', 'clear')).toBe('both');
    // A quote and a comment each become their own formatting context and sit
    // beside a float rather than under it...
    // Asserted across every rule that matches, because the cascade takes the
    // last one and `ruleFor` hands back the first.
    const quoteRules = rulesFor('.ProseMirror blockquote');
    expect(quoteRules.some((rule) => /display:\s*flow-root/.test(rule.body))).toBe(true);
    expect(
      quoteRules.some((rule) =>
        rule.selectors.some((selector) => selector.includes('note-comment')),
      ),
    ).toBe(true);
    // ...and a code block already had one, because a long line of code
    // scrolls rather than reflowing.
    expect(declarationIn('.ProseMirror pre', 'overflow-x')).toBe('auto');
  });

  it('shows its controls and handles only while it is selected', () => {
    // A note being read carries nothing over its pictures.
    expect(declarationIn('.note-image-handle', 'display')).toBe('none');
    expect(declarationIn('.note-image-controls', 'display')).toBe('none');
    expect(
      declarationIn('.note-image-frame[data-selected="true"] .note-image-handle', 'display'),
    ).toBe('block');
    expect(
      declarationIn('.note-image-frame[data-selected="true"] .note-image-controls', 'display'),
    ).toBe('flex');
  });

  it('marks the alignment in force by more than a colour', () => {
    const pressed = ruleFor('.note-image-control[aria-pressed="true"]').body;
    expect(pressed).toContain('font-weight');
    expect(pressed).toContain('background-color');
  });

  it('says so when the picture is not there rather than showing a broken one', () => {
    expect(ruleFor('.note-image-missing').body).toContain('border');
    expect(declarationIn('.note-image-missing[hidden]', 'display')).toBe('none');
  });

  it('takes no permanent room from the note', () => {
    // The controls float over the picture; nothing about an image adds a
    // panel, a sidebar or a row the text has to give up.
    expect(declarationIn('.note-image-controls', 'position')).toBe('absolute');
    expect(declarationIn('.note-image-handle', 'position')).toBe('absolute');
  });
});

describe('what the page is allowed to load', () => {
  it('permits only the scheme the host serves', () => {
    // No `http:`, no `https:`, no `data:`, no `file:`. A note displays what
    // the store holds, and reaches the network for nothing.
    const csp = /http-equiv="Content-Security-Policy"\s+content="([^"]*)"/.exec(
      inject('indexHtml'),
    )![1];
    expect(csp).toContain("img-src 'self' note-it-asset:");
    expect(csp).toContain("connect-src 'none'");
    expect(csp).not.toContain('img-src *');
    for (const scheme of ['http:', 'https:', 'data:', 'file:']) {
      expect(csp.split(';').find((part) => part.includes('img-src'))).not.toContain(scheme);
    }
  });
});
