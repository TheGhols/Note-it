import './styles/theme.css';
import { bridge } from './bridge/bridge.ts';
import { ExternalWriteBarrier } from './bridge/externalWrite.ts';
import { NoteEditor } from './editor/editor.ts';
import { NoteKeyboardController } from './editor/keyboard.ts';
import {
  NoteLayerMode,
  PaperColor,
  PaperIntensity,
  PaperType,
  ThemePreference,
} from './bridge/types.ts';
import {
  appendCapture,
  CaptureDelimiter,
  DEFAULT_CAPTURE_DELIMITER,
  normalizeDelimiter,
} from './capture/autoPaste.ts';
import { imageBytesFromTransfer } from './editor/imageTransfer.ts';
import { PointerGestureController } from './geometry/gesture.ts';
import {
  findStatus,
  replaceActive,
  replaceAll,
  setFindQuery,
  stepFind,
} from './editor/find.ts';
import { FindBar } from './ui/findBar.ts';
import { FlashcardPanel } from './ui/flashcardPanel.ts';
import { flashcardCountsIn } from './editor/flashcardMark.ts';
import { StudyHub, StudyFilter } from './ui/studyHub.ts';
import { bindHeaderShortcuts, updateZoomShortcutState } from './ui/headerShortcuts.ts';
import { applyHeaderActionMetadata } from './ui/actionMetadata.ts';
import { buildGlobalCatalog } from './study/catalog.ts';
import { collapseTransition } from './ui/collapse.ts';
import { HeaderReveal } from './ui/headerReveal.ts';
import { CLIPPER, FLASHCARDS, QUICK_ACTIONS, TRASH_SHORTCUT } from './ui/icons.ts';
import { MenuPanel, NoteMenu } from './ui/menu.ts';
import { MetadataPanel, NoteTagStrip } from './ui/metadataPanel.ts';
import { noteTitle } from './ui/noteTitle.ts';
import { SearchPalette } from './ui/searchPalette.ts';
import { bindSearchEntries } from './ui/searchEntry.ts';
import { NoteStatus, SyncIndicator } from './ui/status.ts';
import { finishMessage } from './timer/format.ts';
import { TimerPanel } from './ui/timerPanel.ts';
import { TrashPanel } from './ui/trashPanel.ts';
import {
  applyPaper,
  DEFAULT_PAPER_INTENSITY,
  DEFAULT_PAPER_TYPE,
  normalizePaperIntensity,
  normalizePaperType,
} from './ui/paper.ts';
import { DEFAULT_THEME, normalizeTheme, ThemeController } from './ui/theme.ts';
import { NoteInfoTooltip } from './ui/tooltip.ts';
import { TextSize } from './editor/textSize.ts';
import {
  applyUiScale,
  clampUiScale,
  DEFAULT_UI_SCALE_PERCENT,
  uiScaleIn,
  uiScaleOut,
} from './ui/uiScale.ts';
import {
  clampZoom,
  DEFAULT_ZOOM_PERCENT,
  MAX_ZOOM_PERCENT,
  MIN_ZOOM_PERCENT,
  zoomIn,
  zoomOut,
} from './editor/zoom.ts';

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
let currentUiScale = DEFAULT_UI_SCALE_PERCENT;
let currentLayerMode: NoteLayerMode = 'overlay';
let currentPaperType: PaperType = DEFAULT_PAPER_TYPE;
let currentPaperIntensity: PaperIntensity = DEFAULT_PAPER_INTENSITY;
let currentTheme: ThemePreference = DEFAULT_THEME;
let themeController: ThemeController | null = null;
/** Whether the gesture in progress actually moved the note. */
let dragMoved = false;
let isCollapsed = false;
/** Whether this note is the one the host is capturing the clipboard into. */
let autoPasteActive = false;
let captureDelimiter: CaptureDelimiter = DEFAULT_CAPTURE_DELIMITER;
let noteEditor: NoteEditor | null = null;
let noteMenu: NoteMenu | null = null;
let metadataPanel: MetadataPanel | null = null;
let noteTagStrip: NoteTagStrip | null = null;
let headerReveal: HeaderReveal | null = null;
let infoTooltip: NoteInfoTooltip | null = null;
let searchPalette: SearchPalette | null = null;
let findBar: FindBar | null = null;
let trashPanel: TrashPanel | null = null;
let timerPanel: TimerPanel | null = null;
let flashcardPanel: FlashcardPanel | null = null;
let studyHub: StudyHub | null = null;
let noteStatus: NoteStatus | null = null;
let syncIndicator: SyncIndicator | null = null;
/**
 * The page's half of an external write.
 *
 * Created before the editor exists so nothing can arrive before there is
 * somewhere to hold it, and consulted by every path that changes the document.
 */
let externalWrite: ExternalWriteBarrier | null = null;

/**
 * Opens the global search palette, or brings the caret back to it.
 *
 * The find bar is closed first: two search fields open at once is two places
 * the keyboard could be, and only one of them is the one the reader just asked
 * for.
 */
function openGlobalSearch(): void {
  findBar?.close();
  trashPanel?.close();
  timerPanel?.close();
  flashcardPanel?.close();
  studyHub?.close();
  searchPalette?.openPalette();
}

