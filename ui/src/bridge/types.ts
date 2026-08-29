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
  | { type: 'set_font_size'; payload: { fontSize: number } }
  | { type: 'search_results'; payload: { requestId: number; results: SearchResult[] } }
  | { type: 'search_result_missing'; payload: { noteId: string } }
  | { type: 'reveal_match'; payload: { query: string } }
  | { type: 'trash_entries'; payload: { requestId: number; entries: TrashEntry[] } }
  | { type: 'data_result'; payload: { action: DataAction; ok: boolean; message: string } }
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
  | { type: 'collapse_changed'; payload: { id: string; collapsed: boolean } }
  | { type: 'zoom_changed'; payload: { id: string; zoomPercent: number } }
  | { type: 'toggle_layer_mode' }
  | { type: 'search_requested'; payload: { requestId: number; query: string } }
  | { type: 'open_search_result'; payload: { noteId: string; query: string } }
  | { type: 'trash_note_requested'; payload: { id: string } }
  | { type: 'trash_list_requested'; payload: { requestId: number } }
  | { type: 'restore_note_requested'; payload: { noteId: string } }
  | { type: 'backup_requested' }
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
