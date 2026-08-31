import { DOMSerializer, Schema } from '@tiptap/pm/model';
import { FlashcardSide, ReviewItem } from '../flashcards/extract.ts';
import { StudySession } from '../flashcards/session.ts';
import { intervalLabel, nextLevel } from '../study/stats.ts';
import type { GlobalReviewItem, StudyRating } from '../study/types.ts';

type StudyPanelItem = ReviewItem | GlobalReviewItem;

export interface StudyRatingRequest {
  requestId: number;
  reviewKey: string;
  rating: StudyRating;
}

export interface FlashcardPanelHandlers {
  /** The panel closed. Whatever opened it should have the keyboard back. */
  onClose(): void;
  /** Sends one commit request. The panel advances only through resolveRating. */
  onRate?(request: StudyRatingRequest): void;
  onReturnToHub?(): void;
}

export interface FlashcardPanelOptions {
  mount: HTMLElement;
  handlers: FlashcardPanelHandlers;
  /** Injected so a test can state an order instead of asserting about chance. */
  random?: () => number;
  document?: Document;
}

export interface StudyRequest {
  /** The questions, as they were when studying started. */
  readonly items: readonly StudyPanelItem[];
  /** How many cards those questions came from, for the count line. */
  readonly cards: number;
  /** The note's own schema, which is what the sides are rendered with. */
  readonly schema: Schema;
  /** What to give the keyboard back to on close. */
  readonly invoker?: HTMLElement | null;
}

/**
 * Studying, as a panel in the note the cards were written in.
 *
 * The shape the trash and the search palette already have, for the reason
 * stated in ADR-028: a second layer-shell window would have to be placed,
 * stacked, focused and torn down, and everything it would do is done by an
 * element that disappears when it is closed.
 *
 * It is **read-only, and structurally so**. The panel never holds the editor,
 * never dispatches a transaction and has no way to reach the document: what it
 * is given is a list of sides, and what it does is draw them. That is what
 * makes "studying changes nothing" a fact about the wiring rather than a
 * promise about the handlers — opening, revealing, moving, shuffling and
 * closing cannot write, because there is nothing here to write with.
 *
 * The sides are drawn with the note's own `DOMSerializer`. Nothing is parsed,
 * no Markdown is rendered a second time, and no string is ever assigned to
 * `innerHTML`: the nodes come out of the document that was already sanitized
 * on the way in, and the schema turns them back into the elements it defined.
 * A picture goes through the same `note-it-asset:` reference the editor uses,
 * so studying fetches nothing and copies nothing — and because a node view is
 * an editor's business and not a serializer's, an image in a card arrives with
 * no frame, no handles and no alignment controls. It is a picture, being read.
 */
export class FlashcardPanel {
  private readonly doc: Document;
  private readonly root: HTMLElement;
  private readonly progress: HTMLElement;
  private readonly summary: HTMLElement;
  private readonly card: HTMLElement;
  private readonly question: HTMLElement;
  private readonly answer: HTMLElement;
  private readonly origin: HTMLElement;
  private readonly revealButton: HTMLButtonElement;
  private readonly previousButton: HTMLButtonElement;
  private readonly nextButton: HTMLButtonElement;
  private readonly shuffleButton: HTMLButtonElement;
  private readonly ratingArea: HTMLElement;
  private readonly ratingStatus: HTMLElement;
  private readonly ratingButtons: Record<StudyRating, HTMLButtonElement>;
  private readonly completion: HTMLElement;
  private readonly handlers: FlashcardPanelHandlers;
  private readonly random: () => number;

  private session: StudySession<StudyPanelItem> | null = null;
  private serializer: DOMSerializer | null = null;
  private cards = 0;
  private invoker: HTMLElement | null = null;
  private open = false;
  private nextRatingRequest = 1;
  private pendingRating: StudyRatingRequest | null = null;
  private readonly rated = new Map<string, StudyRating>();
  private ratingCounts: Record<StudyRating, number> = { difficult: 0, medium: 0, easy: 0 };

