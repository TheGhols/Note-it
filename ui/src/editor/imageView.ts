import { closeHistory } from '@tiptap/pm/history';
import {
  clampImageWidth,
  IMAGE_ALIGNMENTS,
  ImageAlign,
  imageDisplayUri,
  MAX_IMAGE_WIDTH,
  MIN_IMAGE_WIDTH,
  normalizeImageAlign,
} from '../markdown/assetReference.ts';
import { MISSING_IMAGE_LABEL } from './image.ts';

/**
 * The width a drag would settle on, from where it started and how far it went.
 *
 * A pure function so the arithmetic — which edge is being pulled, which way
 * that edge grows, and the floor and ceiling — is testable without a pointer.
 * The ceiling is the editor's own usable width: an image can be made as wide
 * as the note and no wider, whatever the pointer does.
 */
export function widthFromDrag(options: {
  startWidth: number;
  deltaX: number;
  handle: 'left' | 'right';
  available: number;
}): number {
  const direction = options.handle === 'right' ? 1 : -1;
  const proposed = options.startWidth + direction * options.deltaX;
  const ceiling = Math.min(
    MAX_IMAGE_WIDTH,
    Math.max(MIN_IMAGE_WIDTH, Math.round(options.available)),
  );
  return Math.min(Math.max(Math.round(proposed), MIN_IMAGE_WIDTH), ceiling);
}

/**
 * Writes one change to a picture, as its own step in the history.
 *
 * Two things are decided here, and both are the reason this is a function
 * rather than a line inside a click handler.
 *
 * A change that changes nothing is not written at all, so choosing the
 * alignment a picture already has, or releasing a handle on the width it
 * started from, leaves the document — and the note's modification date — alone.
 *
 * And a change that *is* written closes the history group first. Without that,
 * a resize and the alignment chosen a moment later are folded into one entry
 * by the history plugin's own time-based grouping, and one `Ctrl+Z` undoes
 * both. Two deliberate acts are two steps.
 *
 * Returns whether anything was written.
 */
export function commitImageAttributes(
  view: any,
  pos: number,
  node: any,
  attrs: Record<string, unknown>,
): boolean {
  const unchanged = Object.entries(attrs).every(([key, value]) => node.attrs[key] === value);
  if (unchanged) return false;
  const tr = view.state.tr;
  closeHistory(tr);
  view.dispatch(tr.setNodeMarkup(pos, undefined, { ...node.attrs, ...attrs }));
  return true;
}

/**
 * Takes one picture out of the document, as its own step in the history.
 *
 * Its own step for the same reason a resize is: removing a picture a moment
 * after inserting it are two acts, and one `Ctrl+Z` should undo one of them.
 */
export function commitImageRemoval(view: any, pos: number, node: any): void {
  const tr = view.state.tr;
  closeHistory(tr);
  view.dispatch(tr.delete(pos, pos + node.nodeSize));
}

/**
 * One image in the document, with its own handles and its own controls.
 *
 * Two rules shape the whole thing.
 *
 * **A drag is one change, not five hundred.** Pulling a handle repaints the
 * picture by writing a style straight onto the element; not one transaction is
 * dispatched until the pointer is released, and then exactly one is. So the
 * image follows the pointer, the history gets a single step, and `Ctrl+Z`
 * returns the width the reader started from rather than the width they passed
 * through a sixtieth of a second ago.
 *
 * **A change that changes nothing is not a change.** Releasing on the same
 * width, or choosing the alignment the image already has, dispatches nothing —
 * so the note is not rewritten and its modification date does not move for a
 * click that did not alter it.
 */
