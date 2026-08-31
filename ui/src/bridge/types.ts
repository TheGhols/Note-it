import { CaptureDelimiter } from '../capture/autoPaste.ts';
import { TimerFinishKind, TimerSnapshot } from '../timer/engine.ts';
import type { StudyCatalogNote, StudyRating, StudyState } from '../study/types.ts';

export type PaperColor = 'yellow' | 'blue' | 'green' | 'pink' | 'purple' | 'gray' | 'black';

/** Background pattern of a note's paper. A property of the note. */
export type PaperType = 'blank' | 'lined' | 'dotted' | 'grid-small' | 'grid-large';

/** How strongly that pattern is drawn. Also a property of the note. */
export type PaperIntensity = 'subtle' | 'normal' | 'strong';

/**
 * Appearance of the application's own chrome, shared by every note. Distinct
 * from the paper: a theme never repaints a note's colour or pattern.
 */
export type ThemePreference = 'system' | 'light' | 'dark';

export type NoteLayerMode = 'overlay' | 'desktop' | 'hidden';

export interface NoteData {
  id: string;
  content: string;
  color: PaperColor;
  paperType: PaperType;
  paperIntensity: PaperIntensity;
  fontSize: number;
  collapsed: boolean;
  createdAt: string | null;
  updatedAt: string | null;
  zoomPercent: number;
  layerMode: NoteLayerMode;
  theme: ThemePreference;
  /** Global application-chrome scale, independent of this note's zoom. */
  uiScalePercent: number;
  /**
   * The note's Timer or Pomodoro, or `null` when it has none.
   *
   * A running one arrives as the instant it ends rather than as what was left
   * when the WebView went away, so the page works the remainder out against
   * the clock as it is now: a note reopened ten minutes later shows the ten
   * minutes that really went by, and one reopened past its deadline comes back
   * finished rather than counting through zero.
   */
  timer: TimerSnapshot | null;
  /**
   * What AutoPaste would put between the note's content and a capture.
   *
   * A preference, so it travels with the note that is opening. Whether
   * AutoPaste is *on* deliberately does not travel here and is never restored:
   * a WebView that has just been created is never the capture target.
   */
  captureDelimiter: CaptureDelimiter;
}

/** One note that matched, exactly as the host sends it. */
export interface SearchResult {
  /** What every action addresses. Never a path, never the label. */
  noteId: string;
  label: string;
  snippet: string;
  matchCount: number;
  /** The first occurrence as the note spells it. Global search folds accents
   *  and the editor's own find does not, so this is what the opened note is
   *  told to look for. */
  matchedText: string;
}

/** One note in the trash, exactly as the host sends it. */
export interface TrashEntry {
  /** What every action addresses. Never a path, never the label. */
  noteId: string;
  label: string;
  snippet: string;
  /** When it was moved to the trash, or `null` when nothing readable says. */
  deletedAt: string | null;
}

/** Which data action a `data_result` is about. */
export type DataAction = 'trash' | 'restore' | 'backup';

export type HostToWebviewMessage =
  | { type: 'load_note'; payload: NoteData }
  | { type: 'set_timestamps'; payload: { createdAt: string | null; updatedAt: string | null } }
  | { type: 'set_layer_mode'; payload: { layerMode: NoteLayerMode } }
  | { type: 'set_collapsed'; payload: { collapsed: boolean } }
  | { type: 'set_color'; payload: { color: PaperColor } }
  | { type: 'set_theme'; payload: { theme: ThemePreference } }
  | { type: 'set_ui_scale'; payload: { uiScalePercent: number } }
  /** Whether this note is the AutoPaste target, and how a capture is laid out. */
  | { type: 'set_auto_paste'; payload: { active: boolean; delimiter: CaptureDelimiter } }
  /** One clipboard capture, on its way to the editor. Text and nothing else. */
  | { type: 'auto_paste_captured'; payload: { text: string } }
  /** An image is in the store; this is how the note refers to it. */
  | { type: 'image_inserted'; payload: { src: string } }
  /** An image could not be taken in. Already the sentence to show. */
  | { type: 'image_import_failed'; payload: { message: string } }
  | { type: 'set_font_size'; payload: { fontSize: number } }
  | { type: 'search_results'; payload: { requestId: number; results: SearchResult[] } }
  | { type: 'search_result_missing'; payload: { noteId: string } }
  | { type: 'reveal_match'; payload: { query: string } }
  | { type: 'trash_entries'; payload: { requestId: number; entries: TrashEntry[] } }
  | { type: 'data_result'; payload: { action: DataAction; ok: boolean; message: string } }
  | {
      type: 'study_catalog_result';
      payload: {
        requestId: number;
        notes: StudyCatalogNote[];
        studyState: StudyState | null;
        error: string | null;
      };
    }
  | {
      type: 'study_rating_result';
      payload: {
        requestId: number;
        reviewKey: string;
        ok: boolean;
        studyState: StudyState | null;
        message: string;
      };
    }
  | { type: 'request_content' }
  | { type: 'request_save_and_close' }
  | { type: 'request_flush'; payload: { requestId: number } };