/**
 * Opens the trash.
 *
 * Everything else that wants the keyboard closes first, for the same reason
 * the find bar closes when global search opens: only one of them can be where
 * the reader is typing, and it is the one they just asked for.
 */
function openTrash(): void {
  searchPalette?.close();
  findBar?.close();
  timerPanel?.close();
  flashcardPanel?.close();
  studyHub?.close();
  trashPanel?.openPanel();
}

function openMetadata(section: 'tags' | 'properties', invoker?: HTMLElement | null): void {
  searchPalette?.close();
  findBar?.close();
  trashPanel?.close();
  timerPanel?.close();
  flashcardPanel?.close();
  studyHub?.close();
  noteMenu?.close();
  metadataPanel?.open(section, invoker ?? document.getElementById('note-tags-line'));
}

/** Opens Find, or Find with the replace row, seeded with the selection. */
function openFindBar(replace: boolean): void {
  searchPalette?.close();
  trashPanel?.close();
  timerPanel?.close();
  flashcardPanel?.close();
  studyHub?.close();
  findBar?.openBar({ replace, seed: noteEditor?.selectedText() });
}

/**
 * Opens the timer panel.
 *
 * Everything that would sit over the same corner of a small note closes first.
 * The note menu closes too, but from the panel's own `onOpen`, because that
 * covers the path a pointer outside it would not: expanding a collapsed note
 * and opening this in the same click.
 */
function openTimer(): void {
  searchPalette?.close();
  findBar?.close();
  trashPanel?.close();
  flashcardPanel?.close();
  studyHub?.close();
  timerPanel?.openPanel();
}

/**
 * Opens studying, with the cards the note holds at this moment.
 *
 * The list is taken here and handed over once. From this point the note goes
 * on being edited and captured into, and the sitting stays as it started —
 * which is the only way a reader on question four is still on the question
 * they were on. The panel is given content and nothing else: it has no editor
 * to write to.
 */
function openStudyHub(filter: StudyFilter, invoker?: HTMLElement | null): void {
  if (!noteEditor || !studyHub || !activeNoteId) return;
  searchPalette?.close();
  findBar?.close();
  trashPanel?.close();
  timerPanel?.close();
  flashcardPanel?.close();
  noteMenu?.close();
  studyHub.openHub(activeNoteId, invoker ?? document.getElementById('btn-menu'), filter);
}

/**
 * Tells the menu how many cards the note holds now.
 *
 * Reading, and only reading: the count comes from the plugin that already
 * recomputed it for the decoration, so knowing it costs one property access
 * and writes nothing anywhere.
 */
function syncFlashcardCounts(): void {
  if (!noteEditor || !noteMenu) return;
  noteMenu.setFlashcardCounts(flashcardCountsIn(noteEditor.getView().state));
}

/** Mirrors how many occurrences there are into the bar. */
function syncFindStatus(): void {
  if (!noteEditor || !findBar) return;
  findBar.setStatus(findStatus(noteEditor.getRawEditor().state));
}

function setPaperColor(color: PaperColor): void {
  document.body.setAttribute('data-color', color);
  noteMenu?.setSelectedColor(color);
}

/**
 * Applies the note's own paper: its pattern and how strongly it is drawn.
 *
 * A property of the note, saved beside its colour in the front matter, so it
 * travels with the note and never touches the document's text or its
 * modification date. Both halves are applied together because the stylesheet
 * reads them as one surface.
 */
function setPaper(type: PaperType, intensity: PaperIntensity, persist: boolean): void {
  const changed = type !== currentPaperType || intensity !== currentPaperIntensity;
  currentPaperType = type;
  currentPaperIntensity = intensity;
  applyPaper(document.body, type, intensity);
  noteMenu?.setPaper(type, intensity);

  if (persist && changed && activeNoteId) {
    bridge.sendMessage({
      type: 'paper_changed',
      payload: { id: activeNoteId, paperType: type, paperIntensity: intensity },
    });
  }
}

/**
 * Applies the shared interface theme.
 *
 * Unlike the paper, this belongs to the application rather than to the note:
 * the host owns it, stores it once and broadcasts it, so every open note
 * agrees. Nothing about the note's own colour changes here.
 */
function setTheme(theme: ThemePreference, persist: boolean): void {
  const changed = theme !== currentTheme;
  currentTheme = theme;
  themeController?.setPreference(theme);
  noteMenu?.setTheme(theme);

  if (persist && changed) {
    bridge.sendMessage({ type: 'theme_changed', payload: { theme } });
  }
}

/**
 * Scales application chrome, never the note document.
 *
 * The host owns and broadcasts this global preference. CSS variables change
 * real control, type and spacing metrics, while the editor keeps its own
 * independent `--note-zoom` and document font-size marks.
 */
