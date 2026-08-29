/** What the page has to do when a note's collapse state changes. */
export interface CollapseTransition {
  /**
   * Whether everything that needs room to be typed into must close.
   *
   * A collapsed note is a header bar. A search field or a find bar left open
   * would be hanging over a surface that no longer exists.
   */
  closePanels: boolean;
  /**
   * Whether the caret goes back into the text.
   *
   * Collapsing hides the editor with `display: none`, and an element that
   * stops being displayed stops holding the selection. Without this the note
   * comes back looking ready and ignores every keystroke until it happens to
   * be clicked — and a note on the desktop layer sits behind every window, so
   * there may be no click available to give it.
   */
  restoreCaret: boolean;
}

/**
 * The collapse transition, as a decision rather than a branch.
 *
 * Both halves depend on the direction of the change and not merely on the new
 * state, which is why the previous state is an argument: expanding a note that
 * was never collapsed must not steal the caret from whatever the reader was
 * actually doing — the host sends the expanded state on every load.
 */
export function collapseTransition(was: boolean, now: boolean): CollapseTransition {
  return {
    closePanels: now,
    restoreCaret: was && !now,
  };
}
