import { describe, expect, it, vi } from 'vitest';
import {
  EXTERNAL_WRITE_CLIENT_TIMEOUT_MS,
  ExternalWriteBarrier,
  type ExternalDocument,
  type ExternalWriteHooks,
} from '../src/bridge/externalWrite.ts';
import { NoteEditor } from '../src/editor/editor.ts';
import type { WebviewToHostMessage } from '../src/bridge/types.ts';

const NOTE = '11111111-2222-4333-8444-555555555555';
const REQUEST = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';

function emptyMetadata() {
  return { tags: [], properties: [] };
}

function committed(content: string): ExternalDocument {
  return {
    content,
    metadata: emptyMetadata(),
    createdAt: '2026-09-01T10:00:00Z',
    updatedAt: '2026-09-01T10:05:00Z',
  };
}

/**
 * A stand-in for the editor that records what happened and in what order.
 *
 * The order is the thing under test: everything else about this barrier is
 * ordinary bookkeeping, and the one property that cannot be got wrong is that
 * the document stops being editable before its text is read.
 */
function harness(options: { text?: string } = {}) {
  const order: string[] = [];
  const sent: WebviewToHostMessage[] = [];
  const adopted: ExternalDocument[] = [];
  const indicated: boolean[] = [];
  let editable = true;
  let text = options.text ?? 'ABC';
  let timerCallback: (() => void) | null = null;
  let timerCleared = false;

  const hooks: ExternalWriteHooks = {
    freeze: () => {
      order.push('freeze');
      editable = false;
    },
    thaw: () => {
      order.push('thaw');
      editable = true;
    },
    snapshot: () => {
      order.push('snapshot');
      return text;
    },
    adopt: (document) => {
      order.push('adopt');
      adopted.push(document);
      text = document.content;
    },
    send: (message) => {
      order.push(`send:${message.type}`);
      sent.push(message);
    },
    indicate: (active) => indicated.push(active),
    setTimer: (callback) => {
      timerCallback = callback;
      return 1;
    },
    clearTimer: () => {
      timerCleared = true;
    },
  };

  const barrier = new ExternalWriteBarrier(hooks);
  return {
    barrier,
    order,
    sent,
    adopted,
    indicated,
    isEditable: () => editable,
    currentText: () => text,
    type: (extra: string) => {
      // Only possible while the document is editable — which is the point.
      if (!editable) return false;
      text += extra;
      return true;
    },
    fireTimeout: () => timerCallback?.(),
    timerWasCleared: () => timerCleared,
  };
}

