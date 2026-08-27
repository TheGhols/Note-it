import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  clampZoom,
  DEFAULT_ZOOM_PERCENT,
  MAX_ZOOM_PERCENT,
  MIN_ZOOM_PERCENT,
  ZOOM_STEP_PERCENT,
  zoomIn,
  zoomOut,
} from '../src/editor/zoom.ts';
import { NoteKeyboardController } from '../src/editor/keyboard.ts';
import type { NoteKeyboardActions } from '../src/editor/keyboard.ts';

function actions(): NoteKeyboardActions {
  return {
    newNote: vi.fn(),
    closeNote: vi.fn(),
    toggleStrike: vi.fn(),
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    resetZoom: vi.fn(),
    toggleCollapsed: vi.fn(),
    toggleLayerMode: vi.fn(),
    increaseTextSize: vi.fn(),
    decreaseTextSize: vi.fn(),
  };
}

function press(init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    ctrlKey: true,
    bubbles: true,
    cancelable: true,
    ...init,
  });
  window.dispatchEvent(event);
  return event;
}

describe('zoom scale', () => {
  it('defaults to 100%', () => {
    expect(DEFAULT_ZOOM_PERCENT).toBe(100);
    expect(clampZoom(100)).toBe(100);
  });

  it('steps by 10% in each direction', () => {
    expect(ZOOM_STEP_PERCENT).toBe(10);
    expect(zoomIn(100)).toBe(110);
    expect(zoomOut(100)).toBe(90);
    expect(zoomIn(zoomOut(100))).toBe(100);
  });

  it('clamps to the supported range', () => {
    expect(MIN_ZOOM_PERCENT).toBe(75);
    expect(MAX_ZOOM_PERCENT).toBe(200);
    expect(zoomOut(75)).toBe(75);
    expect(zoomIn(200)).toBe(200);
    expect(clampZoom(10)).toBe(75);
    expect(clampZoom(5000)).toBe(200);
  });

  it('rejects values that are not real percentages', () => {
    for (const value of [Number.NaN, Number.POSITIVE_INFINITY, -Infinity, 'abc', null, undefined, {}]) {
      expect(clampZoom(value)).toBe(DEFAULT_ZOOM_PERCENT);
    }
    // A negative number is clamped, never applied.
    expect(clampZoom(-300)).toBe(MIN_ZOOM_PERCENT);
  });
});

describe('zoom shortcuts', () => {
  let controller: NoteKeyboardController | null = null;

  afterEach(() => {
    controller?.destroy();
    controller = null;
  });

  it('Ctrl+= and Ctrl++ zoom in', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    const equals = press({ key: '=' });
    const plus = press({ key: '+' });

    expect(acts.zoomIn).toHaveBeenCalledTimes(2);
    expect(equals.defaultPrevented).toBe(true);
    expect(plus.defaultPrevented).toBe(true);
  });

  it('Ctrl+- zooms out and Ctrl+0 restores 100%', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    press({ key: '-' });
    press({ key: '0' });

    expect(acts.zoomOut).toHaveBeenCalledTimes(1);
    expect(acts.resetZoom).toHaveBeenCalledTimes(1);
  });

  it('does not run while a composition is active', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    window.dispatchEvent(new CompositionEvent('compositionstart'));
    press({ key: '=' });
    press({ key: '0' });
    expect(acts.zoomIn).not.toHaveBeenCalled();
    expect(acts.resetZoom).not.toHaveBeenCalled();

    window.dispatchEvent(new CompositionEvent('compositionend'));
    press({ key: '=' });
    expect(acts.zoomIn).toHaveBeenCalledTimes(1);
  });

  it('leaves AltGr combinations to the editor', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    // AltGr arrives as Ctrl+Alt; it must reach the editor untouched.
    const event = press({ key: '=', altKey: true });
    expect(acts.zoomIn).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});

describe('collapse and layer shortcuts', () => {
  let controller: NoteKeyboardController | null = null;

  afterEach(() => {
    controller?.destroy();
    controller = null;
  });

  it('Ctrl+Shift+M toggles the collapsed state', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    const event = press({ key: 'M', shiftKey: true, code: 'KeyM' });

    expect(acts.toggleCollapsed).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
    // It is a view action: it must not touch the content.
    expect(acts.toggleStrike).not.toHaveBeenCalled();
  });

  it('Ctrl+Shift+Space toggles the layer mode', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    const event = press({ key: ' ', shiftKey: true, code: 'Space' });

    expect(acts.toggleLayerMode).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it('Ctrl+Shift+> and Ctrl+Shift+< step the text size', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    press({ key: '>', shiftKey: true, code: 'Period' });
    press({ key: '<', shiftKey: true, code: 'Comma' });
    // Layouts that report the unshifted character are accepted too.
    press({ key: '.', shiftKey: true, code: 'Period' });
    press({ key: ',', shiftKey: true, code: 'Comma' });

    expect(acts.increaseTextSize).toHaveBeenCalledTimes(2);
    expect(acts.decreaseTextSize).toHaveBeenCalledTimes(2);
  });

  it('ignores the shortcuts during composition and with AltGr', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    window.dispatchEvent(new CompositionEvent('compositionstart'));
    press({ key: 'M', shiftKey: true, code: 'KeyM' });
    press({ key: ' ', shiftKey: true, code: 'Space' });
    window.dispatchEvent(new CompositionEvent('compositionend'));

    press({ key: 'M', shiftKey: true, code: 'KeyM', altKey: true });

    expect(acts.toggleCollapsed).not.toHaveBeenCalled();
    expect(acts.toggleLayerMode).not.toHaveBeenCalled();
  });

  it('leaves unrelated Ctrl+Shift chords alone', () => {
    const acts = actions();
    controller = new NoteKeyboardController(window, acts);

    const event = press({ key: 'Z', shiftKey: true, code: 'KeyZ' });

    expect(event.defaultPrevented).toBe(false);
    expect(acts.toggleCollapsed).not.toHaveBeenCalled();
    expect(acts.toggleLayerMode).not.toHaveBeenCalled();
    expect(acts.increaseTextSize).not.toHaveBeenCalled();
  });
});