  public constructor(options: FlashcardPanelOptions) {
    this.doc = options.document ?? options.mount.ownerDocument;
    this.handlers = options.handlers;
    this.random = options.random ?? Math.random;

    this.root = this.doc.createElement('div');
    this.root.className = 'note-study';
    this.root.hidden = true;
    this.root.setAttribute('role', 'dialog');
    this.root.setAttribute('aria-modal', 'false');
    this.root.setAttribute('aria-label', 'Flashcards');
    // Focusable so the dialog itself can take the keyboard on open, which is
    // what makes Escape and the arrows work before anything is tabbed to.
    this.root.tabIndex = -1;

    const header = this.doc.createElement('div');
    header.className = 'note-study-header';

    const title = this.doc.createElement('span');
    title.className = 'note-study-title';
    title.textContent = 'Flashcards';

    this.progress = this.doc.createElement('span');
    this.progress.className = 'note-study-progress';
    // Announced as it changes, so moving between questions is not silent for
    // somebody who cannot see the counter.
    this.progress.setAttribute('role', 'status');

    const close = this.doc.createElement('button');
    close.type = 'button';
    close.className = 'note-study-close';
    close.textContent = 'Fechar';
    close.setAttribute('aria-label', 'Fechar');
    close.addEventListener('click', () => this.close());

    header.append(title, this.progress, close);

    this.summary = this.doc.createElement('p');
    this.summary.className = 'note-study-summary';

    this.card = this.doc.createElement('div');
    this.card.className = 'note-study-card';

    this.question = this.doc.createElement('div');
    this.question.className = 'note-study-side note-study-question';

    this.origin = this.doc.createElement('p');
    this.origin.className = 'note-study-origin';

    this.revealButton = this.doc.createElement('button');
    this.revealButton.type = 'button';
    this.revealButton.className = 'note-study-reveal';
    this.revealButton.textContent = 'Mostrar resposta';
    this.revealButton.addEventListener('click', () => this.reveal());

    this.answer = this.doc.createElement('div');
    this.answer.className = 'note-study-side note-study-answer';
    this.answer.hidden = true;

    this.ratingArea = this.doc.createElement('div');
    this.ratingArea.className = 'note-study-ratings';
    this.ratingArea.hidden = true;
    this.ratingButtons = {
      difficult: this.ratingButton('Difícil', 'difficult'),
      medium: this.ratingButton('Médio', 'medium'),
      easy: this.ratingButton('Fácil', 'easy'),
    };
    this.ratingStatus = this.doc.createElement('p');
    this.ratingStatus.className = 'note-study-rating-status';
    this.ratingStatus.setAttribute('role', 'status');
    this.ratingArea.append(
      this.ratingButtons.difficult,
      this.ratingButtons.medium,
      this.ratingButtons.easy,
      this.ratingStatus,
    );

    this.completion = this.doc.createElement('div');
    this.completion.className = 'note-study-completion';
    this.completion.hidden = true;

    this.card.append(
      this.origin,
      this.question,
      this.revealButton,
      this.answer,
      this.ratingArea,
      this.completion,
    );

    const footer = this.doc.createElement('div');
    footer.className = 'note-study-footer';

    this.previousButton = this.footerButton('Anterior', 'previous', () => this.move(-1));
    this.nextButton = this.footerButton('Próximo', 'next', () => this.move(1));
    this.shuffleButton = this.footerButton('Embaralhar', 'shuffle', () => this.shuffle());

    footer.append(this.previousButton, this.nextButton, this.shuffleButton);
    this.root.append(header, this.summary, this.card, footer);
    options.mount.append(this.root);

    this.root.addEventListener('keydown', this.handleKeyDown);
  }

  public isOpen(): boolean {
    return this.open;
  }

  public element(): HTMLElement {
    return this.root;
  }

  /** What is on screen, for a test that wants to ask rather than to look. */
  public currentItem(): ReviewItem | null {
    return this.session?.current ?? null;
  }

  public isAnswerVisible(): boolean {
    return !this.answer.hidden;
  }