function setUiScale(percent: number, persist: boolean): void {
  const clamped = clampUiScale(percent);
  const changed = clamped !== currentUiScale;
  if (persist && changed) {
    // The host commits config.toml before broadcasting. Waiting for that
    // broadcast keeps this WebView honest if the atomic write fails.
    bridge.sendMessage({ type: 'ui_scale_changed', payload: { uiScalePercent: clamped } });
    return;
  }
  if (persist) return;

  currentUiScale = applyUiScale(document.documentElement, document.body, clamped);
  noteMenu?.setUiScalePercent(currentUiScale);
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
  const zoomOutButton = document.getElementById('btn-zoom-out') as HTMLButtonElement | null;
  const zoomInButton = document.getElementById('btn-zoom-in') as HTMLButtonElement | null;
  updateZoomShortcutState(
    zoomOutButton,
    zoomInButton,
    clamped,
    MIN_ZOOM_PERCENT,
    MAX_ZOOM_PERCENT,
  );

  if (persist && changed && activeNoteId) {
    bridge.sendMessage({
      type: 'zoom_changed',
      payload: { id: activeNoteId, zoomPercent: clamped },
    });
  }
}

/**
 * Asks the host for a file chooser and the image chosen in it.
 *
 * The one path, and both ways in end here: the paperclip in the bar and
 * *☰ › Mídia › Inserir imagem…* send the same message and get the same
 * chooser. The host owns the dialog, so the path is one the reader picked
 * rather than one the page named — nothing about that changes because there is
 * now a second button.
 */
function requestImageInsert(): void {
  if (!activeNoteId) return;
  bridge.sendMessage({
    type: 'insert_image_requested',
    payload: { id: activeNoteId },
  });
}

/**
 * Hands the bytes of a pasted or dropped image to the host.
 *
 * The page sends what the gesture gave it rather than naming a file, so there
 * is nothing here the host could be talked into reading. What the bytes
 * actually are is the host's decision, made from the bytes.
 */
function importImage(transfer: DataTransfer): boolean {
  if (!activeNoteId) return false;
  // Reading the bytes is asynchronous; deciding to handle the gesture is not.
  // The editor is told at once that the picture is being dealt with, so the
  // paste or drop does not also land as text, and the document changes when
  // the reference comes back from the host.
  void imageBytesFromTransfer(transfer).then((encoded) => {
    if (encoded === null || !activeNoteId) return;
    bridge.sendMessage({
      type: 'image_bytes_received',
      payload: { id: activeNoteId, data: encoded },
    });
  });
  return true;
}

/**
 * Applies what the host says about this note's capture state.
 *
 * Always pushed, never assumed: the target is exclusive across the
 * application, so a note that has just lost it is told, and a note that asked
 * to have it only shows it once the host has agreed. Nothing here reads a
 * clipboard — the page has no part in observing one.
 *
 * The chrome is held out while capturing, which is the persistent sign that a
 * mode watching every copy is running. The bar paints its own paper over the
 * gutter, so keeping it out covers no line of the note.
 */
function setAutoPaste(active: boolean, delimiter: CaptureDelimiter): void {
  autoPasteActive = active;
  captureDelimiter = delimiter;
  noteMenu?.setAutoPaste(active, delimiter);
  headerReveal?.setCapturing(active);

  const indicator = document.getElementById('btn-autopaste');
  if (indicator) {
    indicator.hidden = !active;
    indicator.setAttribute('aria-pressed', String(active));
  }
  document.body.setAttribute('data-autopaste', String(active));
}

/**
 * One clipboard capture, appended to the end of this note.
 *
 * Deliberately passive: no focus, no selection change, no scroll, no window
 * raised. The reader is in another application — that is the whole point of
 * the mode — so the note takes the text and stays exactly where it was.
 *
 * A capture is a real edit, so it goes through the ordinary update path: the
 * editor's own `onUpdate` debounce sends `content_changed` and the existing
 * autosave writes the note. There is no second save channel here.
 */
function applyCapture(text: string): void {
  if (!autoPasteActive) return;
  // A capture happens while the reader is in another application, so it is the
  // one edit nobody is watching arrive. Dropping it because a write happened
  // to be in flight would lose something they can never get back — the
  // clipboard has moved on. It waits and is filed the moment editing resumes.
  deferDocumentEdit(() => {
    const view = noteEditor?.getView();
    if (!view || !autoPasteActive) return;
    appendCapture(view, text, captureDelimiter);
  });
}

function setLayerMode(mode: NoteLayerMode): void {
  currentLayerMode = mode;
  noteMenu?.setLayerMode(mode);
}

/** Mirrors the formatting and the block under the cursor into the menu. */
function syncInlineFormatting(): void {
  if (!noteEditor || !noteMenu) return;
  noteMenu.setInlineFormatting({
    textSize: noteEditor.currentTextSize(),
    textSizeMixed: noteEditor.hasMixedTextSize(),
    textColor: noteEditor.currentTextColor(),
    highlight: noteEditor.currentHighlight(),
  });
  noteMenu.setBlockState(noteEditor.currentBlock());
}

function applyTextSize(size: TextSize | null): void {
  noteEditor?.setTextSize(size);
  syncInlineFormatting();
}

/** Mirrors content into the collapsed label without ever sending it anywhere. */
function setNoteTitle(markdown: string): void {
  const title = document.getElementById('note-title');
  if (title) {
    const label = noteTitle(markdown);
    title.textContent = label;
    title.title = label;
  }
}

/**
 * Applies the collapsed look. The editor is only hidden, never destroyed, so
 * the content and the Tiptap instance survive untouched.
 */
