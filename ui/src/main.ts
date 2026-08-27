import './styles/theme.css';
import { bridge } from './bridge/bridge.ts';
import { NoteEditor } from './editor/editor.ts';
import { NoteKeyboardController } from './editor/keyboard.ts';
import { NoteLayerMode, PaperColor } from './bridge/types.ts';
import { PointerGestureController } from './geometry/gesture.ts';
import { NoteMenu } from './ui/menu.ts';
import { NoteInfoTooltip } from './ui/tooltip.ts';
import { TextSize } from './editor/textSize.ts';
import { clampZoom, DEFAULT_ZOOM_PERCENT, zoomIn, zoomOut } from './editor/zoom.ts';

const PAPER_COLORS: PaperColor[] = [
  'yellow',
  'blue',
  'green',
  'pink',
  'purple',
  'gray',
  'black',
];

let activeNoteId = '';
let currentZoom = DEFAULT_ZOOM_PERCENT;
let currentLayerMode: NoteLayerMode = 'overlay';
/** Whether the gesture in progress actually moved the note. */
let dragMoved = false;
let isCollapsed = false;
let noteEditor: NoteEditor | null = null;
let noteMenu: NoteMenu | null = null;
let infoTooltip: NoteInfoTooltip | null = null;

function setPaperColor(color: PaperColor): void {
  document.body.setAttribute('data-color', color);
  noteMenu?.setSelectedColor(color);
}

/**
 * Base text size of the note, carried in the note's front matter. Distinct from
 * both the zoom, which scales the view, and the inline text size mark, which is
 * part of the content. Ctrl+= / Ctrl+- now drive the zoom instead, so this is
 * applied from stored notes rather than changed from the keyboard.
 */
function setFontSize(size: number): void {
  const clamped = Math.max(11, Math.min(32, size));
  document.documentElement.style.setProperty('--note-font-size', `${clamped}px`);
}

/**
 * Scales the editor view without touching the document.
 *
 * The header keeps its own size so the menu, the close button and the drag bar
 * stay put; only the content below the bar is scaled.
 */
function applyZoom(percent: number, persist: boolean): void {
  const clamped = clampZoom(percent);
  const changed = clamped !== currentZoom;
  currentZoom = clamped;
  document.documentElement.style.setProperty('--note-zoom', String(clamped / 100));
  noteMenu?.setZoomPercent(clamped);

  if (persist && changed && activeNoteId) {
    bridge.sendMessage({
      type: 'zoom_changed',
      payload: { id: activeNoteId, zoomPercent: clamped },
    });
  }
}

function setLayerMode(mode: NoteLayerMode): void {
  currentLayerMode = mode;
  noteMenu?.setLayerMode(mode);
}

/** Mirrors the formatting under the cursor into the menu. */
function syncInlineFormatting(): void {
  if (!noteEditor || !noteMenu) return;
  noteMenu.setInlineFormatting({
    textSize: noteEditor.currentTextSize(),
    textSizeMixed: noteEditor.hasMixedTextSize(),
    textColor: noteEditor.currentTextColor(),
    highlight: noteEditor.currentHighlight(),
  });
}

function applyTextSize(size: TextSize | null): void {
  noteEditor?.setTextSize(size);
  syncInlineFormatting();
}

/**
 * Applies the collapsed look. The editor is only hidden, never destroyed, so
 * the content and the Tiptap instance survive untouched.
 */
function setCollapsed(collapsed: boolean): void {
  isCollapsed = collapsed;
  document.body.setAttribute('data-collapsed', String(collapsed));
  noteMenu?.setCollapsed(collapsed);
}

/**
 * The one collapse path, shared by the menu entry, Ctrl+Shift+M and a click on
 * a collapsed note, so they all go through the same persistence.
 */
function requestCollapsed(collapsed: boolean): void {
  setCollapsed(collapsed);
  if (activeNoteId) {
    bridge.sendMessage({
      type: 'collapse_changed',
      payload: { id: activeNoteId, collapsed },
    });
  }
}

/**
 * Runs `whenReady` once the WebView viewport has caught up with a surface that
 * is being resized by the host.
 *
 * Expanding is asynchronous: the page switches to the expanded layout at once,
 * but the Wayland surface only grows when the host resizes the window. Opening
 * the menu before that would have it clipped by a surface that is still a
 * header bar tall.
 */
function afterViewportGrows(whenReady: () => void): void {
  const startingHeight = window.innerHeight;
  let settled = false;

  const finish = (): void => {
    if (settled) return;
    settled = true;
    window.removeEventListener('resize', onResize);
    window.clearTimeout(fallback);
    whenReady();
  };

  const onResize = (): void => {
    if (window.innerHeight > startingHeight) finish();
  };

  window.addEventListener('resize', onResize);
  // The surface may already be large enough, or the resize may never arrive;
  // either way the menu still opens.
  const fallback = window.setTimeout(finish, 250);
}

