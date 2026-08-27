export type PaperColor = 'yellow' | 'blue' | 'green' | 'pink' | 'purple' | 'gray' | 'black';

export type NoteLayerMode = 'overlay' | 'desktop' | 'hidden';

export interface NoteData {
  id: string;
  content: string;
  color: PaperColor;
  fontSize: number;
  collapsed: boolean;
  createdAt: string | null;
  updatedAt: string | null;
  zoomPercent: number;
  layerMode: NoteLayerMode;
}

export type HostToWebviewMessage =
  | { type: 'load_note'; payload: NoteData }
  | { type: 'set_timestamps'; payload: { createdAt: string | null; updatedAt: string | null } }
  | { type: 'set_layer_mode'; payload: { layerMode: NoteLayerMode } }
  | { type: 'set_collapsed'; payload: { collapsed: boolean } }
  | { type: 'set_color'; payload: { color: PaperColor } }
  | { type: 'set_font_size'; payload: { fontSize: number } }
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
  | { type: 'collapse_changed'; payload: { id: string; collapsed: boolean } }
  | { type: 'zoom_changed'; payload: { id: string; zoomPercent: number } }
  | { type: 'toggle_layer_mode' }
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