  /**
   * Starts a sitting with the questions as they are now.
   *
   * The list is taken once. What happens to the note from here — a keystroke,
   * a capture, a picture arriving — belongs to the next sitting.
   */
  public openPanel(request: StudyRequest): void {
    // The normal way in is disabled at zero, but keep the panel's own contract
    // honest too: no caller can produce an empty, focus-taking experience.
    if (request.items.length === 0) {
      this.close();
      return;
    }
    this.session = new StudySession(request.items, this.random);
    this.serializer = DOMSerializer.fromSchema(request.schema);
    this.cards = request.cards;
    this.invoker = request.invoker ?? null;
    this.pendingRating = null;
    this.rated.clear();
    this.ratingCounts = { difficult: 0, medium: 0, easy: 0 };
    this.open = true;
    this.root.hidden = false;
    this.render();
    this.root.focus();
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.session = null;
    this.serializer = null;
    this.question.replaceChildren();
    this.answer.replaceChildren();
    this.completion.replaceChildren();
    this.pendingRating = null;

    // Back to whatever opened this, when it is still somewhere focusable. The
    // menu that led here has closed, so falling back on the editor is the
    // caller's business rather than a guess made here.
    const invoker = this.invoker;
    this.invoker = null;
    if (invoker && invoker.isConnected && invoker.offsetParent !== null) {
      invoker.focus();
    } else {
      this.handlers.onClose();
    }
  }

  public destroy(): void {
    this.root.removeEventListener('keydown', this.handleKeyDown);
    this.root.remove();
  }

  private footerButton(label: string, name: string, action: () => void): HTMLButtonElement {
    const button = this.doc.createElement('button');
    button.type = 'button';
    button.className = `note-study-button note-study-${name}`;
    // Named rather than drawn: four unlabelled glyphs in a row is a puzzle,
    // and this panel has room for words.
    button.textContent = label;
    button.addEventListener('click', action);
    return button;
  }

  private ratingButton(label: string, rating: StudyRating): HTMLButtonElement {
    const button = this.doc.createElement('button');
    button.type = 'button';
    button.className = `note-study-rating note-study-rating-${rating}`;
    button.dataset.rating = rating;
    const name = this.doc.createElement('span');
    name.className = 'note-study-rating-name';
    name.textContent = label;
    const interval = this.doc.createElement('span');
    interval.className = 'note-study-rating-interval';
    button.append(name, interval);
    button.addEventListener('click', () => this.rate(rating));
    return button;
  }

  private reveal(): void {
    if (!this.session) return;
    this.session.reveal();
    this.render();
  }

  private move(step: number): void {
    if (!this.session) return;
    const moved = step > 0 ? this.session.next() : this.session.previous();
    if (moved) this.render();
  }

  private shuffle(): void {
    if (!this.session) return;
    this.session.shuffle();
    this.render();
  }

  private rate(rating: StudyRating): void {
    const item = this.session?.current;
    if (!item || !('reviewKey' in item) || this.pendingRating || this.rated.has(item.reviewKey)) {
      return;
    }
    if (!this.handlers.onRate) return;
    const request = {
      requestId: this.nextRatingRequest++,
      reviewKey: item.reviewKey,
      rating,
    };
    this.pendingRating = request;
    this.ratingStatus.textContent = 'Salvando avaliação…';
    this.setRatingButtonsDisabled(true);
    this.handlers.onRate(request);
  }

  /** Applies only the ACK for the request and item still pending here. */
  public resolveRating(
    requestId: number,
    reviewKey: string,
    ok: boolean,
    message: string,
  ): boolean {
    const pending = this.pendingRating;
    if (!pending || pending.requestId !== requestId || pending.reviewKey !== reviewKey) return false;
    this.pendingRating = null;
    if (!ok) {
      this.ratingStatus.textContent = message;
      this.setRatingButtonsDisabled(false);
      return true;
    }

    this.rated.set(reviewKey, pending.rating);
    this.ratingCounts[pending.rating] += 1;
    if (this.session?.hasNext) {
      this.session.next();
      this.render();
    } else {
      this.renderCompletion();
    }
    return true;
  }

  private setRatingButtonsDisabled(disabled: boolean): void {
    Object.values(this.ratingButtons).forEach((button) => {
      button.disabled = disabled;
    });
  }