/**
 * Expands a collapsed note when it is clicked.
 *
 * The whole bar is a target, so the note is not a dead strip the user has to
 * hunt a control on. Closing keeps working, and the settings button expands
 * and opens its menu in the same single click.
 */
function handleCollapsedClick(event: MouseEvent): void {
  if (!isCollapsed) return;

  const target = event.target as HTMLElement | null;
  // Closing a collapsed note must still close it.
  if (target?.closest('#btn-close')) return;

  // A drag that happens to end on the bar is a move, not a click.
  if (dragMoved) {
    dragMoved = false;
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  requestCollapsed(false);

  if (target?.closest('#btn-menu')) {
    afterViewportGrows(() => noteMenu?.openMenu());
  }
}

function flushSave(): void {
  if (activeNoteId && noteEditor) {
    const markdown = noteEditor.getMarkdown();
    noteEditor.cancelPendingSave();
    bridge.sendMessage({
      type: 'content_changed',
      payload: { id: activeNoteId, content: markdown },
    });
  }
}

function saveAndClose(): void {
  if (!activeNoteId || !noteEditor) return;
  const content = noteEditor.getMarkdown();
  noteEditor.cancelPendingSave();
  bridge.sendMessage({
    type: 'save_and_close',
    payload: { id: activeNoteId, content },
  });
}

function initUI(): void {
  const editorContainer = document.getElementById('editor-container');
  if (!editorContainer) return;

  noteEditor = new NoteEditor({
    element: editorContainer,
    initialContent: '',
    onUpdate: (markdown) => {
      if (activeNoteId) {
        bridge.sendMessage({
          type: 'content_changed',
          payload: { id: activeNoteId, content: markdown },
        });
      }
    },
  });

  const dragRegion = document.querySelector('.drag-region') as HTMLElement | null;

  // Note settings menu. The trigger and the popover both sit outside the drag
  // region, so interacting with them can never move the window.
  const btnMenu = document.getElementById('btn-menu');
  const menuMount = document.getElementById('note-controls-left');
  if (btnMenu && menuMount) {
    noteMenu = new NoteMenu({
      trigger: btnMenu,
      mount: menuMount,
      colors: PAPER_COLORS,
      handlers: {
        onOpen: () => {
          infoTooltip?.hide();
          syncInlineFormatting();
        },
        onSelectColor: (color) => {
          setPaperColor(color);
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'color_changed',
              payload: { id: activeNoteId, color },
            });
          }
        },
        onToggleCollapsed: (collapsed) => requestCollapsed(collapsed),
        onSelectTextSize: (size) => applyTextSize(size),
        onSelectTextColor: (color) => {
          noteEditor?.setTextColor(color);
          syncInlineFormatting();
        },
        onSelectHighlight: (color) => {
          noteEditor?.setHighlight(color);
          syncInlineFormatting();
        },
        onZoomIn: () => applyZoom(zoomIn(currentZoom), true),
        onZoomOut: () => applyZoom(zoomOut(currentZoom), true),
        onResetZoom: () => applyZoom(DEFAULT_ZOOM_PERCENT, true),
        onSelectLayerMode: (mode) => {
          // The host owns the shared mode; ask only when it would change.
          if (mode !== currentLayerMode) {
            bridge.sendMessage({ type: 'toggle_layer_mode' });
          }
        },
      },
    });
  }

  // Contextual note information on the free area of the header bar.
  if (dragRegion && menuMount) {
    infoTooltip = new NoteInfoTooltip({
      hoverTarget: dragRegion,
      mount: menuMount,
    });
  }

  // A collapsed note expands wherever it is clicked. Registered in the capture
  // phase so it runs before the menu's own click handler, which would
  // otherwise open a popover taller than the collapsed surface.
  document.getElementById('app')?.addEventListener('click', handleCollapsedClick, true);

  // Close button
  const btnClose = document.getElementById('btn-close');
  btnClose?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    saveAndClose();
  });

  // Drag region handling
  if (dragRegion) {
    new PointerGestureController(dragRegion, {
      onStart: () => {
        infoTooltip?.hide();
        noteMenu?.close();
        dragMoved = false;
        bridge.sendMessage({ type: 'drag_start' });
      },
      onDelta: (dx, dy) => {
        dragMoved = true;
        bridge.sendMessage({ type: 'drag_update', payload: { dx, dy } });
      },
      onEnd: () => {
        bridge.sendMessage({ type: 'drag_end' });
      },
    });
  }

  // Resize handle handling
  const resizeHandle = document.getElementById('resize-handle');
  if (resizeHandle) {
    new PointerGestureController(
      resizeHandle,
      {
        onStart: () => {
          infoTooltip?.hide();
          bridge.sendMessage({ type: 'resize_start' });
        },
        onDelta: (dx, dy) => {
          bridge.sendMessage({ type: 'resize_update', payload: { dx, dy } });
        },
        onEnd: () => {
          bridge.sendMessage({ type: 'resize_end' });
        },
      },
      {
        // A collapsed note is only a header bar; resizing it is unavailable
        // until it is expanded again.
        canStart: () => !isCollapsed,
        claimPointerDown: true,
      },
    );
  }

  // Keyboard shortcuts inside WebView. Composition and AltGr events remain native.
  new NoteKeyboardController(window, {
    newNote: () => {
      flushSave();
      bridge.sendMessage({ type: 'new_note_requested' });
    },
    closeNote: () => {
      saveAndClose();
    },
    toggleStrike: () => {
      noteEditor?.toggleStrike();
    },
    zoomIn: () => applyZoom(zoomIn(currentZoom), true),
    zoomOut: () => applyZoom(zoomOut(currentZoom), true),
    resetZoom: () => applyZoom(DEFAULT_ZOOM_PERCENT, true),
    toggleCollapsed: () => requestCollapsed(!isCollapsed),
    toggleLayerMode: () => bridge.sendMessage({ type: 'toggle_layer_mode' }),
    increaseTextSize: () => {
      noteEditor?.increaseTextSize();
      syncInlineFormatting();
    },
    decreaseTextSize: () => {
      noteEditor?.decreaseTextSize();
      syncInlineFormatting();
    },
  });

  // Flush save on blur / beforeunload
  window.addEventListener('beforeunload', () => {
    infoTooltip?.hide();
    if (noteEditor?.hasPendingSave()) flushSave();
  });

  // External link interceptor
  document.addEventListener('click', (e) => {
    const target = e.target as HTMLElement | null;
    const anchor = target?.closest('a');
    if (anchor && anchor.href) {
      e.preventDefault();
      bridge.sendMessage({
        type: 'open_external_url',
        payload: { url: anchor.href },
      });
    }
  });

  // Listen to Host Messages
  bridge.onMessage((msg) => {
    if (msg.type === 'load_note') {
      activeNoteId = msg.payload.id;
      setPaperColor(msg.payload.color);
      setFontSize(msg.payload.fontSize || 15);
      setCollapsed(Boolean(msg.payload.collapsed));
      applyZoom(msg.payload.zoomPercent ?? DEFAULT_ZOOM_PERCENT, false);
      setLayerMode(msg.payload.layerMode ?? 'overlay');
      infoTooltip?.setTimestamps({
        createdAt: msg.payload.createdAt ?? null,
        updatedAt: msg.payload.updatedAt ?? null,
      });
      noteEditor?.setMarkdown(msg.payload.content || '');
      noteEditor?.focus();
      syncInlineFormatting();
    } else if (msg.type === 'set_timestamps') {
      infoTooltip?.setTimestamps({
        createdAt: msg.payload.createdAt ?? null,
        updatedAt: msg.payload.updatedAt ?? null,
      });
    } else if (msg.type === 'set_collapsed') {
      // A collapse the host decided on, such as collapsing every note at once.
      setCollapsed(Boolean(msg.payload.collapsed));
    } else if (msg.type === 'set_layer_mode') {
      setLayerMode(msg.payload.layerMode);
    } else if (msg.type === 'set_color') {
      setPaperColor(msg.payload.color);
    } else if (msg.type === 'set_font_size') {
      setFontSize(msg.payload.fontSize);
    } else if (msg.type === 'request_content') {
      if (activeNoteId && noteEditor) {
        bridge.sendMessage({
          type: 'content_changed',
          payload: { id: activeNoteId, content: noteEditor.getMarkdown() },
        });
      }
    } else if (msg.type === 'request_save_and_close') {
      saveAndClose();
    } else if (msg.type === 'request_flush') {
      const content = noteEditor ? noteEditor.getMarkdown() : '';
      if (noteEditor) {
        noteEditor.cancelPendingSave();
      }
      bridge.sendMessage({
        type: 'flush_response',
        payload: {
          id: activeNoteId,
          requestId: msg.payload.requestId,
          content,
        },
      });
    }
  });

  // Notify Host that Webview is ready
  bridge.sendMessage({ type: 'ready' });
}

document.addEventListener('DOMContentLoaded', initUI);
