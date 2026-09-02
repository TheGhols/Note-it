import type { MetadataView, WebviewToHostMessage } from './types.ts';

/**
 * The page's half of an external write.
 *
 * Something outside this window — a `noteit` command, most likely — is about
 * to change the note this editor is showing. The host cannot simply read the
 * file, change it and write it back: the editor may be holding a paragraph
 * that is not in the file yet, and that paragraph would disappear.
 *
 * So the host asks first, and this is what answers. The order inside {@link
 * begin} is the whole guarantee and it is not negotiable:
 *
 * 1. stop everything that can change the document;
 * 2. *then* read what the document says;
 * 3. hand that text to the host and wait.
 *
 * Doing it the other way round — read, then stop — leaves a window between the
 * two in which a keystroke can land. That keystroke is in the editor and not
 * in the text the host was given, so the commit puts the document back without
 * it. One character, silently lost, on a race nobody can reproduce on demand.
 *
 * ## Nothing that arrives meanwhile is thrown away
 *
 * A clipboard capture, a pasted image finishing its trip through the host, a
 * metadata panel being saved: any of these can land while the document is
 * held. None of them may be dropped, and none of them may be applied to a
 * document that is about to be replaced. They are queued here and run the
 * moment editing resumes, against the document as it then is.
 *
 * ## Only the host decides when editing resumes
 *
 * Once the snapshot has gone out, this page has no idea what is happening to
 * it. The host may be part-way through writing a temp file, syncing it,
 * renaming it, or doing the work that follows a commit. **There is no length of
 * time after which it becomes safe to guess.** So the document is released by
 * exactly two things, both of them decisions the host made and told this page
 * about: `AbortExternalWrite`, meaning nothing was written, or
 * `ApplyExternalDocument`, meaning something was and here it is.
 *
 * A timer of its own would be the one way to reintroduce the failure the
 * barrier exists to remove: the reader typing against a document the host is
 * in the middle of replacing. A slow commit is a slow commit — it is allowed to
 * be slow — and the honest response is to say so, not to start editing again.
 *
 * And "the host said something" is not the same as "it is safe to edit". There
 * are three answers, and only two of them release the document:
 *
 * - `AbortExternalWrite` — nothing was written. The document the page is
 *   holding is still the document on disk, so editing resumes on it.
 * - `ApplyExternalDocument`, adopted — the page now holds the committed
 *   document and the generation that goes with it. Editing resumes on that.
 * - `ApplyExternalDocument`, *not* adopted — the file changed and this page
 *   failed to follow. It is showing text the store no longer has, on a
 *   generation the host has already moved past. Releasing here would hand back
 *   an editor that looks completely normal and whose every save the host
 *   refuses: the reader would type for as long as they liked and lose all of
 *   it, silently. So the document stays held, the queue stays held, and the
 *   page says it is out of step. A blocked note that says why is recoverable
 *   by reopening it; an editor that quietly discards work is not.
 *
 * The host is not a separate program that can vanish and leave this running.
 * The WebView belongs to the very process that owns the barrier: if the host
 * dies, this page dies with it, so there is no orphan to rescue. What a long
 * wait earns is a word to the reader, and this raises the indicator to say the
 * synchronisation is taking a while. It never thaws.
 */

/** When the discreet indicator starts saying a write is taking longer than
 *  usual. It changes what the reader is told and nothing else — in particular
 *  it never releases the document. */
export const EXTERNAL_WRITE_SLOW_NOTICE_MS = 4000;

export interface ExternalWriteHooks {
  /** Closes every path that can change the document. Called before the snapshot. */
  freeze(): void;
  /** Opens them again. */
  thaw(): void;
  /** The Markdown the editor holds right now. */
  snapshot(): string;
  /** Adopts the committed document without emitting an edit of its own. */
  adopt(payload: ExternalDocument): void;
  /** Sends one message back to the host. */
  send(message: WebviewToHostMessage): void;
  /**
   * Reports what the page is doing about an external write. Never a modal.
   *
   * Says what is true and nothing more. `slow` changes the words; being told a
   * save is slow is useful, being handed back a document the host is still
   * replacing is not. `unsynchronised` is the one state that does not end on
   * its own, and it is the one the reader most needs spelled out.
   */
  indicate?(state: SyncState): void;
  /** Schedules the slow-write notice. Injected so tests do not wait for it. */
  setTimer?(callback: () => void, ms: number): number;
  clearTimer?(handle: number): void;
}

/**
 * What the page is doing about an external write.
 *
 * `unsynchronised` means the file changed and this page could not follow it.
 * The document stays held in that state — it is not a transient one, and only
 * reopening the note clears it.
 */
export type SyncState = 'idle' | 'syncing' | 'slow' | 'unsynchronised';

/** The committed note, exactly as the host holds it. Never YAML, never a path. */
export interface ExternalDocument {
  content: string;
  metadata: MetadataView;
  createdAt: string | null;
  updatedAt: string | null;
}

export class ExternalWriteBarrier {
  private hooks: ExternalWriteHooks;
  private requestId: string | null = null;
  private queued: Array<() => void> = [];
  private timer: number | null = null;
  /** Which run of the document the page is on. Quoted on everything it sends. */
  private generation = 0;
  /**
   * The request whose committed document this page failed to adopt.
   *
   * Set only past the commit point, and never cleared: there is nothing this
   * page can do to get back in step, because the text it is holding is not the
   * text on disk and it has no way to ask for the real one. The note stays held
   * until it is reopened.
   */
  private adoptionFailedFor: string | null = null;