export type WebviewToHostMessage =
  | { type: 'ready' }
  | { type: 'content_changed'; payload: { id: string; content: string } }
  | { type: 'save_and_close'; payload: { id: string; content: string } }
  | { type: 'new_note_requested' }
  | { type: 'color_changed'; payload: { id: string; color: PaperColor } }
  | { type: 'font_size_changed'; payload: { id: string; fontSize: number } }
  | {
      type: 'paper_changed';
      payload: { id: string; paperType: PaperType; paperIntensity: PaperIntensity };
    }
  | { type: 'theme_changed'; payload: { theme: ThemePreference } }
  | { type: 'ui_scale_changed'; payload: { uiScalePercent: number } }
  | { type: 'collapse_changed'; payload: { id: string; collapsed: boolean } }
  | { type: 'zoom_changed'; payload: { id: string; zoomPercent: number } }
  | { type: 'toggle_layer_mode' }
  | { type: 'search_requested'; payload: { requestId: number; query: string } }
  | { type: 'open_search_result'; payload: { noteId: string; query: string } }
  | { type: 'trash_note_requested'; payload: { id: string } }
  | { type: 'trash_list_requested'; payload: { requestId: number } }
  | { type: 'restore_note_requested'; payload: { noteId: string } }
  | { type: 'backup_requested' }
  | { type: 'study_catalog_requested'; payload: { requestId: number } }
  | {
      type: 'study_rating_requested';
      payload: { requestId: number; reviewKey: string; rating: StudyRating };
    }
  /** Asks the host for a file chooser and the image chosen in it. */
  | { type: 'insert_image_requested'; payload: { id: string } }
  /** Bytes of a pasted or dropped image, base64 for the length of one message. */
  | { type: 'image_bytes_received'; payload: { id: string; data: string } }
  /** Asks the host to capture into this note, or to stop capturing at all. */
  | { type: 'auto_paste_requested'; payload: { id: string; active: boolean } }
  /** Asks the host to store a different capture delimiter. Application-wide. */
  | { type: 'capture_delimiter_changed'; payload: { delimiter: CaptureDelimiter } }
  /** The note's timer changed in a way worth keeping. Never sent for a tick. */
  | { type: 'timer_changed'; payload: { id: string; timer: TimerSnapshot | null } }
  /** A run reached zero, exactly once. The host owns the words. */
  | { type: 'timer_finished'; payload: { id: string; kind: TimerFinishKind } }
  | { type: 'open_external_url'; payload: { url: string } }
  | { type: 'drag_start' }
  | { type: 'drag_update'; payload: { dx: number; dy: number } }
  | { type: 'drag_end' }
  | { type: 'resize_start' }
  | { type: 'resize_update'; payload: { dx: number; dy: number } }
  | { type: 'resize_end' }
  | { type: 'flush_response'; payload: { id: string; requestId: number; content: string } };

declare global {
  interface Window {
    webkit?: {
      messageHandlers?: {
        noteItHost?: {
          postMessage: (message: unknown) => void;
        };
      };
    };
    handleHostMessage?: (rawMessage: string | HostToWebviewMessage) => void;
  }
}