export function imageNodeView(options: {
  node: any;
  /** The ProseMirror view, which Tiptap hands over at construction. The
   *  editor's own `view` is not available yet at this point — the first render
   *  of the document is what builds this. */
  view: any;
  getPos: () => number | undefined;
}) {
  const view = options.view;
  const doc = (view.dom.ownerDocument ?? globalThis.document) as Document;
  let node = options.node;

  const frame = doc.createElement('span');
  frame.className = 'note-image-frame';
  frame.setAttribute('data-align', normalizeImageAlign(node.attrs.align));

  const image = doc.createElement('img');
  image.className = 'note-image';
  image.draggable = false;
  frame.append(image);

  const fallback = doc.createElement('span');
  fallback.className = 'note-image-missing';
  fallback.textContent = MISSING_IMAGE_LABEL;
  fallback.hidden = true;
  frame.append(fallback);

  image.addEventListener('error', () => {
    // A note pointing at a picture that is no longer there is a note with a
    // gap in it, not a broken note.
    image.hidden = true;
    fallback.hidden = false;
  });
  image.addEventListener('load', () => {
    image.hidden = false;
    fallback.hidden = true;
  });

  const controls = doc.createElement('span');
  controls.className = 'note-image-controls';
  controls.setAttribute('role', 'group');
  controls.setAttribute('aria-label', 'Imagem');
  // The controls sit over the picture and belong to the chrome, so a pointer
  // landing on them must never reach the document underneath.
  controls.addEventListener('pointerdown', (event) => event.stopPropagation());
  controls.addEventListener('mousedown', (event) => event.preventDefault());

  const alignButtons = new Map<ImageAlign, HTMLButtonElement>();
  for (const entry of IMAGE_ALIGNMENTS) {
    const button = doc.createElement('button');
    button.type = 'button';
    button.className = 'note-image-control';
    button.dataset.align = entry.id;
    button.textContent = entry.label;
    button.setAttribute('aria-label', `Alinhar à ${entry.label.toLowerCase()}`);
    button.setAttribute('aria-pressed', 'false');
    button.title = entry.label;
    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      applyAlign(entry.id);
    });
    controls.append(button);
    alignButtons.set(entry.id, button);
  }

  const remove = doc.createElement('button');
  remove.type = 'button';
  remove.className = 'note-image-control note-image-remove';
  remove.textContent = 'Remover';
  remove.setAttribute('aria-label', 'Remover imagem');
  remove.title = 'Remover imagem';
  remove.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    removeImage();
  });
  controls.append(remove);
  frame.append(controls);

  const handles: HTMLElement[] = [];
  for (const side of ['left', 'right'] as const) {
    const handle = doc.createElement('span');
    handle.className = `note-image-handle note-image-handle-${side}`;
    handle.dataset.handle = side;
    handle.setAttribute('aria-hidden', 'true');
    handle.addEventListener('pointerdown', (event) => beginResize(event, side));
    frame.append(handle);
    handles.push(handle);
  }

  function currentPos(): number | null {
    const pos = options.getPos();
    return typeof pos === 'number' ? pos : null;
  }

  function updateAttributes(attrs: Record<string, unknown>): void {
    const pos = currentPos();
    if (pos === null) return;
    commitImageAttributes(view, pos, node, attrs);
  }

  function applyAlign(align: ImageAlign): void {
    updateAttributes({ align });
  }

  function removeImage(): void {
    const pos = currentPos();
    if (pos === null) return;
    commitImageRemoval(view, pos, node);
  }

  let resizing: { pointerId: number; startX: number; startWidth: number; handle: 'left' | 'right' } | null =
    null;

  function beginResize(event: PointerEvent, handle: 'left' | 'right'): void {
    // The note is dragged by its header bar and this is not it. Stopping the
    // event here also keeps ProseMirror from starting a selection drag.
    event.preventDefault();
    event.stopPropagation();
    const startWidth = image.getBoundingClientRect().width || image.naturalWidth || MIN_IMAGE_WIDTH;
    resizing = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth,
      handle,
    };
    frame.setAttribute('data-resizing', 'true');
    try {
      (event.target as Element).setPointerCapture(event.pointerId);
    } catch {
      // Capture is an optimisation; the listeners below still follow the drag.
    }
    doc.addEventListener('pointermove', handleResizeMove);
    doc.addEventListener('pointerup', endResize);
    doc.addEventListener('pointercancel', endResize);
  }

  function availableWidth(): number {
    const editorWidth = view.dom.clientWidth;
    return editorWidth > 0 ? editorWidth : MAX_IMAGE_WIDTH;
  }

  function handleResizeMove(event: PointerEvent): void {
    if (!resizing || event.pointerId !== resizing.pointerId) return;
    const width = widthFromDrag({
      startWidth: resizing.startWidth,
      deltaX: event.clientX - resizing.startX,
      handle: resizing.handle,
      available: availableWidth(),
    });
    // Painted, not dispatched: the document is left alone until the pointer
    // is released, so one drag is one entry in the history.
    image.style.width = `${width}px`;
  }

  function endResize(event: PointerEvent): void {
    if (!resizing || event.pointerId !== resizing.pointerId) return;
    const width = widthFromDrag({
      startWidth: resizing.startWidth,
      deltaX: event.clientX - resizing.startX,
      handle: resizing.handle,
      available: availableWidth(),
    });
    resizing = null;
    frame.removeAttribute('data-resizing');
    doc.removeEventListener('pointermove', handleResizeMove);
    doc.removeEventListener('pointerup', endResize);
    doc.removeEventListener('pointercancel', endResize);
    updateAttributes({ width: clampImageWidth(width) });
  }

  function render(): void {
    const src = imageDisplayUri(node.attrs.src);
    if (src === null) {
      // Not one of the store's own assets: no source, so nothing is requested.
      image.removeAttribute('src');
      image.hidden = true;
      fallback.hidden = false;
    } else if (image.getAttribute('src') !== src) {
      image.setAttribute('src', src);
    }
    image.alt = typeof node.attrs.alt === 'string' ? node.attrs.alt : '';

    const width = clampImageWidth(node.attrs.width);
    if (width === null) {
      image.style.removeProperty('width');
      frame.removeAttribute('data-width');
    } else {
      image.style.width = `${width}px`;
      // Read by the stylesheet, which drops the default cap once the reader
      // has chosen a width of their own.
      frame.setAttribute('data-width', String(width));
    }

    const align = normalizeImageAlign(node.attrs.align);
    frame.setAttribute('data-align', align);
    for (const [id, button] of alignButtons) {
      button.setAttribute('aria-pressed', String(id === align));
    }
  }

  render();

  return {
    dom: frame,
    update(updated: any) {
      if (updated.type.name !== node.type.name) return false;
      node = updated;
      render();
      return true;
    },
    selectNode() {
      frame.setAttribute('data-selected', 'true');
    },
    deselectNode() {
      frame.removeAttribute('data-selected');
    },
    stopEvent(event: Event) {
      // The controls and the handles are the node view's own interface. Letting
      // ProseMirror handle their events would move the selection out from under
      // the very buttons the reader is pressing.
      const target = event.target as Node | null;
      if (!target) return false;
      return controls.contains(target) || handles.some((handle) => handle.contains(target));
    },
    ignoreMutation() {
      // The style written during a drag, and the fallback being swapped in, are
      // this view's business and never a change to the document.
      return true;
    },
    destroy() {
      doc.removeEventListener('pointermove', handleResizeMove);
      doc.removeEventListener('pointerup', endResize);
      doc.removeEventListener('pointercancel', endResize);
    },
  };
}
