import type { SyncState } from '../bridge/externalWrite.ts';

export interface NoteStatusOptions {
  mount: HTMLElement;
  document?: Document;
  /** How long a message stays up. */
  timeoutMs?: number;
}

/** Long enough to read a short sentence, short enough not to sit in the way. */
export const STATUS_TIMEOUT_MS = 4500;

/**
 * A line at the foot of the note saying what just happened.
 *
 * "Backup concluído", "Nota restaurada", or why one of them did not happen. It
 * is not a dialog: nothing has to be dismissed, nothing takes the keyboard, and
 * it disappears on its own. A note is a small surface, and a modal over one to
 * report a success would be worse than saying nothing.
 *
 * The message is always the sentence the host sent, written with `textContent`.
 * The page does not compose its own from a code, because then two places would
 * decide what a failure means.
 */
export class NoteStatus {
  private readonly doc: Document;
  private readonly root: HTMLElement;
  private readonly timeoutMs: number;
  private timer: number | null = null;

  public constructor(options: NoteStatusOptions) {
    this.doc = options.document ?? options.mount.ownerDocument;
    this.timeoutMs = options.timeoutMs ?? STATUS_TIMEOUT_MS;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-status';
    this.root.hidden = true;
    // Announced when it appears, without stealing focus from the editor.
    this.root.setAttribute('role', 'status');
    this.root.setAttribute('aria-live', 'polite');
    options.mount.append(this.root);
  }

  public element(): HTMLElement {
    return this.root;
  }

  public isVisible(): boolean {
    return !this.root.hidden;
  }

  public show(message: string, ok = true): void {
    this.cancel();
    this.root.textContent = message;
    this.root.dataset.ok = String(ok);
    this.root.hidden = false;
    this.timer = this.doc.defaultView?.setTimeout(() => {
      this.timer = null;
      this.hide();
    }, this.timeoutMs) as unknown as number;
  }

  public hide(): void {
    this.cancel();
    this.root.hidden = true;
    this.root.textContent = '';
  }

  public destroy(): void {
    this.cancel();
    this.root.remove();
  }

  private cancel(): void {
    if (this.timer !== null) {
      this.doc.defaultView?.clearTimeout(this.timer);
      this.timer = null;
    }
  }
}

/**
 * The note is being changed by something outside this window.
 *
 * Deliberately almost nothing. An external write is normally over in a few
 * milliseconds, and flashing a message for every one of them would be worse
 * than saying nothing at all — so this element exists the whole time and is
 * only *shown* by CSS after a delay long enough that the reader would have
 * noticed the pause anyway.
 *
 * It is not a modal, it takes no focus, and it dismisses nothing. The editor
 * underneath is held still for the length of the write and the window,
 * rendering and compositor carry on exactly as before.
 */
export class SyncIndicator {
  private readonly root: HTMLElement;
  private readonly labels: Record<Exclude<SyncState, 'idle'>, string>;

  public constructor(
    mount: HTMLElement,
    labels: Partial<Record<Exclude<SyncState, 'idle'>, string>> = {},
  ) {
    const doc = mount.ownerDocument;
    this.labels = {
      syncing: labels.syncing ?? 'Sincronizando…',
      slow: labels.slow ?? 'Sincronização demorando…',
      unsynchronised:
        labels.unsynchronised ??
        'A nota não pôde ser atualizada. A alteração foi gravada; reabra a nota para continuar.',
    };
    this.root = doc.createElement('div');
    this.root.className = 'note-syncing';
    this.root.textContent = this.labels.syncing;
    this.root.hidden = true;
    // Announced politely when it does appear, without taking the keyboard.
    this.root.setAttribute('role', 'status');
    this.root.setAttribute('aria-live', 'polite');
    mount.append(this.root);
  }

  public element(): HTMLElement {
    return this.root;
  }

  public isVisible(): boolean {
    return !this.root.hidden;
  }

  /**
   * Says what the page is doing about an external write.
   *
   * `unsynchronised` is the one state that does not clear on its own: the file
   * changed, this page could not follow it, and the note is held until it is
   * reopened. Saying so is the whole point — the alternative is a note that
   * simply stops responding with no explanation.
   */
  public setState(state: SyncState): void {
    if (state === 'idle') {
      this.root.hidden = true;
      this.root.dataset.state = 'idle';
      return;
    }
    this.root.textContent = this.labels[state];
    this.root.dataset.state = state;
    this.root.hidden = false;
  }

  public destroy(): void {
    this.root.remove();
  }
}
