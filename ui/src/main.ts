import './styles/theme.css';
import { bridge } from './bridge/bridge.ts';
import { NoteEditor } from './editor/editor.ts';
import { NoteKeyboardController } from './editor/keyboard.ts';
import { PaperColor } from './bridge/types.ts';
import { PointerGestureController } from './geometry/gesture.ts';
import { NoteMenu } from './ui/menu.ts';
import { NoteInfoTooltip } from './ui/tooltip.ts';

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
let currentFontSize = 15;
let isCollapsed = false;
let noteEditor: NoteEditor | null = null;
let noteMenu: NoteMenu | null = null;
let infoTooltip: NoteInfoTooltip | null = null;

function setPaperColor(color: PaperColor): void {
  document.body.setAttribute('data-color', color);
  noteMenu?.setSelectedColor(color);
}

function setFontSize(size: number): void {
  const clamped = Math.max(11, Math.min(32, size));
  currentFontSize = clamped;
  document.documentElement.style.setProperty('--note-font-size', `${clamped}px`);
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
 * Tells the host whether the settings popover is on screen. A collapsed note
 * is barely taller than its header bar, so the host lends it enough room to
 * show the menu; the persisted geometry is untouched either way.
 */
function setMenuOverlay(open: boolean): void {
  if (!activeNoteId) return;
  bridge.sendMessage({
    type: 'menu_overlay',
    payload: { id: activeNoteId, open },
  });
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
          setMenuOverlay(true);
        },
        onClose: () => setMenuOverlay(false),
        onSelectColor: (color) => {
          setPaperColor(color);
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'color_changed',
              payload: { id: activeNoteId, color },
            });
          }
        },
        onToggleCollapsed: (collapsed) => {
          setCollapsed(collapsed);
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'collapse_changed',
              payload: { id: activeNoteId, collapsed },
            });
          }
        },
      },
    });
    noteMenu.setCollapsed(false);
  }

  // Contextual note information on the free area of the header bar.
  if (dragRegion && menuMount) {
    infoTooltip = new NoteInfoTooltip({
      hoverTarget: dragRegion,
      mount: menuMount,
    });
  }

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
        bridge.sendMessage({ type: 'drag_start' });
      },
      onDelta: (dx, dy) => {
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
    increaseFontSize: () => {
      setFontSize(currentFontSize + 1);
      if (activeNoteId) {
        bridge.sendMessage({
          type: 'font_size_changed',
          payload: { id: activeNoteId, fontSize: currentFontSize },
        });
      }
    },
    decreaseFontSize: () => {
      setFontSize(currentFontSize - 1);
      if (activeNoteId) {
        bridge.sendMessage({
          type: 'font_size_changed',
          payload: { id: activeNoteId, fontSize: currentFontSize },
        });
      }
    },
    toggleStrike: () => {
      noteEditor?.toggleStrike();
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
      infoTooltip?.setTimestamps({
        createdAt: msg.payload.createdAt ?? null,
        updatedAt: msg.payload.updatedAt ?? null,
      });
      noteEditor?.setMarkdown(msg.payload.content || '');
      noteEditor?.focus();
    } else if (msg.type === 'set_timestamps') {
      infoTooltip?.setTimestamps({
        createdAt: msg.payload.createdAt ?? null,
        updatedAt: msg.payload.updatedAt ?? null,
      });
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
