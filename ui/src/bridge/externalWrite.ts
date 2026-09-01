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
 * ## It always ends
 *
 * The host has its own, shorter deadline and abandons a request that goes
 * unanswered, so a late answer can never commit anything. This has a longer
 * one purely so a host that vanishes cannot leave the editor read-only for
 * ever. Whichever fires, editing comes back and the queue is drained.
 */

/** Long enough that it never races the host's own deadline, short enough that
 *  a host that disappeared does not leave the note frozen. */
export const EXTERNAL_WRITE_CLIENT_TIMEOUT_MS = 15000;

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
  /** Shows or hides a discreet "syncing" state. Never a modal. */
  indicate?(active: boolean): void;
  /** Schedules the safety timeout. Injected so tests do not wait for it. */
  setTimer?(callback: () => void, ms: number): number;
  clearTimer?(handle: number): void;
}

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
    this.hooks.indicate?.(true);

    const content = this.hooks.snapshot();
    this.hooks.send({
      type: 'external_write_ready',
      payload: { id: noteId, requestId, generation, content },
    });

    const setTimer = this.hooks.setTimer;
    if (setTimer) {
      this.timer = setTimer(() => {
        this.timer = null;
        // The host went away. Nothing was committed as far as this page can
        // tell, so the document is simply released exactly as it was.
        this.release(requestId);
      }, EXTERNAL_WRITE_CLIENT_TIMEOUT_MS);
    }
  }

  /**
   * The change was committed; this is the note as it now stands.
   *
   * Returns false for a request this page is not waiting on, so an answer that
   * arrives after the safety timeout cannot replace the document behind the
   * reader's back.
   */
  public apply(requestId: string, generation: number, document: ExternalDocument): boolean {
    if (this.requestId !== requestId) return false;
    this.generation = generation;
    this.hooks.adopt(document);
    this.release(requestId);
    return true;
  }

  /** Nothing was written. The document is released exactly as it was. */
  public abort(requestId: string): boolean {
    if (this.requestId !== requestId) return false;
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
    this.hooks.indicate?.(false);
    this.hooks.thaw();

    // Drained after thawing, so each one is an ordinary edit of the document
    // as it now is and takes the ordinary autosave path.
    const pending = this.queued;
    this.queued = [];
    for (const action of pending) action();
  }
}
