import { describe, expect, it, vi } from 'vitest';
import {
  EXTERNAL_WRITE_SLOW_NOTICE_MS,
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
  const indicated: Array<{ active: boolean; slow: boolean }> = [];
  let editable = true;
  let text = options.text ?? 'ABC';
  let timerCallback: (() => void) | null = null;
  let timerCleared = false;
  let adoptThrows = false;

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
      if (adoptThrows) throw new Error('adoption failed');
      adopted.push(document);
      text = document.content;
    },
    send: (message) => {
      order.push(`send:${message.type}`);
      sent.push(message);
    },
    indicate: (active, slow = false) => indicated.push({ active, slow }),
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
    fireSlowNotice: () => timerCallback?.(),
    hasTimer: () => timerCallback !== null,
    timerWasCleared: () => timerCleared,
    breakAdoption: () => {
      adoptThrows = true;
    },
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

  // 2 — 4.0E.1 §11/§13. The page never releases the document on a timer.
  it('never releases the document on a deadline of its own', () => {
    // Once the snapshot has gone out, the host may be writing a temp file,
    // syncing it or renaming it. There is no length of time after which
    // guessing becomes safe, so the page does not guess: a slow write stays a
    // held write until the host says otherwise.
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    expect(h.isEditable()).toBe(false);

    h.fireSlowNotice();

    expect(h.barrier.active).toBe(true);
    expect(h.isEditable()).toBe(false);
    expect(h.type('E')).toBe(false);
    expect(h.adopted).toHaveLength(0);
    // Only the words change.
    expect(h.indicated).toEqual([
      { active: true, slow: false },
      { active: true, slow: true },
    ]);
  });

  it('says a slow write is slow rather than pretending it finished', () => {
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    h.fireSlowNotice();
    expect(h.sent.filter((m) => m.type !== 'external_write_ready')).toEqual([]);
    expect(h.barrier.queuedCount).toBe(0);
    expect(h.barrier.active).toBe(true);
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

    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(true);
    expect(h.adopted).toEqual([committed('ABCD\nXYZ')]);
    expect(h.currentText()).toBe('ABCD\nXYZ');
    expect(h.barrier.currentGeneration()).toBe(1);
    expect(h.isEditable()).toBe(true);
  });

  it('answers a later request with the generation it was given', () => {
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.apply(NOTE, REQUEST, 7, committed('novo'));

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
    h.barrier.apply(NOTE, REQUEST, 4, committed('novo'));

    h.barrier.begin(NOTE, 'cccccccc-1111-4222-8333-444444444444', 3);
    expect(h.barrier.active).toBe(false);
    expect(h.isEditable()).toBe(true);
  });

  // 7
  it('lets the reader carry on editing once the write has landed', () => {
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

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

    h.barrier.apply(NOTE, REQUEST, 1, committed('novo'));

    expect(ran).toEqual(['capture', 'image', 'metadata']);
    expect(h.barrier.queuedCount).toBe(0);
  });

  it('runs a held edit against the document as it is after the commit', () => {
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => h.type(' + captura'));
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABC\nXYZ'));

    expect(h.currentText()).toBe('ABC\nXYZ + captura');
  });

  it('keeps a held edit even when the write was abandoned', () => {
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => h.type(' + captura'));
    h.barrier.abort(REQUEST);

    expect(h.currentText()).toBe('ABC + captura');
  });

  it('keeps a held edit through a slow write and runs it when the host answers', () => {
    // A capture the reader made in another application is gone from the
    // clipboard by the time anyone notices. Losing it because a write happened
    // to be in flight is not recoverable, which is why nothing here drops one —
    // and a slow write does not release it early either.
    const h = harness({ text: 'ABC' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => h.type(' + captura'));
    h.fireSlowNotice();
    expect(h.currentText()).toBe('ABC');
    expect(h.barrier.queuedCount).toBe(1);

    h.barrier.apply(NOTE, REQUEST, 1, committed('ABC\nXYZ'));
    expect(h.currentText()).toBe('ABC\nXYZ + captura');
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
    expect(h.indicated).toEqual([{ active: true, slow: false }]);
    h.barrier.apply(NOTE, REQUEST, 1, committed('novo'));
    expect(h.indicated).toEqual([
      { active: true, slow: false },
      { active: false, slow: false },
    ]);
  });

  it('notices a slow write well before a person would give up on it', () => {
    expect(EXTERNAL_WRITE_SLOW_NOTICE_MS).toBeGreaterThan(1000);
    expect(EXTERNAL_WRITE_SLOW_NOTICE_MS).toBeLessThan(10000);
  });

  // The acknowledgement — 4.0E.1 §6-§8, §14-§16.

  it('confirms adoption itself, naming the note, the request and the generation', () => {
    // 4.0E.1 §15. The normal path. This message, and nothing else, is what
    // lets the host call the window synchronised.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(true);

    expect(h.sent.at(-1)).toEqual({
      type: 'external_write_applied',
      payload: { id: NOTE, requestId: REQUEST, generation: 1 },
    });
    // Sent only after the document really was adopted and editing resumed.
    expect(h.order.indexOf('adopt')).toBeLessThan(
      h.order.lastIndexOf('send:external_write_applied'),
    );
    expect(h.order.indexOf('thaw')).toBeLessThan(
      h.order.lastIndexOf('send:external_write_applied'),
    );
    expect(h.barrier.currentGeneration()).toBe(1);
    expect(h.isEditable()).toBe(true);
  });

  it('answers a request it is not waiting on with a refusal, not silence', () => {
    // 4.0E.1 §14. The host learns at once rather than waiting out its timeout,
    // and it never mistakes "no answer yet" for "adopted".
    const h = harness();
    const other = '99999999-8888-4777-8666-555555555555';
    expect(h.barrier.apply(NOTE, other, 1, committed('novo'))).toBe(false);

    expect(h.sent.at(-1)).toEqual({
      type: 'external_write_apply_failed',
      payload: { id: NOTE, requestId: other },
    });
    expect(h.adopted).toHaveLength(0);
  });

  it('reports a failed adoption instead of claiming the window is in step', () => {
    // 4.0E.1 §14. Adoption throwing must not be able to look like success.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();

    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(false);
    expect(h.sent.at(-1)).toEqual({
      type: 'external_write_apply_failed',
      payload: { id: NOTE, requestId: REQUEST },
    });
    // No positive acknowledgement anywhere in the conversation.
    expect(h.sent.some((m) => m.type === 'external_write_applied')).toBe(false);
  });

  it('keeps the old generation when adoption failed, so nothing stale can be written', () => {
    // The page is showing text the file no longer has. It must not be able to
    // save that over the change that was just committed — so it stays on the
    // superseded generation, which the host refuses.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.barrier.currentGeneration()).toBe(0);
  });

  it('releases the editor even when adoption failed', () => {
    // The file is already correct. Leaving the note frozen would make it
    // unusable and unclosable for the sake of a write that already succeeded.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();
    h.barrier.defer(() => h.type(' + captura'));
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.barrier.active).toBe(false);
    expect(h.isEditable()).toBe(true);
    expect(h.barrier.queuedCount).toBe(0);
  });

  it('does not acknowledge twice for one write', () => {
    const h = harness();
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.apply(NOTE, REQUEST, 1, committed('novo'));
    const acks = h.sent.filter((m) => m.type === 'external_write_applied').length;

    // A repeat finds nothing waiting and is refused rather than confirmed again.
    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('novo'))).toBe(false);
    expect(h.sent.filter((m) => m.type === 'external_write_applied')).toHaveLength(acks);
  });

  // 4.0E.1 §13 — the slow-commit race, end to end.
  it('holds everything through a commit slower than the old client timeout', () => {
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);

    // Whatever the host is doing, it is taking longer than the fifteen seconds
    // the page used to give up after. Nothing here may change.
    h.fireSlowNotice();
    expect(h.type('E')).toBe(false);
    expect(h.barrier.defer(() => h.type(' + captura'))).toBe(true);
    expect(h.isEditable()).toBe(false);
    expect(h.currentText()).toBe('ABCD');
    expect(h.sent.map((m) => m.type)).toEqual(['external_write_ready']);

    // And then the commit lands.
    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(true);
    expect(h.currentText()).toBe('ABCD\nXYZ + captura');
    expect(h.barrier.currentGeneration()).toBe(1);
    expect(h.isEditable()).toBe(true);
    expect(h.barrier.queuedCount).toBe(0);
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
