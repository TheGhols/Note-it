import './styles/theme.css';
import { bridge } from './bridge/bridge.ts';
import { NoteEditor } from './editor/editor.ts';
import { PaperColor } from './bridge/types.ts';

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

    dragRegion.addEventListener('pointerdown', (e: PointerEvent) => {
      if (e.button !== 0) return;
      isDragging = true;
      lastX = e.screenX;
      lastY = e.screenY;
      dragRegion.setPointerCapture(e.pointerId);
      bridge.sendMessage({ type: 'drag_start' });
    });

    dragRegion.addEventListener('pointermove', (e: PointerEvent) => {
      if (!isDragging) return;
      const dx = e.screenX - lastX;
      const dy = e.screenY - lastY;
      lastX = e.screenX;
      lastY = e.screenY;
      if (dx !== 0 || dy !== 0) {
        bridge.sendMessage({
          type: 'drag_update',
          payload: { dx, dy },
        });
      }
    });

    const endDrag = (e: PointerEvent) => {
      if (!isDragging) return;
      isDragging = false;
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

    resizeHandle.addEventListener('pointerdown', (e: PointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      isResizing = true;
      lastX = e.screenX;
      lastY = e.screenY;
      resizeHandle.setPointerCapture(e.pointerId);
      bridge.sendMessage({ type: 'resize_start' });
    });

    resizeHandle.addEventListener('pointermove', (e: PointerEvent) => {
      if (!isResizing) return;
      const dx = e.screenX - lastX;
      const dy = e.screenY - lastY;
      lastX = e.screenX;
      lastY = e.screenY;
      if (dx !== 0 || dy !== 0) {
        bridge.sendMessage({
          type: 'resize_update',
          payload: { dx, dy },
        });
      }
    });

    const endResize = (e: PointerEvent) => {
      if (!isResizing) return;
      isResizing = false;
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

  // Keyboard Shortcuts inside Webview
  window.addEventListener('keydown', (e) => {
    if (e.ctrlKey || e.metaKey) {
      if (e.key === 'n' || e.key === 'N') {
        e.preventDefault();
        flushSave();
        bridge.sendMessage({ type: 'new_note_requested' });
      } else if (e.key === 'w' || e.key === 'W') {
        e.preventDefault();
        saveAndClose();
      } else if (e.key === '+' || e.key === '=') {
        e.preventDefault();
        setFontSize(currentFontSize + 1);
        if (activeNoteId) {
          bridge.sendMessage({
            type: 'font_size_changed',
            payload: { id: activeNoteId, fontSize: currentFontSize },
          });
        }
      } else if (e.key === '-' || e.key === '_') {
        e.preventDefault();
        setFontSize(currentFontSize - 1);
        if (activeNoteId) {
          bridge.sendMessage({
            type: 'font_size_changed',
            payload: { id: activeNoteId, fontSize: currentFontSize },
          });
        }
      }
    }
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
