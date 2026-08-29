import { afterEach, describe, expect, it, vi } from 'vitest';
import { NoteKeyboardController } from '../src/editor/keyboard.ts';

/**
 * Collapse and layer, and the two chords that drive them.
 *
 * A note that lives on the desktop layer is behind every window, so the only
 * way to reach it is the keyboard — which makes it worth proving, rather than
 * assuming, that neither chord can be read as the other, that neither state
 * change drags the other along, and that one press is one change. What these
 * tests cannot cover is the compositor: whether a Bottom-layer surface still
 * holds keyboard focus is Niri's answer to give, and `docs/` records how the
 * matrix was run against it.
 */
interface Note {
  collapsed: boolean;
  layer: 'overlay' | 'desktop';
}

/** The two operations, as the page performs them: independent and idempotent. */
function noteModel() {
  const note: Note = { collapsed: false, layer: 'overlay' };
  const collapseCalls: boolean[] = [];
  const layerCalls: string[] = [];

  return {
    note,
    collapseCalls,
    layerCalls,
    toggleCollapsed(): void {
      note.collapsed = !note.collapsed;
      collapseCalls.push(note.collapsed);
    },
    /** The host owns the layer; the page only asks. */
    toggleLayerMode(): void {
      layerCalls.push('toggle');
    },
    /** What the host sends back once it has moved every note. */
    applyLayerMode(layer: 'overlay' | 'desktop'): void {
      note.layer = layer;
    },
  };
}

function mountKeyboard(model: ReturnType<typeof noteModel>) {
  return new NoteKeyboardController(window, {
    newNote: vi.fn(),
    closeNote: vi.fn(),
    toggleStrike: vi.fn(),
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    resetZoom: vi.fn(),
    toggleCollapsed: () => model.toggleCollapsed(),
    toggleLayerMode: () => model.toggleLayerMode(),
    increaseTextSize: vi.fn(),
    decreaseTextSize: vi.fn(),
    openGlobalSearch: vi.fn(),
    openFind: vi.fn(),
    openReplace: vi.fn(),
  });
}

function press(key: string, code: string): void {
  window.dispatchEvent(
    new KeyboardEvent('keydown', {
      key,
      code,
      ctrlKey: true,
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }),
  );
}

const collapseChord = () => press('M', 'KeyM');
const layerChord = () => press(' ', 'Space');

describe('collapse and layer are two switches, not one', () => {
  let keyboard: NoteKeyboardController | null = null;

  afterEach(() => {
    keyboard?.destroy();
    keyboard = null;
  });

  it('changing the layer leaves the note as collapsed or expanded as it was', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);

    model.toggleCollapsed();
    expect(model.note.collapsed).toBe(true);

    layerChord();
    model.applyLayerMode('desktop');

    expect(model.note.collapsed).toBe(true);
    expect(model.collapseCalls).toEqual([true]);
  });

  it('collapsing leaves the layer where it was', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);
    model.applyLayerMode('desktop');

    collapseChord();
    collapseChord();

    expect(model.note.layer).toBe('desktop');
    expect(model.layerCalls).toEqual([]);
  });

  it('layer then collapse is exactly one of each', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);

    layerChord();
    model.applyLayerMode('desktop');
    collapseChord();

    expect(model.layerCalls).toHaveLength(1);
    expect(model.collapseCalls).toEqual([true]);
    expect(model.note).toEqual({ collapsed: true, layer: 'desktop' });
  });

  it('collapse then layer does not repeat either operation', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);

    collapseChord();
    layerChord();
    model.applyLayerMode('desktop');

    expect(model.collapseCalls).toEqual([true]);
    expect(model.layerCalls).toHaveLength(1);
    expect(model.note).toEqual({ collapsed: true, layer: 'desktop' });
  });

  it('no key produces two toggles', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);

    collapseChord();
    expect(model.collapseCalls).toHaveLength(1);
    expect(model.layerCalls).toHaveLength(0);

    layerChord();
    expect(model.layerCalls).toHaveLength(1);
    expect(model.collapseCalls).toHaveLength(1);
  });

  it('holding a chord down does not collapse the note over and over', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);

    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'M',
        code: 'KeyM',
        ctrlKey: true,
        shiftKey: true,
        repeat: true,
        cancelable: true,
      }),
    );

    expect(model.collapseCalls).toEqual([]);
  });

  it('the two chords stay distinct, and neither is the other', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);

    // Ctrl+Shift+M is the note's own collapse and always has been.
    collapseChord();
    expect(model.collapseCalls).toHaveLength(1);
    expect(model.layerCalls).toHaveLength(0);

    // Ctrl+Shift+Space is the layer, and asks the host rather than deciding.
    layerChord();
    expect(model.layerCalls).toEqual(['toggle']);
    expect(model.collapseCalls).toHaveLength(1);

    // Neither fires without both modifiers.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'M', code: 'KeyM', ctrlKey: true }));
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'M', code: 'KeyM', shiftKey: true }));
    expect(model.collapseCalls).toHaveLength(1);
  });

  it('an AltGr composition never reaches either of them', () => {
    const model = noteModel();
    keyboard = mountKeyboard(model);

    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'M',
        code: 'KeyM',
        ctrlKey: true,
        shiftKey: true,
        altKey: true,
      }),
    );

    expect(model.collapseCalls).toEqual([]);
  });
});