describe('the external write barrier', () => {
  // 1
  it('stops the document being edited before it reads it', () => {
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);

    expect(h.order.indexOf('freeze')).toBeLessThan(h.order.indexOf('snapshot'));
    expect(h.order).toEqual(['freeze', 'snapshot', 'send:external_write_ready']);
  });

  it('hands the host exactly what the editor held, unsaved text and all', () => {
    // The whole reason a plain flush is not enough: the text the host is given
    // has to include what the reader typed and the debounce has not filed yet.
    const h = harness({ text: 'ABCD' });
    h.barrier.setGeneration(3);
    h.barrier.begin(NOTE, REQUEST, 3);

    expect(h.sent).toEqual([
      {
        type: 'external_write_ready',
        payload: { id: NOTE, requestId: REQUEST, generation: 3, content: 'ABCD' },
      },
    ]);
  });

  it('refuses to let a keystroke land between the freeze and the snapshot', () => {
    const h = harness({ text: 'ABC' });
    const original = h.barrier.begin.bind(h.barrier);
    original(NOTE, REQUEST, 0);

    // Nothing can be typed from the moment the barrier starts.
    expect(h.type('D')).toBe(false);
    expect(h.sent[0]).toMatchObject({ payload: { content: 'ABC' } });
  });

  // 2
  it('gives up on a host that never answers, changing nothing', () => {
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    expect(h.isEditable()).toBe(false);

    h.fireTimeout();

    expect(h.barrier.active).toBe(false);
    expect(h.isEditable()).toBe(true);
    expect(h.adopted).toHaveLength(0);
    expect(h.currentText()).toBe('ABC');
  });

  it('cannot have a document applied after it has given up', () => {
    // The host abandons a request it timed out on, so a late answer must find
    // nothing waiting for it. Applying one here would replace the document
    // behind the reader's back, after they had started editing again.
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    h.fireTimeout();

    expect(h.barrier.apply(REQUEST, 1, committed('do host atrasado'))).toBe(false);
    expect(h.adopted).toHaveLength(0);
  });

  // 3
  it('releases the editor when the write is abandoned', () => {
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);

    expect(h.barrier.abort(REQUEST)).toBe(true);
    expect(h.isEditable()).toBe(true);
    expect(h.timerWasCleared()).toBe(true);
    expect(h.type('D')).toBe(true);
  });

  // 4
  it('leaves the text exactly as it was when the write failed before committing', () => {
    // A write that failed before the commit point changed nothing on disk, so
    // the page must keep the very text it was holding — including the part
    // that was never saved.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.abort(REQUEST);

    expect(h.currentText()).toBe('ABCD');
    expect(h.adopted).toHaveLength(0);
    expect(h.isEditable()).toBe(true);
  });

  // 5
  it('adopts the committed document and takes its generation', () => {
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);

    expect(h.barrier.apply(REQUEST, 1, committed('ABCD\nXYZ'))).toBe(true);
    expect(h.adopted).toEqual([committed('ABCD\nXYZ')]);
    expect(h.currentText()).toBe('ABCD\nXYZ');
    expect(h.barrier.currentGeneration()).toBe(1);
    expect(h.isEditable()).toBe(true);
  });

  it('answers a later request with the generation it was given', () => {
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.apply(REQUEST, 7, committed('novo'));

    const second = 'ffffffff-1111-4222-8333-444444444444';
    h.barrier.begin(NOTE, second, 7);
    expect(h.sent.at(-1)).toMatchObject({ payload: { generation: 7 } });
  });

  // 6
  it('refuses a request quoting a generation the page has moved past', () => {
    // The host would refuse it too. Refusing here as well means a superseded
    // request never even freezes the editor.
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.apply(REQUEST, 4, committed('novo'));

    h.barrier.begin(NOTE, 'cccccccc-1111-4222-8333-444444444444', 3);
    expect(h.barrier.active).toBe(false);
    expect(h.isEditable()).toBe(true);
  });

  // 7
  it('lets the reader carry on editing once the write has landed', () => {
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.apply(REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.type(' e mais')).toBe(true);
    expect(h.currentText()).toBe('ABCD\nXYZ e mais');
  });

  it('refuses a second write while one is in flight', () => {
    // Two of them would each snapshot the same text and the second commit
    // would silently undo the first.
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    const before = h.sent.length;
    h.barrier.begin(NOTE, 'dddddddd-1111-4222-8333-444444444444', 0);
    expect(h.sent).toHaveLength(before);
  });

  // 8, 9, 10
  it('holds every edit that arrives meanwhile and runs them all afterwards', () => {
    const h = harness();
    const ran: string[] = [];
    h.barrier.begin(NOTE, REQUEST, 0);

    // A clipboard capture, an image the host has just imported, and a metadata
    // save. None of them may be applied to a document that is about to be
    // replaced, and none of them may be thrown away.
    expect(h.barrier.defer(() => ran.push('capture'))).toBe(true);
    expect(h.barrier.defer(() => ran.push('image'))).toBe(true);
    expect(h.barrier.defer(() => ran.push('metadata'))).toBe(true);
    expect(h.barrier.queuedCount).toBe(3);
    expect(ran).toEqual([]);

    h.barrier.apply(REQUEST, 1, committed('novo'));

    expect(ran).toEqual(['capture', 'image', 'metadata']);
    expect(h.barrier.queuedCount).toBe(0);
  });

  it('runs a held edit against the document as it is after the commit', () => {
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => h.type(' + captura'));
    h.barrier.apply(REQUEST, 1, committed('ABC\nXYZ'));

    expect(h.currentText()).toBe('ABC\nXYZ + captura');
  });

  it('keeps a held edit even when the write was abandoned', () => {
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => h.type(' + captura'));
    h.barrier.abort(REQUEST);

    expect(h.currentText()).toBe('ABC + captura');
  });

  it('keeps a held edit even when the host never answered at all', () => {
    // A capture the reader made in another application is gone from the
    // clipboard by the time anyone notices. Losing it because a write happened
    // to be in flight is not recoverable, which is why nothing here drops one.
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => h.type(' + captura'));
    h.fireTimeout();

    expect(h.currentText()).toBe('ABC + captura');
  });

  it('runs an edit immediately when no write is in flight', () => {
    const h = harness();
    const action = vi.fn();
    expect(h.barrier.defer(action)).toBe(false);
    expect(action).toHaveBeenCalledOnce();
  });

  it('shows the syncing state only for as long as the write lasts', () => {
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    expect(h.indicated).toEqual([true]);
    h.barrier.apply(REQUEST, 1, committed('novo'));
    expect(h.indicated).toEqual([true, false]);
  });

  it('keeps its safety timeout well clear of a plausible write', () => {
    expect(EXTERNAL_WRITE_CLIENT_TIMEOUT_MS).toBeGreaterThan(5000);
  });
});

