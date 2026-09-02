import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  EXTERNAL_WRITE_SLOW_NOTICE_MS,
  ExternalWriteBarrier,
  type ExternalDocument,
  type ExternalWriteHooks,
  type SyncState,
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
  const indicated: SyncState[] = [];
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
    indicate: (state) => indicated.push(state),
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
    /** The callback exactly as the timer captured it, kept past cancellation. */
    capturedSlowNotice: () => timerCallback,
    hasTimer: () => timerCallback !== null,
    timerWasCleared: () => timerCleared,
    breakAdoption: () => {
      adoptThrows = true;
    },
    mendAdoption: () => {
      adoptThrows = false;
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
    expect(h.indicated).toEqual(['syncing', 'slow']);
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
    expect(h.indicated).toEqual(['syncing']);
    h.barrier.apply(NOTE, REQUEST, 1, committed('novo'));
    expect(h.indicated).toEqual(['syncing', 'idle']);
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

  it('leaves the active write alone when a stale apply arrives', () => {
    // 4.0E.2 §9. A message for another request must not unblock the one that
    // is genuinely in flight — that would release the document mid-commit.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => h.type(' + captura'));
    const other = '99999999-8888-4777-8666-555555555555';

    expect(h.barrier.apply(NOTE, other, 9, committed('de outro pedido'))).toBe(false);

    expect(h.barrier.active).toBe(true);
    expect(h.isEditable()).toBe(false);
    expect(h.barrier.queuedCount).toBe(1);
    expect(h.barrier.currentGeneration()).toBe(0);
    expect(h.adopted).toHaveLength(0);

    // And the real one still completes normally afterwards.
    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(true);
    expect(h.isEditable()).toBe(true);
    expect(h.currentText()).toBe('ABCD\nXYZ + captura');
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

  it('keeps the old generation when adoption failed', () => {
    // The page is showing text the file no longer has. Moving the generation
    // on would mean the host accepting that text back.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.barrier.currentGeneration()).toBe(0);
  });

  // 4.0E.2 — the whole point of the subphase.
  it('does not release the editor when adoption failed', () => {
    // Releasing here would hand back an editor that looks entirely normal and
    // whose every save the host refuses on the stale generation: the reader
    // would type for as long as they liked and lose all of it, silently. A
    // held note that says why is recoverable by reopening it. This is not a
    // convenience trade-off — it is the difference between an inconsistency
    // the reader can see and one that eats their work.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.barrier.active).toBe(true);
    expect(h.barrier.adoptionFailed).toBe(true);
    expect(h.isEditable()).toBe(false);
    expect(h.type('E')).toBe(false);
    expect(h.currentText()).toBe('ABCD');
  });

  it('does not drain the queue when adoption failed', () => {
    // 4.0E.2 §13. The held actions are not lost and they are not run: applying
    // a capture, an image or a metadata save to a document the store has
    // already moved past would be a mutation nobody can see going wrong.
    const h = harness({ text: 'ABCD' });
    const ran: string[] = [];
    h.barrier.begin(NOTE, REQUEST, 0);
    h.barrier.defer(() => ran.push('capture'));
    h.barrier.defer(() => ran.push('image'));
    h.barrier.defer(() => ran.push('metadata'));
    h.breakAdoption();

    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(ran).toEqual([]);
    expect(h.barrier.queuedCount).toBe(3);
  });

  it('says the note is out of step, and keeps saying it', () => {
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.indicated).toEqual(['syncing', 'unsynchronised']);
    // Never returns to idle on its own: only reopening the note clears it.
    expect(h.indicated).not.toContain('idle');
  });

  it('cannot be released afterwards by an abort', () => {
    // The host does not abort past the commit point; this does not rely on
    // that. Releasing after a failed adoption would thaw onto text the file no
    // longer has, whoever asked for it.
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.barrier.abort(REQUEST)).toBe(false);
    expect(h.isEditable()).toBe(false);
    expect(h.barrier.queuedCount).toBe(0 + 0);
    expect(h.barrier.adoptionFailed).toBe(true);
  });

  it('cannot be released afterwards by a second apply that succeeds', () => {
    const h = harness({ text: 'ABCD' });
    h.barrier.begin(NOTE, REQUEST, 0);
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    h.mendAdoption();
    // The request is still the active one, but the page has already told the
    // host it could not follow. It stays where it is.
    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(false);
    expect(h.isEditable()).toBe(false);
    expect(h.barrier.currentGeneration()).toBe(0);
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

  it('stays shut when adoption failed, through every path a document changes', () => {
    // 4.0E.2 §14. Not a flag on a mock: the real editor, wired to the real
    // barrier, with the real ProseMirror gate underneath. A note that could not
    // take on the committed document must refuse typing, commands and direct
    // transactions alike — and must send nothing, because everything it could
    // send would quote a generation the host has already moved past.
    const element = document.createElement('div');
    document.body.append(element);
    const sent: WebviewToHostMessage[] = [];
    const editor = new NoteEditor({
      element,
      initialContent: 'ABCD',
      onUpdate: (markdown) =>
        sent.push({
          type: 'content_changed',
          payload: { id: NOTE, content: markdown, generation: 0 },
        }),
    });

    // Held in an object so TypeScript does not narrow it away: it is assigned
    // from inside a callback the compiler cannot see run.
    const slow: { notice: (() => void) | null } = { notice: null };
    const barrier = new ExternalWriteBarrier({
      freeze: () => editor.setEditable(false),
      thaw: () => editor.setEditable(true),
      snapshot: () => editor.getMarkdown(),
      adopt: () => {
        throw new Error('adoption failed');
      },
      send: (message) => sent.push(message),
      setTimer: (callback) => {
        slow.notice = callback;
        return 1;
      },
      clearTimer: () => {},
    });

    barrier.begin(NOTE, REQUEST, 0);
    const staleNotice = slow.notice;
    expect(barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(false);

    // 4.0E.2R §23: the slow notice that was already on its way runs, and the
    // page is still shut. Then every way the page can change a document.
    staleNotice?.();
    expect(barrier.syncState).toBe('unsynchronised');

    const view = editor.getView();
    view.dispatch(view.state.tr.insertText('E', view.state.doc.content.size - 1));
    editor.insertImage('assets/x.png');
    editor.increaseTextSize();

    expect(editor.isEditable()).toBe(false);
    expect(editor.getMarkdown()).toContain('ABCD');
    expect(editor.getMarkdown()).not.toContain('ABCDE');
    expect(editor.getMarkdown()).not.toContain('assets/x.png');
    expect(editor.hasPendingSave()).toBe(false);

    // Nothing reported a change, and nothing claimed the page was in step.
    expect(sent.some((m) => m.type === 'content_changed')).toBe(false);
    expect(sent.some((m) => m.type === 'external_write_applied')).toBe(false);
    expect(sent.filter((m) => m.type === 'external_write_apply_failed')).toHaveLength(1);
  });

  it('keeps the document locked when adopting one throws part-way', () => {
    // 4.0E.2R, found while auditing rather than from a failing test. Adopting
    // briefly lifts the lock — it is the one change the lock exists to let
    // through — and the restore used to sit after the call. An adoption that
    // threw therefore left the lock off, which is precisely the moment the page
    // is out of step and every command it can run must be refused.
    const { editor } = mount('ABCD');
    editor.setEditable(false);

    const view = editor.getView();
    const seam = view as unknown as { dispatch: (tr: unknown) => void };
    const original = seam.dispatch;
    seam.dispatch = () => {
      throw new Error('adoption blew up half way through');
    };
    expect(() => editor.setMarkdown('ABCD\nXYZ')).toThrow();
    seam.dispatch = original;

    // Still shut, to everything.
    editor.insertImage('assets/x.png');
    expect(editor.getMarkdown()).not.toContain('assets/x.png');
    expect(editor.isEditable()).toBe(false);
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

// The state machine, sealed — 4.0E.2R.
//
// `unsynchronised` is terminal for a page. These are not extra assertions about
// the happy path; they are the list of ways a page might be talked back into
// looking synchronised, each one tried on purpose.
describe('the barrier state machine', () => {
  const OTHER = '77777777-6666-4555-8444-333333333333';

  /** Everything worth asserting about a page, in one place. */
  function snapshotOf(h: ReturnType<typeof harness>) {
    return {
      state: h.barrier.syncState,
      active: h.barrier.active,
      editable: h.isEditable(),
      generation: h.barrier.currentGeneration(),
      queued: h.barrier.queuedCount,
      text: h.currentText(),
    };
  }

  function held(text = 'ABCD') {
    const h = harness({ text });
    h.barrier.begin(NOTE, REQUEST, 0);
    return h;
  }

  // ---- the reported bug ------------------------------------------------

  it('a slow notice that was already on its way cannot undo unsynchronised', () => {
    // THE gate of this subphase. The failure path deliberately keeps the
    // request active, so the old guard — "is this still the active request?" —
    // still passed and the slow notice overwrote the terminal state. The page
    // stayed safe and started telling the reader a write was merely slow, when
    // in fact there was no write in flight at all and reopening was the only
    // way forward.
    const h = held();
    const alreadyQueued = h.capturedSlowNotice();
    expect(alreadyQueued).toBeTruthy();

    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
    expect(h.barrier.syncState).toBe('unsynchronised');

    // Fired by hand, exactly as an event loop would run a callback that was
    // already scheduled when the timer was cancelled.
    alreadyQueued!();

    expect(h.barrier.syncState).toBe('unsynchronised');
    expect(h.indicated).toEqual(['syncing', 'unsynchronised']);
    expect(h.indicated).not.toContain('slow');
    expect(snapshotOf(h)).toEqual({
      state: 'unsynchronised',
      active: true,
      editable: false,
      generation: 0,
      queued: 0,
      text: 'ABCD',
    });
  });

  it('cancels the pending slow notice when adoption fails', () => {
    // Necessary and not sufficient: the guard above is what makes it safe.
    // This is what keeps the common case from queueing a callback at all.
    const h = held();
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
    expect(h.timerWasCleared()).toBe(true);
  });

  it('survives the stale notice being fired again and again', () => {
    const h = held();
    const alreadyQueued = h.capturedSlowNotice()!;
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    for (let attempt = 0; attempt < 5; attempt += 1) alreadyQueued();

    expect(h.barrier.syncState).toBe('unsynchronised');
    expect(h.indicated.filter((state) => state === 'unsynchronised')).toHaveLength(1);
    expect(h.isEditable()).toBe(false);
  });

  // ---- the other ordering ----------------------------------------------

  it('goes from slow to unsynchronised, and stays there', () => {
    // The failure must not depend on arriving before the first slow notice.
    const h = held();
    h.fireSlowNotice();
    expect(h.barrier.syncState).toBe('slow');

    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(h.barrier.syncState).toBe('unsynchronised');
    expect(h.indicated).toEqual(['syncing', 'slow', 'unsynchronised']);
    expect(h.isEditable()).toBe(false);
  });

  // ---- the symmetric bug, on the paths that do release -----------------

  it('a stale notice cannot make a finished write look slow', () => {
    const h = held();
    const alreadyQueued = h.capturedSlowNotice()!;
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
    expect(h.barrier.syncState).toBe('idle');

    alreadyQueued();

    expect(h.barrier.syncState).toBe('idle');
    expect(h.indicated).toEqual(['syncing', 'idle']);
    expect(h.isEditable()).toBe(true);
  });

  it('a stale notice cannot make an abandoned write look slow', () => {
    const h = held();
    const alreadyQueued = h.capturedSlowNotice()!;
    h.barrier.abort(REQUEST);
    expect(h.barrier.syncState).toBe('idle');

    alreadyQueued();

    expect(h.barrier.syncState).toBe('idle');
    expect(h.indicated).toEqual(['syncing', 'idle']);
    expect(h.isEditable()).toBe(true);
  });

  it('still says a genuinely slow write is slow', () => {
    // The guard must not buy safety by never firing. A write that really is
    // still waiting has to reach `slow`.
    const h = held();
    h.fireSlowNotice();

    expect(h.barrier.syncState).toBe('slow');
    expect(h.indicated).toEqual(['syncing', 'slow']);
    expect(h.barrier.active).toBe(true);
    expect(h.isEditable()).toBe(false);
    expect(h.barrier.currentGeneration()).toBe(0);

    // And a slow write still finishes normally.
    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(true);
    expect(snapshotOf(h)).toEqual({
      state: 'idle',
      active: false,
      editable: true,
      generation: 1,
      queued: 0,
      text: 'ABCD\nXYZ',
    });
  });

  // ---- nothing else reopens a terminal page ----------------------------

  it('refuses every message that could reopen an unsynchronised page', () => {
    const h = held();
    h.barrier.defer(() => h.type(' + captura'));
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
    const sealed = snapshotOf(h);
    const sentAfterFailure = h.sent.length;

    h.mendAdoption();
    // A repeat of the same request.
    expect(h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(false);
    // A different request entirely.
    expect(h.barrier.apply(NOTE, OTHER, 2, committed('outro'))).toBe(false);
    // An abort.
    expect(h.barrier.abort(REQUEST)).toBe(false);
    expect(h.barrier.abort(OTHER)).toBe(false);
    // A fresh write.
    h.barrier.begin(NOTE, OTHER, 0);
    // A load telling it about a new generation.
    h.barrier.setGeneration(9);
    // And the stale notice, once more for good measure.
    h.capturedSlowNotice()!();

    expect(snapshotOf(h)).toEqual(sealed);
    expect(h.barrier.syncState).toBe('unsynchronised');
    expect(h.sent).toHaveLength(sentAfterFailure);
    expect(h.sent.some((m) => m.type === 'external_write_applied')).toBe(false);
  });

  it('holds its queued actions for as long as it is out of step', () => {
    const h = held();
    const ran: string[] = [];
    h.barrier.defer(() => ran.push('capture'));
    h.barrier.defer(() => ran.push('image'));
    h.barrier.defer(() => ran.push('metadata'));
    h.breakAdoption();
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    h.capturedSlowNotice()!();
    h.barrier.abort(REQUEST);
    h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));

    expect(ran).toEqual([]);
    expect(h.barrier.queuedCount).toBe(3);
    // And a new one still queues rather than running against stale text.
    expect(h.barrier.defer(() => ran.push('later'))).toBe(true);
    expect(ran).toEqual([]);
    expect(h.barrier.queuedCount).toBe(4);
  });

  // ---- the matrix, run as a table --------------------------------------

  it('follows the state table exactly', () => {
    type Row = {
      from: SyncState;
      event: string;
      to: SyncState;
      editable: boolean;
      generation: number;
    };

    const rows: Row[] = [
      { from: 'idle', event: 'begin', to: 'syncing', editable: false, generation: 0 },
      { from: 'syncing', event: 'slow notice', to: 'slow', editable: false, generation: 0 },
      { from: 'syncing', event: 'abort', to: 'idle', editable: true, generation: 0 },
      { from: 'slow', event: 'abort', to: 'idle', editable: true, generation: 0 },
      { from: 'syncing', event: 'apply ok', to: 'idle', editable: true, generation: 1 },
      { from: 'slow', event: 'apply ok', to: 'idle', editable: true, generation: 1 },
      {
        from: 'syncing',
        event: 'apply failed',
        to: 'unsynchronised',
        editable: false,
        generation: 0,
      },
      {
        from: 'slow',
        event: 'apply failed',
        to: 'unsynchronised',
        editable: false,
        generation: 0,
      },
      {
        from: 'unsynchronised',
        event: 'stale slow notice',
        to: 'unsynchronised',
        editable: false,
        generation: 0,
      },
      {
        from: 'unsynchronised',
        event: 'apply again',
        to: 'unsynchronised',
        editable: false,
        generation: 0,
      },
      {
        from: 'unsynchronised',
        event: 'abort',
        to: 'unsynchronised',
        editable: false,
        generation: 0,
      },
      {
        from: 'unsynchronised',
        event: 'wrong apply',
        to: 'unsynchronised',
        editable: false,
        generation: 0,
      },
      {
        from: 'idle',
        event: 'stale slow notice after success',
        to: 'idle',
        editable: true,
        generation: 1,
      },
      {
        from: 'idle',
        event: 'stale slow notice after abort',
        to: 'idle',
        editable: true,
        generation: 0,
      },
    ];

    for (const row of rows) {
      const h = harness({ text: 'ABCD' });
      let stale: (() => void) | null = null;

      // Drive the page to the starting state.
      if (row.from !== 'idle' || row.event.includes('after')) {
        h.barrier.begin(NOTE, REQUEST, 0);
        stale = h.capturedSlowNotice();
      }
      if (row.from === 'slow') h.fireSlowNotice();
      if (row.from === 'unsynchronised') {
        h.breakAdoption();
        h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
        h.mendAdoption();
      }
      if (row.event === 'stale slow notice after success') {
        h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
      }
      if (row.event === 'stale slow notice after abort') {
        h.barrier.abort(REQUEST);
      }

      // Apply the event.
      switch (row.event) {
        case 'begin':
          h.barrier.begin(NOTE, REQUEST, 0);
          break;
        case 'slow notice':
          h.fireSlowNotice();
          break;
        case 'abort':
          h.barrier.abort(REQUEST);
          break;
        case 'apply ok':
          h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
          break;
        case 'apply failed':
          h.breakAdoption();
          h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
          break;
        case 'apply again':
          h.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
          break;
        case 'wrong apply':
          h.barrier.apply(NOTE, OTHER, 2, committed('outro'));
          break;
        case 'stale slow notice':
        case 'stale slow notice after success':
        case 'stale slow notice after abort':
          stale!();
          break;
        default:
          throw new Error(`unhandled event ${row.event}`);
      }

      const where = `${row.from} --${row.event}-->`;
      expect(h.barrier.syncState, where).toBe(row.to);
      expect(h.isEditable(), where).toBe(row.editable);
      expect(h.barrier.currentGeneration(), where).toBe(row.generation);
    }
  });
});

// The same guarantee, through the timers the page actually uses.
describe('the real timer wiring', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** Wired exactly as `main.ts` wires it: the window's own timers. */
  function pageBarrier(onAdopt: () => void) {
    const indicated: SyncState[] = [];
    const sent: WebviewToHostMessage[] = [];
    let editable = true;
    const barrier = new ExternalWriteBarrier({
      freeze: () => {
        editable = false;
      },
      thaw: () => {
        editable = true;
      },
      snapshot: () => 'ABCD',
      adopt: onAdopt,
      send: (message) => sent.push(message),
      indicate: (state) => indicated.push(state),
      setTimer: (callback, ms) => window.setTimeout(callback, ms),
      clearTimer: (handle) => window.clearTimeout(handle),
    });
    return { barrier, indicated, sent, isEditable: () => editable };
  }

  it('never announces a slow write after a failed adoption, however long it waits', () => {
    // 4.0E.2R §34. Deterministic: no sleeping, no dependence on how fast this
    // machine happens to be. The clock is advanced far past the slow notice.
    vi.useFakeTimers();
    const page = pageBarrier(() => {
      throw new Error('adoption failed');
    });

    page.barrier.begin(NOTE, REQUEST, 0);
    vi.advanceTimersByTime(100);
    expect(page.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'))).toBe(false);
    expect(page.barrier.syncState).toBe('unsynchronised');

    vi.advanceTimersByTime(10_000);

    expect(page.barrier.syncState).toBe('unsynchronised');
    expect(page.indicated).toEqual(['syncing', 'unsynchronised']);
    expect(page.isEditable()).toBe(false);
    expect(page.sent.some((m) => m.type === 'external_write_applied')).toBe(false);
  });

  it('still announces a slow write that really is slow', () => {
    vi.useFakeTimers();
    const page = pageBarrier(() => {});

    page.barrier.begin(NOTE, REQUEST, 0);
    vi.advanceTimersByTime(EXTERNAL_WRITE_SLOW_NOTICE_MS + 1);

    expect(page.barrier.syncState).toBe('slow');
    expect(page.indicated).toEqual(['syncing', 'slow']);
    expect(page.isEditable()).toBe(false);
  });

  it('lets a finished write pass the slow threshold in peace', () => {
    vi.useFakeTimers();
    const page = pageBarrier(() => {});

    page.barrier.begin(NOTE, REQUEST, 0);
    page.barrier.apply(NOTE, REQUEST, 1, committed('ABCD\nXYZ'));
    vi.advanceTimersByTime(10_000);

    expect(page.barrier.syncState).toBe('idle');
    expect(page.indicated).toEqual(['syncing', 'idle']);
    expect(page.isEditable()).toBe(true);
  });

  it('lets an abandoned write pass the slow threshold in peace', () => {
    vi.useFakeTimers();
    const page = pageBarrier(() => {});

    page.barrier.begin(NOTE, REQUEST, 0);
    page.barrier.abort(REQUEST);
    vi.advanceTimersByTime(10_000);

    expect(page.barrier.syncState).toBe('idle');
    expect(page.indicated).toEqual(['syncing', 'idle']);
  });
});
