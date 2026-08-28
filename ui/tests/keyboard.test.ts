import { afterEach, describe, expect, it, vi } from 'vitest';
import { NoteEditor } from '../src/editor/editor.ts';
import { NoteKeyboardController } from '../src/editor/keyboard.ts';
import type { NoteKeyboardActions } from '../src/editor/keyboard.ts';

function keyboardEvent(key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  return new KeyboardEvent('keydown', {
    key,
    ctrlKey: true,
    bubbles: true,
    cancelable: true,
    ...init,
  });
}

function mockActions(): NoteKeyboardActions {
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

describe('NoteKeyboardController', () => {
  let controller: NoteKeyboardController | null = null;

  afterEach(() => {
    controller?.destroy();
    controller = null;
  });

  it('does not block or execute shortcuts while composition is active', () => {
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    window.dispatchEvent(new CompositionEvent('compositionstart'));
    const event = keyboardEvent('r');
    window.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
    expect(actions.toggleStrike).not.toHaveBeenCalled();
    expect(actions.newNote).not.toHaveBeenCalled();

    window.dispatchEvent(new CompositionEvent('compositionend'));
    const completedEvent = keyboardEvent('r');
    window.dispatchEvent(completedEvent);
    expect(completedEvent.defaultPrevented).toBe(true);
    expect(actions.toggleStrike).toHaveBeenCalledOnce();
  });

  it('respects isComposing even without a preceding compositionstart event', () => {
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);
    const event = keyboardEvent('n');
    Object.defineProperty(event, 'isComposing', { value: true });

    window.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
    expect(actions.newNote).not.toHaveBeenCalled();
  });

  it('leaves Ctrl+Alt combinations available to AltGr input', () => {
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);
    const event = keyboardEvent('n', { altKey: true });

    window.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
    expect(actions.newNote).not.toHaveBeenCalled();
  });

  it('toggles the layer with Ctrl+Shift+Space and never types a space', () => {
    // 3.5R. The shortcut had no coverage at all: nothing proved it reached
    // `toggleLayerMode`, and nothing proved it could not leave a space behind
    // in the note it was pressed in.
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    const event = keyboardEvent(' ', { shiftKey: true, code: 'Space' });
    window.dispatchEvent(event);

    expect(actions.toggleLayerMode).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it('recognises Ctrl+Shift+Space by physical key when the layout reports no character', () => {
    // WebKitGTK reports the produced character for this chord, but a layout
    // that reports none must still reach the same action.
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    const event = keyboardEvent('Unidentified', { shiftKey: true, code: 'Space' });
    window.dispatchEvent(event);

    expect(actions.toggleLayerMode).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it('leaves a plain space, Ctrl+Space and AltGr chords alone', () => {
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    // A space typed into the note.
    const plain = keyboardEvent(' ', { ctrlKey: false, code: 'Space' });
    window.dispatchEvent(plain);
    // Ctrl+Space without Shift is not the shortcut.
    const noShift = keyboardEvent(' ', { code: 'Space' });
    window.dispatchEvent(noShift);
    // AltGr is reported as Ctrl+Alt and must reach composition untouched.
    const altGr = keyboardEvent(' ', { shiftKey: true, altKey: true, code: 'Space' });
    window.dispatchEvent(altGr);

    expect(actions.toggleLayerMode).not.toHaveBeenCalled();
    expect(plain.defaultPrevented).toBe(false);
    expect(noShift.defaultPrevented).toBe(false);
    expect(altGr.defaultPrevented).toBe(false);
  });

  it('does not toggle the layer while a pt-BR composition is active', () => {
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    window.dispatchEvent(new CompositionEvent('compositionstart'));
    const during = keyboardEvent(' ', { shiftKey: true, code: 'Space' });
    window.dispatchEvent(during);
    expect(actions.toggleLayerMode).not.toHaveBeenCalled();
    expect(during.defaultPrevented).toBe(false);

    window.dispatchEvent(new CompositionEvent('compositionend'));
    const after = keyboardEvent(' ', { shiftKey: true, code: 'Space' });
    window.dispatchEvent(after);
    expect(actions.toggleLayerMode).toHaveBeenCalledOnce();
  });

  it('keeps every Ctrl+Shift chord on its own action', () => {
    // One table, so a new chord cannot quietly shadow an existing one.
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    window.dispatchEvent(keyboardEvent('M', { shiftKey: true, code: 'KeyM' }));
    window.dispatchEvent(keyboardEvent(' ', { shiftKey: true, code: 'Space' }));
    window.dispatchEvent(keyboardEvent('>', { shiftKey: true, code: 'Period' }));
    window.dispatchEvent(keyboardEvent('<', { shiftKey: true, code: 'Comma' }));

    expect(actions.toggleCollapsed).toHaveBeenCalledOnce();
    expect(actions.toggleLayerMode).toHaveBeenCalledOnce();
    expect(actions.increaseTextSize).toHaveBeenCalledOnce();
    expect(actions.decreaseTextSize).toHaveBeenCalledOnce();
  });

  it('leaves an unclaimed Ctrl+Shift chord to the editor', () => {
    // Ctrl+Shift+Z is redo; swallowing it here would break undo history.
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    const event = keyboardEvent('Z', { shiftKey: true, code: 'KeyZ' });
    window.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
  });

  it('does not insert a space into the note it is pressed in', () => {
    // The end-to-end version of the guarantee: the editor is real, the chord
    // is dispatched at the window as WebKit delivers it, and the document must
    // come out unchanged.
    const container = document.createElement('div');
    document.body.appendChild(container);
    const editor = new NoteEditor({ element: container, initialContent: 'texto' });
    editor.getRawEditor().commands.focus('end');
    const actions = mockActions();
    controller = new NoteKeyboardController(window, actions);

    const before = editor.getMarkdown();
    const event = keyboardEvent(' ', { shiftKey: true, code: 'Space' });
    container.querySelector('.ProseMirror')?.dispatchEvent(event);

    expect(actions.toggleLayerMode).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
    expect(editor.getMarkdown()).toBe(before);

    editor.destroy();
    container.remove();
  });

  it('prevents reload and toggles strike on selected editor text with Ctrl+R', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const editor = new NoteEditor({ element: container, initialContent: 'texto riscado' });
    editor.getRawEditor().commands.setTextSelection({ from: 1, to: 14 });
    const actions = mockActions();
    actions.toggleStrike = () => editor.toggleStrike();
    controller = new NoteKeyboardController(window, actions);

    const applyEvent = keyboardEvent('r');
    window.dispatchEvent(applyEvent);
    expect(applyEvent.defaultPrevented).toBe(true);
    expect(container.querySelector('s')?.textContent).toBe('texto riscado');
    expect(editor.getMarkdown()).toContain('~~texto riscado~~');

    const removeEvent = keyboardEvent('r');
    window.dispatchEvent(removeEvent);
    expect(removeEvent.defaultPrevented).toBe(true);
    expect(container.querySelector('s')).toBeNull();
    expect(editor.getMarkdown()).not.toContain('~~');

    editor.destroy();
    container.remove();
  });

  it('toggles the stored strike mark with Ctrl+R when there is no selection', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const editor = new NoteEditor({ element: container, initialContent: 'antes ' });
    editor.getRawEditor().commands.focus('end');
    const actions = mockActions();
    actions.toggleStrike = () => editor.toggleStrike();
    controller = new NoteKeyboardController(window, actions);

    window.dispatchEvent(keyboardEvent('r'));
    editor.getRawEditor().commands.insertContent('durante');
    expect(container.querySelector('s')?.textContent).toBe('durante');

    window.dispatchEvent(keyboardEvent('r'));
    editor.getRawEditor().commands.insertContent(' depois');
    expect(editor.getMarkdown()).toContain('antes ~~durante~~ depois');

    editor.destroy();
    container.remove();
  });
});