describe('the editor the barrier actually freezes', () => {
  function mount(content: string) {
    const element = document.createElement('div');
    document.body.append(element);
    const updates: string[] = [];
    const editor = new NoteEditor({
      element,
      initialContent: content,
      onUpdate: (markdown) => updates.push(markdown),
    });
    return { editor, updates, element };
  }

  it('refuses every change to the document, not just the ones a reader makes', () => {
    // Editability alone stops typing, pasting and dropping. It does *not* stop
    // a command the page runs itself — inserting an image the host has just
    // imported, say — and that is exactly the kind of change that would be
    // lost, or would overwrite a commit, during an external write. So the gate
    // is at the transaction, which every change goes through.
    const { editor } = mount('ABC');
    expect(editor.isEditable()).toBe(true);

    editor.setEditable(false);
    expect(editor.isEditable()).toBe(false);

    editor.insertImage('assets/x.png');
    expect(editor.getMarkdown()).not.toContain('assets/x.png');

    const view = editor.getView();
    view.dispatch(view.state.tr.insertText('D', view.state.doc.content.size - 1));
    expect(editor.getMarkdown()).not.toContain('ABCD');

    editor.setEditable(true);
    editor.insertImage('assets/x.png');
    expect(editor.getMarkdown()).toContain('assets/x.png');
  });

  it('still lets the committed document be adopted while it is locked', () => {
    // The one change the lock exists to make room for.
    const { editor } = mount('ABC');
    editor.setEditable(false);
    editor.setMarkdown('ABC\nXYZ');
    expect(editor.getMarkdown()).toContain('XYZ');
    expect(editor.isEditable()).toBe(false);
  });

  it('cancels the pending autosave when it freezes, and loses nothing by it', () => {
    // The debounce's text is not lost: whatever froze the editor reads the
    // document immediately afterwards, and that is what gets committed.
    const { editor } = mount('ABC');
    const view = editor.getView();
    view.dispatch(view.state.tr.insertText('D', view.state.doc.content.size - 1));
    expect(editor.hasPendingSave()).toBe(true);
    expect(editor.getMarkdown()).toContain('ABCD');

    editor.setEditable(false);
    expect(editor.hasPendingSave()).toBe(false);
    expect(editor.getMarkdown()).toContain('ABCD');
  });

  it('adopts a committed document without autosaving it straight back', () => {
    // Programmatic adoption must not look like an edit. If it did, the page
    // would answer the commit by sending the content it had just replaced.
    const { editor, updates } = mount('ABC');
    editor.setMarkdown('ABC\nXYZ');
    expect(updates).toEqual([]);
    expect(editor.hasPendingSave()).toBe(false);
    expect(editor.getMarkdown()).toContain('XYZ');
  });
});