function setCollapsed(collapsed: boolean): void {
  const wasCollapsed = isCollapsed;
  isCollapsed = collapsed;
  if (collapsed) setNoteTitle(noteEditor?.getMarkdown() ?? '');
  document.body.setAttribute('data-collapsed', String(collapsed));
  noteMenu?.setCollapsed(collapsed);
  // The bar is the whole of a collapsed note, so it stops hiding itself.
  headerReveal?.setCollapsed(collapsed);
  // A collapsed note is a header bar, and expanding one has to give the reader
  // the text back. Both halves of that are stated in `collapseTransition`.
  const transition = collapseTransition(wasCollapsed, collapsed);
  if (transition.closePanels) {
    searchPalette?.close();
    findBar?.close();
    trashPanel?.close();
    // A collapsed note is a header bar: there is no room to study in, and the
    // sitting is not worth keeping — the cards are still in the note.
    flashcardPanel?.close();
    studyHub?.close();
    // The popover goes; the countdown does not. A collapsed note keeps its
    // timer running and keeps showing it on the bar — that is the whole reason
    // the readout is up there rather than in the panel.
    timerPanel?.close();
    noteStatus?.hide();
  }
  if (transition.restoreCaret) noteEditor?.focus();
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
  } else if (target?.closest('#btn-timer')) {
    afterViewportGrows(openTimer);
  }
}

/** The run of the document everything the page sends is composed against. */
function currentGeneration(): number {
  return externalWrite?.currentGeneration() ?? 0;
}

/**
 * Runs an edit now, or holds it until an external write has finished.
 *
 * Returns true when it was held. The barrier exists before anything can send
 * an edit, so the fallback is only ever taken in a page that has not finished
 * starting; it runs the action rather than dropping it, because losing an edit
 * is never the safer half of that choice.
 */
function deferDocumentEdit(action: () => void): boolean {
  if (!externalWrite) {
    action();
    return false;
  }
  return externalWrite.defer(action);
}

function flushSave(): void {
  // While the document is held still there is nothing to flush: the text has
  // already been handed to the host, and sending it again under the old
  // generation would only be refused.
  if (externalWrite?.active) return;
  if (activeNoteId && noteEditor) {
    const markdown = noteEditor.getMarkdown();
    noteEditor.cancelPendingSave();
    bridge.sendMessage({
      type: 'content_changed',
      payload: { id: activeNoteId, content: markdown, generation: currentGeneration() },
    });
  }
}

function saveAndClose(): void {
  if (!activeNoteId || !noteEditor) return;
  // Closing during an external write would race the commit. It is held until
  // the document is released, and then closes normally.
  if (deferDocumentEdit(() => saveAndClose())) return;
  const content = noteEditor.getMarkdown();
  noteEditor.cancelPendingSave();
  bridge.sendMessage({
    type: 'save_and_close',
    payload: { id: activeNoteId, content, generation: currentGeneration() },
  });
}

