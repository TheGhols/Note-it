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
    increaseFontSize: vi.fn(),
    decreaseFontSize: vi.fn(),
    toggleStrike: vi.fn(),
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
