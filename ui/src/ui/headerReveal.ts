/**
 * Height of the strip along the top of the note that reveals the chrome.
 *
 * The same number is the editor's top padding, and that is the whole point: the
 * strip is the one part of the surface that is always a pointer target, so no
 * line of text may ever sit underneath it. `tests/header_ux.test.ts` compares
 * this constant against the stylesheet rather than trusting the two to agree.
 */
export const REVEAL_ZONE_PX = 24;

/**
 * How far down the chrome stays out once it has been revealed.
 *
 * Revealing and hiding deliberately use different thresholds. The bar is taller
 * than the strip that summons it, so a pointer travelling from the strip to a
 * button passes through rows that would otherwise re-hide the thing it is
 * reaching for. Equal to the bar's own height: the chrome recedes when the
 * pointer leaves it, not while it is still on it.
 */
export const HOLD_ZONE_PX = 28;

export interface HeaderRevealOptions {
  /** The header element. Focus anywhere inside it holds the chrome out. */
  header: HTMLElement;
  /** Element the state is published on. Defaults to the header's own body. */
  body?: HTMLElement;
  /** Defaults to the document owning the header. */
  doc?: Document;
  /** Defaults to that document's window. */
  view?: Window;
}

/**
 * Decides when the note's chrome is on show.
 *
 * The bar is overlaid on the paper rather than given a row of its own, so it
 * has to be *absent* most of the time — not faint, absent. The previous
 * attempt kept the whole header at `opacity: 0.06` and lifted it on
 * `:hover`, which failed physically for two reasons: a permanently dimmed
 * panel is still a panel over the reader's first line, and the hover target
 * was the bar itself, so the bar had to already be covering the text in order
 * to be reachable.
 *
 * Here the state is explicit and lives in one place. Five independent reasons
 * hold the chrome out, and the stylesheet only reads the answer:
 *
 * - the pointer is in the strip along the top of the note;
 * - something inside the header has keyboard focus;
 * - a quick action or the menu is open;
 * - the note is capturing the clipboard, so the indicator saying so stays
 *   where it can be seen;
 * - the note is collapsed, in which case the bar *is* the note.
 *
 * Nothing here paints. It publishes `data-header-revealed` and the stylesheet
 * decides what that looks like — including which elements are pointer targets,
 * because a control that cannot be seen must not be able to take a click meant
 * for the text underneath it.
 */
export class HeaderReveal {
  private readonly header: HTMLElement;
  private readonly body: HTMLElement;
  private readonly doc: Document;
  private readonly view: Window;

  private pointerAtTop = false;
  private focusInside = false;
  private held = false;
  private capturing = false;
  private collapsed = false;

  public constructor(options: HeaderRevealOptions) {
    this.header = options.header;
    this.doc = options.doc ?? options.header.ownerDocument;
    this.body = options.body ?? this.doc.body;
    this.view = options.view ?? this.doc.defaultView!;

    this.view.addEventListener('pointermove', this.handlePointerMove);
    this.doc.addEventListener('pointerleave', this.handlePointerLeave);
    this.doc.addEventListener('focusin', this.handleFocusIn);
    this.doc.addEventListener('focusout', this.handleFocusOut);
    this.publish();
  }

  public destroy(): void {
    this.view.removeEventListener('pointermove', this.handlePointerMove);
    this.doc.removeEventListener('pointerleave', this.handlePointerLeave);
    this.doc.removeEventListener('focusin', this.handleFocusIn);
    this.doc.removeEventListener('focusout', this.handleFocusOut);
  }

  /** Whether the chrome is currently on show. */
  public isRevealed(): boolean {
    return (
      this.collapsed || this.held || this.capturing || this.focusInside || this.pointerAtTop
    );
  }

  /**
   * A collapsed note is only its bar, so the bar stays and the pointer has no
   * say in it. Auto-hide resumes when the note is expanded again.
   */
  public setCollapsed(collapsed: boolean): void {
    this.collapsed = collapsed;
    this.publish();
  }

  /**
   * Holds the chrome out while a popover it belongs to is open.
   *
   * The menu can be taller than the note and the pointer ends up far below the
   * bar that opened it; without this the controls would recede while they are
   * being used.
   */
  public setHeld(held: boolean): void {
    this.held = held;
    this.publish();
  }

  /**
   * Keeps the chrome out while the note is capturing the clipboard.
   *
   * A reason of its own rather than another caller of `setHeld`, because the
   * two end at different times: closing the menu must not take down the one
   * visible sign that everything copied is being filed into this note.
   *
   * Safe against the defect this bar was rebuilt to fix. The header paints the
   * paper under exactly the gutter, which is by definition the strip that is
   * always the note's own and never a line's, so a bar that stays out covers
   * no text even when the note is scrolled.
   */
  public setCapturing(capturing: boolean): void {
    this.capturing = capturing;
    this.publish();
  }

  private publish(): void {
    this.body.setAttribute('data-header-revealed', String(this.isRevealed()));
  }

  private readonly handlePointerMove = (event: Event): void => {
    const y = (event as PointerEvent).clientY;
    if (typeof y !== 'number') return;
    if (y <= REVEAL_ZONE_PX) {
      this.pointerAtTop = true;
    } else if (y > HOLD_ZONE_PX) {
      this.pointerAtTop = false;
    } else {
      // Between the two thresholds nothing changes: this band is the bar
      // itself, and crossing it is how a control gets reached.
      return;
    }
    this.publish();
  };

  private readonly handlePointerLeave = (): void => {
    this.pointerAtTop = false;
    this.publish();
  };

  private readonly handleFocusIn = (event: Event): void => {
    const target = (event as FocusEvent).target as Node | null;
    if (!target || !this.header.contains(target)) return;
    this.focusInside = true;
    this.publish();
  };

  private readonly handleFocusOut = (event: Event): void => {
    const next = (event as FocusEvent).relatedTarget as Node | null;
    if (next && this.header.contains(next)) return;
    this.focusInside = false;
    this.publish();
  };
}
