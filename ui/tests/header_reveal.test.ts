import { afterEach, describe, expect, it } from 'vitest';
import { HeaderReveal, HOLD_ZONE_PX, REVEAL_ZONE_PX } from '../src/ui/headerReveal.ts';

/**
 * The chrome's behaviour, driven the way a pointer drives it.
 *
 * These tests say nothing about opacity or about which selector is written
 * where. They move a pointer, move focus and open a popover, and ask whether
 * the controls are on show — which is the thing that was actually wrong. The
 * previous suite asserted `opacity: 0.06` and a `:hover` rule and passed
 * against a header nobody could use.
 */
function buildNote() {
  document.body.innerHTML = '';
  document.body.removeAttribute('data-collapsed');

  const app = document.createElement('div');
  app.id = 'app';
  const header = document.createElement('div');
  header.id = 'drag-handle';
  header.className = 'note-header';
  const button = document.createElement('button');
  button.id = 'btn-menu';
  const dragRegion = document.createElement('div');
  dragRegion.className = 'drag-region';
  header.append(button, dragRegion);
  const editor = document.createElement('div');
  editor.className = 'editor-wrapper';
  const outside = document.createElement('button');
  outside.id = 'outside';
  app.append(header, editor, outside);
  document.body.append(app);

  const reveal = new HeaderReveal({ header, body: document.body });
  return { reveal, header, button, editor, outside };
}

/** Moves the pointer to `y` pixels from the top of the note. */
function pointerTo(y: number): void {
  window.dispatchEvent(new PointerEvent('pointermove', { clientY: y, bubbles: true }));
}

function published(): string | null {
  return document.body.getAttribute('data-header-revealed');
}

describe('revealing the note chrome', () => {
  let note: ReturnType<typeof buildNote> | null = null;

  afterEach(() => {
    note?.reveal.destroy();
    note = null;
    document.body.innerHTML = '';
    document.body.removeAttribute('data-collapsed');
    document.body.removeAttribute('data-header-revealed');
  });

  it('starts hidden, and says so where the stylesheet can read it', () => {
    note = buildNote();
    expect(note.reveal.isRevealed()).toBe(false);
    expect(published()).toBe('false');
  });

  it('reveals the controls when the pointer reaches the strip at the top', () => {
    note = buildNote();

    pointerTo(REVEAL_ZONE_PX);

    expect(note.reveal.isRevealed()).toBe(true);
    expect(published()).toBe('true');
  });

  it('lets the controls recede when the pointer leaves the bar', () => {
    note = buildNote();
    pointerTo(2);
    expect(note.reveal.isRevealed()).toBe(true);

    pointerTo(HOLD_ZONE_PX + 1);

    expect(note.reveal.isRevealed()).toBe(false);
    expect(published()).toBe('false');
  });

  it('stays out while the pointer travels from the strip onto a control', () => {
    note = buildNote();
    pointerTo(1);

    // The bar is taller than the strip that summons it. Every row in between
    // is a row a button occupies, so passing through must not take the button
    // away from the pointer reaching for it.
    for (let y = REVEAL_ZONE_PX; y <= HOLD_ZONE_PX; y += 1) {
      pointerTo(y);
      expect(note.reveal.isRevealed()).toBe(true);
    }
  });

  it('does not reveal anything while the pointer is over the note body', () => {
    note = buildNote();

    pointerTo(HOLD_ZONE_PX + 40);
    pointerTo(200);

    expect(note.reveal.isRevealed()).toBe(false);
  });

  it('recedes when the pointer leaves the note entirely', () => {
    note = buildNote();
    pointerTo(0);
    expect(note.reveal.isRevealed()).toBe(true);

    document.dispatchEvent(new PointerEvent('pointerleave'));

    expect(note.reveal.isRevealed()).toBe(false);
  });

  it('keeps the controls out while one of them holds the keyboard', () => {
    note = buildNote();

    note.button.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));

    expect(note.reveal.isRevealed()).toBe(true);
    // The pointer is nowhere near the top and the bar stays anyway: a control
    // being used from the keyboard has to remain visible.
    pointerTo(300);
    expect(note.reveal.isRevealed()).toBe(true);
  });

  it('lets go once the keyboard moves out of the header', () => {
    note = buildNote();
    note.button.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));

    note.button.dispatchEvent(
      new FocusEvent('focusout', { bubbles: true, relatedTarget: note.outside }),
    );

    expect(note.reveal.isRevealed()).toBe(false);
  });

  it('ignores focus that never entered the header', () => {
    note = buildNote();

    note.outside.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));

    expect(note.reveal.isRevealed()).toBe(false);
  });

  it('keeps the controls out while a quick action or the menu is open', () => {
    note = buildNote();

    note.reveal.setHeld(true);
    pointerTo(400);

    expect(note.reveal.isRevealed()).toBe(true);

    note.reveal.setHeld(false);
    expect(note.reveal.isRevealed()).toBe(false);
  });

  it('a collapsed note keeps its bar regardless of the pointer', () => {
    note = buildNote();

    note.reveal.setCollapsed(true);
    pointerTo(500);
    document.dispatchEvent(new PointerEvent('pointerleave'));

    expect(note.reveal.isRevealed()).toBe(true);
    expect(published()).toBe('true');
  });

  it('goes back to hiding itself when the note is expanded again', () => {
    note = buildNote();
    note.reveal.setCollapsed(true);
    pointerTo(500);

    note.reveal.setCollapsed(false);

    expect(note.reveal.isRevealed()).toBe(false);
  });

  it('stops listening once destroyed', () => {
    note = buildNote();
    note.reveal.destroy();

    pointerTo(0);

    expect(note.reveal.isRevealed()).toBe(false);
    expect(published()).toBe('false');
  });
});