  private renderCompletion(): void {
    const total =
      this.ratingCounts.difficult + this.ratingCounts.medium + this.ratingCounts.easy;
    this.question.replaceChildren();
    this.answer.replaceChildren();
    this.origin.textContent = '';
    this.revealButton.hidden = true;
    this.answer.hidden = true;
    this.ratingArea.hidden = true;
    this.completion.hidden = false;

    const title = this.doc.createElement('h2');
    title.textContent = 'Revisão concluída';
    const summary = this.doc.createElement('p');
    summary.textContent = `Difícil: ${this.ratingCounts.difficult} · Médio: ${this.ratingCounts.medium} · Fácil: ${this.ratingCounts.easy} · Total: ${total}`;
    const back = this.doc.createElement('button');
    back.type = 'button';
    back.className = 'note-study-button note-study-return';
    back.textContent = 'Voltar à Central';
    back.addEventListener('click', () => {
      this.close();
      this.handlers.onReturnToHub?.();
    });
    this.completion.append(title, summary, back);
    this.previousButton.disabled = true;
    this.nextButton.disabled = true;
    this.shuffleButton.disabled = true;
    back.focus();
  }

  /**
   * Keys this panel owns.
   *
   * Bound to the panel and not to the document, so none of them means anything
   * while studying is closed and none of them reaches the note behind it: a
   * right arrow moves to the next question, it does not move the caret.
   *
   * Space and Enter are deliberately not handled when a button has the focus.
   * The browser already turns both into a click there, and handling them again
   * here is how one press reveals an answer and skips past it in the same
   * moment.
   */
  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (event.isComposing || !this.session) return;

    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.close();
      return;
    }
    if (event.key === 'ArrowRight') {
      event.preventDefault();
      event.stopPropagation();
      this.move(1);
      return;
    }
    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      event.stopPropagation();
      this.move(-1);
      return;
    }
    if (event.key === ' ' || event.key === 'Enter') {
      if ((event.target as HTMLElement | null)?.tagName === 'BUTTON') return;
      if (this.session.isRevealed) return;
      event.preventDefault();
      event.stopPropagation();
      this.reveal();
    }
  };

  private renderSide(target: HTMLElement, side: FlashcardSide | null): void {
    target.replaceChildren();
    if (!side || !this.serializer) return;
    // The document's own nodes, through the schema's own rules. No string of
    // markup is built here and none is parsed.
    target.append(this.serializer.serializeFragment(side.content, { document: this.doc }));
  }

  private render(): void {
    const session = this.session;
    if (!session) return;

    const total = session.total;
    this.progress.textContent = total === 0 ? '' : `${session.position} de ${total}`;
    this.summary.textContent = `${this.cards} ${
      this.cards === 1 ? 'cartão' : 'cartões'
    } · ${total} ${total === 1 ? 'revisão' : 'revisões'}`;

    const item = session.current;
    this.completion.hidden = true;
    this.origin.textContent = item && 'noteTitle' in item ? item.noteTitle : '';
    this.renderSide(this.question, item?.question ?? null);

    const revealed = session.isRevealed;
    this.answer.hidden = !revealed;
    this.revealButton.hidden = revealed;
    if (revealed) {
      this.renderSide(this.answer, item?.answer ?? null);
    } else {
      this.answer.replaceChildren();
    }

    const scheduled = item && 'reviewKey' in item ? item : null;
    const priorRating = scheduled ? this.rated.get(scheduled.reviewKey) : undefined;
    this.ratingArea.hidden = !revealed || !scheduled;
    this.ratingStatus.textContent = priorRating
      ? `Avaliado: ${priorRating === 'difficult' ? 'Difícil' : priorRating === 'medium' ? 'Médio' : 'Fácil'}`
      : '';
    if (scheduled) {
      const current = scheduled.schedule?.level ?? null;
      (Object.keys(this.ratingButtons) as StudyRating[]).forEach((rating) => {
        const interval = this.ratingButtons[rating].querySelector('.note-study-rating-interval');
        if (interval) interval.textContent = intervalLabel(nextLevel(current, rating));
      });
    }
    this.setRatingButtonsDisabled(Boolean(this.pendingRating) || Boolean(priorRating));

    this.previousButton.disabled = !session.hasPrevious;
    this.nextButton.disabled = !session.hasNext;
    this.shuffleButton.disabled = total < 2;

    // A long card scrolls inside the panel; the next one starts at its top
    // rather than wherever the last one was left.
    this.card.scrollTop = 0;
  }
}
