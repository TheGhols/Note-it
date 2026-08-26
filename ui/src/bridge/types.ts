export type PaperColor = 'yellow' | 'blue' | 'green' | 'pink' | 'purple' | 'gray' | 'black';

export interface NoteData {
  id: string;
  content: string;
  color: PaperColor;
  fontSize: number;
}

export type HostToWebviewMessage =
  | { type: 'load_note'; payload: NoteData }
  | { type: 'set_color'; payload: { color: PaperColor } }
  | { type: 'set_font_size'; payload: { fontSize: number } }
  | { type: 'request_content' };

export type WebviewToHostMessage =
  | { type: 'ready' }
  | { type: 'content_changed'; payload: { id: string; content: string } }
  | { type: 'close_requested'; payload: { id: string } }
  | { type: 'new_note_requested' }
  | { type: 'color_changed'; payload: { id: string; color: PaperColor } }
  | { type: 'font_size_changed'; payload: { id: string; fontSize: number } }
  | { type: 'open_external_url'; payload: { url: string } };

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