  constructor(hooks: ExternalWriteHooks) {
    this.hooks = hooks;
  }

  /** Whether the document is currently held still. */
  public get active(): boolean {
    return this.requestId !== null;
  }

  public currentGeneration(): number {
    return this.generation;
  }

  /** Adopts the generation a freshly loaded note arrived with. */
  public setGeneration(generation: number): void {
    this.generation = Number.isFinite(generation) ? generation : 0;
  }

  /**
   * Freezes the document, then answers the host with what it held.
   *
   * A second request while one is in flight is refused rather than queued: two
   * of them would take the same snapshot and the second commit would undo the
   * first. The host serialises them anyway; this refuses in case it ever
   * stops.
   */
  public begin(noteId: string, requestId: string, generation: number): void {
    if (this.requestId !== null) return;
    if (generation !== this.generation) return;

    this.requestId = requestId;
    // Freeze first. Everything below reads a document nothing can change any
    // more, which is the only reason the text it reads is worth committing.
    this.hooks.freeze();
    this.hooks.indicate?.('syncing');

    const content = this.hooks.snapshot();
    this.hooks.send({
      type: 'external_write_ready',
      payload: { id: noteId, requestId, generation, content },
    });

    // The only timer here, and all it does is change what the reader is told.
    // It cannot release the document: see the note at the top of this file.
    const setTimer = this.hooks.setTimer;
    if (setTimer) {
      this.timer = setTimer(() => {
        this.timer = null;
        if (this.requestId === requestId) this.hooks.indicate?.('slow');
      }, EXTERNAL_WRITE_SLOW_NOTICE_MS);
    }
  }

  /**
   * The change was committed; this is the note as it now stands.
   *
   * Answers the host either way, and that answer is the *only* thing the host
   * treats as proof the page is in step. Evaluating the script that delivered
   * this message proves the script ran; it does not prove this method was
   * reached, that the request matched, or that the document was adopted — and a
   * host that mistook the one for the other would report a stale window as a
   * synchronised one.
   *
   * Returns false for a request this page is not waiting on, and for an
   * adoption that threw. In both cases a negative answer goes back rather than
   * silence, so the host learns at once instead of waiting out its timeout.
   */
  public apply(
    noteId: string,
    requestId: string,
    generation: number,
    document: ExternalDocument,
  ): boolean {
    if (this.adoptionFailed) {
      // Already answered, and already out of step. Nothing that arrives now can
      // put this page back on the file — it would have to be handed a document
      // it has no way to verify — so a repeat is refused rather than used as a
      // way back in. Silently: the host has had its answer for this request.
      return false;
    }
    if (this.requestId !== requestId) {
      this.hooks.send({
        type: 'external_write_apply_failed',
        payload: { id: noteId, requestId },
      });
      return false;
    }

    try {
      this.hooks.adopt(document);
    } catch {
      // The write is on disk and this page did not follow it. Everything below
      // is deliberately what does *not* happen: the generation does not move,
      // the document is not released, the queue is not drained, and no positive
      // acknowledgement goes out. See the note at the top of this file for why
      // releasing here would be worse than staying blocked.
      this.adoptionFailedFor = requestId;
      this.hooks.indicate?.('unsynchronised');
      this.hooks.send({
        type: 'external_write_apply_failed',
        payload: { id: noteId, requestId },
      });
      return false;
    }

    // Moved only once the document really is the committed one.
    this.generation = generation;
    this.release(requestId);
    this.hooks.send({
      type: 'external_write_applied',
      payload: { id: noteId, requestId, generation },
    });
    return true;
  }

  /**
   * Whether this page failed to take on a document that was already committed.
   *
   * The note is held and stays held. Nothing here clears it, because nothing
   * here can: reopening the note is what puts the page back on the file.
   */
  public get adoptionFailed(): boolean {
    return this.adoptionFailedFor !== null;
  }

  /**
   * Nothing was written. The document is released exactly as it was.
   *
   * Refused once an adoption has failed. The host does not abort past the
   * commit point, but this does not depend on that: releasing after a failed
   * adoption would thaw onto text the file no longer has, whoever asked.
   */
  public abort(requestId: string): boolean {
    if (this.requestId !== requestId || this.adoptionFailed) return false;
    this.release(requestId);
    return true;
  }

  /**
   * Runs an edit now, or holds it until editing resumes.
   *
   * Returns true when the action was deferred. Nothing is ever discarded: a
   * capture the reader made in another application, or an image the host has
   * just finished importing, is theirs whether or not a write happened to be
   * in flight when it arrived.
   */
  public defer(action: () => void): boolean {
    if (!this.active) {
      action();
      return false;
    }
    this.queued.push(action);
    return true;
  }

  /** How many edits are waiting for the document to be released. */
  public get queuedCount(): number {
    return this.queued.length;
  }

  private release(requestId: string): void {
    if (this.requestId !== requestId) return;
    this.requestId = null;
    if (this.timer !== null) {
      this.hooks.clearTimer?.(this.timer);
      this.timer = null;
    }
    this.hooks.indicate?.('idle');
    this.hooks.thaw();

    // Drained after thawing, so each one is an ordinary edit of the document
    // as it now is and takes the ordinary autosave path.
    const pending = this.queued;
    this.queued = [];
    for (const action of pending) action();
  }
}
