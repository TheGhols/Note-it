import './styles/theme.css';
import { bridge } from './bridge/bridge.ts';
import { NoteEditor } from './editor/editor.ts';
import { NoteKeyboardController } from './editor/keyboard.ts';
import { PaperColor } from './bridge/types.ts';
import { PointerDeltaCoalescer } from './geometry/pointerDelta.ts';

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
let currentColorIndex = 0;
let noteEditor: NoteEditor | null = null;

function setPaperColor(color: PaperColor): void {
  document.body.setAttribute('data-color', color);
  const index = PAPER_COLORS.indexOf(color);
  if (index !== -1) {
    currentColorIndex = index;
  }
}

function setFontSize(size: number): void {
  const clamped = Math.max(11, Math.min(32, size));
  currentFontSize = clamped;
  document.documentElement.style.setProperty('--note-font-size', `${clamped}px`);
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

  // Color cycle button
  const btnTheme = document.getElementById('btn-theme');
  btnTheme?.addEventListener('click', (e) => {
    e.preventDefault();
    currentColorIndex = (currentColorIndex + 1) % PAPER_COLORS.length;
    const newColor = PAPER_COLORS[currentColorIndex];
    setPaperColor(newColor);
    if (activeNoteId) {
      bridge.sendMessage({
        type: 'color_changed',
        payload: { id: activeNoteId, color: newColor },
      });
    }
  });

  // Close button
  const btnClose = document.getElementById('btn-close');
  btnClose?.addEventListener('click', (e) => {
    e.preventDefault();
    saveAndClose();
  });

  // Drag region handling
  const dragRegion = document.querySelector('.drag-region') as HTMLElement | null;
  if (dragRegion) {
    let isDragging = false;
    let lastX = 0;
    let lastY = 0;
    const deltas = new PointerDeltaCoalescer((dx, dy) => {
      bridge.sendMessage({
        type: 'drag_update',
        payload: { dx, dy },
      });
    });

    dragRegion.addEventListener('pointerdown', (e: PointerEvent) => {
      if (e.button !== 0 || !Number.isFinite(e.screenX) || !Number.isFinite(e.screenY)) return;
      isDragging = true;
      lastX = e.screenX;
      lastY = e.screenY;
      deltas.reset();
      dragRegion.setPointerCapture(e.pointerId);
      bridge.sendMessage({ type: 'drag_start' });
    });

    dragRegion.addEventListener('pointermove', (e: PointerEvent) => {
      if (!isDragging) return;
      const dx = e.screenX - lastX;
      const dy = e.screenY - lastY;
      lastX = e.screenX;
      lastY = e.screenY;
      deltas.add(dx, dy);
    });

    const endDrag = (e: PointerEvent) => {
      if (!isDragging) return;
      isDragging = false;
      if (e.type === 'pointerup') {
        deltas.finish(e.screenX - lastX, e.screenY - lastY);
      } else {
        deltas.flush();
      }
      try {
        dragRegion.releasePointerCapture(e.pointerId);
      } catch {
        // ignore
      }
      bridge.sendMessage({ type: 'drag_end' });
    };

    dragRegion.addEventListener('pointerup', endDrag);
    dragRegion.addEventListener('pointercancel', endDrag);
  }

  // Resize handle handling
  const resizeHandle = document.getElementById('resize-handle');
  if (resizeHandle) {
    let isResizing = false;
    let lastX = 0;
    let lastY = 0;
    const deltas = new PointerDeltaCoalescer((dx, dy) => {
      bridge.sendMessage({
        type: 'resize_update',
        payload: { dx, dy },
      });
    });

    resizeHandle.addEventListener('pointerdown', (e: PointerEvent) => {
      if (e.button !== 0 || !Number.isFinite(e.screenX) || !Number.isFinite(e.screenY)) return;
      e.preventDefault();
      e.stopPropagation();
      isResizing = true;
      lastX = e.screenX;
      lastY = e.screenY;
      deltas.reset();
      resizeHandle.setPointerCapture(e.pointerId);
      bridge.sendMessage({ type: 'resize_start' });
    });

    resizeHandle.addEventListener('pointermove', (e: PointerEvent) => {
      if (!isResizing) return;
      const dx = e.screenX - lastX;
      const dy = e.screenY - lastY;
      lastX = e.screenX;
      lastY = e.screenY;
      deltas.add(dx, dy);
    });

    const endResize = (e: PointerEvent) => {
      if (!isResizing) return;
      isResizing = false;
      if (e.type === 'pointerup') {
        deltas.finish(e.screenX - lastX, e.screenY - lastY);
      } else {
        deltas.flush();
      }
      try {
        resizeHandle.releasePointerCapture(e.pointerId);
      } catch {
        // ignore
      }
      bridge.sendMessage({ type: 'resize_end' });
    };

    resizeHandle.addEventListener('pointerup', endResize);
    resizeHandle.addEventListener('pointercancel', endResize);
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
      noteEditor?.setMarkdown(msg.payload.content || '');
      noteEditor?.focus();
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