function initUI(): void {
  const editorContainer = document.getElementById('editor-container');
  if (!editorContainer) return;

  // Created before anything can ask for a theme, and kept for the lifetime of
  // the page: under "Sistema" it watches the environment, so a desktop
  // switching to dark reaches an open note without a restart.
  themeController = new ThemeController(document.documentElement);
  applyHeaderActionMetadata(document);

  // Built before the editor, so nothing can arrive with nowhere to be held.
  // Every hook here is about the *document* and only the document: the window,
  // the rendering and the rest of the chrome carry on untouched for the length
  // of a write.
  externalWrite = new ExternalWriteBarrier({
    freeze: () => noteEditor?.setEditable(false),
    thaw: () => noteEditor?.setEditable(true),
    snapshot: () => noteEditor?.getMarkdown() ?? '',
    adopt: (committed) => {
      // `setMarkdown` replaces the document without emitting an update, so
      // adopting the committed note does not turn straight round and autosave
      // the text that has just been superseded.
      noteEditor?.setMarkdown(committed.content);
      setNoteTitle(committed.content);
      metadataPanel?.setMetadata(committed.metadata);
      noteTagStrip?.setMetadata(committed.metadata, window.innerWidth, window.innerHeight);
      infoTooltip?.setTimestamps({
        createdAt: committed.createdAt,
        updatedAt: committed.updatedAt,
      });
      syncFlashcardCounts();
    },
    send: (message) => bridge.sendMessage(message),
    indicate: (active) => syncIndicator?.setActive(active),
    setTimer: (callback, ms) => window.setTimeout(callback, ms),
    clearTimer: (handle) => window.clearTimeout(handle),
  });

  noteEditor = new NoteEditor({
    element: editorContainer,
    initialContent: '',
    onImageTransfer: importImage,
    // Live, per keystroke: a card exists the moment its delimiter is finished
    // and stops existing the moment it is taken out.
    onDocChange: syncFlashcardCounts,
    onUpdate: (markdown) => {
      setNoteTitle(markdown);
      // The editor is not editable while the document is held, so this cannot
      // normally fire then; if anything ever changes the document
      // programmatically it must still not be written under the old
      // generation.
      if (activeNoteId && !externalWrite?.active) {
        bridge.sendMessage({
          type: 'content_changed',
          payload: { id: activeNoteId, content: markdown, generation: currentGeneration() },
        });
      }
    },
  });

  const dragRegion = document.querySelector('.drag-region') as HTMLElement | null;

  // The chrome is laid over the paper, so something has to decide when it is
  // on show. Created before the menu, which holds it out while it is open.
  const noteHeader = document.getElementById('drag-handle');
  if (noteHeader) {
    headerReveal = new HeaderReveal({ header: noteHeader, body: document.body });
  }

  // Note settings menu. The trigger and the popover both sit outside the drag
  // region, so interacting with them can never move the window.
  const btnMenu = document.getElementById('btn-menu');
  const menuMount = document.getElementById('note-controls-left');
  // The six header actions, each bound to the panel the menu already builds.
  // Nothing here creates a second way of changing anything.
  const quickTriggers: Partial<Record<MenuPanel, HTMLElement>> = {};
  for (const action of QUICK_ACTIONS) {
    const button = document.getElementById(action.buttonId);
    if (button) quickTriggers[action.panel] = button;
  }

  bindSearchEntries([
    document.getElementById('btn-search'),
    document.getElementById('btn-search-pill'),
  ], openGlobalSearch);
  // The AutoPaste indicator opens the panel that switches it off. A second way
  // in, never a second implementation — the same rule the quick actions
  // follow.
  const autoPasteIndicator = document.getElementById('btn-autopaste');
  if (autoPasteIndicator) quickTriggers.capture = autoPasteIndicator;

  // The paperclip is not one of the quick triggers: it opens no panel. It runs
  // the same request the menu entry runs, and that is the whole of it.
  const clipper = document.getElementById(CLIPPER.buttonId);
  clipper?.addEventListener('pointerdown', (event) => event.stopPropagation());
  clipper?.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    requestImageInsert();
  });

  const flashcards = document.getElementById(FLASHCARDS.buttonId);
  const zoomOutButton = document.getElementById('btn-zoom-out') as HTMLButtonElement | null;
  const zoomInButton = document.getElementById('btn-zoom-in') as HTMLButtonElement | null;
  if (btnMenu && menuMount) {
    metadataPanel = new MetadataPanel(menuMount, {
      requestCatalog: (requestId) => {
        bridge.sendMessage({ type: 'metadata_catalog_requested', payload: { requestId } });
      },
      requestSuggestions: (requestId, kind, query) => {
        bridge.sendMessage({
          type: 'metadata_suggestions_requested',
          payload: { requestId, kind, query },
        });
      },
      save: (requestId, draft) => {
        if (!activeNoteId || !noteEditor) return;
        // Metadata travels with the body it was composed against, so saving it
        // in the middle of an external write would carry the pre-commit text
        // back with the tag. It waits instead — nothing is lost and nothing
        // arrives late enough to undo the commit.
        if (
          deferDocumentEdit(() => {
            if (!activeNoteId || !noteEditor) return;
            bridge.sendMessage({
              type: 'metadata_changed',
              payload: {
                requestId,
                id: activeNoteId,
                content: noteEditor.getMarkdown(),
                generation: currentGeneration(),
                tags: draft.tags,
                properties: draft.properties,
              },
            });
          })
        ) {
          return;
        }
        noteEditor.cancelPendingSave();
        bridge.sendMessage({
          type: 'metadata_changed',
          payload: {
            requestId,
            id: activeNoteId,
            content: noteEditor.getMarkdown(),
            generation: currentGeneration(),
            tags: draft.tags,
            properties: draft.properties,
          },
        });
      },
      onOpen: () => {
        infoTooltip?.hide();
        headerReveal?.setHeld(true);
      },
      onClose: () => headerReveal?.setHeld(false),
    });
    const tagsLine = document.getElementById('note-tags-line');
    if (tagsLine) {
      noteTagStrip = new NoteTagStrip(tagsLine, () => openMetadata('tags', tagsLine));
      window.addEventListener('resize', () =>
        noteTagStrip?.render(window.innerWidth, window.innerHeight),
      );
    }
    noteMenu = new NoteMenu({
      trigger: btnMenu,
      mount: menuMount,
      colors: PAPER_COLORS,
      quickTriggers,
      handlers: {
        onOpen: () => {
          infoTooltip?.hide();
          // The header stays reachable while Study fills the note. Opening the
          // menu is therefore also an explicit end to that sitting; otherwise
          // the two panels would be stacked over the same surface.
          flashcardPanel?.close();
          studyHub?.close();
          headerReveal?.setHeld(true);
          syncInlineFormatting();
        },
        onClose: () => headerReveal?.setHeld(false),
        onSelectColor: (color) => {
          setPaperColor(color);
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'color_changed',
              payload: { id: activeNoteId, color },
            });
          }
        },
        onSelectPaperType: (type) => setPaper(type, currentPaperIntensity, true),
        onSelectPaperIntensity: (intensity) => setPaper(currentPaperType, intensity, true),
        onSelectTheme: (theme) => setTheme(theme, true),
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
        onUiScaleIn: () => setUiScale(uiScaleIn(currentUiScale), true),
        onUiScaleOut: () => setUiScale(uiScaleOut(currentUiScale), true),
        onResetUiScale: () => setUiScale(DEFAULT_UI_SCALE_PERCENT, true),
        onSelectLayerMode: (mode) => {
          // The host owns the shared mode; ask only when it would change.
          if (mode !== currentLayerMode) {
            bridge.sendMessage({ type: 'toggle_layer_mode' });
          }
        },
        onToggleCodeBlock: () => {
          noteEditor?.toggleCodeBlock();
          syncInlineFormatting();
        },
        onSelectCodeLanguage: (language) => {
          noteEditor?.setCodeLanguage(language);
          syncInlineFormatting();
        },
        onToggleBlockquote: () => {
          noteEditor?.toggleBlockquote();
          syncInlineFormatting();
        },
        onSelectCallout: (type) => {
          noteEditor?.setCallout(type);
          syncInlineFormatting();
        },
        onInsertComment: () => {
          noteEditor?.insertComment();
          syncInlineFormatting();
        },
        onOpenGlobalSearch: openGlobalSearch,
        onOpenFind: () => openFindBar(false),
        onOpenReplace: () => openFindBar(true),
        onTrashNote: () => {
          // The reader has confirmed. The host still flushes this note and
          // only moves the file once that has succeeded, so nothing here has
          // to be undone if the save fails: the note simply stays.
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'trash_note_requested',
              payload: { id: activeNoteId },
            });
          }
        },
        onOpenTrash: openTrash,
        onCreateBackup: () => bridge.sendMessage({ type: 'backup_requested' }),
        // The menu item is hidden as soon as it delegates. Return focus to the
        // still-visible menu button when the metadata panel closes.
        onOpenMetadata: () => openMetadata('tags', btnMenu),
        onInsertImage: requestImageInsert,
        onOpenStudy: () => openStudyHub('current', document.getElementById('btn-menu')),
        onOpenStudyHub: () => openStudyHub('review', document.getElementById('btn-menu')),
        onToggleAutoPaste: (active) => {
          // A request, not a decision: the host owns the single target, and
          // the answer comes back as `set_auto_paste` to every note affected.
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'auto_paste_requested',
              payload: { id: activeNoteId, active },
            });
          }
        },
        onSelectCaptureDelimiter: (delimiter) => {
          bridge.sendMessage({
            type: 'capture_delimiter_changed',
            payload: { delimiter },
          });
        },
      },
    });
  }

  const trashShortcut = document.getElementById(TRASH_SHORTCUT.buttonId);
  bindHeaderShortcuts(
    {
      study: flashcards,
      zoomOut: zoomOutButton,
      zoomIn: zoomInButton,
      trash: trashShortcut,
    },
    {
      openStudyHub: (invoker) => openStudyHub('review', invoker),
      zoomOut: () => applyZoom(zoomOut(currentZoom), true),
      zoomIn: () => applyZoom(zoomIn(currentZoom), true),
      openTrashConfirmation: (invoker) => {
        flashcardPanel?.close();
        studyHub?.close();
        noteMenu?.openTrashConfirmation(invoker);
      },
    },
  );

  // The note's Timer and Pomodoro. Anchored under its own header button, in
  // the same group the menu is mounted in, so it sits outside the drag region
  // and a click on it can never move the window.
  //
  // It is created whether or not anything ever opens it, because the countdown
  // is not the panel's: a restored timer has to keep running, keep the header
  // readout in step and be able to finish while the popover is shut.
  const btnTimer = document.getElementById('btn-timer');
  const timerReadout = document.getElementById('note-timer-readout');
  if (btnTimer && timerReadout && menuMount) {
    timerPanel = new TimerPanel({
      trigger: btnTimer,
      readout: timerReadout,
      mount: menuMount,
      handlers: {
        // Operational state, and only on a real change: a start, a pause, a
        // resume, a cancel, a phase change or a completion. Never a tick, so a
        // running timer writes nothing once a second — and never a
        // `content_changed`, so the note's Markdown and its modification date
        // are untouched by any of this.
        onPersist: (snapshot) => {
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'timer_changed',
              payload: { id: activeNoteId, timer: snapshot },
            });
          }
        },
        onFinished: (kind) => {
          // Once, because the engine transitions once. The host turns the kind
          // into the desktop notification; the line at the foot of the note is
          // the signal that does not depend on a notification daemon existing.
          if (activeNoteId) {
            bridge.sendMessage({
              type: 'timer_finished',
              payload: { id: activeNoteId, kind },
            });
          }
          if (!isCollapsed) noteStatus?.show(finishMessage(kind), true);
        },
        onOpen: () => {
          infoTooltip?.hide();
          noteMenu?.close();
          headerReveal?.setHeld(true);
        },
        onClose: () => headerReveal?.setHeld(false),
      },
    });
  }

  // Search across every note, and search inside this one. Both live in the
  // page rather than in a second window: a window would be another layer-shell
  // surface to place, stack and tear down for something that disappears when
  // it is closed.
  const appRoot = document.getElementById('app');
  if (appRoot) {
    searchPalette = new SearchPalette({
      mount: appRoot,
      handlers: {
        onQuery: (requestId, query) => {
          // Reading only. Nothing here saves, flushes or touches a timestamp.
          bridge.sendMessage({ type: 'search_requested', payload: { requestId, query } });
        },
        onOpen: (noteId, query) => {
          bridge.sendMessage({ type: 'open_search_result', payload: { noteId, query } });
        },
        onClose: () => noteEditor?.focus(),
      },
    });

    studyHub = new StudyHub({
      mount: appRoot,
      handlers: {
        onRequestCatalog: (requestId) => {
          bridge.sendMessage({ type: 'study_catalog_requested', payload: { requestId } });
        },
        onStart: (items, cards, schema) => {
          studyHub?.close();
          flashcardPanel?.openPanel({
            items,
            cards,
            schema,
            invoker: document.getElementById('btn-flashcards'),
          });
        },
        onClose: () => noteEditor?.focus(),
      },
    });

    // One renderer for current-note and global study. It holds no editor and
    // advances after, and only after, the host commits a rating.
    flashcardPanel = new FlashcardPanel({
      mount: appRoot,
      handlers: {
        onClose: () => noteEditor?.focus(),
        onRate: ({ requestId, reviewKey, rating }) => {
          bridge.sendMessage({
            type: 'study_rating_requested',
            payload: { requestId, reviewKey, rating },
          });
        },
        onReturnToHub: () =>
          openStudyHub('review', document.getElementById('btn-flashcards')),
      },
    });

    trashPanel = new TrashPanel({
      mount: appRoot,
      handlers: {
        onList: (requestId) => {
          // Reading only. Opening the trash saves nothing and moves no date.
          bridge.sendMessage({ type: 'trash_list_requested', payload: { requestId } });
        },
        onRestore: (noteId) => {
          bridge.sendMessage({ type: 'restore_note_requested', payload: { noteId } });
        },
        onClose: () => noteEditor?.focus(),
      },
    });

    noteStatus = new NoteStatus({ mount: appRoot });
    syncIndicator = new SyncIndicator(appRoot);

    findBar = new FindBar({
      mount: appRoot,
      handlers: {
        onQuery: (query, caseSensitive) => {
          const view = noteEditor?.getView();
          if (view) setFindQuery(view, query, caseSensitive);
          syncFindStatus();
        },
        onStep: (step) => {
          const view = noteEditor?.getView();
          if (view) stepFind(view, step);
          syncFindStatus();
        },
        onReplaceOne: (replacement) => {
          const view = noteEditor?.getView();
          if (view) replaceActive(view, replacement);
          syncFindStatus();
        },
        onReplaceAll: (replacement) => {
          const view = noteEditor?.getView();
          if (view) replaceAll(view, replacement);
          syncFindStatus();
        },
        onClose: () => noteEditor?.focus(),
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
    openGlobalSearch,
    openFind: () => openFindBar(false),
    openReplace: () => openFindBar(true),
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
      // The run this document starts on. Everything the page sends back
      // quotes it, and the host refuses anything quoting an older one.
      externalWrite?.setGeneration(msg.payload.generation ?? 0);
      setPaperColor(msg.payload.color);
      setPaper(
        normalizePaperType(msg.payload.paperType),
        normalizePaperIntensity(msg.payload.paperIntensity),
        false,
      );
      setTheme(normalizeTheme(msg.payload.theme), false);
      setUiScale(clampUiScale(msg.payload.uiScalePercent), false);
      setFontSize(msg.payload.fontSize || 15);
      metadataPanel?.setMetadata(msg.payload.metadata);
      noteTagStrip?.setMetadata(msg.payload.metadata, window.innerWidth, window.innerHeight);
      applyZoom(msg.payload.zoomPercent ?? DEFAULT_ZOOM_PERCENT, false);
      setLayerMode(msg.payload.layerMode ?? 'overlay');
      infoTooltip?.setTimestamps({
        createdAt: msg.payload.createdAt ?? null,
        updatedAt: msg.payload.updatedAt ?? null,
      });
      // A note that has just been loaded is not mid-search: whatever was open
      // belonged to whatever was there before.
      searchPalette?.close();
      findBar?.close();
      trashPanel?.close();
      timerPanel?.close();
      flashcardPanel?.close();
      studyHub?.close();
      // The stored timer, resolved against the clock as it is now rather than
      // resumed for whatever was left when this note was last on screen.
      timerPanel?.restore(msg.payload.timer ?? null);
      // A WebView that has just been created is never the capture target: the
      // mode does not survive the destruction of the previous one, and there
      // is no field on this message that could bring it back.
      setAutoPaste(false, normalizeDelimiter(msg.payload.captureDelimiter));
      noteStatus?.hide();
      noteEditor?.setMarkdown(msg.payload.content || '');
      setNoteTitle(msg.payload.content || '');
      setCollapsed(Boolean(msg.payload.collapsed));
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
    } else if (msg.type === 'set_auto_paste') {
      setAutoPaste(Boolean(msg.payload.active), normalizeDelimiter(msg.payload.delimiter));
    } else if (msg.type === 'image_inserted') {
      // The picture is in the store and this is how the note refers to it.
      // Inserting it is an ordinary edit: the editor's own update path sends
      // `content_changed` and the existing autosave writes the note. If a
      // write is holding the document, the insertion waits rather than being
      // lost — the file is already in the store either way, and a reference
      // dropped here would leave it there with nothing pointing at it.
      const src = msg.payload.src;
      deferDocumentEdit(() => noteEditor?.insertImage(src));
    } else if (msg.type === 'image_import_failed') {
      // A line at the foot of the note, not a dialog over it.
      noteStatus?.show(msg.payload.message, false);
    } else if (msg.type === 'auto_paste_captured') {
      applyCapture(msg.payload.text);
    } else if (msg.type === 'set_theme') {
      // A theme chosen from another note's menu.
      setTheme(normalizeTheme(msg.payload.theme), false);
    } else if (msg.type === 'set_ui_scale') {
      // A scale chosen from another note's menu.
      setUiScale(clampUiScale(msg.payload.uiScalePercent), false);
    } else if (msg.type === 'set_font_size') {
      setFontSize(msg.payload.fontSize);
    } else if (msg.type === 'search_results') {
      searchPalette?.showResults(msg.payload.requestId, msg.payload.results);
    } else if (msg.type === 'trash_entries') {
      trashPanel?.showEntries(msg.payload.requestId, msg.payload.entries);
    } else if (msg.type === 'study_catalog_result') {
      const { requestId, notes, studyState, error } = msg.payload;
      if (error || !studyState) {
        studyHub?.showError(requestId, error ?? 'O histórico de estudos está indisponível.');
        return;
      }
      const current =
        activeNoteId && noteEditor
          ? { id: activeNoteId, content: noteEditor.getMarkdown() }
          : null;
      void buildGlobalCatalog(notes, studyState, current)
        .then((catalog) => studyHub?.showCatalog(requestId, catalog, studyState))
        .catch(() => {
          studyHub?.showError(requestId, 'Não foi possível montar a Central de estudos.');
        });
    } else if (msg.type === 'study_rating_result') {
      const accepted = flashcardPanel?.resolveRating(
        msg.payload.requestId,
        msg.payload.reviewKey,
        msg.payload.ok,
        msg.payload.message,
      );
      if (accepted && msg.payload.ok && msg.payload.studyState) {
        studyHub?.updateStudyState(msg.payload.studyState);
      }
    } else if (msg.type === 'metadata_catalog_result') {
      metadataPanel?.setCatalog(msg.payload.requestId, msg.payload.catalog);
    } else if (msg.type === 'metadata_save_result') {
      metadataPanel?.resolveSave(
        msg.payload.requestId,
        msg.payload.ok,
        msg.payload.message,
        msg.payload.metadata,
      );
      if (msg.payload.ok) {
        noteTagStrip?.setMetadata(msg.payload.metadata, window.innerWidth, window.innerHeight);
        noteStatus?.show(msg.payload.message, true);
      }
    } else if (msg.type === 'metadata_suggestions_result') {
      metadataPanel?.setSuggestions(msg.payload.requestId, msg.payload.suggestions);
    } else if (msg.type === 'data_result') {
      // The sentence arrives ready to show; the page never composes one.
      noteStatus?.show(msg.payload.message, msg.payload.ok);
      if (msg.payload.action === 'restore') {
        trashPanel?.setStatus(msg.payload.message);
        // A note that came back is not in the trash any more.
        if (msg.payload.ok) trashPanel?.refresh();
      }
    } else if (msg.type === 'search_result_missing') {
      searchPalette?.reportMissing(msg.payload.noteId);
    } else if (msg.type === 'reveal_match') {
      // The host has brought this note to the front and is saying what was
      // being looked for. Only the editor can turn a query into a position in
      // its own document, which is why the query travelled and not an offset.
      //
      // The bar opens without taking the keyboard: the occurrences are
      // highlighted and the first one is revealed, and the reader carries on
      // editing where they landed rather than having to dismiss something
      // first.
      searchPalette?.close();
      trashPanel?.close();
      findBar?.openBar({ replace: false, seed: msg.payload.query, focus: false });
      syncFindStatus();
      noteEditor?.focus();
    } else if (msg.type === 'request_content') {
      if (activeNoteId && noteEditor) {
        bridge.sendMessage({
          type: 'content_changed',
          payload: {
            id: activeNoteId,
            content: noteEditor.getMarkdown(),
            generation: currentGeneration(),
          },
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
          generation: currentGeneration(),
        },
      });
    } else if (msg.type === 'begin_external_write') {
      // Something outside this window is about to change the note. The
      // document stops being editable *before* its text is read; see
      // `ExternalWriteBarrier`.
      if (activeNoteId) {
        externalWrite?.begin(activeNoteId, msg.payload.requestId, msg.payload.generation);
      }
    } else if (msg.type === 'apply_external_document') {
      externalWrite?.apply(msg.payload.requestId, msg.payload.generation, {
        content: msg.payload.content,
        metadata: msg.payload.metadata,
        createdAt: msg.payload.createdAt ?? null,
        updatedAt: msg.payload.updatedAt ?? null,
      });
    } else if (msg.type === 'abort_external_write') {
      externalWrite?.abort(msg.payload.requestId);
    }
  });

  // Notify Host that Webview is ready
  bridge.sendMessage({ type: 'ready' });
}

document.addEventListener('DOMContentLoaded', initUI);
